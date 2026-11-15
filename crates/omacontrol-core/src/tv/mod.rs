//! One module per TV protocol, all behind the same [`Remote`] trait.
//!
//! | No. | Brand | Protocol | Port | Pairing |
//! |:----|:------|:---------|:-----|:--------|
//! | 1 | Samsung (2016+) | Tizen remote over secure WebSocket | `8002` | "Allow" on the TV, then a token |
//! | 2 | LG (webOS) | SSAP over WebSocket + pointer socket | `3000` | "Yes" on the TV, then a client key |
//! | 3 | Roku / Roku TV | ECP over HTTP | `8060` | none |
//! | 4 | Android TV / Google TV | Android TV Remote v2 | `6466` / `6467` | 6-character code shown on the TV |
//! | 5 | Any TV with an IR receiver | infrared (phone's IR blaster) | - | none |

pub mod androidtv;
mod infrared;
mod proto;
mod lg;
mod roku;
mod samsung;
mod tls;
mod ws;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ir::IrTransmitter;
use crate::{App, Key, Result};

/// Who made the TV, which decides the protocol we speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Brand {
    Samsung,
    Lg,
    Roku,
    AndroidTv,
    Sony,
    Unknown,
}

impl Brand {
    /// What the TVs list shows under the name (`Samsung · Tizen`).
    pub fn label(self) -> &'static str {
        match self {
            Brand::Samsung => "Samsung · Tizen",
            Brand::Lg => "LG · webOS",
            Brand::Roku => "Roku",
            Brand::AndroidTv => "Android TV",
            Brand::Sony => "Sony",
            Brand::Unknown => "Unknown",
        }
    }
}

/// What kind of device it is: a TV, or an air conditioner driven over IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeviceKind {
    #[default]
    Tv,
    AirConditioner,
}

/// How we reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Transport {
    /// Wi-Fi / Ethernet, with the brand's own protocol.
    #[default]
    Network,
    /// The phone's IR blaster.
    Infrared,
}

/// A device OmaControl can drive (a TV on the network, or anything in front of the IR blaster).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    /// The name the TV gives itself (`Living Room`).
    pub name: String,
    /// The model, when the TV tells us (`QN90`).
    pub model: String,
    pub brand: Brand,
    pub ip: String,
    /// The pairing token (Samsung) or client key (LG) the TV gave us.
    pub token: Option<String>,
    #[serde(default)]
    pub kind: DeviceKind,
    #[serde(default)]
    pub transport: Transport,
    /// Air conditioners: the IR protocol that made it beep (None until setup finds it).
    #[serde(default)]
    pub ac_protocol: Option<crate::ac::AcProtocol>,
    /// Air conditioners: a pasted Pronto code, when no protocol above reaches the unit.
    /// The power key sends it; the other keys do nothing.
    #[serde(default)]
    pub ac_code: Option<String>,
    /// Air conditioners: the last state we sent (IR is one way, so we remember it).
    #[serde(default)]
    pub ac_state: crate::ac::AcState,
    /// TVs: channel numbers saved by name (`4002` → `BFMTV`), in the order they were saved.
    #[serde(default)]
    pub channels: Vec<Channel>,
}

/// A channel number the user saved under a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub number: String,
    pub name: String,
}

impl Device {
    pub fn new(name: &str, model: &str, brand: Brand, ip: &str) -> Device {
        Device { name: name.into(), model: model.into(), brand, ip: ip.into(), token: None, kind: DeviceKind::Tv, transport: Transport::Network, ac_protocol: None, ac_code: None, ac_state: Default::default(), channels: Vec::new() }
    }

    /// A TV driven by the IR blaster. Its `ip` is a stable id (`ir:samsung`), not an address.
    pub fn infrared_tv(brand: Brand) -> Device {
        let id = format!("ir:{}", format!("{brand:?}").to_lowercase());
        let name = format!("{} TV", brand.label().split(' ').next().unwrap_or("IR"));
        Device { name, model: "infrared".into(), brand, ip: id, token: None, kind: DeviceKind::Tv, transport: Transport::Infrared, ac_protocol: None, ac_code: None, ac_state: Default::default(), channels: Vec::new() }
    }

