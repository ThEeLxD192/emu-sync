use crate::{
    steam::{shortcut::SteamShortcut, vdf::BinaryVdf},
    util::shell_argument,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use emusync_domain::{
    execution::parse_arguments,
    library::{Emulator, GameEntry, NativeGame},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

const FULLSCREEN_FLAGS: &[(&str, &str)] = &[
    ("duckstation", "-bigpicture -fullscreen"),
    ("pcsx2", "-bigpicture -fullscreen"),
    ("dolphin", "-b -e"),
    ("ppsspp", "--fullscreen"),
    ("retroarch", "-f"),
    ("eden", "-f"),
    ("yuzu", "-f"),
    ("suyu", "-f"),
    ("citron", "-f"),
    ("ryujinx", "--fullscreen"),
    ("cemu", "-f"),
    ("mgba", "-f"),
    ("melonds", "--fullscreen"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowsLaunchConfiguration {
    pub executable: String,
    #[serde(rename = "signalFile")]
    pub signal_file: String,
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SteamShortcutManager {
    custom_userdata_dir: Option<PathBuf>,
    custom_launcher_dir: Option<PathBuf>,
    custom_shortcuts_file: Option<PathBuf>,
    simulate_windows: bool,
}

impl SteamShortcutManager {
    pub fn new() -> Self {
        Self {
            custom_userdata_dir: None,
            custom_launcher_dir: None,
            custom_shortcuts_file: None,
            simulate_windows: cfg!(windows),
        }
    }

    pub fn with_custom_launcher_dir(mut self, dir: PathBuf) -> Self {
        self.custom_launcher_dir = Some(dir);
        self
    }

    pub fn with_custom_userdata_dir(mut self, dir: PathBuf) -> Self {
        self.custom_userdata_dir = Some(dir);
        self
    }

    pub fn with_custom_shortcuts_file(mut self, file: PathBuf) -> Self {
        self.custom_shortcuts_file = Some(file);
        self
    }

    pub fn with_simulate_windows(mut self, simulate: bool) -> Self {
        self.simulate_windows = simulate;
        self
    }

    pub fn is_windows(&self) -> bool {
        self.simulate_windows
    }

    pub fn detect_fullscreen_args(executable_path: &str) -> String {
        let exe_name = Path::new(executable_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        FULLSCREEN_FLAGS
            .iter()
            .find(|(pattern, _)| exe_name.contains(pattern))
            .map(|(_, flags)| flags.to_string())
            .unwrap_or_default()
    }

    pub fn resolve_fullscreen_args(entry: &Emulator) -> String {
        entry
            .fullscreen_args
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| Self::detect_fullscreen_args(&entry.common.executable_path))
    }

    pub fn launcher_key(&self, name: &str) -> String {
        let readable: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
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
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();

        format!("{readable}-{digest}")
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

        accounts.sort_by_key(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        accounts.reverse();

        for account in &accounts {
            let vdf = account.join("config/shortcuts.vdf");
            if vdf.exists() || vdf.parent().is_some_and(|p| p.exists()) {
                return Some(vdf);
            }
        }

        accounts.first().map(|a| a.join("config/shortcuts.vdf"))
    }

    pub fn load_shortcuts(&self) -> io::Result<Vec<SteamShortcut>> {
        let file = match self.find_shortcuts_file() {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        BinaryVdf::read(&file)
    }

    pub fn is_registered(&self, entry: &GameEntry) -> bool {
        let shortcuts = self.load_shortcuts().unwrap_or_default();
        let target_name = format!("[EmuSync] {}", entry.common().name);
        let target_app_id = entry.common().steam_app_id;

        shortcuts.iter().any(|s| {
            s.app_name == target_name
                || (target_app_id.is_some() && Some(s.app_id) == target_app_id)
        })
    }

    pub fn prepare_launch(&self, entry: &Emulator, rom_file: &Path) -> io::Result<()> {
        let signal_file = self.get_signal_file(entry);
        if let Some(parent) = signal_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(signal_file, rom_file.to_string_lossy().as_bytes())
    }

    pub fn register_entry(&self, entry: &GameEntry) -> io::Result<SteamShortcut> {
        let vdf_file = self.find_shortcuts_file().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Steam userdata directory not found",
            )
        })?;

        if !vdf_file.exists() {
            if let Some(parent) = vdf_file.parent() {
                fs::create_dir_all(parent)?;
            }
            BinaryVdf::write(&vdf_file, &[])?;
        }

        // Backup
        if let Some(parent) = vdf_file.parent() {
            let backup_file = parent.join("shortcuts.vdf.bak");
            let _ = fs::copy(&vdf_file, backup_file);
        }

        let existing = BinaryVdf::read(&vdf_file)?;
        let app_name = format!("[EmuSync] {}", entry.common().name);
        let target_app_id = entry.common().steam_app_id;

        let duplicate = existing.iter().find(|s| {
            s.app_name == app_name || (target_app_id.is_some() && Some(s.app_id) == target_app_id)
        });

        if let Some(dup) = duplicate {
            let regenerated = self.build_shortcut(entry, &app_name)?;
            let updated = SteamShortcut {
                app_id: dup.app_id,
                app_name: regenerated.app_name,
                exe: regenerated.exe,
                start_dir: regenerated.start_dir,
                launch_options: regenerated.launch_options,
                icon: dup.icon.clone(),
                shortcut_path: dup.shortcut_path.clone(),
                is_hidden: dup.is_hidden,
                allow_desktop_config: dup.allow_desktop_config,
                allow_overlay: dup.allow_overlay,
                open_vr: dup.open_vr,
                devkit: dup.devkit,
                devkit_game_id: dup.devkit_game_id.clone(),
                devkit_override_app_id: dup.devkit_override_app_id,
                last_play_time: dup.last_play_time,
                flatpak_app_id: dup.flatpak_app_id.clone(),
                sort_as: dup.sort_as.clone(),
                tags: dup.tags.clone(),
                extra_fields: dup.extra_fields.clone(),
            };

            let updated_list: Vec<SteamShortcut> = existing
                .iter()
                .map(|s| {
                    if s.app_id == dup.app_id {
                        updated.clone()
                    } else {
                        s.clone()
                    }
                })
                .collect();

            if updated != *dup {
                BinaryVdf::write(&vdf_file, &updated_list)?;
            }

            if let GameEntry::Emulator(emu) = entry {
                self.bridge_previous_name(emu, dup, &existing)?;
                self.install_legacy_bridge(emu, &updated_list)?;
            }

            return Ok(updated);
        }

        let new_shortcut = self.build_shortcut(entry, &app_name)?;
        let mut updated_list = existing;
        updated_list.push(new_shortcut.clone());
        BinaryVdf::write(&vdf_file, &updated_list)?;

        if let GameEntry::Emulator(emu) = entry {
            self.install_legacy_bridge(emu, &updated_list)?;
        }

        Ok(new_shortcut)
    }

    pub fn unregister_entry(&self, entry: &GameEntry) -> io::Result<()> {
        let vdf_file = match self.find_shortcuts_file() {
            Some(f) => f,
            None => return Ok(()),
        };
        if !vdf_file.exists() {
            return Ok(());
        }

        if let Some(parent) = vdf_file.parent() {
            let backup_file = parent.join("shortcuts.vdf.bak");
            let _ = fs::copy(&vdf_file, backup_file);
        }

        let target_name = format!("[EmuSync] {}", entry.common().name);
        let target_app_id = entry.common().steam_app_id;

        let existing = BinaryVdf::read(&vdf_file)?;
        let filtered: Vec<SteamShortcut> = existing
            .into_iter()
            .filter(|s| {
                s.app_name != target_name
                    && (target_app_id.is_none() || Some(s.app_id) != target_app_id)
            })
            .collect();

        BinaryVdf::write(&vdf_file, &filtered)
    }

    pub fn migrate_legacy_launchers(
        &self,
        entries: &[GameEntry],
    ) -> io::Result<Vec<SteamShortcut>> {
        let file = match self.find_shortcuts_file() {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let existing = BinaryVdf::read(&file)?;

        let mut matches = Vec::new();
        for entry in entries {
            if let GameEntry::Emulator(emu) = entry {
                let app_name = format!("[EmuSync] {}", emu.common.name);
                if let Some(s) = existing.iter().find(|s| {
                    s.app_name == app_name
                        || (emu.common.steam_app_id.is_some()
                            && Some(s.app_id) == emu.common.steam_app_id)
                }) {
                    matches.push((emu, s.clone()));
                }
            }
        }

        let mut replacements: HashMap<i32, SteamShortcut> = HashMap::new();
        for (emu, shortcut) in &matches {
            let app_name = format!("[EmuSync] {}", emu.common.name);
            let generated = self.build_emulator_shortcut(emu, &app_name)?;
            let updated = SteamShortcut {
                exe: generated.exe,
                start_dir: generated.start_dir,
                launch_options: generated.launch_options,
                ..shortcut.clone()
            };
            replacements.insert(shortcut.app_id, updated);
        }

        let updated: Vec<SteamShortcut> = existing
            .iter()
            .map(|s| {
                replacements
                    .get(&s.app_id)
                    .cloned()
                    .unwrap_or_else(|| s.clone())
            })
            .collect();

        if updated != existing {
            if let Some(parent) = file.parent() {
                let _ = fs::copy(&file, parent.join("shortcuts.vdf.bak"));
            }
            BinaryVdf::write(&file, &updated)?;
        }

        for (emu, previous) in &matches {
            self.bridge_previous_name(emu, previous, &existing)?;
            self.install_legacy_bridge(emu, &existing)?;
        }

        Ok(updated)
    }

    pub fn create_wrapper_script(&self, entry: &Emulator) -> io::Result<PathBuf> {
        if self.is_windows() {
            self.create_windows_wrapper_script(entry)
        } else {
            self.create_linux_wrapper_script(entry)
        }
    }

    pub fn create_linux_wrapper_script(&self, entry: &Emulator) -> io::Result<PathBuf> {
        let launcher_dir = self.get_launcher_dir();
        fs::create_dir_all(&launcher_dir)?;
        let key = self.launcher_key(&entry.common.name);
        let script_file = launcher_dir.join(format!("launch_{key}.sh"));
        let signal_file = self.get_signal_file(entry);

        let args_part = entry
            .common
            .arguments
            .iter()
            .map(|arg| shell_argument(arg, true).unwrap_or_else(|_| format!("\"{arg}\"")))
            .collect::<Vec<_>>()
            .join(" ");

        let raw_fs = Self::resolve_fullscreen_args(entry);
        let fullscreen_tokens = parse_arguments(&raw_fs).unwrap_or_default();
        let fs_args_tokens = fullscreen_tokens
            .iter()
            .map(|t| shell_argument(t, false).unwrap_or_else(|_| format!("\"{t}\"")))
            .collect::<Vec<_>>()
            .join(" ");
        let fs_args_line = format!("FULLSCREEN_ARGS=({fs_args_tokens})");

        let signal_arg = shell_argument(&signal_file.to_string_lossy(), false)
            .unwrap_or_else(|_| format!("\"{}\"", signal_file.display()));
        let exe_arg = shell_argument(&entry.common.executable_path, false)
            .unwrap_or_else(|_| format!("\"{}\"", entry.common.executable_path));

        let sanitized_name = entry.common.name.replace(['\r', '\n'], " ");

        let contents = format!(
            "#!/bin/bash\n# Auto-generated by EmuSync — do not edit manually.\n# Launcher for: {sanitized_name}\n\nSIGNAL_FILE={signal_arg}\n\nif [ ! -f \"$SIGNAL_FILE\" ]; then\n    echo \"EmuSync: No ROM path found. Launch a game from EmuSync first.\"\n    exit 1\nfi\n\nROM_PATH=$(cat \"$SIGNAL_FILE\")\n\nif [ ! -e \"$ROM_PATH\" ]; then\n    echo \"EmuSync: ROM file not found: $ROM_PATH\"\n    exit 1\nfi\n\n# Fullscreen/Big Picture flags to bypass Qt windowed GUI\n{fs_args_line}\n\necho \"EmuSync: Launching with: $ROM_PATH\"\nexec {exe_arg} \"${{FULLSCREEN_ARGS[@]}}\" {args_part}\n"
        );

        write_launcher_file(&script_file, &contents)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&script_file, fs::Permissions::from_mode(0o755));
        }

        Ok(script_file)
    }

    pub fn create_windows_wrapper_script(&self, entry: &Emulator) -> io::Result<PathBuf> {
        let launcher_dir = self.get_launcher_dir();
        fs::create_dir_all(&launcher_dir)?;
        let key = self.launcher_key(&entry.common.name);
        let signal_file = self.get_signal_file(entry);

        let raw_fs = Self::resolve_fullscreen_args(entry);
        let fullscreen_tokens = parse_arguments(&raw_fs).unwrap_or_default();

        let mut all_args = fullscreen_tokens;
        all_args.extend(entry.common.arguments.clone());

        for arg in std::iter::once(&entry.common.executable_path)
            .chain(std::iter::once(&signal_file.to_string_lossy().to_string()))
            .chain(all_args.iter())
        {
            if arg.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Launcher paths and arguments cannot contain NUL",
                ));
            }
        }

        let script_file = launcher_dir.join("windows-launcher-v1.ps1");
        write_launcher_file(&script_file, WINDOWS_LAUNCHER_V1_PS1)?;

        let config = WindowsLaunchConfiguration {
            executable: entry.common.executable_path.clone(),
            signal_file: signal_file.to_string_lossy().to_string(),
            arguments: all_args,
        };
        let config_json = serde_json::to_string(&config)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let config_file = launcher_dir.join(format!("launch_{key}.json"));
        write_launcher_file(&config_file, &config_json)?;

        let encoded = self.encoded_command(&key);
        let batch_contents = format!(
            "@echo off\r\nsetlocal DisableDelayedExpansion\r\npowershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}\r\nexit /b %ERRORLEVEL%\r\n"
        );
        let batch_file = launcher_dir.join(format!("launch_{key}.bat"));
        write_launcher_file(&batch_file, &batch_contents)?;

        Ok(batch_file)
    }

    pub fn encoded_command(&self, key: &str) -> String {
        let launcher_dir = self.get_launcher_dir();
        let script_path = launcher_dir.join("windows-launcher-v1.ps1");
        let config_path = launcher_dir.join(format!("launch_{key}.json"));

        let script_b64 = BASE64.encode(script_path.to_string_lossy().as_bytes());
        let config_b64 = BASE64.encode(config_path.to_string_lossy().as_bytes());

        let command = format!(
            "& ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{script_b64}'))) -ConfigPath ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{config_b64}')))\nexit $LASTEXITCODE"
        );

        let utf16_bytes: Vec<u8> = command
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();

        BASE64.encode(&utf16_bytes)
    }

    fn bridge_previous_name(
        &self,
        entry: &Emulator,
        previous: &SteamShortcut,
        shortcuts: &[SteamShortcut],
    ) -> io::Result<()> {
        let previous_name = previous.app_name.trim_start_matches("[EmuSync] ");
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

        let legacy_old_name = previous_name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let legacy_script = launcher_dir.join(format!("launch_{legacy_old_name}.{extension}"));

        if !old_script.exists() && !legacy_script.exists() {
            return Ok(());
        }

        let contents = if self.is_windows() {
            let key = self.launcher_key(&entry.common.name);
            let encoded = self.encoded_command(&key);
            format!(
                "@echo off\r\nsetlocal DisableDelayedExpansion\r\npowershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}\r\nexit /b %ERRORLEVEL%\r\n"
            )
        } else {
            let target_sh = launcher_dir.join(format!(
                "launch_{}.sh",
                self.launcher_key(&entry.common.name)
            ));
            let target_arg = shell_argument(&target_sh.to_string_lossy(), false)
                .unwrap_or_else(|_| format!("\"{}\"", target_sh.display()));
            format!("#!/bin/bash\nexec {target_arg} \"$@\"\n")
        };

        write_launcher_file(&old_script, &contents)?;
        #[cfg(unix)]
        if !self.is_windows() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&old_script, fs::Permissions::from_mode(0o755));
        }

        Ok(())
    }

    fn install_legacy_bridge(
        &self,
        entry: &Emulator,
        shortcuts: &[SteamShortcut],
    ) -> io::Result<()> {
        let old_name: String = entry
            .common
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();

        let conflicting_names = shortcuts
            .iter()
            .filter(|s| s.app_name.starts_with("[EmuSync] "))
            .map(|s| s.app_name.trim_start_matches("[EmuSync] "))
            .filter(|name| {
                let norm: String = name
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                norm == old_name
            })
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
            let _ = fs::copy(&previous, &backup);
        }

        let contents = if conflicting_names > 1 {
            if self.is_windows() {
                "@echo off\r\necho EmuSync: Restart Steam to load the separate emulator launchers.\r\nexit /b 1\r\n".to_string()
            } else {
                "#!/bin/bash\necho 'EmuSync: Restart Steam to load the separate emulator launchers.' >&2\nexit 1\n".to_string()
            }
        } else if self.is_windows() {
            let key = self.launcher_key(&entry.common.name);
            let encoded = self.encoded_command(&key);
            format!(
                "@echo off\r\nsetlocal DisableDelayedExpansion\r\npowershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {encoded}\r\nexit /b %ERRORLEVEL%\r\n"
            )
        } else {
            let target_sh = launcher_dir.join(format!(
                "launch_{}.sh",
                self.launcher_key(&entry.common.name)
            ));
            let target_arg = shell_argument(&target_sh.to_string_lossy(), false)
                .unwrap_or_else(|_| format!("\"{}\"", target_sh.display()));
            format!("#!/bin/bash\nexec {target_arg} \"$@\"\n")
        };

        write_launcher_file(&previous, &contents)?;
        #[cfg(unix)]
        if !self.is_windows() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&previous, fs::Permissions::from_mode(0o755));
        }

        Ok(())
    }

    fn resolve_start_dir(&self, executable_path: &str) -> String {
        let normalized = executable_path.replace('\\', "/");
        match normalized.rfind('/') {
            None => ".".to_string(),
            Some(last_slash) => {
                let parent = &normalized[..last_slash];
                if parent.is_empty() {
                    "/".to_string()
                } else if self.is_windows() {
                    parent.replace('/', "\\")
                } else {
                    parent.to_string()
                }
            }
        }
    }

    fn generate_app_id(&self) -> i32 {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        // High range negative integer (matching Steam's non-steam game id range)
        -1000 - ((nanos % 1_000_000_000) as i32)
    }

    fn build_shortcut(&self, entry: &GameEntry, app_name: &str) -> io::Result<SteamShortcut> {
        match entry {
            GameEntry::Emulator(emu) => self.build_emulator_shortcut(emu, app_name),
            GameEntry::Native(nat) => Ok(self.build_native_shortcut(nat, app_name)),
        }
    }

    fn build_emulator_shortcut(
        &self,
        entry: &Emulator,
        app_name: &str,
    ) -> io::Result<SteamShortcut> {
        let launcher_script = self.create_wrapper_script(entry)?;
        let start_dir = self.resolve_start_dir(&entry.common.executable_path);

        let mut tags = BTreeMap::new();
        tags.insert("0".to_string(), "EmuSync".to_string());

        if self.is_windows() {
            let win_root =
                std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
            let powershell =
                format!("{win_root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe");
            let key = self.launcher_key(&entry.common.name);
            let launch_options = format!(
                "-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
                self.encoded_command(&key)
            );
            Ok(SteamShortcut {
                app_id: self.generate_app_id(),
                app_name: app_name.to_string(),
                exe: format!("\"{powershell}\""),
                start_dir,
                launch_options,
                tags,
                ..SteamShortcut::default()
            })
        } else {
            Ok(SteamShortcut {
                app_id: self.generate_app_id(),
                app_name: app_name.to_string(),
                exe: format!("\"{}\"", launcher_script.display()),
                start_dir,
                launch_options: String::new(),
                tags,
                ..SteamShortcut::default()
            })
        }
    }

    fn build_native_shortcut(&self, entry: &NativeGame, app_name: &str) -> SteamShortcut {
        let start_dir = self.resolve_start_dir(&entry.common.executable_path);
        let base_args = entry.common.arguments.join(" ");
        let launch_options = if !base_args.trim().is_empty() {
            format!("%command% {base_args}")
        } else {
            String::new()
        };

        let mut tags = BTreeMap::new();
        tags.insert("0".to_string(), "EmuSync".to_string());

        SteamShortcut {
            app_id: self.generate_app_id(),
            app_name: app_name.to_string(),
            exe: format!("\"{}\"", entry.common.executable_path),
            start_dir,
            launch_options,
            tags,
            ..SteamShortcut::default()
        }
    }
}

