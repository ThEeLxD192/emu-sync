use crate::steam::{shortcut::SteamShortcut, vdf::BinaryVdf};
use emusync_domain::library::{Emulator, GameEntry, NativeGame};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) const NAME_PREFIX: &str = "[EmuSync] ";

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
    pub(super) custom_userdata_dir: Option<PathBuf>,
    pub(super) custom_launcher_dir: Option<PathBuf>,
    pub(super) custom_shortcuts_file: Option<PathBuf>,
    pub(super) simulate_windows: bool,
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

    pub fn load_shortcuts(&self) -> io::Result<Vec<SteamShortcut>> {
        let file = match self.find_shortcuts_file() {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        BinaryVdf::read(&file)
    }

    pub fn is_registered(&self, entry: &GameEntry) -> io::Result<bool> {
        let shortcuts = self.load_shortcuts()?;
        let target_name = format!("{NAME_PREFIX}{}", entry.common().name);
        let target_app_id = entry.common().steam_app_id;

        Ok(shortcuts.iter().any(|s| {
            s.app_name == target_name
                || (target_app_id.is_some() && Some(s.app_id) == target_app_id)
        }))
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

        backup_shortcuts(&vdf_file)?;

        let existing = BinaryVdf::read(&vdf_file)?;
        let app_name = format!("{NAME_PREFIX}{}", entry.common().name);
        let target_app_id = entry.common().steam_app_id;

        let dup_index = existing.iter().position(|s| {
            s.app_name == app_name || (target_app_id.is_some() && Some(s.app_id) == target_app_id)
        });

        if let Some(dup_index) = dup_index {
            let dup = &existing[dup_index];
            let regenerated = self.build_shortcut(entry, &app_name, &existing)?;
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

            // Reemplazo por posición: otro atajo con el mismo `app_id` no se toca.
            let mut updated_list = existing.clone();
            updated_list[dup_index] = updated.clone();

            if updated != *dup {
                BinaryVdf::write(&vdf_file, &updated_list)?;
            }

            if let GameEntry::Emulator(emu) = entry {
                self.bridge_previous_name(emu, dup, &existing)?;
                self.install_legacy_bridge(emu, &updated_list)?;
            }

            return Ok(updated);
        }

        let new_shortcut = self.build_shortcut(entry, &app_name, &existing)?;
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

        backup_shortcuts(&vdf_file)?;

        let target_name = format!("{NAME_PREFIX}{}", entry.common().name);
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

        let mut matches: Vec<(&Emulator, usize)> = Vec::new();
        for entry in entries {
            if let GameEntry::Emulator(emu) = entry {
                let app_name = format!("{NAME_PREFIX}{}", emu.common.name);
                if let Some(index) = existing.iter().position(|s| {
                    s.app_name == app_name
                        || (emu.common.steam_app_id.is_some()
                            && Some(s.app_id) == emu.common.steam_app_id)
                }) {
                    matches.push((emu, index));
                }
            }
        }

        // Un fallo en una entrada (argumentos inválidos, NUL, bridge) deja su atajo intacto y no
        // afecta a las demás: devolver `Err` aquí haría perder los `steam_app_id` de toda la
        // biblioteca. Si dos entradas resuelven al mismo índice, la última gana.
        let mut replacements: HashMap<usize, SteamShortcut> = HashMap::new();
        let mut migrated: Vec<(&Emulator, usize)> = Vec::new();
        for &(emu, index) in &matches {
            let app_name = format!("{NAME_PREFIX}{}", emu.common.name);
            let Ok(generated) = self.build_emulator_shortcut(emu, &app_name, &existing) else {
                continue;
            };
            let updated = SteamShortcut {
                exe: generated.exe,
                start_dir: generated.start_dir,
                launch_options: generated.launch_options,
                ..existing[index].clone()
            };
            replacements.insert(index, updated);
            migrated.push((emu, index));
        }

        let mut updated = existing.clone();
        for (index, replacement) in replacements {
            updated[index] = replacement;
        }

        let mut result = updated.clone();
        if updated != existing {
            // Sin backup verificado no se escribe, pero la lista sigue siendo válida.
            if backup_shortcuts(&file).is_ok() {
                BinaryVdf::write(&file, &updated)?;
            } else {
                result = existing.clone();
            }
        }

        for &(emu, index) in &migrated {
            let _ = self.bridge_previous_name(emu, &existing[index], &existing);
            let _ = self.install_legacy_bridge(emu, &existing);
        }

        Ok(result)
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

    /// Candidato negativo en el rango de Steam para no-Steam; evita colisiones con `existing`.
    fn generate_app_id(&self, existing: &[SteamShortcut]) -> i32 {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let mut candidate = -1000 - ((nanos % 1_000_000_000) as i32);
        let used: HashSet<i32> = existing.iter().map(|s| s.app_id).collect();
        while used.contains(&candidate) {
            let next = candidate.wrapping_sub(1);
            candidate = if next < -1000 { next } else { -1001 };
        }
        candidate
    }

    fn build_shortcut(
        &self,
        entry: &GameEntry,
        app_name: &str,
        existing: &[SteamShortcut],
    ) -> io::Result<SteamShortcut> {
        match entry {
            GameEntry::Emulator(emu) => self.build_emulator_shortcut(emu, app_name, existing),
            GameEntry::Native(nat) => Ok(self.build_native_shortcut(nat, app_name, existing)),
        }
    }

    fn build_emulator_shortcut(
        &self,
        entry: &Emulator,
        app_name: &str,
        existing: &[SteamShortcut],
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
                app_id: self.generate_app_id(existing),
                app_name: app_name.to_string(),
                exe: format!("\"{powershell}\""),
                start_dir,
                launch_options,
                tags,
                ..SteamShortcut::default()
            })
        } else {
            Ok(SteamShortcut {
                app_id: self.generate_app_id(existing),
                app_name: app_name.to_string(),
                exe: format!("\"{}\"", launcher_script.display()),
                start_dir,
                launch_options: String::new(),
                tags,
                ..SteamShortcut::default()
            })
        }
    }

    fn build_native_shortcut(
        &self,
        entry: &NativeGame,
        app_name: &str,
        existing: &[SteamShortcut],
    ) -> SteamShortcut {
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
            app_id: self.generate_app_id(existing),
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

/// Copia `shortcuts.vdf` a `shortcuts.vdf.bak`; un fallo impide modificar el original.
fn backup_shortcuts(vdf_file: &Path) -> io::Result<()> {
    let parent = vdf_file.parent().unwrap_or_else(|| Path::new("."));
    fs::copy(vdf_file, parent.join("shortcuts.vdf.bak")).map(|_| ())
}
