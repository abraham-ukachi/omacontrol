//! One module per TV protocol, all behind the same [`Remote`] trait.
//!
//! | No. | Brand | Protocol | Port | Pairing |
//! |:----|:------|:---------|:-----|:--------|
//! | 1 | Samsung (2016+) | Tizen remote over secure WebSocket | `8002` | "Allow" on the TV, then a token |
//! | 2 | LG (webOS) | SSAP over WebSocket + pointer socket | `3000` | "Yes" on the TV, then a client key |
//! | 3 | Roku / Roku TV | ECP over HTTP | `8060` | none |
//! | 4 | Android TV / Google TV | Android TV Remote v2 | `6466` / `6467` | PIN shown on the TV (planned) |
//! | 5 | Any TV with an IR receiver | infrared (phone's IR blaster) | - | none |

mod androidtv;
mod infrared;
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
    /// Air conditioners: the last state we sent (IR is one way, so we remember it).
    #[serde(default)]
    pub ac_state: crate::ac::AcState,
}

impl Device {
    pub fn new(name: &str, model: &str, brand: Brand, ip: &str) -> Device {
        Device { name: name.into(), model: model.into(), brand, ip: ip.into(), token: None, kind: DeviceKind::Tv, transport: Transport::Network, ac_protocol: None, ac_state: Default::default() }
    }

    /// A TV driven by the IR blaster. Its `ip` is a stable id (`ir:samsung`), not an address.
    pub fn infrared_tv(brand: Brand) -> Device {
        let id = format!("ir:{}", format!("{brand:?}").to_lowercase());
        let name = format!("{} TV", brand.label().split(' ').next().unwrap_or("IR"));
        Device { name, model: "infrared".into(), brand, ip: id, token: None, kind: DeviceKind::Tv, transport: Transport::Infrared, ac_protocol: None, ac_state: Default::default() }
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
            ac_state: Default::default(),
        }
    }

    pub fn is_ac(&self) -> bool {
        self.kind == DeviceKind::AirConditioner
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
        Brand::AndroidTv => Box::new(androidtv::AndroidTv::connect(&device.ip)?),
        Brand::Sony | Brand::Unknown => return Err(crate::Error::Unsupported("this TV over Wi-Fi".into())),
    })
}
