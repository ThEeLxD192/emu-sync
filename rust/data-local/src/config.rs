use crate::{
    config_dto::Document,
    storage::{absolute, atomic_write},
};
use emusync_domain::{
    config::{
        AppConfig, ConfigurationStore, CredentialSnapshot, CredentialStore, CredentialTransaction,
        Credentials,
    },
    library::{Emulator, GameEntry, LibraryStore, SaveTarget, SaveTargetType},
};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Default)]
struct State {
    revision: i64,
}
/// In-process serialized configuration access. Separate Kotlin and Rust processes must not share it concurrently.
#[derive(Clone)]
pub struct ConfigManager {
    path: PathBuf,
    state: Arc<Mutex<State>>,
}
impl ConfigManager {
    pub fn new(path: &Path) -> io::Result<Self> {
        static STATES: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<State>>>>> = OnceLock::new();
        let path = absolute(path)?;
        let mut states = STATES
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| io::Error::other("configuration registry poisoned"))?;
        let state = states.entry(path.clone()).or_default().clone();
        Ok(Self { path, state })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    fn read_raw(&self) -> io::Result<Option<AppConfig>> {
        let data = match fs::read(&self.path) {
            Ok(data) if !data.is_empty() => data,
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "empty configuration",
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut value: serde_json::Value = serde_json::from_slice(&data)?;
        if let Some(entries) = value.get_mut("entries").and_then(|v| v.as_array_mut()) {
            for entry in entries {
                if let Some(object) = entry.as_object_mut()
                    && object.get("type").and_then(|v| v.as_str()) == Some("emulator")
                    && !object.contains_key("arguments")
                {
                    object.insert("arguments".into(), serde_json::json!(["{ROM}"]));
                }
            }
        }
        let document: Document = serde_json::from_value(value)?;
        Ok(Some(AppConfig::from(document)))
    }
    fn load_inner(&self, auto_create: bool) -> io::Result<AppConfig> {
        let Some(config) = self.read_raw()? else {
            if auto_create {
                let default = AppConfig::default();
                self.save_inner(&default)?;
                return Ok(default);
            }
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "missing configuration",
            ));
        };
        let mut migrated = config.clone();
        for entry in &mut migrated.entries {
            migrate(entry);
        }
        if migrated != config {
            self.save_inner(&migrated)?;
        }
        Ok(migrated)
    }
    fn save_inner(&self, config: &AppConfig) -> io::Result<()> {
        let document = Document::from(config.clone());
        atomic_write(&self.path, &serde_json::to_vec_pretty(&document)?)
    }
    pub fn save(&self, config: &AppConfig) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("configuration lock poisoned"))?;
        let previous = match self.read_raw() {
            Ok(Some(c)) => Some(c.credentials),
            Ok(None) => Some(None),
            Err(_) => None,
        };
        self.save_inner(config)?;
        if previous.as_ref() != Some(&config.credentials) {
            state.revision = state.revision.wrapping_add(1);
        }
        Ok(())
    }
    fn lock(&self) -> io::Result<std::sync::MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("configuration lock poisoned"))
    }
}
impl ConfigurationStore for ConfigManager {
    type Error = io::Error;
    fn load(&self) -> io::Result<AppConfig> {
        let _guard = self.lock()?;
        self.load_inner(true)
    }
    fn update(&self, transform: impl FnOnce(AppConfig) -> AppConfig) -> io::Result<AppConfig> {
        let mut state = self.lock()?;
        let current = self.load_inner(true)?;
        let updated = transform(current.clone());
        if updated != current {
            self.save_inner(&updated)?;
            if updated.credentials != current.credentials {
                state.revision = state.revision.wrapping_add(1);
            }
        }
        Ok(updated)
    }
    fn update_credentials(
        &self,
        transform: impl FnOnce(Option<Credentials>) -> Option<Credentials>,
    ) -> io::Result<AppConfig> {
        let mut state = self.lock()?;
        let current = self.load_inner(true)?;
        let mut updated = current.clone();
        updated.credentials = transform(updated.credentials);
        if updated != current {
            self.save_inner(&updated)?;
        }
        state.revision = state.revision.wrapping_add(1);
        Ok(updated)
    }
}
impl CredentialStore for ConfigManager {
    type Error = io::Error;
    fn snapshot(&self, fallback: &AppConfig) -> io::Result<CredentialSnapshot> {
        let state = self.lock()?;
        let credentials = match self.read_raw()? {
            Some(config) => config.credentials,
            None => fallback.credentials.clone(),
        };
        Ok(CredentialSnapshot {
            credentials,
            revision: state.revision,
        })
    }
    fn compare_and_set(
        &self,
        expected: &CredentialSnapshot,
        credentials: Option<Credentials>,
        fallback: &AppConfig,
    ) -> io::Result<bool> {
        let mut state = self.lock()?;
        let current = self.read_raw()?;
        let current_credentials = match &current {
            Some(c) => &c.credentials,
            None => &fallback.credentials,
        };
        if state.revision != expected.revision || current_credentials != &expected.credentials {
            return Ok(false);
        }
        if current_credentials != &credentials {
            let mut current = match current {
                Some(mut c) => {
                    for entry in &mut c.entries {
                        migrate(entry);
                    }
                    c
                }
                None => fallback.clone(),
            };
            current.credentials = credentials;
            self.save_inner(&current)?;
            state.revision = state.revision.wrapping_add(1);
        }
        Ok(true)
    }
}
impl CredentialTransaction for ConfigManager {
    fn while_current<R>(
        &self,
        expected: &CredentialSnapshot,
        fallback: &AppConfig,
        action: impl FnOnce() -> R,
    ) -> io::Result<Option<R>> {
        let state = self.lock()?;
        let current_credentials = match self.read_raw()? {
            Some(config) => config.credentials,
            None => fallback.credentials.clone(),
        };
        if state.revision != expected.revision || current_credentials != expected.credentials {
            return Ok(None);
        }
        Ok(Some(action()))
    }
}
impl LibraryStore for ConfigManager {
    type Error = io::Error;
    fn load_entries(&self) -> io::Result<Vec<GameEntry>> {
        Ok(self.load()?.entries)
    }
    fn update_entries(
        &self,
        transform: impl FnOnce(Vec<GameEntry>) -> Vec<GameEntry>,
    ) -> io::Result<Vec<GameEntry>> {
        Ok(self
            .update(|mut c| {
                c.entries = transform(c.entries);
                c
            })?
            .entries)
    }
}
pub(crate) fn infer(path: &str) -> SaveTarget {
    SaveTarget {
        path: path.into(),
        kind: match fs::metadata(path) {
            Ok(m) if m.is_dir() => SaveTargetType::Directory,
            Ok(_) => SaveTargetType::File,
            Err(_) => SaveTargetType::Unknown,
        },
        cloud_path: None,
    }
}
pub(crate) fn resolve(target: &mut SaveTarget) {
    if target
        .cloud_path
        .as_ref()
        .is_some_and(|s| s.trim().is_empty())
    {
        target.cloud_path = None;
    }
    if let Ok(metadata) = fs::metadata(&target.path)
        && (target.kind == SaveTargetType::Unknown
            || (target.kind == SaveTargetType::File && metadata.is_dir())
            || (target.kind == SaveTargetType::Directory && metadata.is_file()))
    {
        target.kind = if metadata.is_dir() {
            SaveTargetType::Directory
        } else {
            SaveTargetType::File
        };
    }
}
fn migrate(entry: &mut GameEntry) {
    let common = match entry {
        GameEntry::Emulator(e) => &mut e.common,
        GameEntry::Native(n) => &mut n.common,
    };
    if common.save_targets.is_empty() {
        common.save_targets = common
            .save_paths
            .iter()
            .filter(|p| !p.trim().is_empty())
            .map(|p| infer(p))
            .collect();
    }
    common.save_targets.iter_mut().for_each(resolve);
    if let GameEntry::Emulator(e) = entry {
        migrate_emulator(e);
    }
}
fn migrate_emulator(e: &mut Emulator) {
    for (_, targets) in &mut e.save_targets_by_rom {
        targets.iter_mut().for_each(resolve);
    }
    for (rom, paths) in &e.save_paths_by_rom {
        let inferred: Vec<_> = paths
            .iter()
            .filter(|p| !p.trim().is_empty())
            .map(|p| infer(p))
            .collect();
        if inferred.is_empty() {
            continue;
        }
        if let Some((_, targets)) = e.save_targets_by_rom.iter_mut().find(|(k, _)| k == rom) {
            if targets.is_empty() {
                *targets = inferred;
            }
        } else {
            e.save_targets_by_rom.push((rom.clone(), inferred));
        }
    }
}
