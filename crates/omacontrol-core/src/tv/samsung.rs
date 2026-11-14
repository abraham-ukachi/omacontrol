//! Samsung Tizen TVs (2016 and later): the remote channel over secure WebSocket on port `8002`.
//!
//! The first time, the TV asks "Allow OmaControl?"; once allowed, it hands us a token
//! that we keep, so it never asks again.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::json;

use super::ws::{self, Socket};
use super::Remote;
use crate::{App, Error, Key, Result};

const NAME: &str = "OmaControl";

pub(crate) struct Samsung {
    socket: Socket,
    token: Option<String>,
}

impl Samsung {
    pub(crate) fn connect(ip: &str, token: Option<String>) -> Result<Samsung> {
        let mut url = format!(
            "wss://{ip}:8002/api/v2/channels/samsung.remote.control?name={}",
            B64.encode(NAME)
        );
        if let Some(t) = &token {
            url.push_str(&format!("&token={t}"));
        }
        // 30 seconds: long enough for someone to press "Allow" on the TV.
        let mut socket = ws::open(&url, &format!("{ip}:8002"), Duration::from_secs(30))?;
        let hello = ws::read_json(&mut socket)?;
        match hello["event"].as_str() {
            Some("ms.channel.connect") => {
                let token = hello["data"]["token"].as_str().map(String::from).or(token);
                Ok(Samsung { socket, token })
            }
            Some("ms.channel.unauthorized") | Some("ms.channel.timeOut") => Err(Error::PairingRefused),
            other => Err(Error::Protocol(format!("unexpected Samsung event {other:?}"))),
        }
    }

    fn remote(&mut self, params: serde_json::Value) -> Result<()> {
        ws::send_json(&mut self.socket, &json!({ "method": "ms.remote.control", "params": params }))
    }
}

fn key_name(key: Key) -> Option<String> {
    Some(
        match key {
            Key::Power => "KEY_POWER",
            Key::Input => "KEY_SOURCE",
            Key::Mute => "KEY_MUTE",
            Key::Up => "KEY_UP",
            Key::Down => "KEY_DOWN",
            Key::Left => "KEY_LEFT",
            Key::Right => "KEY_RIGHT",
            Key::Ok => "KEY_ENTER",
            Key::Back => "KEY_RETURN",
            Key::Home => "KEY_HOME",
            Key::Menu => "KEY_MENU",
            Key::VolumeUp => "KEY_VOLUP",
            Key::VolumeDown => "KEY_VOLDOWN",
            Key::ChannelUp => "KEY_CHUP",
            Key::ChannelDown => "KEY_CHDOWN",
            Key::PlayPause => "KEY_PLAY",
            Key::Rewind => "KEY_REWIND",
            Key::FastForward => "KEY_FF",
            Key::Digit(d) => return Some(format!("KEY_{d}")),
            Key::Dash => "KEY_PLUS100",
        }
        .to_string(),
    )
}

/// Tizen app ids of the shortcut apps.
fn app_id(app: App) -> Option<&'static str> {
    match app {
        App::Netflix => Some("3201907018807"),
        App::YouTube => Some("111299001912"),
        App::PrimeVideo => Some("3201910019365"),
        App::DisneyPlus => Some("3201901017640"),
        App::Spotify => Some("3201606009684"),
        App::CanalPlus => None,
    }
}

impl Remote for Samsung {
    fn press(&mut self, key: Key) -> Result<()> {
        let name = key_name(key).ok_or_else(|| Error::Unsupported(format!("{key:?}")))?;
        self.remote(json!({
            "Cmd": "Click",
            "DataOfCmd": name,
            "Option": "false",
            "TypeOfRemote": "SendRemoteKey"
        }))
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        self.remote(json!({
            "Cmd": B64.encode(text),
            "DataOfCmd": "base64",
            "TypeOfRemote": "SendInputString"
        }))?;
        self.remote(json!({ "TypeOfRemote": "SendInputEnd" }))
    }

    fn launch(&mut self, app: App) -> Result<()> {
        let id = app_id(app).ok_or_else(|| Error::Unsupported(app.label().into()))?;
        ws::send_json(
            &mut self.socket,
            &json!({
                "method": "ms.channel.emit",
                "params": {
                    "event": "ed.apps.launch",
                    "to": "host",
                    "data": { "appId": id, "action_type": "DEEP_LINK" }
                }
            }),
        )
    }

    fn token(&self) -> Option<String> {
        self.token.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys() {
        assert_eq!(key_name(Key::Ok).as_deref(), Some("KEY_ENTER"));
        assert_eq!(key_name(Key::Digit(0)).as_deref(), Some("KEY_0"));
    }
}
