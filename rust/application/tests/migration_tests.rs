use emusync_application::{CloudEntryMigration, SyncContext, SyncServices};
use emusync_domain::config::{AppConfig, CloudAuthorization, Credentials};
use emusync_domain::execution::{GameLaunchRequest, GameLauncher, GameResult};
use emusync_domain::library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget};
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
    folders: Arc<Mutex<std::collections::HashMap<String, String>>>,
    renamed: Arc<Mutex<Vec<(String, String)>>>,
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
        _file: &str,
        _add_parent: &str,
        _remove_parent: &str,
    ) -> Result<(), Self::Error> {
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

    let services = SyncServices {
        files: DummyAccess,
        search: MockListing::default(),
        folders: folders.clone(),
        tracked_sync: DummyTracked,
        authorization: auth,
        runner: DummyLauncher,
    };

    let config = AppConfig {
        credentials,
        entries,
    };

    let ctx = SyncContext::new(config, services, on_event);
    MigrationHarness { ctx, folders }
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
    assert!(evs.iter().any(|e| matches!(
        e,
        SyncEvent::Error(msg) if msg.contains("is shared with another entry")
    )));
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
