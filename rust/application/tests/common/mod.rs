#![allow(dead_code, unused_imports)]

use emusync_application::{SyncOrchestrator, SyncOrchestratorFactory, SyncServices};
use emusync_domain::config::{AppConfig, CloudAuthorization, Credentials};
use emusync_domain::execution::{GameLaunchRequest, GameLauncher, GameResult};
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
pub use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, DirectoryDownload, LocalSaveAccess,
    LocalSaveFiles, LocalSavePath, RemoteFile, RemoteState, SaveSetup, SaveSetupRequest,
    SaveSetupResult, SaveSynchronizer, SaveSynchronizerFactory, SyncEvent, SyncGame,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// --- Mock Filesystem ---

#[derive(Clone, Debug, PartialEq)]
pub enum MockNode {
    File { modified_ms: i64 },
    Directory,
}

#[derive(Clone, Default)]
pub struct MockFs {
    pub nodes: Arc<Mutex<HashMap<String, MockNode>>>,
}

impl MockFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&self, path: &str, modified_ms: i64) {
        let mut lock = self.nodes.lock().unwrap();
        let norm = normalize(path);
        lock.insert(norm.clone(), MockNode::File { modified_ms });
        let mut curr = norm;
        while let Some((parent, _)) = curr.rsplit_once('/') {
            if parent.is_empty() {
                break;
            }
            lock.entry(parent.to_string())
                .or_insert(MockNode::Directory);
            curr = parent.to_string();
        }
    }

    pub fn add_dir(&self, path: &str) {
        let mut lock = self.nodes.lock().unwrap();
        lock.insert(normalize(path), MockNode::Directory);
    }

    pub fn touch_file(&self, path: &str, new_time: i64) {
        let mut lock = self.nodes.lock().unwrap();
        let norm = normalize(path);
        lock.insert(
            norm,
            MockNode::File {
                modified_ms: new_time,
            },
        );
    }
}

pub fn normalize(p: &str) -> String {
    let s = p.trim().replace('\\', "/");
    let mut parts = Vec::new();
    for seg in s.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            parts.pop();
        } else {
            parts.push(seg);
        }
    }
    let norm = parts.join("/");
    if s.starts_with('/') {
        format!("/{norm}")
    } else {
        norm
    }
}

#[derive(Clone)]
pub struct MockPath {
    pub fs: Arc<Mutex<HashMap<String, MockNode>>>,
    pub raw_path: String,
}

impl LocalSavePath for MockPath {
    type Error = String;

    fn path(&self) -> &str {
        &self.raw_path
    }

    fn name(&self) -> &str {
        self.raw_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.raw_path)
    }

    fn absolute_path(&self) -> Result<String, Self::Error> {
        Ok(normalize(&self.raw_path))
    }

    fn normalized_path(&self) -> Result<String, Self::Error> {
        Ok(normalize(&self.raw_path))
    }

    fn ancestors(&self) -> Result<Vec<String>, Self::Error> {
        let norm = normalize(&self.raw_path);
        let mut result = Vec::new();
        let mut curr = norm.as_str();
        while let Some((parent, _)) = curr.rsplit_once('/') {
            if parent.is_empty() {
                result.push("/".to_string());
                break;
            }
            result.push(parent.to_string());
            curr = parent;
        }
        Ok(result)
    }

    fn is_file(&self) -> Result<bool, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        Ok(matches!(lock.get(&norm), Some(MockNode::File { .. })))
    }

    fn is_directory(&self) -> Result<bool, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        Ok(matches!(lock.get(&norm), Some(MockNode::Directory)))
    }

    fn exists(&self) -> Result<bool, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        Ok(lock.contains_key(&norm))
    }

    fn last_modified_ms(&self) -> Result<i64, Self::Error> {
        let norm = normalize(&self.raw_path);
        let lock = self.fs.lock().unwrap();
        match lock.get(&norm) {
            Some(MockNode::File { modified_ms }) => Ok(*modified_ms),
            _ => Ok(0),
        }
    }

    fn resolve(&self, relative: &str) -> Result<Self, Self::Error> {
        let norm_parent = normalize(&self.raw_path);
        let full = format!("{}/{}", norm_parent, relative.trim_start_matches('/'));
        Ok(MockPath {
            fs: Arc::clone(&self.fs),
            raw_path: full,
        })
    }

    fn resolve_remote(&self, name: &str) -> Result<Option<Self>, Self::Error> {
        if name
            .split('/')
            .any(|s| s == ".." || s == "." || s.is_empty())
        {
            return Ok(None);
        }
        let resolved = self.resolve(name)?;
        Ok(Some(resolved))
    }

    fn walk_top_down(&self) -> Result<Vec<Self>, Self::Error> {
        let norm_parent = normalize(&self.raw_path);
        let prefix = format!("{norm_parent}/");
        let lock = self.fs.lock().unwrap();
        let mut files = Vec::new();
        for key in lock.keys() {
            if key.starts_with(&prefix) {
                files.push(MockPath {
                    fs: Arc::clone(&self.fs),
                    raw_path: key.clone(),
                });
            }
        }
        files.sort_by(|a, b| a.raw_path.cmp(&b.raw_path));
        Ok(files)
    }

    fn relative_to(&self, root: &Self) -> Result<String, Self::Error> {
        let norm_self = normalize(&self.raw_path);
        let norm_root = normalize(&root.raw_path);
        let prefix = format!("{norm_root}/");
        if let Some(stripped) = norm_self.strip_prefix(&prefix) {
            Ok(stripped.to_string())
        } else if norm_self == norm_root {
            Ok(String::new())
        } else {
            Err(format!("{norm_self} is not relative to {norm_root}"))
        }
    }
}

