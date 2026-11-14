//! LG webOS TVs: SSAP over WebSocket (`wss://:3001`, or `ws://:3000` on older firmware).
//!
//! The first time, the TV asks to accept OmaControl; it then gives us a client key that we keep.
//! Arrows, OK, Back... go through a second "pointer input" socket, as `type:button` frames.

use std::time::Duration;

use serde_json::{json, Value};

use super::ws::{self, Socket};
use super::Remote;
use crate::{App, Error, Key, Result};

/// The registration payload (manifest + permissions) every webOS remote app sends.
const REGISTER: &str = include_str!("lg_register.json");

pub(crate) struct Lg {
    socket: Socket,
    pointer: Option<Socket>,
    client_key: Option<String>,
    next_id: u32,
}

impl Lg {
    pub(crate) fn connect(ip: &str, client_key: Option<String>) -> Result<Lg> {
        let pairing = Duration::from_secs(60);
        let socket = ws::open(&format!("wss://{ip}:3001"), &format!("{ip}:3001"), pairing)
            .or_else(|_| ws::open(&format!("ws://{ip}:3000"), &format!("{ip}:3000"), pairing))?;
        let mut lg = Lg { socket, pointer: None, client_key, next_id: 1 };
        lg.register()?;
        Ok(lg)
    }

    fn register(&mut self) -> Result<()> {
        let mut payload: Value = serde_json::from_str(REGISTER)?;
        if let Some(key) = &self.client_key {
            payload["client-key"] = json!(key);
        }
        ws::send_json(&mut self.socket, &json!({ "type": "register", "id": "register_0", "payload": payload }))?;
        loop {
            let msg = ws::read_json(&mut self.socket)?;
            match msg["type"].as_str() {
                Some("registered") => {
                    self.client_key = msg["payload"]["client-key"].as_str().map(String::from);
                    return Ok(());
                }
                Some("error") => return Err(Error::PairingRefused),
                // "response" with pairingType PROMPT: the TV is asking, keep waiting.
                _ => continue,
            }
        }
    }

    /// Sends an SSAP request and waits for its answer.
    fn request(&mut self, uri: &str, payload: Value) -> Result<Value> {
        let id = format!("req_{}", self.next_id);
        self.next_id += 1;
        ws::send_json(&mut self.socket, &json!({ "type": "request", "id": id, "uri": uri, "payload": payload }))?;
        loop {
            let msg = ws::read_json(&mut self.socket)?;
            if msg["id"].as_str() == Some(id.as_str()) {
                if msg["type"].as_str() == Some("error") {
                    return Err(Error::Protocol(msg["error"].as_str().unwrap_or("webOS error").into()));
                }
                return Ok(msg["payload"].clone());
            }
        }
    }

    fn button(&mut self, name: &str) -> Result<()> {
        if self.pointer.is_none() {
            let path = self.request("ssap://com.webos.service.networkinput/getPointerInputSocket", json!({}))?;
            let url = path["socketPath"]
                .as_str()
                .ok_or_else(|| Error::Protocol("no pointer socket".into()))?
                .to_string();
            let host = url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or_default().to_string();
            self.pointer = Some(ws::open(&url, &host, Duration::from_secs(5))?);
        }
        let pointer = self.pointer.as_mut().expect("pointer socket is open");
        pointer.send(tungstenite::Message::Text(format!("type:button\nname:{name}\n\n")))?;
        Ok(())
    }

    fn open_app(&mut self, id: &str) -> Result<()> {
        self.request("ssap://system.launcher/launch", json!({ "id": id })).map(|_| ())
    }
}

fn button_name(key: Key) -> Option<String> {
    Some(
        match key {
            Key::Mute => "MUTE",
            Key::Up => "UP",
            Key::Down => "DOWN",
            Key::Left => "LEFT",
            Key::Right => "RIGHT",
            Key::Ok => "ENTER",
            Key::Back => "BACK",
            Key::Home => "HOME",
            Key::Menu => "MENU",
            Key::VolumeUp => "VOLUMEUP",
            Key::VolumeDown => "VOLUMEDOWN",
            Key::ChannelUp => "CHANNELUP",
            Key::ChannelDown => "CHANNELDOWN",
            Key::PlayPause => "PLAY",
            Key::Rewind => "REWIND",
            Key::FastForward => "FASTFORWARD",
            Key::Digit(d) => return Some(d.to_string()),
            Key::Dash => "DASH",
            Key::Power | Key::Input => return None,
        }
        .to_string(),
    )
}

/// webOS app ids of the shortcut apps.
fn app_id(app: App) -> Option<&'static str> {
    match app {
        App::Netflix => Some("netflix"),
        App::YouTube => Some("youtube.leanback.v4"),
        App::PrimeVideo => Some("amazon"),
        App::DisneyPlus => Some("com.disney.disneyplus-prod"),
        App::Spotify => Some("spotify-beehive"),
        App::CanalPlus => None,
    }
}

impl Remote for Lg {
    fn press(&mut self, key: Key) -> Result<()> {
        match key {
            Key::Power => self.request("ssap://system/turnOff", json!({})).map(|_| ()),
            Key::Input => self.open_app("com.webos.app.inputpicker"),
            other => {
                let name = button_name(other).ok_or_else(|| Error::Unsupported(format!("{other:?}")))?;
                self.button(&name)
            }
        }
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        self.request("ssap://com.webos.service.ime/insertText", json!({ "text": text, "replace": 0 }))
            .map(|_| ())
    }

    fn launch(&mut self, app: App) -> Result<()> {
        let id = app_id(app).ok_or_else(|| Error::Unsupported(app.label().into()))?;
        self.open_app(id)
    }

    fn token(&self) -> Option<String> {
        self.client_key.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_payload_is_valid_json() {
        let v: Value = serde_json::from_str(REGISTER).unwrap();
        assert_eq!(v["pairingType"], "PROMPT");
    }

    #[test]
    fn maps_buttons() {
        assert_eq!(button_name(Key::Ok).as_deref(), Some("ENTER"));
        assert_eq!(button_name(Key::Power), None);
    }
}
