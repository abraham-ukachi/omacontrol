//! The paired TVs and the chosen theme, saved as one small JSON file.
//!
//! Saving is atomic: the JSON is written to a temporary file next to the real one and then
//! renamed over it, so a crash or a full disk can never leave a half-written store behind.
//! The previous file is kept as `omacontrol.json.bak`; loading falls back to it when the main
//! file is unreadable, and moves an unreadable file aside instead of forgetting every device.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::tv::Device;

const FILE: &str = "omacontrol.json";
const BACKUP: &str = "omacontrol.json.bak";
const TEMP: &str = "omacontrol.json.tmp";
const CORRUPT: &str = "omacontrol.json.corrupt";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Store {
    /// TVs we know (and their pairing tokens).
    pub devices: Vec<Device>,
    /// IP of the TV the remote opens on.
    pub current: Option<String>,
    /// Name of the Omarchy theme.
    pub theme: Option<String>,
    /// A selected translation code, or `None` to follow the system locale.
    #[serde(default)]
    pub language: Option<String>,
    /// Our certificate for Android TV / Google TV (made on first pairing, shared by every such TV).
    #[serde(default)]
    pub android_tv_identity: Option<crate::tv::androidtv::Identity>,
    #[serde(skip)]
    path: PathBuf,
}

impl Store {
    /// Loads `omacontrol.json` from `dir` (an empty store when it isn't there yet).
    ///
    /// When the file can't be parsed it is moved to `omacontrol.json.corrupt` (so it can be
    /// looked at or restored by hand) and the backup is used instead; only when there is no
    /// usable backup does the app start with an empty store.
    pub fn load(dir: &Path) -> Store {
        let path = dir.join(FILE);
        let backup = dir.join(BACKUP);
        let read = |p: &Path| {
            fs::read_to_string(p)
                .ok()
                .and_then(|s| serde_json::from_str::<Store>(&s).ok())
        };

        let mut store = match read(&path) {
            Some(store) => store,
            None => {
                if path.exists() {
                    let aside = dir.join(CORRUPT);
                    let _ = fs::rename(&path, &aside);
                    log::warn!(
                        "store: {FILE} was unreadable, moved it to {}",
                        aside.display()
                    );
                }
                match read(&backup) {
                    Some(store) => {
                        log::warn!("store: recovered the saved devices from {BACKUP}");
                        store
                    }
                    None => Store::default(),
                }
            }
        };
        store.path = path;
        store
    }