impl Default for SteamShortcutManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn write_launcher_file(file: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    let random: u64 = {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(999)
    };
    let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let staged = parent.join(format!(".{file_name}.{random}.tmp"));

    struct StagedGuard<'a>(&'a Path);
    impl Drop for StagedGuard<'_> {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0);
        }
    }
    let guard = StagedGuard(&staged);

    let mut f = File::create(&staged)?;
    f.write_all(contents.as_bytes())?;
    f.sync_all()?;
    drop(f);

    fs::rename(&staged, file)?;
    std::mem::forget(guard);

    Ok(())
}

const WINDOWS_LAUNCHER_V1_PS1: &str = r#"param([Parameter(Mandatory=$true)][string]$ConfigPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

# Serialize a native Windows argument according to the Microsoft CRT rules.
function ConvertTo-NativeArgument([AllowEmptyString()][string]$Value) {
    $builder = New-Object System.Text.StringBuilder
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') { $slashes++; continue }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (2 * $slashes + 1)))
        } else {
            [void]$builder.Append(('\' * $slashes))
        }
        [void]$builder.Append($character)
        $slashes = 0
    }
    [void]$builder.Append(('\' * (2 * $slashes)))
    [void]$builder.Append('"')
    return $builder.ToString()
}

try {
    $configuration = [IO.File]::ReadAllText($ConfigPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
    if ($configuration.executable -match '(?i)\.(bat|cmd)$') {
        throw 'Choose an emulator executable instead of a batch script.'
    }
    if (!(Test-Path -LiteralPath $configuration.signalFile -PathType Leaf)) {
        throw 'No ROM path found. Launch a game from EmuSync first.'
    }
    $romPath = [IO.File]::ReadAllText($configuration.signalFile, [Text.Encoding]::UTF8).TrimEnd([char[]]"`r`n")
    if (!(Test-Path -LiteralPath $romPath)) { throw 'The selected ROM no longer exists.' }
    $nativeArguments = @($configuration.arguments | ForEach-Object {
        ConvertTo-NativeArgument (([string]$_).Replace('{ROM}', $romPath))
    })
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $configuration.executable
    $start.UseShellExecute = $false
    $start.Arguments = $nativeArguments -join ' '
    $workingDirectory = [IO.Path]::GetDirectoryName($configuration.executable)
    if ($workingDirectory -and (Test-Path -LiteralPath $workingDirectory -PathType Container)) {
        $start.WorkingDirectory = $workingDirectory
    }
    $process = [Diagnostics.Process]::Start($start)
    $process.WaitForExit()
    $exitCode = $process.ExitCode
    $process.Dispose()
    exit $exitCode
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
"#;
