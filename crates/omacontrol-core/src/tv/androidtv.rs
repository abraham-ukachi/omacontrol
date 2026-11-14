//! Android TV / Google TV: the Android TV Remote protocol v2.
//!
//! Pairing happens on port `6467` (TLS with a client certificate, protobuf messages,
//! and a 6-character PIN shown on the TV), then remote control on port `6466`.
//! It lands in 0.2.0; until then the TV is found and listed, but not controlled.

use super::Remote;
use crate::{App, Error, Key, Result};

pub(crate) struct AndroidTv;

impl AndroidTv {
    pub(crate) fn connect(_ip: &str) -> Result<AndroidTv> {
        Err(Error::Unsupported("Android TV (coming in 0.2.0)".into()))
    }
}

impl Remote for AndroidTv {
    fn press(&mut self, _key: Key) -> Result<()> {
        Err(Error::Unsupported("Android TV".into()))
    }
    fn type_text(&mut self, _text: &str) -> Result<()> {
        Err(Error::Unsupported("Android TV".into()))
    }
    fn launch(&mut self, _app: App) -> Result<()> {
        Err(Error::Unsupported("Android TV".into()))
    }
}