impl LocalSaveAccess for MockFs {
    type Error = String;
    type Path = MockPath;

    fn path(&self, path: &str) -> Self::Path {
        MockPath {
            fs: Arc::clone(&self.nodes),
            raw_path: path.to_string(),
        }
    }

    fn effective_targets(&self, entry: &GameEntry) -> Result<Vec<SaveTarget>, Self::Error> {
        Ok(entry.common().save_targets.clone())
    }

    fn effective_targets_for_rom(
        &self,
        emu: &Emulator,
        rom: &str,
    ) -> Result<Vec<SaveTarget>, Self::Error> {
        let mut targets = Vec::new();
        if let Some((_, list)) = emu.save_targets_by_rom.iter().find(|(r, _)| r == rom) {
            targets.extend(list.clone());
        }
        if let Some((_, paths)) = emu.save_paths_by_rom.iter().find(|(r, _)| r == rom) {
            for p in paths {
                targets.push(SaveTarget {
                    path: p.clone(),
                    kind: SaveTargetType::File,
                    cloud_path: None,
                    excluded_folders: Vec::new(),
                });
            }
        }
        Ok(targets)
    }
}

// --- Mock Cloud Drive ---

#[derive(Clone, Default)]
pub struct MockDrive {
    pub folders: Arc<Mutex<HashMap<String, String>>>,
    pub files: Arc<Mutex<HashMap<String, Vec<RemoteFile>>>>,
    pub fail_find: Arc<AtomicBool>,
    pub fail_list: Arc<AtomicBool>,
    pub fail_ensure: Arc<AtomicBool>,
}

impl MockDrive {
    pub fn add_folder(&self, path: &[String], id: &str) {
        let key = path.join("/");
        self.folders.lock().unwrap().insert(key, id.to_string());
    }

    pub fn add_file(&self, folder_id: &str, file: RemoteFile) {
        self.files
            .lock()
            .unwrap()
            .entry(folder_id.to_string())
            .or_default()
            .push(file);
    }
}

impl CloudFileListing for MockDrive {
    type Error = String;

    async fn list_files(&self, _token: &str, folder: &str) -> Result<Vec<RemoteFile>, Self::Error> {
        if self.fail_list.load(Ordering::SeqCst) {
            return Err("listing failed".into());
        }
        let lock = self.files.lock().unwrap();
        Ok(lock.get(folder).cloned().unwrap_or_default())
    }

    async fn list_children(
        &self,
        _token: &str,
        folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        if self.fail_list.load(Ordering::SeqCst) {
            return Err("listing failed".into());
        }
        let lock = self.files.lock().unwrap();
        Ok(lock.get(folder).cloned().unwrap_or_default())
    }
}

