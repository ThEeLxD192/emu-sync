//! Library models and portable naming rules.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveTargetType {
    File,
    Directory,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTarget {
    pub path: String,
    pub kind: SaveTargetType,
    pub cloud_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntryCommon {
    pub name: String,
    pub executable_path: String,
    pub arguments: Vec<String>,
    pub save_paths: Vec<String>,
    pub save_targets: Vec<SaveTarget>,
    pub group: Option<String>,
    pub cloud_folder: Option<String>,
    pub drive_file_id: Option<String>,
    pub steam_app_id: Option<i32>,
}
impl EntryCommon {
    pub fn effective_cloud_folder(&self) -> &str {
        self.cloud_folder
            .as_deref()
            .and_then(nonblank)
            .or_else(|| self.group.as_deref().and_then(nonblank))
            .unwrap_or(&self.name)
    }
}
fn nonblank(s: &str) -> Option<&str> {
    let s = s.trim();
    (!s.is_empty()).then_some(s)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emulator {
    pub common: EntryCommon,
    pub roms_directory: String,
    pub extensions: Vec<String>,
    pub save_paths_by_rom: Vec<(String, Vec<String>)>,
    pub save_targets_by_rom: Vec<(String, Vec<SaveTarget>)>,
    pub cover_path_by_rom: Vec<(String, String)>,
    pub title_by_rom: Vec<(String, String)>,
    pub steam_process_name: Option<String>,
    pub fullscreen_args: Option<String>,
    pub covers_directory: Option<String>,
}
impl Default for Emulator {
    fn default() -> Self {
        Self {
            common: EntryCommon {
                arguments: vec!["{ROM}".into()],
                ..EntryCommon::default()
            },
            roms_directory: String::new(),
            extensions: Vec::new(),
            save_paths_by_rom: Vec::new(),
            save_targets_by_rom: Vec::new(),
            cover_path_by_rom: Vec::new(),
            title_by_rom: Vec::new(),
            steam_process_name: None,
            fullscreen_args: None,
            covers_directory: None,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeGame {
    pub common: EntryCommon,
    pub wait_for_process: Option<String>,
    pub cover_path: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameEntry {
    Emulator(Emulator),
    Native(NativeGame),
}
impl GameEntry {
    pub fn common(&self) -> &EntryCommon {
        match self {
            Self::Emulator(e) => &e.common,
            Self::Native(g) => &g.common,
        }
    }
}
impl Emulator {
    /// Constructs a configured emulator with Kotlin's default ROM argument.
    pub fn new(
        name: impl Into<String>,
        executable_path: impl Into<String>,
        roms_directory: impl Into<String>,
        extensions: Vec<String>,
    ) -> Self {
        Self {
            common: EntryCommon {
                name: name.into(),
                executable_path: executable_path.into(),
                arguments: vec!["{ROM}".into()],
                ..EntryCommon::default()
            },
            roms_directory: roms_directory.into(),
            extensions,
            ..Self::default()
        }
    }
    pub fn effective_title(&self, rom: &str, fallback: &str) -> String {
        let stem = rom.rsplit_once('.').map_or(rom, |(s, _)| s);
        for key in [rom, stem] {
            if let Some(title) = self
                .title_by_rom
                .iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| nonblank(v))
            {
                return title.to_owned();
            }
        }
        let target = normalize_game_key(rom);
        if !target.is_empty()
            && let Some(title) = self.title_by_rom.iter().find_map(|(key, value)| {
                (normalize_game_key(key) == target)
                    .then(|| nonblank(value))
                    .flatten()
            })
        {
            return title.to_owned();
        }
        clean_game_title(fallback)
    }
}

/// Preserves ordered title overrides, including the first matching normalized title.
pub fn clean_game_title(raw: &str) -> String {
    let base = raw
        .rsplit_once('.')
        .filter(|(_, ext)| {
            (1..=5).contains(&ext.len()) && ext.bytes().all(|c| c.is_ascii_alphanumeric())
        })
        .map_or(raw, |(base, _)| base);
    let base = base.replace('_', " ");
    let mut result = String::new();
    let mut chars = base.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch == '['
            && let Some(end) = base[start + 1..].find(']')
        {
            let end = start + 1 + end;
            while chars.peek().is_some_and(|(i, _)| *i <= end) {
                chars.next();
            }
            continue;
        }
        if ch == '('
            && let Some(end) = base[start + 1..].find(')')
        {
            let end = start + 1 + end;
            let inner = &base[start + 1..end];
            let lower = inner.to_ascii_lowercase();
            let keep = ["disc", "cd", "part"].iter().any(|prefix| {
                lower
                    .strip_prefix(prefix)
                    .is_some_and(|s| s.trim_start().starts_with(|c: char| c.is_ascii_digit()))
            });
            if !keep {
                while chars.peek().is_some_and(|(i, _)| *i <= end) {
                    chars.next();
                }
                continue;
            }
        }
        result.push(ch);
    }
    let result = result.split_whitespace().collect::<Vec<_>>().join(" ");
    if result.is_empty() {
        raw.trim().to_owned()
    } else {
        result
    }
}
pub fn normalize_game_key(name: &str) -> String {
    clean_game_title(name)
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn scope_new_targets(targets: &[SaveTarget]) -> Vec<SaveTarget> {
    if targets
        .iter()
        .filter(|t| t.kind == SaveTargetType::Directory)
        .count()
        < 2
    {
        return targets.to_vec();
    }
    let mut used: Vec<String> = targets
        .iter()
        .filter_map(|t| t.cloud_path.clone())
        .collect();
    let mut next = 1;
    targets
        .iter()
        .cloned()
        .map(|mut target| {
            if target.kind == SaveTargetType::Directory
                && target
                    .cloud_path
                    .as_deref()
                    .is_none_or(|p| p.trim().is_empty())
            {
                loop {
                    let candidate = format!("save-target-{next}");
                    next += 1;
                    if !used.iter().any(|p| {
                        p == &candidate
                            || p.starts_with(&format!("{candidate}/"))
                            || candidate.starts_with(&format!("{p}/"))
                    }) {
                        used.push(candidate.clone());
                        target.cloud_path = Some(candidate);
                        break;
                    }
                }
            }
            target
        })
        .collect()
}

pub trait LibraryStore {
    type Error;
    fn load_entries(&self) -> Result<Vec<GameEntry>, Self::Error>;
    /// Atomically transforms the latest entries, retaining unrelated settings.
    fn update_entries(
        &self,
        transform: impl FnOnce(Vec<GameEntry>) -> Vec<GameEntry>,
    ) -> Result<Vec<GameEntry>, Self::Error>;
}
pub trait SaveTargetAccess {
    type Error;
    fn type_of(&self, path: &str) -> Result<Option<SaveTargetType>, Self::Error>;
    fn for_entry(&self, entry: &GameEntry) -> Result<Vec<SaveTarget>, Self::Error>;
    fn for_rom(&self, entry: &Emulator, rom: &str) -> Result<Vec<SaveTarget>, Self::Error>;
    fn for_restore(
        &self,
        destination: &str,
        cloud_names: &[String],
    ) -> Result<Vec<SaveTarget>, Self::Error>;
}
pub trait CoverFiles {
    type Error;
    fn find(
        &self,
        name: &str,
        entry: &GameEntry,
        rom_path: Option<&str>,
    ) -> Result<Option<String>, Self::Error>;
}
pub trait SteamLibrary {
    type Error;
    fn is_available(&self) -> Result<bool, Self::Error>;
    fn is_registered(&self, entry: &GameEntry) -> Result<bool, Self::Error>;
    fn migrate_launchers(&self, entries: &[GameEntry]) -> Result<Vec<(String, i32)>, Self::Error>;
    fn register(&self, entry: &GameEntry) -> Result<i32, Self::Error>;
    fn unregister(&self, entry: &GameEntry) -> Result<(), Self::Error>;
}
