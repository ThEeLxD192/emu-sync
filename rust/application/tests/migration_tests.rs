use emusync_application::{CloudEntryMigration, SyncContext, SyncServices};
use emusync_domain::config::{AppConfig, CloudAuthorization, Credentials};
use emusync_domain::execution::{GameLaunchRequest, GameLauncher, GameResult};
use emusync_domain::library::{
    Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType,
};
use emusync_domain::sync::SyncMessage;
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, LocalSaveAccess, LocalSaveFiles, LocalSavePath,
    RemoteFile, RemoteState, SyncEvent,
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
        rom: &str,
    ) -> Result<Vec<SaveTarget>, Self::Error> {
        if let Some((_, list)) = emu.save_targets_by_rom.iter().find(|(r, _)| r == rom) {
            return Ok(list.clone());
        }
        Ok(emu.common.save_targets.clone())
    }
}

#[derive(Clone, Default)]
struct MockListing {
    files: Arc<Mutex<Vec<RemoteFile>>>,
    files_by_folder: Arc<Mutex<std::collections::HashMap<String, Vec<RemoteFile>>>>,
}

impl CloudFileListing for MockListing {
    type Error = String;
    async fn list_files(&self, _token: &str, folder: &str) -> Result<Vec<RemoteFile>, Self::Error> {
        let lock = self.files_by_folder.lock().unwrap();
        if let Some(list) = lock.get(folder) {
            return Ok(list.clone());
        }
        Ok(self.files.lock().unwrap().clone())
    }
    async fn list_children(
        &self,
        _token: &str,
        folder: &str,
    ) -> Result<Vec<RemoteFile>, Self::Error> {
        let lock = self.files_by_folder.lock().unwrap();
        if let Some(list) = lock.get(folder) {
            return Ok(list.clone());
        }
        Ok(self.files.lock().unwrap().clone())
    }
}

#[derive(Clone, Default)]
struct MockFolders {
    folders: Arc<Mutex<std::collections::HashMap<String, String>>>,
    renamed: Arc<Mutex<Vec<(String, String)>>>,
    move_log: Arc<Mutex<Vec<(String, String, String)>>>,
}

impl CloudFolderStore for MockFolders {
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
        let key = segments.join("/");
        Ok(self.folders.lock().unwrap().get(&key).cloned())
    }

    async fn rename_folder(
        &self,
        _token: &str,
        folder: &str,
        name: &str,
        _add_parent: Option<&str>,
        _remove_parent: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.renamed
            .lock()
            .unwrap()
            .push((folder.to_string(), name.to_string()));
        Ok(())
    }

    async fn move_file(
        &self,
        _token: &str,
        file: &str,
        add_parent: &str,
        remove_parent: &str,
    ) -> Result<(), Self::Error> {
        self.move_log.lock().unwrap().push((
            file.to_string(),
            remove_parent.to_string(),
            add_parent.to_string(),
        ));
        Ok(())
    }
}

#[derive(Clone, Default)]
struct DummyTracked;

impl LocalSaveFiles for DummyTracked {
    type Error = String;