impl CloudFolderStore for MockDrive {
    type Error = String;

    async fn ensure_path(&self, _token: &str, segments: &[String]) -> Result<String, Self::Error> {
        if self.fail_ensure.load(Ordering::SeqCst) {
            return Err("ensure_path failed".into());
        }
        let key = segments.join("/");
        let mut lock = self.folders.lock().unwrap();
        if let Some(id) = lock.get(&key) {
            Ok(id.clone())
        } else {
            let id = format!("id_{key}");
            lock.insert(key, id.clone());
            Ok(id)
        }
    }

    async fn find_path(
        &self,
        _token: &str,
        segments: &[String],
    ) -> Result<Option<String>, Self::Error> {
        if self.fail_find.load(Ordering::SeqCst) {
            return Err("folder lookup failed".into());
        }
        let key = segments.join("/");
        let lock = self.folders.lock().unwrap();
        Ok(lock.get(&key).cloned())
    }

    async fn rename_folder(
        &self,
        _token: &str,
        _folder: &str,
        _name: &str,
        _add_parent: Option<&str>,
        _remove_parent: Option<&str>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn move_file(
        &self,
        _token: &str,
        _file: &str,
        _add_parent: &str,
        _remove_parent: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

// --- Mock Tracked Sync ---

#[derive(Clone, Default)]
pub struct MockTrackedSync {
    pub planned_decisions: Arc<Mutex<HashMap<String, Decision>>>,
    pub downloads: Arc<Mutex<Vec<String>>>,
    pub uploads: Arc<Mutex<Vec<String>>>,
    pub remembers: Arc<Mutex<Vec<String>>>,
    pub fail_upload_for: Arc<Mutex<Option<String>>>,
    pub fail_download_for: Arc<Mutex<Option<String>>>,
    pub fail_plan: Arc<AtomicBool>,
    /// When set, directory downloads create the file there, as a real restore does.
    pub materialize_in: Arc<Mutex<Option<MockFs>>>,
}

impl LocalSaveFiles for MockTrackedSync {
    type Error = String;

    async fn plan(
        &self,
        local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<Decision, Self::Error> {
        if self.fail_plan.load(Ordering::SeqCst) {
            return Err("history read failed".into());
        }
        let lock = self.planned_decisions.lock().unwrap();
        Ok(lock.get(local).cloned().unwrap_or(Decision::InSync))
    }

    async fn remember(
        &self,
        local: &str,
        _remote: &RemoteState,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        self.remembers.lock().unwrap().push(local.to_string());
        Ok(())
    }

    async fn download(
        &self,
        _token: &str,
        _remote: &RemoteState,
        local: &str,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        if let Some(fail) = self.fail_download_for.lock().unwrap().as_deref()
            && local == fail
        {
            return Err(format!("checksum mismatch / disk full: {local}"));
        }
        self.downloads.lock().unwrap().push(local.to_string());
        Ok(())
    }

    async fn download_in_directory(
        &self,
        download: DirectoryDownload<'_>,
    ) -> Result<(), Self::Error> {
        let path = format!("{}/{}", download.root, download.relative);
        if let Some(fail) = self.fail_download_for.lock().unwrap().as_deref()
            && path == fail
        {
            return Err(format!("checksum mismatch / disk full: {path}"));
        }
        if let Some(fs) = self.materialize_in.lock().unwrap().as_ref() {
            fs.add_file(&path, 1);
        }
        self.downloads.lock().unwrap().push(path);
        Ok(())
    }

    async fn upload(
        &self,
        _token: &str,
        local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        if let Some(fail) = self.fail_upload_for.lock().unwrap().as_deref()
            && local == fail
        {
            return Err(format!("Upload error for {local}"));
        }
        self.uploads.lock().unwrap().push(local.to_string());
        Ok(())
    }
}

// --- Mock Authorization ---

#[derive(Clone, Default)]
pub struct MockAuth {
    pub token: Arc<Mutex<Option<String>>>,
    pub fail_with_transport: Arc<AtomicBool>,
    pub fail_auth: Arc<AtomicBool>,
}

impl CloudAuthorization for MockAuth {
    type Error = String;

    async fn authorize(
        &self,
        _config: &AppConfig,
        _allow_interactive: bool,
    ) -> Result<String, Self::Error> {
        if self.fail_with_transport.load(Ordering::SeqCst) {
            return Err("Network unreachable (offline)".into());
        }
        if self.fail_auth.load(Ordering::SeqCst) {
            return Err("Authorization failed".into());
        }
        self.token
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "No token".into())
    }
}

// --- Mock Launcher ---

pub type LaunchHook = Box<dyn Fn() + Send + Sync>;

#[derive(Clone, Default)]
pub struct MockLauncher {
    pub launched_games: Arc<Mutex<Vec<String>>>,
    pub on_launch_hook: Arc<Mutex<Option<LaunchHook>>>,
    pub exit_code: Arc<Mutex<i32>>,
}

impl GameLauncher for MockLauncher {
    type Error = String;

    async fn launch(&self, request: GameLaunchRequest) -> Result<GameResult, Self::Error> {
        self.launched_games
            .lock()
            .unwrap()
            .push(request.entry.common().name.clone());
        if let Some(hook) = self.on_launch_hook.lock().unwrap().as_ref() {
            hook();
        }
        let code = *self.exit_code.lock().unwrap();
        Ok(GameResult {
            exit_code: code,
            duration_ms: 50,
        })
    }
}

// --- Mock Setup ---

pub type SetupHandler =
    Box<dyn Fn(SaveSetupRequest) -> Result<SaveSetupResult, String> + Send + Sync>;

#[derive(Clone, Default)]
pub struct MockSetup {
    pub handler: Arc<Mutex<Option<SetupHandler>>>,
}

impl SaveSetup for MockSetup {
    type Error = String;

    async fn request(&self, request: SaveSetupRequest) -> Result<SaveSetupResult, Self::Error> {
        if let Some(ref handler) = *self.handler.lock().unwrap() {
            return handler(request);
        }
        let targets = request.game.save_targets.clone();
        Ok(SaveSetupResult {
            game: request.game,
            targets,
        })
    }
}

#[derive(Clone, Default)]
pub struct CancelSetup;

impl SaveSetup for CancelSetup {
    type Error = String;

    async fn request(&self, request: SaveSetupRequest) -> Result<SaveSetupResult, Self::Error> {
        Ok(SaveSetupResult {
            game: request.game,
            targets: Vec::new(),
        })
    }
}

// --- Test Helpers ---

pub fn sample_native_game(name: &str, save_path: &str) -> SyncGame {
    let target = SaveTarget {
        path: save_path.to_string(),
        kind: SaveTargetType::File,
        cloud_path: None,
        excluded_folders: Vec::new(),
    };
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/games/{name}"),
            save_targets: vec![target.clone()],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });
    SyncGame {
        name: name.to_string(),
        entry,
        rom_path: None,
        effective_title: name.to_string(),
        save_targets: vec![target],
    }
}

pub fn sample_remote_file(id: &str, name: &str, modified: i64) -> RemoteFile {
    RemoteFile {
        id: id.to_string(),
        name: name.to_string(),
        modified: modified as i128,
        is_folder: false,
        sha256: None,
        version: None,
    }
}

pub fn sample_app_config() -> AppConfig {
    AppConfig {
        language: Default::default(),
        credentials: Some(Credentials {
            client_id: "client_id".into(),
            client_secret: "client_secret".into(),
            refresh_token: Some("refresh_token".into()),
        }),
        entries: Vec::new(),
    }
}

pub fn create_test_orchestrator(
    files: MockFs,
    drive: MockDrive,
    tracked: MockTrackedSync,
    auth: MockAuth,
    launcher: MockLauncher,
    config: AppConfig,
    on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
) -> SyncOrchestrator<MockFs, MockDrive, MockDrive, MockTrackedSync, MockAuth, MockLauncher> {
    let services = SyncServices {
        files,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: launcher,
    };
    let factory = SyncOrchestratorFactory::new(services);
    factory.create(config, on_status)
}
