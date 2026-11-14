//! The paired TVs and the chosen theme, saved as one small JSON file.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::tv::Device;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Store {
    /// TVs we know (and their pairing tokens).
    pub devices: Vec<Device>,
    /// IP of the TV the remote opens on.
    pub current: Option<String>,
    /// Name of the Omarchy theme.
    pub theme: Option<String>,
    #[serde(skip)]
    path: PathBuf,
}

impl Store {
    /// Loads `omacontrol.json` from `dir` (an empty store when it isn't there yet).
    pub fn load(dir: &Path) -> Store {
        let path = dir.join("omacontrol.json");
        let mut store: Store = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        store.path = path;
        store
    }

    pub fn save(&self) -> crate::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Adds a TV, or refreshes the one with the same IP (keeping its token).
    pub fn upsert(&mut self, mut device: Device) {
        if let Some(old) = self.devices.iter_mut().find(|d| d.ip == device.ip) {
            if device.token.is_none() {
                device.token = old.token.take();
            }
            *old = device;
        } else {
            self.devices.push(device);
        }
    }

    pub fn device(&self, ip: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.ip == ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tv::Brand;

    #[test]
    fn upsert_keeps_the_token() {
        let mut s = Store::default();
        let mut tv = Device::new("Living Room", "QN90", Brand::Samsung, "192.168.1.24");
        tv.token = Some("abc".into());
        s.upsert(tv);
        s.upsert(Device::new("Living Room", "QN90", Brand::Samsung, "192.168.1.24"));
        assert_eq!(s.devices.len(), 1);
        assert_eq!(s.devices[0].token.as_deref(), Some("abc"));
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("omacontrol-test-{}", std::process::id()));
        let mut s = Store::load(&dir);
        s.theme = Some("Gruvbox".into());
        s.save().unwrap();
        assert_eq!(Store::load(&dir).theme.as_deref(), Some("Gruvbox"));
        let _ = fs::remove_dir_all(dir);
    }
}
