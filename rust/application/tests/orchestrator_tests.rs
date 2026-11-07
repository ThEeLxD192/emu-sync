use emusync_application::{SyncOrchestratorFactory, SyncServices};
use emusync_domain::config::{AppConfig, CloudAuthorization, Credentials};
use emusync_domain::execution::{GameLaunchRequest, GameLauncher, GameResult};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, EntrySyncStatus, LocalSaveAccess, LocalSaveFiles,
    LocalSavePath, RemoteFile, RemoteState, SaveSetup, SaveSetupRequest, SaveSetupResult,
    SaveSynchronizer, SaveSynchronizerFactory, SyncEvent, SyncGame,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// --- Mock Filesystem ---

#[derive(Clone, Debug, PartialEq)]
enum MockNode {
    File { modified_ms: i64 },
    Directory,
}

#[derive(Clone, Default)]
struct MockFs {
    nodes: Arc<Mutex<HashMap<String, MockNode>>>,
}

impl MockFs {
    fn add_file(&self, path: &str, modified_ms: i64) {
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

    #[allow(dead_code)]
    fn add_dir(&self, path: &str) {
        let mut lock = self.nodes.lock().unwrap();
        lock.insert(normalize(path), MockNode::Directory);
    }

    fn touch_file(&self, path: &str, new_time: i64) {
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

fn normalize(p: &str) -> String {
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
struct MockPath {
    fs: Arc<Mutex<HashMap<String, MockNode>>>,
    raw_path: String,
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
        _entry: &emusync_domain::library::Emulator,
        _rom: &str,
    ) -> Result<Vec<SaveTarget>, Self::Error> {
        Ok(Vec::new())
    }
}

// --- Mock Cloud Drive ---

#[derive(Clone, Default)]
struct MockDrive {
    folders: Arc<Mutex<HashMap<String, String>>>,
    files: Arc<Mutex<HashMap<String, Vec<RemoteFile>>>>,
    fail_find: Arc<AtomicBool>,
    fail_list: Arc<AtomicBool>,
}

impl MockDrive {
    fn add_folder(&self, path: &[String], id: &str) {
        let key = path.join("/");
        self.folders.lock().unwrap().insert(key, id.to_string());
    }

    fn add_file(&self, folder_id: &str, file: RemoteFile) {
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
        let lock = self.files.lock().unwrap();
        Ok(lock.get(folder).cloned().unwrap_or_default())
    }
}

impl CloudFolderStore for MockDrive {
    type Error = String;

    async fn ensure_path(&self, _token: &str, segments: &[String]) -> Result<String, Self::Error> {
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
struct MockTrackedSync {
    planned_decisions: Arc<Mutex<HashMap<String, Decision>>>,
    downloads: Arc<Mutex<Vec<String>>>,
    uploads: Arc<Mutex<Vec<String>>>,
    remembers: Arc<Mutex<Vec<String>>>,
    fail_upload_for: Arc<Mutex<Option<String>>>,
    fail_plan: Arc<AtomicBool>,
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
        self.downloads.lock().unwrap().push(local.to_string());
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
struct MockAuth {
    token: Arc<Mutex<Option<String>>>,
    fail_with_transport: Arc<AtomicBool>,
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
        self.token
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "No token".into())
    }
}

// --- Mock Launcher ---

type LaunchHook = Box<dyn Fn() + Send + Sync>;

#[derive(Clone, Default)]
struct MockLauncher {
    launched_games: Arc<Mutex<Vec<String>>>,
    on_launch_hook: Arc<Mutex<Option<LaunchHook>>>,
    exit_code: Arc<Mutex<i32>>,
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

struct MockSetup;

impl SaveSetup for MockSetup {
    type Error = String;

    async fn request(&self, request: SaveSetupRequest) -> Result<SaveSetupResult, Self::Error> {
        let targets = request.game.save_targets.clone();
        Ok(SaveSetupResult {
            game: request.game,
            targets,
        })
    }
}

fn sample_native_game(name: &str, save_path: &str) -> SyncGame {
    let target = SaveTarget {
        path: save_path.to_string(),
        kind: SaveTargetType::File,
        cloud_path: None,
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

#[tokio::test]
async fn status_reports_errors_instead_of_claiming_unverified_saves_are_in_sync() {
    let files = MockFs::default();
    files.add_file("/saves/game.sav", 1000);
    let drive = MockDrive::default();
    drive.add_folder(&["Game".into()], "folder");
    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("synthetic-token".into());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    let factory = SyncOrchestratorFactory::new(SyncServices {
        files,
        search: drive.clone(),
        folders: drive.clone(),
        tracked_sync: tracked.clone(),
        authorization: auth,
        runner: MockLauncher::default(),
    });
    let sync = factory.create(config.clone(), |_| {});
    let entry = sample_native_game("Game", "/saves/game.sav").entry;
    assert_eq!(sync.check_entry(&entry).await, Ok(EntrySyncStatus::InSync));

    drive.fail_find.store(true, Ordering::SeqCst);
    assert_eq!(sync.check_entry(&entry).await, Ok(EntrySyncStatus::Error));
    drive.fail_find.store(false, Ordering::SeqCst);
    drive.fail_list.store(true, Ordering::SeqCst);
    assert_eq!(sync.check_entry(&entry).await, Ok(EntrySyncStatus::Error));
    drive.fail_list.store(false, Ordering::SeqCst);
    tracked.fail_plan.store(true, Ordering::SeqCst);
    assert_eq!(sync.check_entry(&entry).await, Ok(EntrySyncStatus::Error));
    tracked.fail_plan.store(false, Ordering::SeqCst);

    let duplicate = RemoteFile {
        id: "first".into(),
        name: "game.sav".into(),
        modified: 0,
        is_folder: false,
        sha256: None,
        version: None,
    };
    drive.add_file("folder", duplicate.clone());
    drive.add_file(
        "folder",
        RemoteFile {
            id: "second".into(),
            ..duplicate
        },
    );
    assert_eq!(sync.check_entry(&entry).await, Ok(EntrySyncStatus::Error));

    let blank = AppConfig {
        credentials: Some(Credentials {
            refresh_token: Some("  ".into()),
            ..config.credentials.unwrap()
        }),
        entries: vec![],
    };
    assert_eq!(
        factory.create(blank, |_| {}).check_entry(&entry).await,
        Ok(EntrySyncStatus::NotConfigured)
    );
}

#[tokio::test]
async fn multiple_directory_targets_sharing_folder_evaluates_to_in_sync() {
    let files = MockFs::default();
    files.add_dir("/game/saves/savedata");
    files.add_file("/game/saves/savedata/save1.dat", 1000);
    files.add_dir("/game/saves/trophies");
    files.add_file("/game/saves/trophies/trophy.dat", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["DualDirectoryGame".into()], "f_dual");
    drive.add_file(
        "f_dual",
        RemoteFile {
            id: "id_save1".into(),
            name: "save1.dat".into(),
            modified: 1000,
            is_folder: false,
            sha256: Some("sha_save1".into()),
            version: None,
        },
    );
    drive.add_file(
        "f_dual",
        RemoteFile {
            id: "id_trophy".into(),
            name: "trophy.dat".into(),
            modified: 1000,
            is_folder: false,
            sha256: Some("sha_trophy".into()),
            version: None,
        },
    );

    let tracked = MockTrackedSync::default();
    // If files are planned against the wrong sibling directory, return DownloadCloud
    tracked.planned_decisions.lock().unwrap().insert(
        "/game/saves/savedata/trophy.dat".into(),
        Decision::DownloadCloud,
    );
    tracked.planned_decisions.lock().unwrap().insert(
        "/game/saves/trophies/save1.dat".into(),
        Decision::DownloadCloud,
    );

    let target1 = SaveTarget {
        path: "/game/saves/savedata".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    };
    let target2 = SaveTarget {
        path: "/game/saves/trophies".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
    };

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "DualDirectoryGame".into(),
            executable_path: "/games/app".into(),
            save_targets: vec![target1, target2],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![entry.clone()],
    };

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let factory = SyncOrchestratorFactory::new(SyncServices {
        files,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: MockLauncher::default(),
    });
    let sync = factory.create(config, |_| {});
    assert_eq!(sync.check_entry(&entry).await, Ok(EntrySyncStatus::InSync));
}

#[tokio::test]
async fn test_full_lifecycle_pre_sync_launch_post_sync() {
    let fs = MockFs::default();
    fs.add_file("/saves/metroid.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["Metroid".into()], "f_metroid");
    drive.add_file(
        "f_metroid",
        RemoteFile {
            id: "rf1".into(),
            name: "metroid.sav".into(),
            modified: 2_000_000_000,
            is_folder: false,
            sha256: None,
            version: None,
        },
    );

    let tracked = MockTrackedSync::default();
    // Pre-sync plan: DownloadCloud
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/metroid.sav".into(), Decision::DownloadCloud);

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let launcher = MockLauncher::default();
    let tracked_clone = tracked.clone();
    *launcher.on_launch_hook.lock().unwrap() = Some(Box::new(move || {
        tracked_clone
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/conflict.sav".into(), Decision::InSync);
    }));
    // Simulate game modifying the file during play
    let fs_clone = fs.clone();
    let tracked_clone = tracked.clone();
    *launcher.on_launch_hook.lock().unwrap() = Some(Box::new(move || {
        fs_clone.touch_file("/saves/metroid.sav", 3000);
        tracked_clone
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/metroid.sav".into(), Decision::UploadLocal);
    }));

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs.clone(),
        search: drive.clone(),
        folders: drive.clone(),
        tracked_sync: tracked.clone(),
        authorization: auth.clone(),
        runner: launcher.clone(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let game = sample_native_game("Metroid", "/saves/metroid.sav");
    let res = synchronizer.play_with_sync(game, &MockSetup).await;
    assert_eq!(res, Ok(()));

    // Assertions:
    // 1. Pre-sync downloaded remote file
    assert_eq!(
        *tracked.downloads.lock().unwrap(),
        vec!["/saves/metroid.sav"]
    );
    // 2. Game was launched
    assert_eq!(*launcher.launched_games.lock().unwrap(), vec!["Metroid"]);
    // 3. Post-sync uploaded modified local file
    assert_eq!(*tracked.uploads.lock().unwrap(), vec!["/saves/metroid.sav"]);
    // 4. Final state is Idle
    let recorded = events.lock().unwrap().clone();
    assert!(recorded.contains(&SyncEvent::Idle));
}

#[tokio::test]
async fn test_offline_play_on_transport_failure() {
    let fs = MockFs::default();
    fs.add_file("/saves/offline.sav", 1000);

    let drive = MockDrive::default();
    let tracked = MockTrackedSync::default();
    let launcher = MockLauncher::default();
    let tracked_clone = tracked.clone();
    *launcher.on_launch_hook.lock().unwrap() = Some(Box::new(move || {
        tracked_clone
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/conflict.sav".into(), Decision::InSync);
    }));

    let auth = MockAuth::default();
    // Simulate transport error
    auth.fail_with_transport.store(true, Ordering::SeqCst);

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: launcher.clone(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, |_| {});

    let game = sample_native_game("OfflineGame", "/saves/offline.sav");
    let res = synchronizer.play_with_sync(game, &MockSetup).await;

    // Must succeed and allow playing offline!
    assert_eq!(res, Ok(()));
    assert_eq!(
        *launcher.launched_games.lock().unwrap(),
        vec!["OfflineGame"]
    );
}

#[tokio::test]
async fn test_pre_sync_mapping_error_aborts_launch() {
    let fs = MockFs::default();
    let drive = MockDrive::default();
    let tracked = MockTrackedSync::default();
    let launcher = MockLauncher::default();
    let tracked_clone = tracked.clone();
    *launcher.on_launch_hook.lock().unwrap() = Some(Box::new(move || {
        tracked_clone
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/conflict.sav".into(), Decision::InSync);
    }));
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: launcher.clone(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    // Target with illegal cloud traversal path
    let mut game = sample_native_game("TraversalGame", "/saves/game.sav");
    game.save_targets[0].cloud_path = Some("../escaped".into());

    let res = synchronizer.play_with_sync(game, &MockSetup).await;
    assert_eq!(res, Ok(()));

    // Must NOT launch the game
    assert!(launcher.launched_games.lock().unwrap().is_empty());

    // Must emit SyncEvent::Error describing the mapping failure
    let recorded = events.lock().unwrap().clone();
    assert!(
        recorded
            .iter()
            .any(|e| matches!(e, SyncEvent::Error(msg) if msg.contains("escaped")))
    );
}

#[tokio::test]
async fn test_conflict_resolution_by_matching_request_id() {
    let fs = MockFs::default();
    fs.add_file("/saves/conflict.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["ConflictGame".into()], "f_conflict");
    drive.add_file(
        "f_conflict",
        RemoteFile {
            id: "rf1".into(),
            name: "conflict.sav".into(),
            modified: 2_000_000_000,
            is_folder: false,
            sha256: None,
            version: None,
        },
    );

    let tracked = MockTrackedSync::default();
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/conflict.sav".into(), Decision::Conflict);

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    let tracked_clone = tracked.clone();
    *launcher.on_launch_hook.lock().unwrap() = Some(Box::new(move || {
        tracked_clone
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/conflict.sav".into(), Decision::InSync);
    }));

    let conflict_tx = Arc::new(Mutex::new(None));
    let conflict_tx_clone = Arc::clone(&conflict_tx);

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked.clone(),
        authorization: auth,
        runner: launcher,
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = Arc::new(factory.create(config, move |ev| {
        if let SyncEvent::Conflict(req) = ev {
            *conflict_tx_clone.lock().unwrap() = Some(req);
        }
    }));

    let sync_clone = Arc::clone(&synchronizer);
    let play_task = tokio::spawn(async move {
        let game = sample_native_game("ConflictGame", "/saves/conflict.sav");
        sync_clone.play_with_sync(game, &MockSetup).await
    });

    // Wait until conflict is emitted
    let mut req = None;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if let Some(r) = conflict_tx.lock().unwrap().clone() {
            req = Some(r);
            break;
        }
    }
    let conflict_req = req.expect("Conflict should be emitted");

    // Resolve with matching RequestId
    synchronizer.resolve_conflict(&conflict_req, Decision::UploadLocal);

    let res = play_task.await.unwrap();
    assert_eq!(res, Ok(()));
}

#[tokio::test]
async fn test_post_sync_failure_and_retry_by_request_id() {
    let fs = MockFs::default();
    fs.add_file("/saves/retry.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["RetryGame".into()], "f_retry");

    let tracked = MockTrackedSync::default();
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/retry.sav".into(), Decision::UploadLocal);
    // Simulate upload failing on first attempt
    *tracked.fail_upload_for.lock().unwrap() = Some("/saves/retry.sav".into());

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    let tracked_clone = tracked.clone();
    *launcher.on_launch_hook.lock().unwrap() = Some(Box::new(move || {
        tracked_clone
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/conflict.sav".into(), Decision::InSync);
    }));

    let failure_store = Arc::new(Mutex::new(None));
    let failure_store_clone = Arc::clone(&failure_store);

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked.clone(),
        authorization: auth,
        runner: launcher,
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = Arc::new(factory.create(config, move |ev| {
        if let SyncEvent::PostSyncFailed(f) = ev {
            *failure_store_clone.lock().unwrap() = Some(f);
        }
    }));

    let game = sample_native_game("RetryGame", "/saves/retry.sav");
    let res = synchronizer.play_with_sync(game, &MockSetup).await;
    assert_eq!(res, Ok(()));

    // PostSyncFailed event must have been emitted
    let failure = failure_store
        .lock()
        .unwrap()
        .clone()
        .expect("PostSyncFailed expected");
    assert_eq!(failure.game_name, "RetryGame");

    // Fix the error condition so retry can succeed
    *tracked.fail_upload_for.lock().unwrap() = None;

    // Retry with mismatched RequestId should do nothing
    let mut bad_failure = failure.clone();
    bad_failure.request_id = emusync_domain::sync::RequestId(999999);
    let bad_retry = synchronizer.retry_post_sync(&bad_failure).await;
    assert_eq!(bad_retry, Ok(()));
    assert!(tracked.uploads.lock().unwrap().is_empty());

    // Retry with correct RequestId succeeds and executes upload!
    let good_retry = synchronizer.retry_post_sync(&failure).await;
    assert_eq!(good_retry, Ok(()));
    assert_eq!(*tracked.uploads.lock().unwrap(), vec!["/saves/retry.sav"]);
}

#[derive(Clone, Default)]
struct CancelSetup;

impl SaveSetup for CancelSetup {
    type Error = String;

    async fn request(&self, request: SaveSetupRequest) -> Result<SaveSetupResult, Self::Error> {
        Ok(SaveSetupResult {
            game: request.game,
            targets: Vec::new(),
        })
    }
}

#[tokio::test]
async fn test_process_exit_code_failure_emits_error_and_does_not_upload() {
    let fs = MockFs::default();
    fs.add_file("/saves/crash.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["CrashGame".into()], "f_crash");

    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    *launcher.exit_code.lock().unwrap() = 1; // Process crashes / exits with non-zero

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked.clone(),
        authorization: auth,
        runner: launcher.clone(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let game = sample_native_game("CrashGame", "/saves/crash.sav");
    let res = synchronizer.play_with_sync(game, &MockSetup).await;
    assert_eq!(res, Ok(()));

    // Launched was recorded
    assert_eq!(*launcher.launched_games.lock().unwrap(), vec!["CrashGame"]);

    // No uploads attempted
    assert!(tracked.uploads.lock().unwrap().is_empty());

    // Error event was emitted
    let evs = events.lock().unwrap().clone();
    assert!(evs.iter().any(|ev| matches!(
        ev,
        SyncEvent::Error(msg) if msg.contains("CrashGame did not start")
    )));
}

#[tokio::test]
async fn test_play_without_drive_launches_directly_and_returns_idle() {
    let fs = MockFs::default();
    fs.add_file("/saves/local.sav", 1000);

    let drive = MockDrive::default();
    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    let launcher = MockLauncher::default();

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    // No credentials configured
    let config = AppConfig {
        credentials: None,
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked.clone(),
        authorization: auth,
        runner: launcher.clone(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let game = sample_native_game("LocalGame", "/saves/local.sav");
    let res = synchronizer.play_with_sync(game, &MockSetup).await;
    assert_eq!(res, Ok(()));

    assert_eq!(*launcher.launched_games.lock().unwrap(), vec!["LocalGame"]);
    assert!(tracked.uploads.lock().unwrap().is_empty());

    let evs = events.lock().unwrap().clone();
    assert_eq!(evs, vec![SyncEvent::Playing, SyncEvent::Idle]);
}

#[tokio::test]
async fn test_save_setup_cancellation_aborts_launch() {
    let fs = MockFs::default();
    let drive = MockDrive::default();
    drive.add_folder(&["UnconfiguredGame".into()], "f_unconf");
    drive.add_file(
        "f_unconf",
        RemoteFile {
            id: "rf_unconf".into(),
            name: "unconf.sav".into(),
            modified: 1000,
            is_folder: false,
            sha256: None,
            version: None,
        },
    );

    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: launcher.clone(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, |_| {});

    // Game has NO local targets
    let mut game = sample_native_game("UnconfiguredGame", "/saves/unconf.sav");
    game.save_targets.clear();

    // CancelSetup returns targets: Vec::new()
    let res = synchronizer.play_with_sync(game, &CancelSetup).await;
    assert_eq!(res, Ok(()));

    // Game should NOT have been launched
    assert!(launcher.launched_games.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_post_sync_directory_with_conflicting_file_emits_error() {
    let fs = MockFs::default();
    fs.add_file("/saves/dir/file.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["DirGame".into()], "f_dir");

    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: launcher,
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let mut game = sample_native_game("DirGame", "/saves/dir");
    game.save_targets = vec![
        SaveTarget {
            path: "/saves/dir".into(),
            kind: SaveTargetType::Directory,
            cloud_path: None,
        },
        SaveTarget {
            path: "/saves/other/file.sav".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
        },
    ];

    let res = synchronizer.play_with_sync(game, &MockSetup).await;
    assert_eq!(res, Ok(()));

    let evs = events.lock().unwrap().clone();
    assert!(evs.iter().any(|ev| matches!(
        ev,
        SyncEvent::Error(msg) if msg.contains("conflicts with another configured file target")
    )));
}

#[tokio::test]
async fn test_check_entry_sync_status_idle_out_of_sync_and_conflict() {
    let files = MockFs::default();
    files.add_file("/saves/idle_game.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["Game".into()], "folder");

    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("synthetic-token".into());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    let factory = SyncOrchestratorFactory::new(SyncServices {
        files: files.clone(),
        search: drive.clone(),
        folders: drive.clone(),
        tracked_sync: tracked.clone(),
        authorization: auth,
        runner: MockLauncher::default(),
    });
    let sync = factory.create(config, |_| {});

    // 1. Idle when game has empty targets
    let mut no_targets = sample_native_game("NoTargets", "/saves/idle_game.sav").entry;
    if let GameEntry::Native(ref mut n) = no_targets {
        n.common.save_targets.clear();
    }
    assert_eq!(
        sync.check_entry(&no_targets).await,
        Ok(EntrySyncStatus::Idle)
    );

    // 2. OutOfSync when decision is UploadLocal or DownloadCloud
    let entry = sample_native_game("Game", "/saves/idle_game.sav").entry;
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/idle_game.sav".into(), Decision::UploadLocal);
    assert_eq!(
        sync.check_entry(&entry).await,
        Ok(EntrySyncStatus::OutOfSync)
    );

    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/idle_game.sav".into(), Decision::DownloadCloud);
    assert_eq!(
        sync.check_entry(&entry).await,
        Ok(EntrySyncStatus::OutOfSync)
    );

    // 3. Conflict when decision is Conflict
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/idle_game.sav".into(), Decision::Conflict);
    assert_eq!(
        sync.check_entry(&entry).await,
        Ok(EntrySyncStatus::Conflict)
    );
}

#[tokio::test]
async fn test_orchestrator_delegates_sync_entry_and_renames() {
    let fs = MockFs::default();
    fs.add_file("/saves/game.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["Game".into()], "f_game");

    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "c".into(),
            client_secret: "s".into(),
            refresh_token: Some("rt".into()),
        }),
        entries: Vec::new(),
    };

    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive.clone(),
        tracked_sync: tracked,
        authorization: auth,
        runner: launcher,
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = factory.create(config, |_| {});

    let game = sample_native_game("Game", "/saves/game.sav");

    // 1. sync_entry through SaveSynchronizer
    let sync_res = synchronizer.sync_entry(&game.entry).await;
    assert_eq!(sync_res, Ok(true));

    // 2. rename_game through SaveSynchronizer
    let rename_res = synchronizer
        .rename_game(&game.entry, None, "Game", "NewGame", &game.save_targets)
        .await;
    assert_eq!(rename_res, Ok(true));

    // 3. rename_entry through SaveSynchronizer
    let new_game = sample_native_game("NewGame", "/saves/game.sav");
    let rename_entry_res = synchronizer
        .rename_entry(&game.entry, &new_game.entry)
        .await;
    assert_eq!(rename_entry_res, Ok(true));
}