    async fn plan(
        &self,
        _local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<Decision, Self::Error> {
        Ok(Decision::InSync)
    }

    async fn remember(
        &self,
        _local: &str,
        _remote: &RemoteState,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn download(
        &self,
        _token: &str,
        _remote: &RemoteState,
        _local: &str,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn upload(
        &self,
        _token: &str,
        _local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), Self::Error> {
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
            return Err("Authorization failed".into());
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
    SyncContext<DummyAccess, MockListing, MockFolders, DummyTracked, MockAuth, DummyLauncher>;

struct MigrationHarness {
    ctx: TestContext,
    folders: MockFolders,
    search: MockListing,
}

fn make_harness(
    entries: Vec<GameEntry>,
    credentials: Option<Credentials>,
    on_event: impl Fn(SyncEvent) + Send + Sync + 'static,
) -> MigrationHarness {
    let auth = MockAuth {
        token: Arc::new(Mutex::new(Some("mock_token".into()))),
        should_fail: Arc::new(AtomicBool::new(false)),
    };
    let folders = MockFolders::default();
    let search = MockListing::default();

    let services = SyncServices {
        files: DummyAccess,
        search: search.clone(),
        folders: folders.clone(),
        tracked_sync: DummyTracked,
        authorization: auth,
        runner: DummyLauncher,
    };

    let config = AppConfig {
        language: Default::default(),
        credentials,
        entries,
    };

    let ctx = SyncContext::new(config, services, on_event);
    MigrationHarness {
        ctx,
        folders,
        search,
    }
}

fn sample_native(name: &str, cloud_folder: Option<&str>) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: format!("/games/{name}"),
            cloud_folder: cloud_folder.map(str::to_string),
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    })
}

#[tokio::test]
async fn rename_entry_aborts_when_cloud_folder_is_shared() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let e1 = sample_native("GameA", Some("SharedFolder"));
    let e2 = sample_native("GameB", Some("SharedFolder"));

    let harness = make_harness(vec![e1.clone(), e2], None, move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let migration = CloudEntryMigration::new(&harness.ctx);

    let mut e1_renamed = e1.clone();
    if let GameEntry::Native(ref mut n) = e1_renamed {
        n.common.cloud_folder = Some("NewFolder".into());
    }

    let res = migration
        .rename_entry_cloud_data(&e1, &e1_renamed)
        .await
        .unwrap();

    // Must abort to protect shared saves
    assert!(!res);

    let evs = events.lock().unwrap().clone();
    assert!(
        evs.iter()
            .any(|e| matches!(e, SyncEvent::Error(SyncMessage::SharedCloudFolder { .. })))
    );
}

#[tokio::test]
async fn rename_entry_without_drive_connection_returns_true_without_migrating() {
    let e1 = sample_native("SoloGame", Some("OldFolder"));
    let harness = make_harness(vec![e1.clone()], None, |_| {});

    let migration = CloudEntryMigration::new(&harness.ctx);

    let mut e1_renamed = e1.clone();
    if let GameEntry::Native(ref mut n) = e1_renamed {
        n.common.cloud_folder = Some("NewFolder".into());
    }

    let res = migration
        .rename_entry_cloud_data(&e1, &e1_renamed)
        .await
        .unwrap();

    assert!(res);
    // No remote rename was called
    assert!(harness.folders.renamed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rename_entry_same_path_returns_true_immediately() {
    let e1 = sample_native("Game", Some("SameFolder"));
    let harness = make_harness(vec![e1.clone()], None, |_| {});

    let migration = CloudEntryMigration::new(&harness.ctx);
    let res = migration.rename_entry_cloud_data(&e1, &e1).await.unwrap();

    assert!(res);
    assert!(harness.folders.renamed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rename_entry_with_drive_connected_invokes_migrator() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let creds = Credentials {
        client_id: "c".into(),
        client_secret: "s".into(),
        refresh_token: Some("rt".into()),
    };

    let e1 = sample_native("Game", Some("OldFolder"));
    let harness = make_harness(vec![e1.clone()], Some(creds), move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    // Populate old folder in mock drive
    harness
        .folders
        .folders
        .lock()
        .unwrap()
        .insert("OldFolder".into(), "f_old".into());

    let migration = CloudEntryMigration::new(&harness.ctx);

    let mut e1_renamed = e1.clone();
    if let GameEntry::Native(ref mut n) = e1_renamed {
        n.common.cloud_folder = Some("NewFolder".into());
    }

    let res = migration
        .rename_entry_cloud_data(&e1, &e1_renamed)
        .await
        .unwrap();

    assert!(res);
    assert_eq!(
        *harness.folders.renamed.lock().unwrap(),
        vec![("f_old".to_string(), "NewFolder".to_string())]
    );

    let evs = events.lock().unwrap().clone();
    assert_eq!(evs.last(), Some(&SyncEvent::Idle));
}

#[tokio::test]
async fn rename_game_cloud_data_without_drive_returns_true_immediately() {
    let e1 = sample_native("Game", Some("Folder"));
    let harness = make_harness(vec![e1.clone()], None, |_| {});

    let migration = CloudEntryMigration::new(&harness.ctx);
    let res = migration
        .rename_game_cloud_data(&e1, None, "Old Title", "New Title", &[])
        .await
        .unwrap();

    assert!(res);
    assert!(harness.folders.renamed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rename_entry_aborts_when_old_path_is_used_by_another_entry_save_targets() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    // Old entry: NativeGame with group = "SharedRoot", name = "SubGame", cloud_folder = None
    // This resolves old_path to ["SharedRoot", "SubGame"] (len 2)
    let e1 = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "SubGame".to_string(),
            executable_path: "/games/subgame".to_string(),
            group: Some("SharedRoot".to_string()),
            cloud_folder: None,
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });

    // Other entry: Emulator whose ROM targets resolve to ["SharedRoot", "SubGame"]
    let e2 = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "RetroArch".to_string(),
            cloud_folder: Some("SharedRoot".to_string()),
            ..EntryCommon::default()
        },
        save_targets_by_rom: vec![(
            "rom_sub.sfc".to_string(),
            vec![SaveTarget {
                path: "/saves/rom_sub.srm".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
                excluded_folders: Vec::new(),
            }],
        )],
        title_by_rom: vec![("rom_sub.sfc".to_string(), "SubGame".to_string())],
        ..Default::default()
    });

    let creds = Credentials {
        client_id: "c".into(),
        client_secret: "s".into(),
        refresh_token: Some("rt".into()),
    };

    let harness = make_harness(vec![e1.clone(), e2], Some(creds), move |ev| {
        events_clone.lock().unwrap().push(ev);
    });

    let migration = CloudEntryMigration::new(&harness.ctx);

    let mut e1_renamed = e1.clone();
    if let GameEntry::Native(ref mut n) = e1_renamed {
        n.common.group = Some("NewRoot".into());
    }

    let res = migration
        .rename_entry_cloud_data(&e1, &e1_renamed)
        .await
        .unwrap();

    // Must return false to abort migration
    assert!(
        !res,
        "Must abort rename when old_path matches save_targets of another entry"
    );

    let evs = events.lock().unwrap().clone();
    assert!(
        evs.iter().any(|e| matches!(
            e,
            SyncEvent::Error(SyncMessage::SharedCloudFolder { folder }) if folder == "SharedRoot/SubGame"
        )),
        "Must emit error about shared cloud folder, got: {:?}",
        evs
    );

    // Must not attempt rename_folder or move_file
    assert!(
        harness.folders.renamed.lock().unwrap().is_empty(),
        "Must not call rename_folder"
    );
    assert!(
        harness.folders.move_log.lock().unwrap().is_empty(),
        "Must not call move_file"
    );
}

#[tokio::test]
async fn rename_game_cloud_data_with_drive_connected_relocates_merges_and_aborts_on_post_merge_sync_error()
 {
    let creds = Credentials {
        client_id: "c".into(),
        client_secret: "s".into(),
        refresh_token: Some("rt".into()),
    };

    let emu = GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: "Emu".to_string(),
            cloud_folder: Some("CloudEmu".to_string()),
            ..EntryCommon::default()
        },
        ..Default::default()
    });

    // Subcase 1: Relocation (old_folder_id = Some, new_folder_id = None)
    {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let harness = make_harness(vec![emu.clone()], Some(creds.clone()), move |ev| {
            events_clone.lock().unwrap().push(ev);
        });

        harness
            .folders
            .folders
            .lock()
            .unwrap()
            .insert("CloudEmu/OldTitle".into(), "f_old".into());

        let migration = CloudEntryMigration::new(&harness.ctx);
        let target = SaveTarget {
            path: "/saves/old.srm".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
            excluded_folders: Vec::new(),
        };

        let res = migration
            .rename_game_cloud_data(&emu, Some("rom.sfc"), "OldTitle", "NewTitle", &[target])
            .await
            .unwrap();

        assert!(res, "Relocation should succeed");
        assert_eq!(
            *harness.folders.renamed.lock().unwrap(),
            vec![("f_old".to_string(), "NewTitle".to_string())]
        );
        let evs = events.lock().unwrap().clone();
        assert_eq!(evs.last(), Some(&SyncEvent::Idle));
    }

    // Subcase 2: Merge with unverified duplicate remote files in new_folder_id -> aborts post-merge sync
    {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let harness = make_harness(vec![emu.clone()], Some(creds), move |ev| {
            events_clone.lock().unwrap().push(ev);
        });

        harness
            .folders
            .folders
            .lock()
            .unwrap()
            .insert("CloudEmu/OldTitle".into(), "f_old".into());
        harness
            .folders
            .folders
            .lock()
            .unwrap()
            .insert("CloudEmu/NewTitle".into(), "f_new".into());

        // f_new contains unverified duplicate files
        let dup1 = RemoteFile {
            id: "id_dup1".into(),
            name: "save.srm".into(),
            modified: 1000,
            is_folder: false,
            sha256: None,
            version: None,
        };
        let dup2 = RemoteFile {
            id: "id_dup2".into(),
            name: "save.srm".into(),
            modified: 2000,
            is_folder: false,
            sha256: None,
            version: None,
        };
        harness
            .search
            .files_by_folder
            .lock()
            .unwrap()
            .insert("f_new".into(), vec![dup1, dup2]);

        let migration = CloudEntryMigration::new(&harness.ctx);
        let target = SaveTarget {
            path: "/saves/save.srm".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
            excluded_folders: Vec::new(),
        };

        let res = migration
            .rename_game_cloud_data(&emu, Some("rom.sfc"), "OldTitle", "NewTitle", &[target])
            .await
            .unwrap();

        assert!(!res, "Must return false on post-merge sync error");

        let evs = events.lock().unwrap().clone();
        assert!(
            evs.iter().any(|e| matches!(
                e,
                SyncEvent::Error(SyncMessage::CloudRenameFailed { detail }) if detail.contains("Duplicate cloud file: save.srm")
            )),
            "Expected Cloud rename error for duplicate cloud file, got: {:?}",
            evs
        );
    }
}
