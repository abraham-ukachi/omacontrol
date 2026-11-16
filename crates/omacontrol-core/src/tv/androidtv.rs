//! Android TV / Google TV: the Android TV Remote protocol v2 (what Google's own TV remote speaks).
//!
//! Works with any Android TV or Google TV box or set, the Freebox Player TV Free 4K (Pop, Ultra,
//! Ultra Essentiel) included.
//!
//! | Step | Port | What happens |
//! |:-----|:-----|:-------------|
//! | 1 | - | We make our own RSA certificate once ([`Identity`]); the TV pairs *that*, not a token |
//! | 2 | `6467` | TLS with our certificate, then `pairing_request` → `options` → `configuration`: the TV shows a 6-character code |
//! | 3 | `6467` | The secret: `SHA-256(our modulus, our exponent, the TV's modulus, the TV's exponent, code[2..])`; its first byte must equal `code[0..2]` |
//! | 4 | `6466` | TLS with the same certificate: the TV sends `remote_configure`, `remote_set_active`, `remote_start`, and pings every 5 s |
//! | 5 | `6466` | Keys are `remote_key_inject` (Android key codes), apps are `remote_app_link_launch_request` |
//!
//! Every message is a protobuf with its varint length in front (see [`super::proto`]).
//! Reference: the public `polo.proto` / `remotemessage.proto` and the androidtvremote2 project's notes.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rustls::{ClientConnection, StreamOwned};
use serde::{Deserialize, Serialize};

use super::proto::{self, Msg};
use super::{tls, Remote};
use crate::{App, Error, Key, Result};

const PAIRING_PORT: u16 = 6467;
const REMOTE_PORT: u16 = 6466;
const STATUS_OK: u64 = 200;
const SHORT: u64 = 3;
/// PING | KEY | IME | VOICE | POWER | VOLUME | APP_LINK.
const FEATURE_IME: u64 = 4;
const FEATURE_VOICE: u64 = 8;
const FEATURES: u64 = 1 | 2 | FEATURE_IME | FEATURE_VOICE | 32 | 64 | 512;
/// Voice payloads: 16-bit PCM, 8 kHz, mono, sent in pieces of 8–20 KiB (smaller ones get dropped).
const VOICE_CHUNK_MAX: usize = 20 * 1024;
const VOICE_CHUNK_MIN: usize = 8 * 1024;
const KEYCODE_ENTER: u64 = 66;
const KEYCODE_ASSIST: u64 = 219;
const KEYCODE_SEARCH: u64 = 84;

type Tls = StreamOwned<ClientConnection, TcpStream>;

/// Our certificate and its private key (DER, base64 in the store). Made once, kept forever:
/// a new one means pairing every TV again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub cert: String,
    pub key: String,
}

impl Identity {
    /// A fresh 2048-bit RSA key and a self-signed certificate (a second or so on a phone).
    pub fn generate() -> Result<Identity> {
        use rsa::pkcs8::EncodePrivateKey;
        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048)
            .map_err(|e| Error::Protocol(format!("key: {e}")))?;
        let pkcs8 = key
            .to_pkcs8_der()
            .map_err(|e| Error::Protocol(format!("key: {e}")))?;
        let der = rustls::pki_types::PrivatePkcs8KeyDer::from(pkcs8.as_bytes().to_vec());
        let pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&der, &rcgen::PKCS_RSA_SHA256)
            .map_err(|e| Error::Protocol(format!("cert: {e}")))?;
        let mut params = rcgen::CertificateParams::new(vec!["omacontrol".to_string()])
            .map_err(|e| Error::Protocol(format!("cert: {e}")))?;
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "OmaControl");
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2044, 1, 1);
        let cert = params
            .self_signed(&pair)
            .map_err(|e| Error::Protocol(format!("cert: {e}")))?;
        Ok(Identity {
            cert: B64.encode(cert.der()),
            key: B64.encode(pkcs8.as_bytes()),
        })
    }

    fn der(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let bad = |_| Error::Protocol("the saved Android TV certificate is damaged".into());
        Ok((
            B64.decode(&self.cert).map_err(bad)?,
            B64.decode(&self.key).map_err(bad)?,
        ))
    }
}

/// The RSA modulus and exponent of a certificate, as minimal big-endian bytes.
fn rsa_numbers(cert_der: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    use x509_parser::prelude::*;
    let (_, cert) = X509Certificate::from_der(cert_der)
        .map_err(|e| Error::Protocol(format!("certificate: {e}")))?;
    match cert.public_key().parsed() {
        Ok(x509_parser::public_key::PublicKey::RSA(k)) => Ok((trim(k.modulus), trim(k.exponent))),
        _ => Err(Error::Protocol("the TV's certificate is not RSA".into())),
    }
}

fn trim(b: &[u8]) -> Vec<u8> {
    let start = b.iter().position(|&x| x != 0).unwrap_or(b.len());
    b[start..].to_vec()
}

