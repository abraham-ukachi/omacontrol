//! Roku External Control Protocol: plain HTTP `POST`s on port `8060`.
//! Docs: <https://developer.roku.com/docs/developer-program/dev-tools/external-control-api.md>

use std::time::Duration;

use super::Remote;
use crate::{App, Error, Key, Result};

pub(crate) struct Roku {
    base: String,
    agent: ureq::Agent,
}

impl Roku {
    pub(crate) fn new(ip: &str) -> Roku {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).build();
        Roku { base: format!("http://{ip}:8060"), agent }
    }

    fn post(&self, path: &str) -> Result<()> {
        self.agent.post(&format!("{}{path}", self.base)).call()?;
        Ok(())
    }
}

fn key_name(key: Key) -> Option<String> {
    Some(
        match key {
            Key::Power => "Power",
            Key::Input => "InputHDMI1",
            Key::Mute => "VolumeMute",
            Key::Up => "Up",
            Key::Down => "Down",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::Ok => "Select",
            Key::Back => "Back",
            Key::Home => "Home",
            Key::Menu => "Info",
            Key::VolumeUp => "VolumeUp",
            Key::VolumeDown => "VolumeDown",
            Key::ChannelUp => "ChannelUp",
            Key::ChannelDown => "ChannelDown",
            Key::PlayPause => "Play",
            Key::Rewind => "Rev",
            Key::FastForward => "Fwd",
            Key::Digit(d) => return Some(format!("Lit_{d}")),
            Key::Dash => "Lit_-",
        }
        .to_string(),
    )
}

/// Roku channel ids of the shortcut apps.
fn app_id(app: App) -> Option<&'static str> {
    match app {
        App::Netflix => Some("12"),
        App::YouTube => Some("837"),
        App::PrimeVideo => Some("13"),
        App::DisneyPlus => Some("291097"),
        App::Spotify => Some("22297"),
        App::CanalPlus => None,
    }
}

/// Percent-encodes one character for a `Lit_` key.
fn encode(c: char) -> String {
    let mut buf = [0u8; 4];
    c.encode_utf8(&mut buf).bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

impl Remote for Roku {
    fn press(&mut self, key: Key) -> Result<()> {
        let name = key_name(key).ok_or_else(|| Error::Unsupported(format!("{key:?}")))?;
        self.post(&format!("/keypress/{name}"))
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        for c in text.chars() {
            self.post(&format!("/keypress/Lit_{}", encode(c)))?;
        }
        Ok(())
    }

    fn launch(&mut self, app: App) -> Result<()> {
        let id = app_id(app).ok_or_else(|| Error::Unsupported(app.label().into()))?;
        self.post(&format!("/launch/{id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys() {
        assert_eq!(key_name(Key::Ok).as_deref(), Some("Select"));
        assert_eq!(key_name(Key::Digit(4)).as_deref(), Some("Lit_4"));
    }

    #[test]
    fn encodes_text() {
        assert_eq!(encode('a'), "a");
        assert_eq!(encode(' '), "%20");
        assert_eq!(encode('é'), "%C3%A9");
    }
}
