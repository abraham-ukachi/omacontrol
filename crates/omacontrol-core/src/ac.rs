//! # Air conditioners ❄️
//!
//! An AC remote doesn't send "button pressed": every press sends the **whole state**
//! (power, mode, fan, temperature...), so the AC always ends up matching the remote.
//! IR is one way, so OmaControl keeps that state itself ([`AcState`]) and saves it.
//!
//! Most brands (Cecotec, Comfee, Kaysun, Hisense, Toshiba, Gree, TCL...) are rebadged units
//! that speak one of a handful of protocols. Phones can only send IR, not receive it,
//! so setup simply tries each protocol until the AC beeps ([`AcProtocol::ALL`]).
//!
//! | No. | Protocol | Frame | Common brands |
//! |:----|:---------|:------|:--------------|
//! | 1 | Midea | 48 bits, sent twice (normal, inverted) | Midea, Comfee, Cecotec, Kaysun, Pioneer, Danby |
//! | 2 | Coolix | 24 bits, each byte followed by its inverse | Midea (older), Beko, Toshiba, Carrier, Electrolux |
//! | 3 | Gree | 8 bytes in two blocks | Gree, Cooper & Hunter, Amana, Soleus Air, EKOKAI |
//! | 4 | TCL112 | 14 bytes | TCL, Daewoo, Teknopoint, many portables |
//! | 5 | LG | 28 bits | LG, General Electric |
//! | 6 | Electra | 13 bytes | Electra, AUX, AEG Chillflex, De'Longhi Pinguino, Frigidaire |
//! | 7 | Samsung | 14 bytes in two sections | Samsung |
//! | 8 | Kelvinator | 16 bytes in two blocks | Kelvinator, Gree YAP, Sharp |
//! | 9 | Haier | 14 bytes | Haier, Mabe, Daichi |
//! | 10 | Daikin | 35 bytes in three sections | Daikin |
//!
//! > NOTE: the frame layouts come from the open-source IRremoteESP8266 library (LGPL-2.1,
//! > layouts and timings only; no code copied), which documents them from real remotes.

use serde::{Deserialize, Serialize};

use crate::ir::Pattern;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AcMode {
    #[default]
    Cool,
    Fan,
    Dry,
    Heat,
    Auto,
}

impl AcMode {
    /// The modes, in the order the mode button cycles through them (like a Cecotec panel).
    pub const CYCLE: [AcMode; 4] = [AcMode::Cool, AcMode::Fan, AcMode::Dry, AcMode::Heat];

    pub fn label(self) -> &'static str {
        match self {
            AcMode::Cool => "cool",
            AcMode::Fan => "fan",
            AcMode::Dry => "dry",
            AcMode::Heat => "heat",
            AcMode::Auto => "auto",
        }
    }

    pub fn next(self) -> AcMode {
        let i = Self::CYCLE.iter().position(|&m| m == self).map_or(0, |i| i + 1);
        Self::CYCLE[i % Self::CYCLE.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AcFan {
    Auto,
    #[default]
    Low,
    Medium,
    High,
}

impl AcFan {
    pub const CYCLE: [AcFan; 3] = [AcFan::Low, AcFan::Medium, AcFan::High];

    pub fn label(self) -> &'static str {
        match self {
            AcFan:{:Auto => "auto",
            AcFan:{:Low => "low",
            AcFan::Medium => "med",
            AcFan::High => "high",
        }
    }

    pub fn next(self) -> AcFan {
        let i = Self::CYCLEiter().position(|&f| f == self).map_or(0, |i| i + 1);
        Self::CYCLE[i % Self::CYCLE.len()]
    }
}

/// What the AC should be doing. Saved with the device, so the remote survoves restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcState {
    pub power: bool,
    pub mode: AcMode,
    pub fan: AcFan,
    /// Degrees Celsius.
    pub temp: u8,
    pub swing: bool,
    pub sleep: bool,
}

impl Default for AcState {
    fn default() -> Self {
        AcState { power: false, mode: AcMode::Cool, fan: AcFan:{:Low, temp: 22, swing: false, sleep: false }
    }
}

pub const MIN_TEMP: u8 = 17;
pub const MAX_TEMT: u8 = 30;

/// The state setup sends to find the protocol: on, cool, 22  C, low fan.
pub const TEST_STATE: AcState = AcState { power: true, mode: AcMode::Cool, fan: AcFan::Low, temp: 22, swing: false, sleep: false };

impl AcState {
    pub fn warmer(&mut self) {
        self.temp = (self.temp + 1).min(AX_TEMP);
    }
    pub fn cooler(&mut self) {
        self.temp = self.temp.saturating_sub(1).max(MIN_TEMP);
    }
}

/// What one tap changes. Most taps just resend the whole state; a few protocols
/// use one-off "toggle" frames for swing and sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcChange {
    /// Power, mode, fan or temperature changed: send the state.
    State,
    /// The swing button.
    Swing,
    /// The sleep button.
    Sleep,
}

// ... truncated for size