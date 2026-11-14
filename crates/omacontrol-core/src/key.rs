//! The buttons of the remote, and the streaming apps of the shortcuts row.

use serde::{Deserialize, Serialize};

/// One button of the remote. Every protocol maps these to its own key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    Power,
    Input,
    Mute,
    Up,
    Down,
    Left,
    Right,
    Ok,
    Back,
    Home,
    Menu,
    VolumeUp,
    VolumeDown,
    ChannelUp,
    ChannelDown,
    PlayPause,
    Rewind,
    FastForward,
    /// `0` to `9`.
    Digit(u8),
    /// The `-` of sub-channels (`7-1`).
    Dash,
}

impl Key {
    /// Parses the ids the UI sends (`"up"`, `"vol+"`, `"7"`...).
    pub fn from_id(id: &str) -> Option<Key> {
        Some(match id {
            "power" => Key::Power,
            "input" => Key::Input,
            "mute" => Key::Mute,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "ok" => Key::Ok,
            "back" => Key::Back,
            "home" => Key::Home,
            "menu" => Key::Menu,
            "vol+" => Key::VolumeUp,
            "vol-" => Key::VolumeDown,
            "ch+" => Key::ChannelUp,
            "ch-" => Key::ChannelDown,
            "play" => Key::PlayPause,
            "rew" => Key::Rewind,
            "ff" => Key::FastForward,
            "dash" => Key::Dash,
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => Key::Digit(d.as_bytes()[0] - b'0'),
            _ => return None,
        })
    }
}

/// The streaming apps of the shortcuts grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum App {
    Netflix,
    YouTube,
    PrimeVideo,
    DisneyPlus,
    CanalPlus,
    Spotify,
}

impl App {
    pub const ALL: [App; 6] = [App::Netflix, App::YouTube, App::PrimeVideo, App::DisneyPlus, App::CanalPlus, App::Spotify];

    pub fn label(self) -> &'static str {
        match self {
            App::Netflix => "Netflix",
            App::YouTube => "YouTube",
            App::PrimeVideo => "Prime",
            App::DisneyPlus => "Disney+",
            App::CanalPlus => "Canal+",
            App::Spotify => "Spotify",
        }
    }

    pub fn from_label(label: &str) -> Option<App> {
        App::ALL.into_iter().find(|a| a.label() == label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ui_ids() {
        assert_eq!(Key::from_id("vol+"), Some(Key::VolumeUp));
        assert_eq!(Key::from_id("7"), Some(Key::Digit(7)));
        assert_eq!(Key::from_id("nope"), None);
    }

    #[test]
    fn app_labels_round_trip() {
        for app in App::ALL {
            assert_eq!(App::from_label(app.label()), Some(app));
        }
    }
}
