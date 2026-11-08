use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore},
    library::{EntryCommon, GameEntry, NativeGame},
};
use emusync_presentation::{actions::UiAction, model::GameItem};
use std::sync::{Arc, mpsc::channel};
use tempfile::tempdir;

fn test_native(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/bin/true".to_string(),
            arguments: vec![],
            save_paths: vec!["/tmp/saves".to_string()],
            save_targets: vec![],
            group: None,
            cloud_folder: None,
            drive_file_id: None,
            steam_app_id: None,
        },
        cover_path: None,
        wait_for_process: None,
    })
}

#[tokio::test]
async fn deleting_last_category_clears_library_cleanly() {
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native("OnlyCategory");
    manager
        .update(|_| AppConfig {
            credentials: None,
            entries: vec![entry.clone()],
        })
        .unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    );

    coordinator.handle_action(UiAction::DeleteEntry(entry));

    let mut saw_config_reloaded = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::ConfigReloaded(cfg) = event {
            assert!(cfg.entries.is_empty(), "Config should have 0 entries");
            saw_config_reloaded = true;
        }
    }
    assert!(saw_config_reloaded);
}

#[test]
fn stale_scan_event_with_lower_revision_is_ignored() {
    use emusync_desktop_app::apply_entry_games_scanned;

    let mut current_revision = 0u64;
    let mut displayed_games = vec![];

    // Event 1 with revision 2 arrives first for "Cat"
    let event1_games = vec![GameItem::new("GameV2", test_native("Cat"), None)];
    let applied1 = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "Cat",
        event1_games,
        2,
    );
    assert!(applied1);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");

    // Stale Event with revision 1 arrives later for "Cat"
    let event0_games = vec![GameItem::new("GameV1_Stale", test_native("Cat"), None)];
    let applied0 = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "Cat",
        event0_games,
        1,
    );
    assert!(!applied0);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");

    // Event for a different category arrives
    let other_games = vec![GameItem::new("OtherGame", test_native("OtherCat"), None)];
    let applied_other = apply_entry_games_scanned(
        &mut current_revision,
        &mut displayed_games,
        Some("Cat"),
        "OtherCat",
        other_games,
        3,
    );
    assert!(!applied_other);
    assert_eq!(current_revision, 2);
    assert_eq!(displayed_games[0].name, "GameV2");
}

struct DummyTransfer;
impl emusync_domain::sync::CloudSaveTransfer for DummyTransfer {
    type Error = std::io::Error;
    async fn download(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: Option<emusync_domain::sync::Timestamp>,
        _: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn upload(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<emusync_domain::sync::RemoteState, Self::Error> {
        unreachable!()
    }
    async fn replace(
        &self,
        _: &str,
        _: &emusync_domain::sync::RemoteState,
        _: &str,
        _: &str,
    ) -> Result<emusync_domain::sync::RemoteState, Self::Error> {
        unreachable!()
    }
}

#[tokio::test]
async fn non_linux_directory_download_rejection_contract() {
    use emusync_data_local::{
        ConfigManager, RecoveryCredentials, RecoveryStore, SyncHistoryStore, TrackedFileSync,
    };
    use emusync_desktop_app::GuardedFiles;
    use emusync_domain::{
        config::CredentialStore,
        sync::{DirectoryDownload, RemoteState},
    };
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let manager = Arc::new(ConfigManager::new(&config_path).unwrap());
    let history = Arc::new(SyncHistoryStore::new(&config_path).unwrap());
    let store = Arc::new(RecoveryStore::new(&config_path).unwrap());
    let tracked = Arc::new(TrackedFileSync::new(DummyTransfer, history).with_recovery(store));
    let config = AppConfig::default();
    let snapshot = manager.snapshot(&config).unwrap();
    let credentials = RecoveryCredentials::new(manager.clone(), snapshot, config);
    let guarded = GuardedFiles::new(tracked, credentials, |_| false);

    let remote = RemoteState {
        id: "dummy_id".into(),
        modified: 1_700_000_000,
        sha256: None,
        version: None,
    };
    let local_file = dir.path().join("rel");
    let req = DirectoryDownload {
        token: "token",
        remote: &remote,
        local: local_file.to_str().unwrap(),
        root: dir.path().to_str().unwrap(),
        relative: "rel",
        folder: "folder",
        remote_name: "remote_name",
    };

    #[cfg(not(target_os = "linux"))]
    {
        use emusync_domain::sync::LocalSaveFiles;
        let res = guarded.download_in_directory(req).await;
        assert!(
            matches!(res, Err(emusync_data_local::TrackedError::Local(e)) if e.kind() == std::io::ErrorKind::Unsupported)
        );
    }
    #[cfg(target_os = "linux")]
    {
        let _ = guarded;
        let _ = req;
    }
}
