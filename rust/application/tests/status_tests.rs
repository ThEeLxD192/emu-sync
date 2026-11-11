mod common;

use common::*;
use emusync_application::{SyncOrchestratorFactory, SyncServices};
use emusync_domain::config::{AppConfig, Credentials};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType};
use emusync_domain::sync::{Decision, EntrySyncStatus, RemoteFile};
use std::sync::atomic::Ordering;

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
        excluded_folders: Vec::new(),
    };
    let target2 = SaveTarget {
        path: "/game/saves/trophies".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
        excluded_folders: Vec::new(),
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

// --- Adversarial Status Tests ---

#[tokio::test]
async fn cross_file_upload_and_download_in_same_entry_reports_conflict_instead_of_out_of_sync() {
    let files = MockFs::default();
    files.add_dir("/saves/gamedir");
    files.add_file("/saves/gamedir/slot1.sav", 1000);
    files.add_file("/saves/gamedir/slot2.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["MixedStatusGame".into()], "f_mixed");
    // slot2 exists remotely, slot1 does not exist remotely
    drive.add_file(
        "f_mixed",
        RemoteFile {
            id: "id_slot2".into(),
            name: "slot2.sav".into(),
            modified: 2000,
            is_folder: false,
            sha256: Some("sha_slot2".into()),
            version: None,
        },
    );

    let tracked = MockTrackedSync::default();
    // slot1 local is newer -> UploadLocal
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/gamedir/slot1.sav".into(), Decision::UploadLocal);
    // slot2 remote is newer -> DownloadCloud
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/gamedir/slot2.sav".into(), Decision::DownloadCloud);

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let target = SaveTarget {
        path: "/saves/gamedir".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
        excluded_folders: Vec::new(),
    };
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "MixedStatusGame".into(),
            executable_path: "/games/mixed".into(),
            save_targets: vec![target],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });

    let config = sample_app_config();
    let services = SyncServices {
        files,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: MockLauncher::default(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let sync = factory.create(config, |_| {});

    // Must evaluate to Conflict because both local_changed and remote_changed are true!
    let status = sync.check_entry(&entry).await;
    assert_eq!(
        status,
        Ok(EntrySyncStatus::Conflict),
        "Mixed UploadLocal and DownloadCloud within the same entry must evaluate to Conflict, not OutOfSync"
    );
}

#[tokio::test]
async fn unresolvable_or_traversal_remote_file_in_directory_reports_error_status() {
    let files = MockFs::default();
    files.add_dir("/saves/dir");

    let drive = MockDrive::default();
    drive.add_folder(&["TraversalStatusGame".into()], "f_trav");
    // Remote contains directory traversal file name "../escape.sav"
    drive.add_file(
        "f_trav",
        RemoteFile {
            id: "id_escape".into(),
            name: "../escape.sav".into(),
            modified: 1000,
            is_folder: false,
            sha256: None,
            version: None,
        },
    );

    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let target = SaveTarget {
        path: "/saves/dir".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
        excluded_folders: Vec::new(),
    };
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "TraversalStatusGame".into(),
            executable_path: "/games/trav".into(),
            save_targets: vec![target],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });

    let config = sample_app_config();
    let services = SyncServices {
        files,
        search: drive.clone(),
        folders: drive,
        tracked_sync: tracked,
        authorization: auth,
        runner: MockLauncher::default(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let sync = factory.create(config, |_| {});

    let status = sync.check_entry(&entry).await;
    assert_eq!(
        status,
        Ok(EntrySyncStatus::Error),
        "Unresolvable or traversal remote file must report Error status, never InSync or silently ignored"
    );
}
