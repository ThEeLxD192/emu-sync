mod common;

use common::*;
use emusync_application::{SyncOrchestratorFactory, SyncServices};
use emusync_domain::config::AppConfig;
use emusync_domain::library::{SaveTarget, SaveTargetType};
use emusync_domain::sync::SyncMessage;
use emusync_domain::sync::{Decision, RemoteFile, SaveSetupResult, SyncEvent};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

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
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/metroid.sav".into(), Decision::DownloadCloud);

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let launcher = MockLauncher::default();
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

    let config = sample_app_config();
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
    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
    assert_eq!(res, Ok(()));

    assert_eq!(
        *tracked.downloads.lock().unwrap(),
        vec!["/saves/metroid.sav"]
    );
    assert_eq!(*launcher.launched_games.lock().unwrap(), vec!["Metroid"]);
    assert_eq!(*tracked.uploads.lock().unwrap(), vec!["/saves/metroid.sav"]);
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

    let auth = MockAuth::default();
    auth.fail_with_transport.store(true, Ordering::SeqCst);

    let config = sample_app_config();
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
    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;

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
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let config = sample_app_config();
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

    let mut game = sample_native_game("TraversalGame", "/saves/game.sav");
    game.save_targets[0].cloud_path = Some("../escaped".into());

    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
    assert_eq!(res, Ok(()));
    assert!(launcher.launched_games.lock().unwrap().is_empty());

    let recorded = events.lock().unwrap().clone();
    assert!(
        recorded
            .iter()
            .any(|e| matches!(e, SyncEvent::Error(SyncMessage::Failed { detail }) if detail.contains("escaped")))
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

    let (conflict_tx, mut conflict_rx) = tokio::sync::mpsc::unbounded_channel();

    let config = sample_app_config();
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
            let _ = conflict_tx.send(req);
        }
    }));

    let sync_clone = Arc::clone(&synchronizer);
    let play_task = tokio::spawn(async move {
        let game = sample_native_game("ConflictGame", "/saves/conflict.sav");
        sync_clone.play_with_sync(game, &MockSetup::default()).await
    });

    let conflict_req = conflict_rx
        .recv()
        .await
        .expect("Conflict should be emitted deterministically");

    synchronizer.resolve_conflict(&conflict_req, Decision::UploadLocal);

    let res = play_task.await.unwrap();
    assert_eq!(res, Ok(()));
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
    *launcher.exit_code.lock().unwrap() = 1;

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let config = sample_app_config();
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
    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
    assert_eq!(res, Ok(()));

    assert_eq!(*launcher.launched_games.lock().unwrap(), vec!["CrashGame"]);
    assert!(tracked.uploads.lock().unwrap().is_empty());

    let evs = events.lock().unwrap().clone();
    assert!(evs.iter().any(|ev| matches!(
        ev,
        SyncEvent::Error(SyncMessage::GameNotTracked { game }) if game == "CrashGame"
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

    let config = AppConfig {
        language: Default::default(),
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
    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
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
    let config = sample_app_config();

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

    let mut game = sample_native_game("UnconfiguredGame", "/saves/unconf.sav");
    game.save_targets.clear();

    let res = synchronizer.play_with_sync(game, &CancelSetup).await;
    assert_eq!(res, Ok(()));
    assert!(launcher.launched_games.lock().unwrap().is_empty());
}

// --- Adversarial Lifecycle Tests ---

#[tokio::test]
async fn pre_sync_unverified_duplicate_or_mid_download_failure_aborts_game_launch() {
    // Subcase A: list_files returns two RemoteFiles with name "save.sav", distinct id, sha256: None
    {
        let fs = MockFs::default();
        fs.add_file("/saves/save.sav", 1000);

        let drive = MockDrive::default();
        drive.add_folder(&["DupGame".into()], "f_dup");
        drive.add_file(
            "f_dup",
            RemoteFile {
                id: "id_1".into(),
                name: "save.sav".into(),
                modified: 1000,
                is_folder: false,
                sha256: None,
                version: None,
            },
        );
        drive.add_file(
            "f_dup",
            RemoteFile {
                id: "id_2".into(),
                name: "save.sav".into(),
                modified: 2000,
                is_folder: false,
                sha256: None,
                version: None,
            },
        );

        let tracked = MockTrackedSync::default();
        let auth = MockAuth::default();
        *auth.token.lock().unwrap() = Some("token".into());

        let launcher = MockLauncher::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let config = sample_app_config();
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

        let game = sample_native_game("DupGame", "/saves/save.sav");
        let res = synchronizer
            .play_with_sync(game, &MockSetup::default())
            .await;

        assert_eq!(res, Ok(()));
        assert!(
            launcher.launched_games.lock().unwrap().is_empty(),
            "Game must not be launched when remote file has unverified duplicates"
        );
        assert!(
            tracked.uploads.lock().unwrap().is_empty(),
            "Post-game sync must not execute"
        );

        let recorded = events.lock().unwrap().clone();
        assert!(
            recorded.iter().any(|e| matches!(
                e,
                SyncEvent::Error(SyncMessage::Failed { detail }) if detail == "Unverified duplicate remote file: save.sav"
            )),
            "Expected exact error event for duplicate remote file, got: {:?}",
            recorded
        );
    }

    // Subcase B: list_files returns valid "save.sav" with DownloadCloud, but tracked_sync.download fails
    {
        let fs = MockFs::default();
        fs.add_file("/saves/save.sav", 1000);

        let drive = MockDrive::default();
        drive.add_folder(&["DownloadFailGame".into()], "f_dl");
        drive.add_file(
            "f_dl",
            RemoteFile {
                id: "id_valid".into(),
                name: "save.sav".into(),
                modified: 2000,
                is_folder: false,
                sha256: Some("sha_hash".into()),
                version: None,
            },
        );

        let tracked = MockTrackedSync::default();
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/save.sav".into(), Decision::DownloadCloud);
        *tracked.fail_download_for.lock().unwrap() = Some("/saves/save.sav".into());

        let auth = MockAuth::default();
        *auth.token.lock().unwrap() = Some("token".into());

        let launcher = MockLauncher::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let config = sample_app_config();
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

        let game = sample_native_game("DownloadFailGame", "/saves/save.sav");
        let res = synchronizer
            .play_with_sync(game, &MockSetup::default())
            .await;

        assert_eq!(res, Ok(()));
        assert!(
            launcher.launched_games.lock().unwrap().is_empty(),
            "Game must not be launched when pre-sync download fails"
        );
        assert!(
            tracked.uploads.lock().unwrap().is_empty(),
            "Post-game sync must not execute"
        );

        let recorded = events.lock().unwrap().clone();
        assert!(
            recorded.iter().any(|e| matches!(
                e,
                SyncEvent::Error(SyncMessage::Failed { detail })
                    if detail.contains("Could not finish preparing saves for DownloadFailGame")
                        && detail.contains("checksum mismatch / disk full")
            )),
            "Expected error event mentioning failure preparing saves, got: {:?}",
            recorded
        );
    }
}

#[tokio::test]
async fn save_setup_before_and_after_play_updates_cloud_path_or_aborts_when_cancelled_or_errored() {
    // Subcase A (Pre-play setup error):
    // Game launched without local save_targets and in cloud files exist.
    // ensure_save_setup emits SyncEvent::Idle and calls SaveSetup::request.
    // If SaveSetup::request returns Err(...), emits SyncEvent::Error("Error: Save setup failed") and NO launch.
    {
        let fs = MockFs::default();
        let drive = MockDrive::default();
        drive.add_folder(&["PreSetupErrGame".into()], "f_presetup");
        drive.add_file(
            "f_presetup",
            sample_remote_file("rf_1", "remote_save.sav", 1000),
        );

        let tracked = MockTrackedSync::default();
        let auth = MockAuth::default();
        *auth.token.lock().unwrap() = Some("token".into());

        let launcher = MockLauncher::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let config = sample_app_config();
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

        let mut game = sample_native_game("PreSetupErrGame", "/saves/dummy.sav");
        game.save_targets.clear();

        let setup = MockSetup::default();
        *setup.handler.lock().unwrap() = Some(Box::new(|req| {
            assert_eq!(req.cloud_file_names, vec!["remote_save.sav"]);
            Err("Dialog cancelled by user / error".into())
        }));

        let res = synchronizer.play_with_sync(game, &setup).await;
        assert_eq!(res, Ok(()));
        assert!(
            launcher.launched_games.lock().unwrap().is_empty(),
            "Game must not launch when pre-play save setup fails"
        );

        let recorded = events.lock().unwrap().clone();
        assert!(
            recorded.contains(&SyncEvent::Idle),
            "Must emit SyncEvent::Idle before calling save setup request"
        );
        assert!(
            recorded.iter().any(|e| matches!(
                e,
                SyncEvent::Error(SyncMessage::Failed { detail }) if detail == "Save setup failed"
            )),
            "Expected exact SyncEvent::Error(\"Error: Save setup failed\"), got: {:?}",
            recorded
        );
    }

    // Subcase B (Post-play setup success):
    // Game launched without local save_targets and in cloud NO files exist.
    // Game launches, exits with code 0, launch_prepared calls SaveSetup::request with cloud_file_names empty.
    // User provides new targets, uploaded immediately in PostGameSync.
    {
        let fs = MockFs::default();
        fs.add_file("/saves/post_game.sav", 1000);

        let drive = MockDrive::default();
        drive.add_folder(&["PostSetupOkGame".into()], "f_postsetup");

        let tracked = MockTrackedSync::default();
        tracked
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/post_game.sav".into(), Decision::UploadLocal);

        let auth = MockAuth::default();
        *auth.token.lock().unwrap() = Some("token".into());

        let launcher = MockLauncher::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = Arc::clone(&events);

        let config = sample_app_config();
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

        let mut game = sample_native_game("PostSetupOkGame", "/saves/post_game.sav");
        game.save_targets.clear();

        let setup = MockSetup::default();
        *setup.handler.lock().unwrap() = Some(Box::new(|req| {
            assert!(
                req.cloud_file_names.is_empty(),
                "Post-play setup must receive empty cloud_file_names"
            );
            let target = SaveTarget {
                path: "/saves/post_game.sav".into(),
                kind: SaveTargetType::File,
                cloud_path: None,
                excluded_folders: Vec::new(),
            };
            let mut updated_game = req.game;
            updated_game.save_targets = vec![target.clone()];
            Ok(SaveSetupResult {
                game: updated_game,
                targets: vec![target],
            })
        }));

        let res = synchronizer.play_with_sync(game, &setup).await;
        assert_eq!(res, Ok(()));

        assert_eq!(
            *launcher.launched_games.lock().unwrap(),
            vec!["PostSetupOkGame"]
        );
        assert_eq!(
            *tracked.uploads.lock().unwrap(),
            vec!["/saves/post_game.sav"],
            "Configured save target returned by post-play setup must be uploaded immediately"
        );

        let recorded = events.lock().unwrap().clone();
        assert!(recorded.contains(&SyncEvent::Idle));
    }
}
