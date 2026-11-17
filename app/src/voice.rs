//! The mic key.
//!
//! Hold it to talk and let go to send (walkie-talkie), or tap it to listen hands-free and tap
//! again to send; `remote.slint` tells the two apart and calls `voice-start` / `voice-stop`.
//!
//! | TV | What happens |
//! |:---|:-------------|
//! | Takes the phone's audio (Android TV voice feature) | The phone records 8 kHz PCM and streams it to the TV's voice search (`Job::Voice*`) |
//! | Doesn't (or never opens a voice session) | The phone's own `SpeechRecognizer` turns speech into text, and the text goes into the TV's search (`Job::VoiceText`) |
//!
//! Everything here runs on the UI thread; the state lives in a thread-local.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use slint::ComponentHandle;

#[cfg(target_os = "android")]
use super::{android_speech, android_voice};
use super::{AppWindow, Job};

/// Who a worker report is about: the audio stream to the TV, or text the phone heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VoiceSource {
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    Stream,
    Phone,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Mode {
    #[default]
    Idle,
    /// Recording audio for the TV.
    Stream,
    /// The phone's recognizer.
    Phone,
}

#[derive(Default)]
struct VoiceUi {
    mode: Mode,
    /// Stops the audio recording, while there is one.
    capture: Option<Arc<AtomicBool>>,
    /// The phone's recognizer is listening (no result yet).
    listening: bool,
    /// The finger left the mic (hold) or the mic was tapped again (tap): send.
    released: bool,
    /// What the recognizer heard, waiting for the release.
    text: Option<String>,
    /// This TV never opened a voice session: go straight to the phone's recognizer.
    streams_failed: bool,
    jobs: Option<Sender<Job>>,
}

thread_local! {
    static VOICE: RefCell<VoiceUi> = RefCell::new(VoiceUi::default());
}

fn status(ui: &AppWindow, code: &str, detail: &str, active: bool) {
    ui.set_voice_active(active);
    ui.set_voice_status_code(code.into());
    ui.set_voice_status_detail(detail.into());
}

/// A worker's word on the voice session, shown only while that session is the current one.
pub(crate) fn voice_report(
    weak: &slint::Weak<AppWindow>,
    source: VoiceSource,
    code: &str,
    detail: String,
    active: bool,
) {
    let code = code.to_string();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        let current = VOICE.with(|v| {
            let v = v.borrow();
            let mode = match source {
                VoiceSource::Stream => Mode::Stream,
                VoiceSource::Phone => Mode::Phone,
            };
            // A late "listening" after the finger already let go would be wrong.
            v.mode == mode && !(code == "active" && v.released)
        });
        if current {
            status(&ui, &code, &detail, active);
        }
    });
}

/// Wires the mic key's callbacks.
pub(crate) fn wire_voice(ui: &AppWindow, jobs: &Sender<Job>) {
    VOICE.with(|v| v.borrow_mut().jobs = Some(jobs.clone()));
    #[cfg(target_os = "android")]
    {
        let weak = ui.as_weak();
        android_speech::set_sink(move |event| {
            let _ = weak.upgrade_in_event_loop(move |ui| speech_event(&ui, event));
        });
    }
    let weak = ui.as_weak();
    ui.on_voice_start(move || {
        if let Some(ui) = weak.upgrade() {
            start(&ui);
        }
    });
    let weak = ui.as_weak();
    ui.on_voice_stop(move || {
        if let Some(ui) = weak.upgrade() {
            release(&ui);
        }
    });
}

/// A new TV: give its voice stream a fresh chance.
pub(crate) fn connected() {
    VOICE.with(|v| v.borrow_mut().streams_failed = false);
}

fn reset(v: &mut VoiceUi) {
    if let Some(stop) = v.capture.take() {
        stop.store(true, Ordering::Relaxed);
    }
    #[cfg(target_os = "android")]
    if v.listening {
        android_speech::cancel();
    }
    v.mode = Mode::Idle;
    v.listening = false;
    v.released = false;
    v.text = None;
}

fn start(ui: &AppWindow) {
    if !ui.get_voice_supported() {
        return;
    }
    let phone = VOICE.with(|v| {
        let mut v = v.borrow_mut();
        reset(&mut v);
        !ui.get_voice_streams() || v.streams_failed
    });
    if phone {
        start_phone(ui);
    } else {
        start_stream(ui);
    }
}

#[cfg(target_os = "android")]
fn start_stream(ui: &AppWindow) {
    let Some(tx) = VOICE.with(|v| v.borrow().jobs.clone()) else { return };
    match android_voice::start(tx, ui.as_weak()) {
        Ok(stop) => {
            VOICE.with(|v| {
                let mut v = v.borrow_mut();
                v.capture = Some(stop);
                v.mode = Mode::Stream;
            });
            status(ui, "starting", "", true);
        }
        Err(android_voice::StartError::Permission) => status(ui, "permission", "", false),
        Err(android_voice::StartError::Other(error)) => status(ui, "error", &error, false),
    }
}