/// The pairing secret for a 6-hex-character code, or an error when the code can't be right.
pub(crate) fn secret(
    client: &(Vec<u8>, Vec<u8>),
    server: &(Vec<u8>, Vec<u8>),
    code: &str,
) -> Result<Vec<u8>> {
    let code = code.trim().to_ascii_uppercase();
    let wrong =
        || Error::Protocol("that's not the code on the TV (6 characters, 0-9 and A-F)".into());
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(wrong());
    }
    let hex = |s: &str| {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect::<std::result::Result<Vec<u8>, _>>()
    };
    let check = hex(&code[..2]).map_err(|_| wrong())?[0];
    let tail = hex(&code[2..]).map_err(|_| wrong())?;
    let mut h = ring::digest::Context::new(&ring::digest::SHA256);
    for part in [&client.0, &client.1, &server.0, &server.1, &tail] {
        h.update(part);
    }
    let digest = h.finish().as_ref().to_vec();
    if digest[0] != check {
        return Err(wrong());
    }
    Ok(digest)
}

fn open(ip: &str, port: u16, identity: &Identity, remote: bool) -> Result<Tls> {
    let (cert, key) = identity.der()?;
    let config = tls::local_tv_config_with_identity(&cert, &key)
        .map_err(|e| Error::Protocol(e.to_string()))?;
    let addr: SocketAddr = format!("{ip}:{port}")
        .parse()
        .map_err(|_| Error::Protocol(format!("bad address {ip}")))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
    tcp.set_nodelay(true)?;
    let name = rustls::pki_types::ServerName::try_from(ip.to_string())
        .map_err(|e| Error::Protocol(e.to_string()))?;
    let conn = ClientConnection::new(config, name).map_err(|e| Error::Protocol(e.to_string()))?;
    let mut stream = StreamOwned::new(conn, tcp);
    // Finish the handshake now: an unknown certificate is refused here, on the remote port.
    while stream.conn.is_handshaking() {
        stream.conn.complete_io(&mut stream.sock).map_err(|e| {
            if remote {
                Error::NotPaired
            } else {
                Error::Io(e)
            }
        })?;
    }
    Ok(stream)
}

fn send(stream: &mut Tls, msg: &Msg) -> Result<()> {
    stream.write_all(&msg.framed())?;
    stream.flush()?;
    Ok(())
}

