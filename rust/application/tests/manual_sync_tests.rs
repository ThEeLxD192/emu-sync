mod common;

use emusync_application::{
    ManualEntrySync, ManualFileSync, PlannedSave, SyncContext, SyncServices,
};
use emusync_domain::config::{AppConfig, CloudAuthorization, Credentials};
use emusync_domain::execution::{GameLaunchRequest, GameLauncher, GameResult};
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, DirectoryDownload, LocalSaveAccess,
    LocalSaveFiles, LocalSavePath, RemoteFile, RemoteState, SyncEvent,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct DummyPath(String);

impl LocalSavePath for DummyPath {
    type Error = String;
    fn path(&self) -> &str {
        &self.0
    }
    fn name(&self) -> &str {
        &self.0
    }
    fn absolute_path(&self) -> Result<String, Self::Error> {
        Ok(self.0.clone())
    }
    fn normalized_path(&self) -> Result<String, Self::Error> {
        Ok(self.0.clone())
    }
    fn ancestors(&self) -> Result<Vec<String>, Self::Error> {
        Ok(Vec::new())
    }
    fn is_file(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn is_directory(&self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn exists(&self) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn last_modified_ms(&self) -> Result<i64, Self::Error> {
        Ok(1000)
    }
    fn resolve(&self, relative: &str) -> Result<Self, Self::Error> {
        Ok(DummyPath(format!("{}/{}", self.0, relative)))
    }
    fn resolve_remote(&self, name: &str) -> Result<Option<Self>, Self::Error> {
        Ok(Some(DummyPath(format!("{}/{}", self.0, name))))
    }
    fn walk_top_down(&self) -> Result<Vec<Self>, Self::Error> {
        Ok(Vec::new())
    }
    fn relative_to(&self, _root: &Self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}

#[derive(Clone, Default)]
struct DummyAccess;

impl LocalSaveAccess for DummyAccess {
    type Error = String;
    type Path = DummyPath;

    fn path(&self, path: &str) -> Self::Path {
        DummyPath(path.to_string())
    }
    fn effective_targets(&self, entry: &GameEntry) -> Result<Vec<SaveTarget>, Self::Error> {
        Ok(entry.common().save_targets.clone())
    }
    fn effective_targets_for_rom(
        &self,
        emu: &Emulator,
        _rom: &str,
    ) -> Result<Vec<SaveTarget>, Self::Error> {
        Ok(emu.common.save_targets.clone())
    }
}

#[derive(Clone, Default)]
struct MockListing {
    files: Arc<Mutex<Vec<RemoteFile>>>,
}

impl CloudFileListing for MockListing {
    type Error = String;
    async fn list_files(
        &self,
        _token: &str,
        _folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        Ok(self.files.lock().unwrap().clone())
    }
    async fn list_children(
        &self,
        _token: &str,
        _folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        Ok(self.files.lock().unwrap().clone())
    }
}

#[derive(Clone, Default)]
struct MockFolders {
    ensured: Arc<Mutex<Vec<Vec<String>>>>,
}

impl CloudFolderStore for MockFolders {
    type Error = String;
    async fn ensure_path(&self, _token: &str, segments: &[String]) -> Result<String, Self::Error> {
        self.ensured.lock().unwrap().push(segments.to_vec());
        Ok("folder_123".into())
    }
    async fn find_path(
        &self,
        _token: &str,
        _segments: &[String],
    ) -> Result<Option<String>, Self::Error> {
        Ok(Some("folder_123".into()))
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

#[derive(Clone, Default)]
struct MockTracked {
    downloads: Arc<Mutex<Vec<String>>>,
    uploads: Arc<Mutex<Vec<String>>>,
    remembers: Arc<Mutex<Vec<String>>>,
}

impl LocalSaveFiles for MockTracked {
    type Error = String;

    async fn plan(
        &self,
        _local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<Decision, Self::Error> {
        Ok(Decision::UploadLocal)
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
        self.uploads.lock().unwrap().push(local.to_string());
        Ok(())
    }

    async fn download_in_directory(&self, req: DirectoryDownload<'_>) -> Result<(), Self::Error> {
        self.downloads.lock().unwrap().push(req.local.to_string());
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MockAuth {
    token: Arc<Mutex<Option<String>>>,
    should_fail: Arc<AtomicBool>,
}

impl CloudAuthorization for MockAuth {
    type Error = String;
    async fn authorize(
        &self,
        _config: &AppConfig,
        _interactive: bool,
    ) -> Result<String, Self::Error> {
        if self.should_fail.load(Ordering::SeqCst) {
            return Err("Token expired".into());
        }
        self.token
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "No token".into())
    }
}

#[derive(Clone, Default)]
struct DummyLauncher;

impl GameLauncher for DummyLauncher {
    type Error = String;
    async fn launch(&self, _request: GameLaunchRequest) -> Result<GameResult, Self::Error> {
        Ok(GameResult {
            exit_code: 0,
            duration_ms: 0,
        })
    }
}

type TestContext =
    SyncContext<DummyAccess, MockListing, MockFolders, MockTracked, MockAuth, DummyLauncher>;

struct TestHarness {
    ctx: TestContext,
    uploads: Arc<Mutex<Vec<String>>>,
    #[allow(dead_code)]
    downloads: Arc<Mutex<Vec<String>>>,
    auth_fail: Arc<AtomicBool>,
}

fn make_context(
    credentials: Option<Credentials>,
    on_event: impl Fn(SyncEvent) + Send + Sync + 'static,
) -> TestHarness {
    let auth = MockAuth {
        token: Arc::new(Mutex::new(Some("mock_token".into()))),
        should_fail: Arc::new(AtomicBool::new(false)),
    };
    let auth_fail = Arc::clone(&auth.should_fail);
    let tracked = MockTracked::default();
    let uploads = Arc::clone(&tracked.uploads);
    let downloads = Arc::clone(&tracked.downloads);

    let services = SyncServices {
        files: DummyAccess,
        search: MockListing::default(),
        folders: MockFolders::default(),
        tracked_sync: tracked,
        authorization: auth,
        runner: DummyLauncher,
    };

    let config = AppConfig {
        language: Default::default(),
        credentials,
        entries: Vec::new(),
    };

    let ctx = SyncContext::new(config, services, on_event);
    TestHarness {
        ctx,
        uploads,
        downloads,
        auth_fail,
    }
}

fn sample_native(name: &str, save_path: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/games/{name}"),
            save_paths: vec![save_path.to_string()],
            save_targets: vec![SaveTarget {
                path: save_path.to_string(),
                kind: SaveTargetType::File,
                cloud_path: None,
                excluded_folders: Vec::new(),
            }],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    })
}

#[tokio::test]
async fn sync_entry_without_drive_connection_returns_false_and_emits_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let harness = make_context(None, move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let sync = ManualEntrySync::new(&harness.ctx);
    let entry = sample_native("TestGame", "/saves/test.sav");

    let res = sync.sync_entry(&entry).await;
    assert_eq!(res, Ok(false));

    let evs = events.lock().unwrap().clone();
    assert!(evs.iter().any(|e| matches!(
        e,
        SyncEvent::Error(msg) if msg.contains("Google Drive is not connected")
    )));
}

#[tokio::test]
async fn sync_entry_auth_failure_returns_false_and_emits_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let creds = Credentials {
        client_id: "c".into(),
        client_secret: "s".into(),
        refresh_token: Some("rt".into()),
    };

    let harness = make_context(Some(creds), move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    harness.auth_fail.store(true, Ordering::SeqCst);

    let sync = ManualEntrySync::new(&harness.ctx);
    let entry = sample_native("TestGame", "/saves/test.sav");

    let res = sync.sync_entry(&entry).await;
    assert_eq!(res, Ok(false));

    let evs = events.lock().unwrap().clone();
    assert!(evs.iter().any(|e| matches!(
        e,
        SyncEvent::Error(msg) if msg.contains("Could not connect to Google Drive")
    )));
}

#[tokio::test]
async fn sync_entry_happy_path_syncs_targets_and_returns_true() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let creds = Credentials {
        client_id: "c".into(),
        client_secret: "s".into(),
        refresh_token: Some("rt".into()),
    };

    let harness = make_context(Some(creds), move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let sync = ManualEntrySync::new(&harness.ctx);
    let entry = sample_native("Metroid", "/saves/metroid.sav");

    let res = sync.sync_entry(&entry).await;
    assert_eq!(res, Ok(true));

    // Upload was executed
    assert_eq!(*harness.uploads.lock().unwrap(), vec!["/saves/metroid.sav"]);

    // Event sequence finishes in Idle
    let evs = events.lock().unwrap().clone();
    assert_eq!(evs.last(), Some(&SyncEvent::Idle));
}

#[tokio::test]
async fn apply_file_decision_rejects_unresolved_conflict() {
    let harness = make_context(None, |_| {});
    let file_sync = ManualFileSync::new(&harness.ctx);

    let planned = PlannedSave {
        local: DummyPath("/saves/file.sav".into()),
        remote: None,
        folder_id: "fid".into(),
        drive_name: "file.sav".into(),
        directory: None,
    };

    let res = file_sync
        .apply_file_decision("tok", &planned, Decision::Conflict)
        .await;

    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err(),
        "A conflict must be resolved before applying a save"
    );
}

#[tokio::test]
async fn directory_sync_mixed_upload_and_download_decisions_prompts_single_conflict_and_respects_side_existence()
 {
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "DirGame".to_string(),
            executable_path: "/games/dirgame".to_string(),
            save_paths: vec!["/saves/dir".to_string()],
            save_targets: vec![SaveTarget {
                path: "/saves/dir".to_string(),
                kind: SaveTargetType::Directory,
                cloud_path: None,
                excluded_folders: Vec::new(),
            }],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });

    // Subcase A: User resolves conflict with DownloadCloud
    {
        let fs = common::MockFs::default();
        fs.add_dir("/saves/dir");
        fs.add_file("/saves/dir/local_only.sav", 1000);
        fs.add_file("/saves/dir/in_sync.sav", 1000);

        let drive = common::MockDrive::default();
        drive.add_folder(&["DirGame".into()], "f_dir");
        drive.add_file(
            "f_dir",
            common::sample_remote_file("rf1", "cloud_only.sav", 2000),
        );
        drive.add_file(
            "f_dir",
            common::sample_remote_file("rf2", "in_sync.sav", 1000),
        );

        let tracked = common::MockTrackedSync::default();
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/dir/local_only.sav".into(), Decision::UploadLocal);
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/dir/cloud_only.sav".into(), Decision::DownloadCloud);
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/dir/in_sync.sav".into(), Decision::InSync);

        let (conflict_tx, conflict_rx) = tokio::sync::oneshot::channel();
        let conflict_tx = Arc::new(Mutex::new(Some(conflict_tx)));
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let auth = common::MockAuth::default();
        *auth.token.lock().unwrap() = Some("valid_token".into());

        let services = SyncServices {
            files: fs,
            search: drive.clone(),
            folders: drive,
            tracked_sync: tracked.clone(),
            authorization: auth,
            runner: common::MockLauncher::default(),
        };

        let mut cfg = common::sample_app_config();
        cfg.entries = vec![entry.clone()];

        let ctx = Arc::new(SyncContext::new(cfg, services, move |ev| {
            events_clone.lock().unwrap().push(ev.clone());
            if let SyncEvent::Conflict(req) = ev
                && let Some(tx) = conflict_tx.lock().unwrap().take()
            {
                let _ = tx.send(req);
            }
        }));

        let ctx_clone = Arc::clone(&ctx);
        let entry_clone = entry.clone();
        let handle = tokio::spawn(async move {
            let sync = ManualEntrySync::new(&ctx_clone);
            sync.sync_entry(&entry_clone).await
        });

        let conflict = conflict_rx.await.expect("conflict must be emitted");
        assert_eq!(conflict.game_name, "DirGame");

        // User chooses DownloadCloud
        ctx.resolve_conflict(&conflict, Decision::DownloadCloud);

        let res = handle.await.unwrap();
        assert_eq!(res, Ok(true));

        // Subcase A post-state verification:
        // Only cloud_only.sav was downloaded via download_in_directory
        assert_eq!(
            *tracked.downloads.lock().unwrap(),
            vec!["/saves/dir/cloud_only.sav"]
        );
        // local_only.sav was NOT uploaded because user chose DownloadCloud
        assert!(tracked.uploads.lock().unwrap().is_empty());
        // in_sync.sav was remembered
        assert_eq!(
            *tracked.remembers.lock().unwrap(),
            vec!["/saves/dir/in_sync.sav"]
        );
        // Verify event lifecycle finished in Idle
        let evs = events.lock().unwrap().clone();
        assert_eq!(evs.last(), Some(&SyncEvent::Idle));
    }

    // Subcase B: User resolves conflict with UploadLocal
    {
        let fs = common::MockFs::default();
        fs.add_dir("/saves/dir");
        fs.add_file("/saves/dir/local_only.sav", 1000);
        fs.add_file("/saves/dir/in_sync.sav", 1000);

        let drive = common::MockDrive::default();
        drive.add_folder(&["DirGame".into()], "f_dir");
        drive.add_file(
            "f_dir",
            common::sample_remote_file("rf1", "cloud_only.sav", 2000),
        );
        drive.add_file(
            "f_dir",
            common::sample_remote_file("rf2", "in_sync.sav", 1000),
        );

        let tracked = common::MockTrackedSync::default();
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/dir/local_only.sav".into(), Decision::UploadLocal);
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/dir/cloud_only.sav".into(), Decision::DownloadCloud);
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/dir/in_sync.sav".into(), Decision::InSync);

        let (conflict_tx, conflict_rx) = tokio::sync::oneshot::channel();
        let conflict_tx = Arc::new(Mutex::new(Some(conflict_tx)));
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let auth = common::MockAuth::default();
        *auth.token.lock().unwrap() = Some("valid_token".into());

        let services = SyncServices {
            files: fs,
            search: drive.clone(),
            folders: drive,
            tracked_sync: tracked.clone(),
            authorization: auth,
            runner: common::MockLauncher::default(),
        };

        let mut cfg = common::sample_app_config();
        cfg.entries = vec![entry.clone()];

        let ctx = Arc::new(SyncContext::new(cfg, services, move |ev| {
            events_clone.lock().unwrap().push(ev.clone());
            if let SyncEvent::Conflict(req) = ev
                && let Some(tx) = conflict_tx.lock().unwrap().take()
            {
                let _ = tx.send(req);
            }
        }));

        let ctx_clone = Arc::clone(&ctx);
        let entry_clone = entry.clone();
        let handle = tokio::spawn(async move {
            let sync = ManualEntrySync::new(&ctx_clone);
            sync.sync_entry(&entry_clone).await
        });

        let conflict = conflict_rx.await.expect("conflict must be emitted");
        assert_eq!(conflict.game_name, "DirGame");

        // User chooses UploadLocal
        ctx.resolve_conflict(&conflict, Decision::UploadLocal);

        let res = handle.await.unwrap();
        assert_eq!(res, Ok(true));

        // Subcase B post-state verification:
        // Only local_only.sav was uploaded
        assert_eq!(
            *tracked.uploads.lock().unwrap(),
            vec!["/saves/dir/local_only.sav"]
        );
        // cloud_only.sav was NOT downloaded because user chose UploadLocal
        assert!(tracked.downloads.lock().unwrap().is_empty());
        // in_sync.sav was remembered
        assert_eq!(
            *tracked.remembers.lock().unwrap(),
            vec!["/saves/dir/in_sync.sav"]
        );
        // Verify event lifecycle finished in Idle
        let evs = events.lock().unwrap().clone();
        assert_eq!(evs.last(), Some(&SyncEvent::Idle));
    }
}