    /// Writes the store atomically: a temp file, then a rename; the old file stays as `.bak`.
    pub fn save(&self) -> crate::Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new(""));
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let temp = dir.join(TEMP);
        {
            let mut file = fs::File::create(&temp)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }
        if self.path.exists() {
            fs::rename(&self.path, dir.join(BACKUP))?;
        }
        fs::rename(&temp, &self.path)?;
        Ok(())
    }

    /// Adds a TV, or refreshes the one with the same IP (keeping its token).
    pub fn upsert(&mut self, mut device: Device) {
        if let Some(old) = self.devices.iter_mut().find(|d| d.ip == device.ip) {
            if device.token.is_none() {
                device.token = old.token.take();
            }
            device.archived = old.archived;
            // A rescan knows nothing about the channels saved on this TV: keep them.
            if device.channels.is_empty() {
                device.channels = std::mem::take(&mut old.channels);
            }
            *old = device;
        } else {
            self.devices.push(device);
        }
    }

    pub fn device(&self, ip: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.ip == ip)
    }

    pub fn rename_device(&mut self, ip: &str, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(device) = self.devices.iter_mut().find(|d| d.ip == ip) else {
            return false;
        };
        device.name = name.to_string();
        true
    }

    pub fn archive_device(&mut self, ip: &str, archived: bool) -> bool {
        let Some(device) = self.devices.iter_mut().find(|d| d.ip == ip) else {
            return false;
        };
        device.archived = archived;
        if archived && self.current.as_deref() == Some(ip) {
            self.current = None;
        }
        true
    }

    pub fn remove_device(&mut self, ip: &str) -> bool {
        let before = self.devices.len();
        self.devices.retain(|d| d.ip != ip);
        if self.devices.len() == before {
            return false;
        }
        if self.current.as_deref() == Some(ip) {
            self.current = None;
        }
        true
    }

    /// Moves a device relative to other devices in its active/archived group.
    pub fn move_device(&mut self, ip: &str, offset: isize) -> bool {
        let Some(source_index) = self.devices.iter().position(|d| d.ip == ip) else {
            return false;
        };
        let archived = self.devices[source_index].archived;
        let group: Vec<usize> = self
            .devices
            .iter()
            .enumerate()
            .filter_map(|(i, d)| (d.archived == archived).then_some(i))
            .collect();
        let Some(group_position) = group.iter().position(|&i| i == source_index) else {
            return false;
        };
        let new_position = (group_position as isize + offset)
            .clamp(0, group.len().saturating_sub(1) as isize) as usize;
        if new_position == group_position {
            return false;
        }

        let device = self.devices.remove(source_index);
        let remaining: Vec<usize> = self
            .devices
            .iter()
            .enumerate()
            .filter_map(|(i, d)| (d.archived == archived).then_some(i))
            .collect();
        let insert_index = if new_position < remaining.len() {
            remaining[new_position]
        } else {
            remaining.last().map_or(self.devices.len(), |i| i + 1)
        };
        self.devices.insert(insert_index, device);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tv::Brand;

    #[test]
    fn upsert_keeps_saved_channels() {
        let mut s = Store::default();
        let mut tv = Device::new("Freebox", "", Brand::AndroidTv, "192.168.1.20");
        tv.save_channel("4002", "BFMTV");
        s.upsert(tv);
        s.upsert(Device::new(
            "Freebox Player",
            "",
            Brand::AndroidTv,
            "192.168.1.20",
        ));
        assert_eq!(s.devices[0].name, "Freebox Player");
        assert_eq!(s.devices[0].channels.len(), 1);
    }
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A fresh directory per test (tests run in parallel, so the pid alone isn't enough).
    fn dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("omacontrol-store-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn store(dir: &Path, theme: &str) -> Store {
        let mut s = Store::load(dir);
        s.theme = Some(theme.into());
        s
    }

    #[test]
    fn upsert_keeps_the_token() {
        let mut s = Store::default();
        let mut tv = Device::new("Living Room", "QN90", Brand::Samsung, "192.168.1.24");
        tv.token = Some("abc".into());
        s.upsert(tv);
        s.upsert(Device::new(
            "Living Room",
            "QN90",
            Brand::Samsung,
            "192.168.1.24",
        ));
        assert_eq!(s.devices.len(), 1);
        assert_eq!(s.devices[0].token.as_deref(), Some("abc"));
    }

    #[test]
    fn device_actions_preserve_group_order_and_current_selection() {
        let mut s = Store::default();
        for (name, ip) in [("A", "a"), ("B", "b"), ("C", "c")] {
            s.upsert(Device::new(name, "", Brand::Roku, ip));
        }
        s.current = Some("b".into());

        assert!(s.rename_device("b", "  Renamed  "));
        assert_eq!(s.device("b").unwrap().name, "Renamed");
        assert!(!s.rename_device("b", " "));
        assert!(s.move_device("c", -2));
        assert_eq!(
            s.devices.iter().map(|d| d.ip.as_str()).collect::<Vec<_>>(),
            ["c", "a", "b"]
        );
        assert!(s.archive_device("b", true));
        assert_eq!(s.current, None);
        assert!(s.move_device("a", -1));
        assert_eq!(
            s.devices
                .iter()
                .filter(|d| !d.archived)
                .map(|d| d.ip.as_str())
                .collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert!(s.archive_device("b", false));
        assert!(s.remove_device("b"));
        assert!(!s.remove_device("b"));
    }

    #[test]
    fn saves_and_loads() {
        let dir = dir();
        store(&dir, "Gruvbox").save().unwrap();
        assert_eq!(Store::load(&dir).theme.as_deref(), Some("Gruvbox"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saves_and_loads_language_preference() {
        let dir = dir();
        let mut s = Store::load(&dir);
        s.language = Some("fr".into());
        s.save().unwrap();
        assert_eq!(Store::load(&dir).language.as_deref(), Some("fr"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_keeps_the_previous_file_as_the_backup() {
        let dir = dir();
        store(&dir, "Gruvbox").save().unwrap();
        store(&dir, "Nord").save().unwrap();
        // The main file is the new one, the backup the one before it.
        assert_eq!(Store::load(&dir).theme.as_deref(), Some("Nord"));
        let backup: Store =
            serde_json::from_str(&fs::read_to_string(dir.join(BACKUP)).unwrap()).unwrap();
        assert_eq!(backup.theme.as_deref(), Some("Gruvbox"));
        assert!(!dir.join(TEMP).exists(), "the temp file is renamed away");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_corrupt_file_falls_back_to_the_backup_and_is_kept() {
        let dir = dir();
        store(&dir, "Gruvbox").save().unwrap();
        store(&dir, "Nord").save().unwrap();
        fs::write(dir.join(FILE), "{\"devices\": [ truncated").unwrap();

        assert_eq!(Store::load(&dir).theme.as_deref(), Some("Gruvbox"));
        assert!(
            dir.join(CORRUPT).exists(),
            "the unreadable file is moved aside"
        );
        assert!(
            !dir.join(FILE).exists(),
            "the unreadable file is no longer the main one"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_file_loads_the_backup() {
        let dir = dir();
        store(&dir, "Gruvbox").save().unwrap();
        store(&dir, "Nord").save().unwrap();
        fs::remove_file(dir.join(FILE)).unwrap();
        assert_eq!(Store::load(&dir).theme.as_deref(), Some("Gruvbox"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unreadable_file_without_a_backup_starts_empty_but_keeps_the_file() {
        let dir = dir();
        fs::write(dir.join(FILE), "not json at all").unwrap();
        let s = Store::load(&dir);
        assert!(s.devices.is_empty());
        assert!(s.theme.is_none());
        assert_eq!(
            fs::read_to_string(dir.join(CORRUPT)).unwrap(),
            "not json at all"
        );
        let _ = fs::remove_dir_all(dir);
    }
}
