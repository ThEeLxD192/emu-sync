//! Rutas de launchers, señal de ROM y userdata de Steam.

use crate::steam::manager::SteamShortcutManager;
use emusync_domain::library::Emulator;
use sha2::{Digest, Sha256};
use std::{
    cmp::Reverse,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Normalización histórica de nombres de launcher (minúsculas, no alfanumérico → `_`).
pub(crate) fn legacy_launcher_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

impl SteamShortcutManager {
    pub fn launcher_key(&self, name: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let readable = legacy_launcher_name(name);
        let readable = readable.trim_matches('_');
        let readable = if readable.len() > 40 {
            &readable[..40]
        } else if readable.is_empty() {
            "emulator"
        } else {
            readable
        };

        let mut hasher = Sha256::new();
        hasher.update(name.as_bytes());
        let mut key = String::with_capacity(readable.len() + 1 + 64);
        key.push_str(readable);
        key.push('-');
        for byte in hasher.finalize() {
            key.push(char::from(HEX[usize::from(byte >> 4)]));
            key.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        key
    }

    pub fn get_launcher_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.custom_launcher_dir {
            return dir.clone();
        }
        if self.is_windows() {
            if let Ok(app_data) = std::env::var("APPDATA") {
                PathBuf::from(app_data).join("emusync/launchers")
            } else {
                let home = std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join("AppData/Roaming/emusync/launchers")
            }
        } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg).join("emusync/launchers")
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config/emusync/launchers")
        }
    }

    pub fn get_signal_file(&self, entry: &Emulator) -> PathBuf {
        self.get_launcher_dir().join(format!(
            ".current_rom_{}",
            self.launcher_key(&entry.common.name)
        ))
    }

    pub fn get_steam_userdata_dir(&self) -> Option<PathBuf> {
        if let Some(ref dir) = self.custom_userdata_dir {
            return Some(dir.clone());
        }
        if self.is_windows() {
            self.find_windows_steam_userdata_dir()
        } else {
            self.find_linux_steam_userdata_dir()
        }
    }

    fn find_linux_steam_userdata_dir(&self) -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let home_path = Path::new(&home);
        let candidates = [
            home_path.join(".local/share/Steam/userdata"),
            home_path.join(".steam/steam/userdata"),
            home_path.join(".steam/root/userdata"),
            home_path.join(".var/app/com.valvesoftware.Steam/.local/share/Steam/userdata"),
            home_path.join(".var/app/com.valvesoftware.Steam/.steam/steam/userdata"),
        ];
        candidates
            .iter()
            .find(|p| p.is_dir())
            .cloned()
            .or_else(|| candidates.into_iter().next())
    }

    fn find_windows_steam_userdata_dir(&self) -> Option<PathBuf> {
        let candidates = [
            std::env::var("ProgramFiles(x86)")
                .ok()
                .map(|p| PathBuf::from(p).join("Steam/userdata")),
            std::env::var("ProgramFiles")
                .ok()
                .map(|p| PathBuf::from(p).join("Steam/userdata")),
            Some(PathBuf::from("C:\\Program Files (x86)\\Steam\\userdata")),
            Some(PathBuf::from("C:\\Program Files\\Steam\\userdata")),
            Some(PathBuf::from("C:\\Steam\\userdata")),
            Some(PathBuf::from("D:\\Steam\\userdata")),
        ];
        candidates.into_iter().flatten().find(|c| c.is_dir())
    }

    pub fn find_shortcuts_file(&self) -> Option<PathBuf> {
        if let Some(ref file) = self.custom_shortcuts_file {
            return Some(file.clone());
        }
        let base = self.get_steam_userdata_dir()?;
        if !base.is_dir() {
            return None;
        }

        let mut accounts: Vec<PathBuf> = fs::read_dir(&base)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("0"))
            .collect();

        // Cuenta modificada más recientemente primero.
        accounts.sort_by_cached_key(|p| {
            Reverse(
                fs::metadata(p)
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH),
            )
        });

        for account in &accounts {
            let vdf = account.join("config/shortcuts.vdf");
            if vdf.exists() || vdf.parent().is_some_and(|p| p.exists()) {
                return Some(vdf);
            }
        }

        accounts.first().map(|a| a.join("config/shortcuts.vdf"))
    }
}