/// Reads one framed message (blocking up to the socket's timeout).
fn recv(stream: &mut Tls, buf: &mut Vec<u8>) -> Result<Vec<u8>> {
    loop {
        if let Some(m) = proto::drain_frames_one(buf) {
            return Ok(m);
        }
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(Error::Protocol("the TV closed the connection".into()));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// A pairing in progress: the TV is showing its code.
pub struct Pairing {
    stream: Tls,
    buf: Vec<u8>,
    client: (Vec<u8>, Vec<u8>),
}

fn polo() -> Msg {
    Msg::new().int(1, 2).int(2, STATUS_OK)
}

impl Pairing {
    /// Opens the pairing port and walks the TV to its code screen.
    pub fn start(ip: &str, identity: &Identity, client_name: &str) -> Result<Pairing> {
        Pairing::start_on(ip, PAIRING_PORT, identity, client_name)
    }

    pub(crate) fn start_on(
        ip: &str,
        port: u16,
        identity: &Identity,
        client_name: &str,
    ) -> Result<Pairing> {
        let (cert, _) = identity.der()?;
        let client = rsa_numbers(&cert)?;
        let mut stream = open(ip, port, identity, false)?;
        let mut buf = Vec::new();
        let hex6 = || Msg::new().int(1, 3).int(2, 6);
        let steps = [
            (
                polo().msg(10, Msg::new().str(1, "atvremote").str(2, client_name)),
                11,
            ),
            (polo().msg(20, Msg::new().msg(1, hex6()).int(3, 1)), 20),
            (polo().msg(30, Msg::new().msg(1, hex6()).int(2, 1)), 31),
        ];
        for (msg, expect) in steps {
            send(&mut stream, &msg)?;
            let reply = recv(&mut stream, &mut buf)?;
            check_status(&reply)?;
            if proto::sub(&reply, expect).is_none() && !(expect == 31 && has_field(&reply, 31)) {
                return Err(Error::Protocol(format!(
                    "unexpected pairing reply (wanted field {expect})"
                )));
            }
        }
        Ok(Pairing {
            stream,
            buf,
            client,
        })
    }

    /// Checks a code against both certificates without sending anything: a typo is caught
    /// here and the TV keeps showing its code.
    pub fn check(&self, code: &str) -> Result<Vec<u8>> {
        let server_cert = self
            .stream
            .conn
            .peer_certificates()
            .and_then(|c| c.first())
            .map(|c| c.as_ref().to_vec())
            .ok_or_else(|| Error::Protocol("the TV sent no certificate".into()))?;
        let server = rsa_numbers(&server_cert)?;
        secret(&self.client, &server, code)
    }

    /// Sends the secret made from the code the TV shows. Ok means the TV now knows our certificate.
    pub fn finish(mut self, code: &str) -> Result<()> {
        let secret = self.check(code)?;
        send(
            &mut self.stream,
            &polo().msg(40, Msg::new().bytes(1, &secret)),
        )?;
        let reply = recv(&mut self.stream, &mut self.buf)?;
        check_status(&reply).map_err(|_| Error::PairingRefused)?;
        if has_field(&reply, 41) {
            Ok(())
        } else {
            Err(Error::PairingRefused)
        }
    }
}

fn has_field(msg: &[u8], field: u32) -> bool {
    proto::fields(msg).iter().any(|(f, _)| *f == field)
}

fn check_status(reply: &[u8]) -> Result<()> {
    match proto::int(reply, 2) {
        Some(STATUS_OK) => Ok(()),
        Some(s) => Err(Error::Protocol(format!("the TV answered status {s}"))),
        None => Err(Error::Protocol(
            "the TV sent an empty pairing message".into(),
        )),
    }
}

fn config_supports_voice(config: &[u8]) -> bool {
    proto::int(config, 1).is_some_and(|features| features & FEATURE_VOICE != 0)
}

fn voice_payload(session: u64, samples: &[u8]) -> Msg {
    Msg::new().msg(31, Msg::new().int(1, session).bytes(2, samples))
}

fn voice_begin(session: u64) -> Msg {
    Msg::new().msg(30, Msg::new().int(1, session))
}

fn voice_end(session: u64) -> Msg {
    Msg::new().msg(32, Msg::new().int(1, session))
}

/// The session a `remote_voice_begin` opens; protobuf leaves out a session id of 0.
fn voice_session_id(message: &[u8]) -> Option<u64> {
    proto::sub(message, 30).map(|begin| proto::int(begin, 1).unwrap_or(0))
}

/// The focused text field, as the TV reports it with the IME feature on.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct ImeField {
    ime_counter: u64,
    field_counter: u64,
    /// Whether the TV told us about a field at all; without it, text goes as key presses.
    known: bool,
}

/// Reads the IME counters from `remote_ime_key_inject` (20), `remote_ime_show_request` (22)
/// or `remote_ime_batch_edit` (21).
fn ime_update(message: &[u8], ime: &mut ImeField) -> bool {
    if let Some(inject) = proto::sub(message, 20) {
        if let Some(app) = proto::sub(inject, 1) {
            ime.ime_counter = proto::int(app, 1).unwrap_or(0);
        }
        if let Some(field) = proto::sub(inject, 2) {
            ime.field_counter = proto::int(field, 1).unwrap_or(0);
        }
        ime.known = true;
        return true;
    }
    if let Some(show) = proto::sub(message, 22) {
        if let Some(field) = proto::sub(show, 2) {
            ime.field_counter = proto::int(field, 1).unwrap_or(0);
        }
        ime.known = true;
        return true;
    }
    if let Some(edit) = proto::sub(message, 21) {
        ime.ime_counter = proto::int(edit, 1).unwrap_or(ime.ime_counter);
        ime.field_counter = proto::int(edit, 2).unwrap_or(ime.field_counter);
        ime.known = true;
        return true;
    }
    false
}

/// Replaces the focused field's text with `text`, cursor at its end.
fn ime_batch_edit(ime: ImeField, text: &str) -> Msg {
    let len = text.chars().count() as u64;
    let object = Msg::new().int(1, len).int(2, len).str(3, text);
    let edit = Msg::new().int(1, 1).msg(2, object);
    Msg::new().msg(
        21,
        Msg::new()
            .int(1, ime.ime_counter)
            .int(2, ime.field_counter)
            .msg(3, edit),
    )
}

/// Splits captured audio into payloads the TV accepts, padding short ones with silence.
fn voice_chunks(samples: &[u8]) -> Vec<Vec<u8>> {
    samples
        .chunks(VOICE_CHUNK_MAX)
        .map(|piece| {
            let mut piece = piece.to_vec();
            if piece.len() < VOICE_CHUNK_MIN {
                piece.resize(VOICE_CHUNK_MIN, 0);
            }
            piece
        })
        .collect()
}

/// The remote, once paired. A reader thread answers the TV's pings so it keeps us connected.
pub(crate) struct AndroidTv {
    ip: String,
    port: u16,
    identity: Identity,
    link: Option<Link>,
}

struct Link {
    stream: Arc<Mutex<Tls>>,
    alive: Arc<AtomicBool>,
    voice_supported: bool,
    ime_supported: bool,
    voice_session: Arc<Mutex<Option<u64>>>,
    ime: Arc<Mutex<ImeField>>,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

/// Answers one message from the TV; `Some(true)` once the remote has started.
fn answer(
    stream: &mut Tls,
    msg: &[u8],
    voice_session: &Mutex<Option<u64>>,
    ime: &Mutex<ImeField>,
    active: &AtomicU64,
) -> Result<Option<bool>> {
    if ime_update(msg, &mut ime.lock().unwrap()) {
        return Ok(None);
    }
    if let Some(cfg) = proto::sub(msg, 1) {
        let theirs = proto::int(cfg, 1).unwrap_or(FEATURES);
        let info = Msg::new()
            .str(1, "OmaControl")
            .str(2, "OmaControl")
            .int(3, 1)
            .str(4, "1")
            .str(5, "atvremote")
            .str(6, env!("CARGO_PKG_VERSION"));
        active.store(FEATURES & theirs, Ordering::Relaxed);
        send(
            stream,
            &Msg::new().msg(1, Msg::new().int(1, FEATURES & theirs).msg(2, info)),
        )?;
    } else if proto::sub(msg, 2).is_some() || has_field(msg, 2) {
        // set_active: answer with the features both sides agreed on.
        let features = active.load(Ordering::Relaxed);
        send(stream, &Msg::new().msg(2, Msg::new().int(1, features)))?;
    } else if let Some(ping) = proto::sub(msg, 8) {
        let val = proto::int(ping, 1).unwrap_or(0);
        send(stream, &Msg::new().msg(9, Msg::new().int(1, val)))?;
    } else if let Some(session) = voice_session_id(msg) {
        *voice_session.lock().unwrap() = Some(session);
    } else if proto::sub(msg, 32).is_some() {
        // The TV closed the voice session (it heard a pause, or gave up).
        *voice_session.lock().unwrap() = None;
    } else if has_field(msg, 40) {
        return Ok(Some(true));
    }
    Ok(None)
}

fn link(ip: &str, port: u16, identity: &Identity) -> Result<Link> {
    let mut stream = open(ip, port, identity, true)?;
    let mut buf = Vec::new();
    let voice_session = Arc::new(Mutex::new(None));
    let ime = Arc::new(Mutex::new(ImeField::default()));
    let active = Arc::new(AtomicU64::new(FEATURES));
    let deadline = Instant::now() + Duration::from_secs(6);
    // configure → set_active → start (the TV may skip start while it sleeps).
    let mut configured = false;
    let mut voice_supported = false;
    while Instant::now() < deadline {
        let msg = match recv(&mut stream, &mut buf) {
            Ok(m) => m,
            Err(Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
                    && configured =>
            {
                break
            }
            Err(Error::Io(e))
                if matches!(
                    e.kind(),
                    ErrorKind::ConnectionReset | ErrorKind::UnexpectedEof | ErrorKind::InvalidData
                ) =>
            {
                return Err(Error::NotPaired)
            }
            Err(e) => return Err(e),
        };
        if let Some(config) = proto::sub(&msg, 1) {
            configured = true;
            voice_supported = config_supports_voice(config);
        }
        if answer(&mut stream, &msg, &voice_session, &ime, &active)? == Some(true) {
            break;
        }
    }
    if !configured {
        return Err(Error::Protocol(
            "the TV didn't start the remote session".into(),
        ));
    }
    stream
        .sock
        .set_read_timeout(Some(Duration::from_millis(80)))?;
    let stream = Arc::new(Mutex::new(stream));
    let alive = Arc::new(AtomicBool::new(true));
    let (s, a, voice, ime2, active2) = (
        stream.clone(),
        alive.clone(),
        voice_session.clone(),
        ime.clone(),
        active.clone(),
    );
    thread::spawn(move || {
        let mut buf = buf;
        while a.load(Ordering::Relaxed) {
            {
                let mut st = s.lock().unwrap();
                let mut chunk = [0u8; 4096];
                match st.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                    Err(_) => break,
                }
                for msg in proto::drain_frames(&mut buf) {
                    if answer(&mut st, &msg, &voice, &ime2, &active2).is_err() {
                        a.store(false, Ordering::Relaxed);
                    }
                }
            }
            thread::sleep(Duration::from_millis(40));
        }
        a.store(false, Ordering::Relaxed);
    });
    let ime_supported = active.load(Ordering::Relaxed) & FEATURE_IME != 0;
    Ok(Link {
        stream,
        alive,
        voice_supported,
        ime_supported,
        voice_session,
        ime,
    })
}

impl AndroidTv {
    /// Opens the remote port. [`Error::NotPaired`] means: pair first ([`Pairing`]).
    pub(crate) fn connect(ip: &str, identity: &Identity) -> Result<AndroidTv> {
        AndroidTv::connect_on(ip, REMOTE_PORT, identity)
    }

    pub(crate) fn connect_on(ip: &str, port: u16, identity: &Identity) -> Result<AndroidTv> {
        let link = link(ip, port, identity)?;
        Ok(AndroidTv {
            ip: ip.to_string(),
            port,
            identity: identity.clone(),
            link: Some(link),
        })
    }

    /// Sends one message, reconnecting once when the TV dropped us (it does after sleep).
    fn send(&mut self, msg: Msg) -> Result<()> {
        for attempt in 0..2 {
            if self
                .link
                .as_ref()
                .is_none_or(|l| !l.alive.load(Ordering::Relaxed))
                || attempt == 1
            {
                self.link = None;
                self.link = Some(link(&self.ip, self.port, &self.identity)?);
            }
            let l = self.link.as_ref().unwrap();
            let result = send(&mut l.stream.lock().unwrap(), &msg);
            if result.is_ok() {
                return Ok(());
            }
            l.alive.store(false, Ordering::Relaxed);
            if attempt == 1 {
                return result;
            }
        }
        Ok(())
    }

    fn key(&mut self, code: u64) -> Result<()> {
        self.send(Msg::new().msg(10, Msg::new().int(1, code).int(2, SHORT)))
    }

    /// Types text as key presses, for TVs that never reported a focused field.
    fn type_keys(&mut self, text: &str) -> Result<()> {
        for c in text.chars() {
            let Some((code, shift)) = char_keycode(c) else {
                continue;
            };
            if shift {
                // SHIFT_LEFT down, the letter, SHIFT_LEFT up.
                self.send(Msg::new().msg(10, Msg::new().int(1, 59).int(2, 1)))?;
                self.key(code)?;
                self.send(Msg::new().msg(10, Msg::new().int(1, 59).int(2, 2)))?;
            } else {
                self.key(code)?;
            }
        }
        Ok(())
    }
}

/// Android `KeyEvent` codes for the remote's buttons.
pub(crate) fn keycode(key: Key) -> u64 {
    match key {
        Key::Power => 26,
        Key::Input => 178,
        Key::Mute => 164,
        Key::Up => 19,
        Key::Down => 20,
        Key::Left => 21,
        Key::Right => 22,
        Key::Ok => 23,
        Key::Back => 4,
        Key::Home => 3,
        Key::Menu => 82,
        Key::VolumeUp => 24,
        Key::VolumeDown => 25,
        Key::ChannelUp => 166,
        Key::ChannelDown => 167,
        Key::PlayPause => 85,
        Key::Rewind => 89,
        Key::FastForward => 90,
        Key::Digit(d) => 7 + u64::from(d.min(9)),
        Key::Dash => 69,
    }
}

/// Android key codes for a typed character (letters, digits and common signs).
pub(crate) fn char_keycode(c: char) -> Option<(u64, bool)> {
    let c_lower = c.to_ascii_lowercase();
    Some(match c_lower {
        'a'..='z' => (29 + (c_lower as u64 - 'a' as u64), c.is_ascii_uppercase()),
        '0'..='9' => (7 + (c as u64 - '0' as u64), false),
        ' ' => (62, false),
        '.' => (56, false),
        ',' => (55, false),
        '-' => (69, false),
        '@' => (77, false),
        '/' => (76, false),
        '\'' => (75, false),
        ';' => (74, false),
        ':' => (74, true),
        '?' => (76, true),
        '_' => (69, true),
        '=' => (70, false),
        '+' => (81, false),
        '#' => (18, false),
        '*' => (17, false),
        '&' => (14, true),
        '!' => (8, true),
        '%' => (12, true),
        _ => return None,
    })
}

/// The deep link each streaming app opens with.
pub(crate) fn app_link(app: App) -> &'static str {
    match app {
        App::Netflix => "https://www.netflix.com/title",
        App::YouTube => "https://www.youtube.com",
        App::PrimeVideo => "https://app.primevideo.com",
        App::DisneyPlus => "https://www.disneyplus.com",
        App::CanalPlus => "https://www.canalplus.com",
        App::Spotify => "spotify://",
    }
}

impl Remote for AndroidTv {
    fn press(&mut self, key: Key) -> Result<()> {
        self.key(keycode(key))
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        // With IME on, the TV tells us which field is focused: replace its text in one edit,
        // then Enter, so a browser goes to the URL and a search box searches.
        let field = self
            .link
            .as_ref()
            .filter(|l| l.ime_supported)
            .map(|l| *l.ime.lock().unwrap())
            .filter(|f| f.known);
        if let Some(field) = field {
            self.send(ime_batch_edit(field, text))?;
            thread::sleep(Duration::from_millis(150));
            return self.key(KEYCODE_ENTER);
        }
        self.type_keys(text)?;
        self.key(KEYCODE_ENTER)
    }

    fn launch(&mut self, app: App) -> Result<()> {
        self.send(Msg::new().msg(90, Msg::new().str(1, app_link(app))))
    }

    fn supports_voice(&self) -> bool {
        // Every Android TV gets the mic: without the voice feature it opens the Assistant there.
        self.link.is_some()
    }

    fn voice_start(&mut self) -> Result<()> {
        let link = self
            .link
            .as_ref()
            .ok_or_else(|| Error::Protocol("the Android TV remote is disconnected".into()))?;
        if !link.voice_supported {
            // No phone-mic streaming on this TV: open its Google Assistant instead.
            self.key(KEYCODE_ASSIST)?;
            return Err(Error::Unsupported(super::ASSISTANT_ON_TV.into()));
        }
        let voice_session = link.voice_session.clone();
        *voice_session.lock().unwrap() = None;
        self.key(KEYCODE_SEARCH)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(session) = *voice_session.lock().unwrap() {
                return self.send(voice_begin(session));
            }
            if Instant::now() >= deadline {
                // The Search key opened the Assistant, but it listens to its own remote.
                return Err(Error::Unsupported(super::ASSISTANT_ON_TV.into()));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn voice_audio(&mut self, samples: &[u8]) -> Result<()> {
        let session = self
            .link
            .as_ref()
            .and_then(|link| *link.voice_session.lock().unwrap());
        // The TV ends the session itself once it has heard enough: drop the rest.
        let Some(session) = session else { return Ok(()) };
        for chunk in voice_chunks(samples) {
            self.send(voice_payload(session, &chunk))?;
        }
        Ok(())
    }

    fn voice_stop(&mut self) -> Result<()> {
        let session = self
            .link
            .as_ref()
            .and_then(|link| link.voice_session.lock().unwrap().take());
        match session {
            Some(session) => self.send(voice_end(session)),
            None => Ok(()),
        }
    }

    fn token(&self) -> Option<String> {
        Some("paired".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_checks_the_first_byte() {
        let client = (vec![0xC1; 256], vec![1, 0, 1]);
        let server = (vec![0xD2; 256], vec![1, 0, 1]);
        // Work out the right first byte for the tail "ABCD", then try it and a wrong one.
        let mut h = ring::digest::Context::new(&ring::digest::SHA256);
        for part in [
            &client.0,
            &client.1,
            &server.0,
            &server.1,
            &vec![0xAB, 0xCD],
        ] {
            h.update(part);
        }
        let digest = h.finish().as_ref().to_vec();
        let good = format!("{:02X}ABCD", digest[0]);
        assert_eq!(secret(&client, &server, &good).unwrap(), digest);
        assert_eq!(
            secret(&client, &server, &good.to_lowercase()).unwrap(),
            digest
        );
        let bad = format!("{:02X}ABCD", digest[0].wrapping_add(1));
        assert!(secret(&client, &server, &bad).is_err());
        assert!(secret(&client, &server, "12345").is_err());
        assert!(secret(&client, &server, "12345G").is_err());
    }

    #[test]
    fn keycodes_match_android() {
        assert_eq!(keycode(Key::Ok), 23);
        assert_eq!(keycode(Key::Digit(0)), 7);
        assert_eq!(keycode(Key::Digit(9)), 16);
        assert_eq!(char_keycode('a'), Some((29, false)));
        assert_eq!(char_keycode('Z'), Some((54, true)));
        assert_eq!(char_keycode('5'), Some((12, false)));
        assert_eq!(char_keycode('é'), None);
    }

    #[test]
    fn trims_leading_zeros() {
        assert_eq!(trim(&[0, 0, 1, 0, 1]), vec![1, 0, 1]);
    }

    #[test]
    fn a_voice_session_id_of_zero_still_opens_the_session() {
        let begin = Msg::new().msg(30, Msg::new());
        assert_eq!(voice_session_id(&begin.0), Some(0));
    }

    #[test]
    fn voice_audio_is_split_and_padded_for_the_tv() {
        let chunks = voice_chunks(&vec![1u8; 25 * 1024]);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), VOICE_CHUNK_MAX);
        assert_eq!(chunks[1].len(), VOICE_CHUNK_MIN);
        assert_eq!(voice_chunks(&[7u8; 100])[0].len(), VOICE_CHUNK_MIN);
    }

    #[test]
    fn ime_counters_come_from_the_focused_field() {
        let mut ime = ImeField::default();
        let inject = Msg::new().msg(
            20,
            Msg::new()
                .msg(1, Msg::new().int(1, 5).str(12, "com.android.chrome"))
                .msg(2, Msg::new().int(1, 9)),
        );
        assert!(ime_update(&inject.0, &mut ime));
        assert_eq!((ime.ime_counter, ime.field_counter, ime.known), (5, 9, true));
        let show = Msg::new().msg(22, Msg::new().msg(2, Msg::new().int(1, 11)));
        assert!(ime_update(&show.0, &mut ime));
        assert_eq!(ime.field_counter, 11);
        assert!(!ime_update(&Msg::new().msg(8, Msg::new().int(1, 1)).0, &mut ime));

        let edit = ime_batch_edit(ime, "bfmtv.com");
        let batch = proto::sub(&edit.0, 21).unwrap();
        assert_eq!(proto::int(batch, 1), Some(5));
        assert_eq!(proto::int(batch, 2), Some(11));
        let info = proto::sub(batch, 3).unwrap();
        assert_eq!(proto::int(info, 1), Some(1));
        let object = proto::sub(info, 2).unwrap();
        assert_eq!(proto::int(object, 1), Some(9));
        assert_eq!(proto::sub(object, 3), Some(&b"bfmtv.com"[..]));
    }

    #[test]
    fn url_signs_have_key_codes() {
        for c in "https://a.b/c?d=e_f".chars() {
            assert!(char_keycode(c).is_some(), "{c}");
        }
    }

    #[test]
    fn voice_messages_use_negotiated_features_and_the_tv_session_id() {
        assert!(config_supports_voice(&Msg::new().int(1, FEATURE_VOICE).0));
        assert!(!config_supports_voice(&Msg::new().int(1, 2).0));

        let begin = Msg::new().msg(30, Msg::new().int(1, 17));
        assert_eq!(voice_session_id(&begin.0), Some(17));
        assert_eq!(proto::int(proto::sub(&voice_begin(17).0, 30).unwrap(), 1), Some(17));

        let payload = voice_payload(17, &[1, 2, 3]);
        let message = proto::sub(&payload.0, 31).unwrap();
        assert_eq!(proto::int(message, 1), Some(17));
        assert_eq!(proto::sub(message, 2), Some(&[1, 2, 3][..]));

        let end = voice_end(17);
        assert_eq!(proto::int(proto::sub(&end.0, 32).unwrap(), 1), Some(17));
    }

    #[test]
    #[ignore = "slow in debug builds (RSA key generation)"]
    fn generated_identity_is_rsa_65537() {
        let id = Identity::generate().unwrap();
        let (cert, key) = id.der().unwrap();
        let (n, e) = rsa_numbers(&cert).unwrap();
        assert_eq!(e, vec![1, 0, 1]);
        assert_eq!(n.len(), 256);
        assert!(tls::local_tv_config_with_identity(&cert, &key).is_ok());
    }
}

/// A pretend Android TV on localhost: it pairs like the real thing (checks our secret against
/// its own certificate), then runs a remote session and records the keys it gets.
#[cfg(test)]
mod fake_tv {
    use super::*;
    use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
    use rustls::{
        DigitallySignedStruct, DistinguishedName, ServerConfig, ServerConnection, SignatureScheme,
    };
    use std::net::TcpListener;

    #[derive(Debug)]
    struct AnyClient(Arc<rustls::crypto::CryptoProvider>);

    impl ClientCertVerifier for AnyClient {
        fn root_hint_subjects(&self) -> &[DistinguishedName] {
            &[]
        }
        fn verify_client_cert(
            &self,
            _: &rustls::pki_types::CertificateDer<'_>,
            _: &[rustls::pki_types::CertificateDer<'_>],
            _: rustls::pki_types::UnixTime,
        ) -> std::result::Result<ClientCertVerified, rustls::Error> {
            Ok(ClientCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            m: &[u8],
            c: &rustls::pki_types::CertificateDer<'_>,
            d: &DigitallySignedStruct,
        ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
        {
            rustls::crypto::verify_tls12_signature(
                m,
                c,
                d,
                &self.0.signature_verification_algorithms,
            )
        }
        fn verify_tls13_signature(
            &self,
            m: &[u8],
            c: &rustls::pki_types::CertificateDer<'_>,
            d: &DigitallySignedStruct,
        ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
        {
            rustls::crypto::verify_tls13_signature(
                m,
                c,
                d,
                &self.0.signature_verification_algorithms,
            )
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    fn server_config(tv: &Identity) -> Arc<ServerConfig> {
        let (cert, key) = tv.der().unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(Arc::new(AnyClient(provider)))
            .with_single_cert(
                vec![cert.into()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(key.into()),
            )
            .unwrap();
        Arc::new(config)
    }

    type Server = StreamOwned<ServerConnection, TcpStream>;

    fn accept(listener: &TcpListener, config: &Arc<ServerConfig>) -> Server {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        StreamOwned::new(ServerConnection::new(config.clone()).unwrap(), tcp)
    }

    fn read(s: &mut Server, buf: &mut Vec<u8>) -> Vec<u8> {
        loop {
            if let Some(m) = proto::drain_frames_one(buf) {
                return m;
            }
            let mut chunk = [0u8; 4096];
            let n = s.read(&mut chunk).unwrap();
            assert!(n > 0, "client hung up");
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    fn write(s: &mut Server, m: Msg) {
        s.write_all(&m.framed()).unwrap();
        s.flush().unwrap();
    }

    #[test]
    #[ignore = "slow in debug builds (RSA key generation); run with --release -- --ignored"]
    fn pairs_and_presses_keys() {
        let tv = Identity::generate().unwrap();
        let phone = Identity::generate().unwrap();
        let config = server_config(&tv);
        let pairing_port = TcpListener::bind("127.0.0.1:0").unwrap();
        let remote_port = TcpListener::bind("127.0.0.1:0").unwrap();
        let (pp, rp) = (
            pairing_port.local_addr().unwrap().port(),
            remote_port.local_addr().unwrap().port(),
        );
        let code = Arc::new(Mutex::new(String::new()));

        let (cfg, shown) = (config.clone(), code.clone());
        let tv_numbers = rsa_numbers(&tv.der().unwrap().0).unwrap();
        let server = thread::spawn(move || {
            // Pairing.
            let mut s = accept(&pairing_port, &cfg);
            let mut buf = Vec::new();
            let req = read(&mut s, &mut buf);
            assert_eq!(
                proto::sub(proto::sub(&req, 10).unwrap(), 2),
                Some(&b"OmaControl test"[..])
            );
            write(&mut s, polo().msg(11, Msg::new().str(1, "Fake TV")));
            let opts = read(&mut s, &mut buf);
            assert!(proto::sub(&opts, 20).is_some());
            write(
                &mut s,
                polo().msg(
                    20,
                    Msg::new().msg(1, Msg::new().int(1, 3).int(2, 6)).int(3, 1),
                ),
            );
            let conf = read(&mut s, &mut buf);
            assert!(proto::sub(&conf, 30).is_some());
            // The TV "shows" a code made from both certificates.
            let client_cert = s.conn.peer_certificates().unwrap()[0].as_ref().to_vec();
            let client_numbers = rsa_numbers(&client_cert).unwrap();
            let digest = {
                let mut h = ring::digest::Context::new(&ring::digest::SHA256);
                for part in [
                    &client_numbers.0,
                    &client_numbers.1,
                    &tv_numbers.0,
                    &tv_numbers.1,
                    &vec![0x12, 0x34],
                ] {
                    h.update(part);
                }
                h.finish().as_ref().to_vec()
            };
            *shown.lock().unwrap() = format!("{:02x}1234", digest[0]);
            write(&mut s, polo().msg(31, Msg::new()));
            let secret = read(&mut s, &mut buf);
            assert_eq!(
                proto::sub(proto::sub(&secret, 40).unwrap(), 1),
                Some(&digest[..])
            );
            write(&mut s, polo().msg(41, Msg::new().bytes(1, &digest)));

            // Remote session.
            let mut s = accept(&remote_port, &cfg);
            let mut buf = Vec::new();
            write(
                &mut s,
                Msg::new().msg(
                    1,
                    Msg::new()
                        .int(1, 639)
                        .msg(2, Msg::new().str(1, "Fake").str(2, "Free")),
                ),
            );
            let cfg_reply = read(&mut s, &mut buf);
            assert_eq!(
                proto::int(proto::sub(&cfg_reply, 1).unwrap(), 1),
                Some(FEATURES & 639)
            );
            write(&mut s, Msg::new().msg(2, Msg::new().int(1, 639)));
            let active = read(&mut s, &mut buf);
            assert!(proto::sub(&active, 2).is_some());
            write(&mut s, Msg::new().msg(40, Msg::new().int(1, 1)));
            write(&mut s, Msg::new().msg(8, Msg::new().int(1, 7).int(2, 1)));
            let mut keys = Vec::new();
            let mut pong = None;
            while keys.len() < 3 || pong.is_none() {
                let m = read(&mut s, &mut buf);
                if let Some(p) = proto::sub(&m, 9) {
                    pong = proto::int(p, 1);
                } else if let Some(k) = proto::sub(&m, 10) {
                    keys.push(proto::int(k, 1).unwrap());
                } else if let Some(l) = proto::sub(&m, 90) {
                    keys.push(proto::sub(l, 1).unwrap().len() as u64);
                }
            }
            (keys, pong)
        });

        let pairing = Pairing::start_on("127.0.0.1", pp, &phone, "OmaControl test").unwrap();
        let shown = code.lock().unwrap().clone();
        // A wrong code is refused locally, before anything is sent.
        assert!(pairing.check(&shown.replace("1234", "9999")).is_err());
        pairing.finish(&shown).unwrap();

        let mut remote = AndroidTv::connect_on("127.0.0.1", rp, &phone).unwrap();
        remote.press(Key::Ok).unwrap();
        remote.press(Key::Digit(4)).unwrap();
        remote.launch(App::YouTube).unwrap();
        let (keys, pong) = server.join().unwrap();
        assert_eq!(keys, vec![23, 11, app_link(App::YouTube).len() as u64]);
        assert_eq!(pong, Some(7));
    }
}