    /// An air conditioner driven by the IR blaster. `n` keeps several ACs apart (`ir:ac:2`).
    pub fn infrared_ac(n: usize) -> Device {
        let name = if n <= 1 { "Air conditioner".to_string() } else { format!("Air conditioner {n}") };
        Device {
            name,
            model: "air conditioner".into(),
            brand: Brand::Unknown,
            ip: format!("ir:ac:{n}"),
            token: None,
            kind: DeviceKind::AirConditioner,
            transport: Transport::Infrared,
            ac_protocol: None,
            ac_code: None,
            ac_state: Default::default(),
            channels: Vec::new(),
        }
    }

    pub fn is_ac(&self) -> bool {
        self.kind == DeviceKind::AirConditioner
    }

    /// Saves a channel by name. The same number again renames it; an empty name or number does nothing.
    /// Returns whether anything changed.
    pub fn save_channel(&mut self, number: &str, name: &str) -> bool {
        let number: String = number.chars().filter(|c| c.is_ascii_digit() || *c == '-').collect();
        let name = name.trim();
        if number.is_empty() || name.is_empty() {
            return false;
        }
        match self.channels.iter_mut().find(|c| c.number == number) {
            Some(c) => c.name = name.to_string(),
            None => self.channels.push(Channel { number, name: name.to_string() }),
        }
        true
    }

    /// Forgets a saved channel by its number.
    pub fn remove_channel(&mut self, number: &str) -> bool {
        let before = self.channels.len();
        self.channels.retain(|c| c.number != number);
        self.channels.len() != before
    }
}

/// What every protocol can do.
pub trait Remote: Send {
    /// Presses (and releases) one button.
    fn press(&mut self, key: Key) -> Result<()>;
    /// Types text into the field the TV has focused.
    fn type_text(&mut self, text: &str) -> Result<()>;
    /// Opens a streaming app.
    fn launch(&mut self, app: App) -> Result<()>;
    /// The pairing token, once the TV gave us one (to save it).
    fn token(&self) -> Option<String> {
        None
    }
}

/// Connects to a TV (and pairs with it when it asks). This can block while the TV shows its "Allow?" prompt.
/// `ir` is the phone's IR blaster, when it has one.
pub fn connect(device: &Device, ir: Option<Arc<dyn IrTransmitter>>) -> Result<Box<dyn Remote>> {
    if device.transport == Transport::Infrared {
        let ir = ir.filter(|t| t.has_emitter()).ok_or_else(|| crate::Error::Unsupported("IR (this phone has no IR blaster)".into()))?;
        return Ok(Box::new(infrared::Infrared::new(device.brand, ir)));
    }
    Ok(match device.brand {
        Brand::Samsung => Box::new(samsung::Samsung::connect(&device.ip, device.token.clone())?),
        Brand::Lg => Box::new(lg::Lg::connect(&device.ip, device.token.clone())?),
        Brand::Roku => Box::new(roku::Roku::new(&device.ip)),
        // Android TV pairs our certificate: see [`connect_android_tv`] and [`androidtv::Pairing`].
        Brand::AndroidTv => return Err(crate::Error::NotPaired),
        Brand::Sony | Brand::Unknown => return Err(crate::Error::Unsupported("this TV over Wi-Fi".into())),
    })
}

/// Connects to an Android TV / Google TV with our certificate. [`crate::Error::NotPaired`]
/// means the TV doesn't know it yet: run an [`androidtv::Pairing`] first.
pub fn connect_android_tv(ip: &str, identity: &androidtv::Identity) -> Result<Box<dyn Remote>> {
    Ok(Box::new(androidtv::AndroidTv::connect(ip, identity)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_renames_and_removes_channels() {
        let mut tv = Device::new("Freebox", "", Brand::AndroidTv, "192.168.1.20");
        assert!(tv.save_channel("4002", "BFMTV"));
        assert!(tv.save_channel(" 4 0 0 2 ", "BFM TV"));
        assert!(tv.save_channel("2", "France 2"));
        assert!(!tv.save_channel("", "Nothing"));
        assert!(!tv.save_channel("7", "  "));
        assert_eq!(tv.channels, vec![Channel { number: "4002".into(), name: "BFM TV".into() }, Channel { number: "2".into(), name: "France 2".into() }]);
        assert!(tv.remove_channel("4002"));
        assert!(!tv.remove_channel("4002"));
        assert_eq!(tv.channels.len(), 1);
    }

    #[test]
    fn old_stores_load_without_channels() {
        let json = r#"{"name":"TV","model":"","brand":"Roku","ip":"1.2.3.4","token":null}"#;
        let d: Device = serde_json::from_str(json).unwrap();
        assert!(d.channels.is_empty());
    }
}