#[cfg(not(target_os = "android"))]
fn start_stream(ui: &AppWindow) {
    status(ui, "no-recognizer", "", false);
}

#[cfg(target_os = "android")]
fn start_phone(ui: &AppWindow) {
    match android_voice::microphone_allowed() {
        Ok(true) => {}
        Ok(false) => return status(ui, "permission", "", false),
        Err(error) => return status(ui, "error", &error, false),
    }
    match android_speech::start("") {
        Ok(()) => {
            VOICE.with(|v| {
                let mut v = v.borrow_mut();
                v.mode = Mode::Phone;
                v.listening = true;
            });
            status(ui, "active", "", true);
        }
        Err(error) => {
            log::warn!("speech recognizer: {error}");
            status(ui, "no-recognizer", "", false);
        }
    }
}

#[cfg(not(target_os = "android"))]
fn start_phone(ui: &AppWindow) {
    status(ui, "no-recognizer", "", false);
}

/// The finger let go (hold) or the mic was tapped again (tap): send what was heard.
fn release(ui: &AppWindow) {
    enum Next {
        Nothing,
        StreamStopped,
        Send(String),
        Recognize,
    }
    let next = VOICE.with(|v| {
        let mut v = v.borrow_mut();
        v.released = true;
        if let Some(stop) = v.capture.take() {
            stop.store(true, Ordering::Relaxed);
            return Next::StreamStopped;
        }
        if v.mode != Mode::Phone {
            return Next::Nothing;
        }
        match v.text.take() {
            Some(text) => Next::Send(text),
            None if v.listening => Next::Recognize,
            None => Next::Nothing,
        }
    });
    match next {
        // The worker sends the rest and lets the TV finish hearing; it reports "sent".
        Next::StreamStopped => status(ui, "finishing", "", false),
        Next::Send(text) => send_text(ui, text),
        Next::Recognize => {
            #[cfg(target_os = "android")]
            android_speech::stop();
            status(ui, "recognizing", "", false);
        }
        Next::Nothing => ui.set_voice_active(false),
    }
}

fn send_text(ui: &AppWindow, text: String) {
    let jobs = VOICE.with(|v| v.borrow().jobs.clone());
    status(ui, "sending", &text, false);
    if let Some(jobs) = jobs {
        let _ = jobs.send(Job::VoiceText(text));
    }
}

/// The TV opened no voice session after all: listen on the phone for the rest of this press.
#[cfg(target_os = "android")]
pub(crate) fn fallback(ui: &AppWindow) {
    let still_talking = VOICE.with(|v| {
        let mut v = v.borrow_mut();
        v.streams_failed = true;
        if v.mode != Mode::Stream {
            return None;
        }
        if let Some(stop) = v.capture.take() {
            stop.store(true, Ordering::Relaxed);
        }
        v.mode = Mode::Idle;
        Some(!v.released)
    });
    match still_talking {
        // The press is still going on: what the phone hears goes out on release.
        Some(true) => start_phone(ui),
        // Already let go: what was said went nowhere, so ask again.
        Some(false) => status(ui, "no-speech", "", false),
        None => {}
    }
}

#[cfg(target_os = "android")]
fn speech_event(ui: &AppWindow, event: android_speech::Event) {
    use android_speech::Event;
    enum Next {
        Nothing,
        Show(&'static str, String, bool),
        Send(String),
    }
    let next = VOICE.with(|v| {
        let mut v = v.borrow_mut();
        if v.mode != Mode::Phone {
            return Next::Nothing;
        }
        match event {
            Event::Partial(text) if v.listening && !v.released => Next::Show("heard", text, true),
            Event::Result(text) => {
                v.listening = false;
                let text = text.trim().to_string();
                if text.is_empty() {
                    Next::Show("no-speech", String::new(), false)
                } else if v.released {
                    Next::Send(text)
                } else {
                    // Heard it; it goes out on release (hold) or the next tap.
                    v.text = Some(text.clone());
                    Next::Show("heard", text, true)
                }
            }
            Event::Error(code) => {
                v.listening = false;
                v.text = None;
                match code.as_str() {
                    // ERROR_SPEECH_TIMEOUT, ERROR_NO_MATCH.
                    "6" | "7" => Next::Show("no-speech", String::new(), false),
                    // ERROR_INSUFFICIENT_PERMISSIONS.
                    "9" => Next::Show("permission", String::new(), false),
                    _ => Next::Show("error", format!("speech recognizer error {code}"), false),
                }
            }
            Event::Unavailable => {
                v.listening = false;
                Next::Show("no-recognizer", String::new(), false)
            }
            Event::Partial(_) | Event::Ready | Event::Begin | Event::End => Next::Nothing,
        }
    });
    match next {
        Next::Show(code, detail, active) => status(ui, code, &detail, active),
        Next::Send(text) => send_text(ui, text),
        Next::Nothing => {}
    }
}
