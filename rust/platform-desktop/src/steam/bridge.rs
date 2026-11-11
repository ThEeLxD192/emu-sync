//! Puentes de compatibilidad con nombres de launcher anteriores.

use crate::steam::{
    manager::{NAME_PREFIX, SteamShortcutManager},
    paths::legacy_launcher_name,
    scripts::{linux_bridge, windows_batch, write_launcher_file},
    shortcut::SteamShortcut,
};
use emusync_domain::library::Emulator;
use std::{fs, io, path::Path};

impl SteamShortcutManager {
    fn bridge_contents(&self, entry: &Emulator, launcher_dir: &Path) -> io::Result<String> {
        if self.is_windows() {
            let key = self.launcher_key(&entry.common.name);
            Ok(windows_batch(&self.encoded_command(&key)))
        } else {
            let target_sh = launcher_dir.join(format!(
                "launch_{}.sh",
                self.launcher_key(&entry.common.name)
            ));
            linux_bridge(&target_sh)
        }
    }

    pub(crate) fn bridge_previous_name(
        &self,
        entry: &Emulator,
        previous: &SteamShortcut,
        shortcuts: &[SteamShortcut],
    ) -> io::Result<()> {
        let previous_name = previous
            .app_name
            .strip_prefix(NAME_PREFIX)
            .unwrap_or(&previous.app_name);
        if previous_name == entry.common.name {
            return Ok(());
        }
        let mut old_entry = entry.clone();
        old_entry.common.name = previous_name.to_string();
        self.install_legacy_bridge(&old_entry, shortcuts)?;

        let old_signal = self.get_signal_file(&old_entry);
        let new_signal = self.get_signal_file(entry);
        if old_signal.is_file() && !new_signal.exists() {
            let _ = fs::copy(&old_signal, &new_signal);
        }

        let extension = if self.is_windows() { "bat" } else { "sh" };
        let launcher_dir = self.get_launcher_dir();
        let old_script = launcher_dir.join(format!(
            "launch_{}.{extension}",
            self.launcher_key(previous_name)
        ));
        let legacy_script = launcher_dir.join(format!(
            "launch_{}.{extension}",
            legacy_launcher_name(previous_name)
        ));

        if !old_script.exists() && !legacy_script.exists() {
            return Ok(());
        }

        let contents = self.bridge_contents(entry, &launcher_dir)?;
        write_launcher_file(&old_script, &contents, !self.is_windows())
    }

    pub(crate) fn install_legacy_bridge(
        &self,
        entry: &Emulator,
        shortcuts: &[SteamShortcut],
    ) -> io::Result<()> {
        let old_name = legacy_launcher_name(&entry.common.name);

        let conflicting_names = shortcuts
            .iter()
            .filter_map(|s| s.app_name.strip_prefix(NAME_PREFIX))
            .filter(|name| legacy_launcher_name(name) == old_name)
            .count();

        let launcher_dir = self.get_launcher_dir();
        let old_signal = launcher_dir.join(format!(".current_rom_{old_name}"));
        let new_signal = self.get_signal_file(entry);

        if conflicting_names <= 1 && old_signal.is_file() && !new_signal.exists() {
            let _ = fs::copy(&old_signal, &new_signal);
        }

        let extension = if self.is_windows() { "bat" } else { "sh" };
        let previous = launcher_dir.join(format!("launch_{old_name}.{extension}"));
        if !previous.exists() {
            return Ok(());
        }

        let backup = launcher_dir.join(format!("launch_{old_name}.{extension}.legacy.bak"));
        if !backup.exists() {
            fs::copy(&previous, &backup)?;
        }

        let contents = if conflicting_names > 1 {
            if self.is_windows() {
                "@echo off\r\necho EmuSync: Restart Steam to load the separate emulator launchers.\r\nexit /b 1\r\n".to_string()
            } else {
                "#!/bin/bash\necho 'EmuSync: Restart Steam to load the separate emulator launchers.' >&2\nexit 1\n".to_string()
            }
        } else {
            self.bridge_contents(entry, &launcher_dir)?
        };

        write_launcher_file(&previous, &contents, !self.is_windows())
    }
}
