//! # omacontrol-core
//!
//! Everything OmaControl knows about TVs, with no UI attached:
//!
//! - [`discovery`]: finds TVs on the local network (SSDP), and tells their brand apart.
//! - [`tv`]: one module per protocol (Samsung Tizen, LG webOS, Roku ECP, Android TV),
//!   all behind the same [`tv::Remote`] trait.
//! - [`ac`]: air conditioners over IR (Midea, Coolix, Gree, TCL) and their state.
//! - [`ir`]: infrared patterns (NEC, Samsung32, Sony SIRC, Pronto) for the phone's IR blaster.
//! - [`theme`]: the Omarchy themes and the Omacalc color mixing rules.
//! - [`store`]: the paired TVs (and their pairing tokens), saved as JSON.

pub mod ac;
pub mod discovery;
pub mod ir;
pub mod key;
pub mod store;
pub mod theme;
mod themes_gen;
pub mod tv;

pub use discovery::discover;
pub use key::{App, Key};
pub use store::Store;
pub use theme::Theme;
pub use ir::IrTransmitter;
pub use ac::{AcChange, AcFan, AcMode, AcProtocol, AcState};
pub use tv::{connect, Brand, Device, DeviceKind, Remote, Transport};

/// Everything that can go wrong while talking to a TV.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("websocket: {0}")]
    WebSocket(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the TV refused the pairing (or nobody said yes on the TV)")]
    PairingRefused,
    #[error("{0} is not supported by this TV yet")]
    Unsupported(String),
    #[error("{0}")]
    Protocol(String),
}

impl From<tungstenite::Error> for Error {
    fn from(e: tungstenite::Error) -> Self {
        Error::WebSocket(e.to_string())
    }
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Http(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
