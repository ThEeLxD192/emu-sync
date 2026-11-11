mod common;

use common::*;
use emusync_application::{SyncOrchestratorFactory, SyncServices};
use emusync_domain::library::{EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType};
use emusync_domain::sync::{Decision, RequestId, SyncEvent, SyncGame};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

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
    *tracked.fail_upload_for.lock().unwrap() = Some("/saves/retry.sav".into());

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    let failure_store = Arc::new(Mutex::new(None));
    let failure_store_clone = Arc::clone(&failure_store);

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
        if let SyncEvent::PostSyncFailed(f) = ev {
            *failure_store_clone.lock().unwrap() = Some(f);
        }
    }));

    let game = sample_native_game("RetryGame", "/saves/retry.sav");
    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
    assert_eq!(res, Ok(()));

    let failure = failure_store
        .lock()
        .unwrap()
        .clone()
        .expect("PostSyncFailed expected");
    assert_eq!(failure.game_name, "RetryGame");

    // Fix error
    *tracked.fail_upload_for.lock().unwrap() = None;

    // Retry with mismatched RequestId
    let mut bad_failure = failure.clone();
    bad_failure.request_id = RequestId(999999);
    let bad_retry = synchronizer.retry_post_sync(&bad_failure).await;
    assert_eq!(bad_retry, Ok(()));
    assert!(tracked.uploads.lock().unwrap().is_empty());

    // Retry with correct RequestId
    let good_retry = synchronizer.retry_post_sync(&failure).await;
    assert_eq!(good_retry, Ok(()));
    assert_eq!(*tracked.uploads.lock().unwrap(), vec!["/saves/retry.sav"]);
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

    let config = sample_app_config();
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
            excluded_folders: Vec::new(),
        },
        SaveTarget {
            path: "/saves/other/file.sav".into(),
            kind: SaveTargetType::File,
            cloud_path: None,
            excluded_folders: Vec::new(),
        },
    ];

    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
    assert_eq!(res, Ok(()));

    let evs = events.lock().unwrap().clone();
    assert!(evs.iter().any(|ev| matches!(
        ev,
        SyncEvent::Error(msg) if msg.contains("conflicts with another configured file target")
    )));
}

// --- Adversarial Post-Game Sync Tests ---

#[tokio::test]
async fn post_sync_partial_failure_with_blocked_sibling_reports_exact_counts_and_retries_only_failed_target()
 {
    let fs = MockFs::default();
    fs.add_dir("/saves/dir");
    fs.add_file("/saves/dir/ok.sav", 1000);
    fs.add_file("/saves/dir/flaky.sav", 1000);
    fs.add_file("/saves/dir/collision.sav", 1000);
    fs.add_file("/saves/other/collision.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["MultiTargetGame".into()], "f_multi");

    let tracked = MockTrackedSync::default();
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/dir/ok.sav".into(), Decision::UploadLocal);
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/dir/flaky.sav".into(), Decision::UploadLocal);
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/dir/collision.sav".into(), Decision::UploadLocal);
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/other/collision.sav".into(), Decision::UploadLocal);

    // Flaky fails on initial attempt
    *tracked.fail_upload_for.lock().unwrap() = Some("/saves/dir/flaky.sav".into());

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let launcher = MockLauncher::default();
    let failure_store = Arc::new(Mutex::new(None));
    let failure_store_clone = Arc::clone(&failure_store);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

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
        events_clone.lock().unwrap().push(ev.clone());
        if let SyncEvent::PostSyncFailed(f) = ev {
            *failure_store_clone.lock().unwrap() = Some(f);
        }
    }));

    let target_dir = SaveTarget {
        path: "/saves/dir".into(),
        kind: SaveTargetType::Directory,
        cloud_path: None,
        excluded_folders: Vec::new(),
    };
    let target_file = SaveTarget {
        path: "/saves/other/collision.sav".into(),
        kind: SaveTargetType::File,
        cloud_path: None,
        excluded_folders: Vec::new(),
    };

    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "MultiTargetGame".into(),
            executable_path: "/games/multi".into(),
            save_targets: vec![target_dir.clone(), target_file.clone()],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });
    let game = SyncGame {
        name: "MultiTargetGame".into(),
        entry,
        rom_path: None,
        effective_title: "MultiTargetGame".into(),
        save_targets: vec![target_dir, target_file],
    };

    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
    assert_eq!(res, Ok(()));

    // First pass assertions:
    // 2 succeeded: ok.sav and other/collision.sav
    // 1 failed in network: flaky.sav
    // 1 blocked by sibling collision: collision.sav in dir
    let failure = failure_store
        .lock()
        .unwrap()
        .clone()
        .expect("PostSyncFailed event must be emitted");

    assert_eq!(
        failure.completed_save_count, 2,
        "Completed save count must be 2 (ok.sav + other/collision.sav)"
    );
    assert_eq!(
        failure.pending_save_count, 2,
        "Pending save count must be 2 (flaky.sav + collision.sav)"
    );
    assert!(
        failure
            .reason
            .contains("Upload error for /saves/dir/flaky.sav"),
        "Failure reason must include network upload error"
    );
    assert!(
        failure
            .reason
            .contains("conflicts with another configured file target"),
        "Failure reason must include collision message"
    );

    // Second pass: fix network error for flaky.sav, clear uploads log, retry
    *tracked.fail_upload_for.lock().unwrap() = None;
    tracked.uploads.lock().unwrap().clear();
    events.lock().unwrap().clear();

    let retry_res = synchronizer.retry_post_sync(&failure).await;
    assert_eq!(retry_res, Ok(()));

    // Must ONLY re-upload the failed target, NEVER ok.sav or other/collision.sav!
    assert_eq!(
        *tracked.uploads.lock().unwrap(),
        vec!["/saves/dir/flaky.sav"],
        "Retry must only upload the failed target"
    );

    // As no failed_targets remain but collision.sav block persists, must emit SyncEvent::Error with blocked reason
    let retry_events = events.lock().unwrap().clone();
    assert!(
        retry_events.iter().any(|e| matches!(
            e,
            SyncEvent::Error(msg) if msg.contains("conflicts with another configured file target")
        )),
        "Must emit remaining blocked reason as SyncEvent::Error on retry completion"
    );
}

#[tokio::test]
async fn post_sync_prepare_cloud_state_failure_marks_all_configured_and_retry_targets_as_pending() {
    let fs = MockFs::default();
    fs.add_file("/saves/s1.sav", 1000);
    fs.add_file("/saves/s2.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["BatchFailGame".into()], "f_batch");
    // Induce list_files failure during pre-sync / post-sync
    drive.fail_list.store(true, Ordering::SeqCst);

    let tracked = MockTrackedSync::default();
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/s1.sav".into(), Decision::UploadLocal);
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/s2.sav".into(), Decision::UploadLocal);

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("valid_token".into());

    let launcher = MockLauncher::default();
    let failure_store = Arc::new(Mutex::new(None));
    let failure_store_clone = Arc::clone(&failure_store);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    let config = sample_app_config();
    let services = SyncServices {
        files: fs,
        search: drive.clone(),
        folders: drive.clone(),
        tracked_sync: tracked.clone(),
        authorization: auth,
        runner: launcher.clone(),
    };

    let factory = SyncOrchestratorFactory::new(services);
    let synchronizer = Arc::new(factory.create(config, move |ev| {
        events_clone.lock().unwrap().push(ev.clone());
        if let SyncEvent::PostSyncFailed(f) = ev {
            *failure_store_clone.lock().unwrap() = Some(f);
        }
    }));

    let t1 = SaveTarget {
        path: "/saves/s1.sav".into(),
        kind: SaveTargetType::File,
        cloud_path: None,
        excluded_folders: Vec::new(),
    };
    let t2 = SaveTarget {
        path: "/saves/s2.sav".into(),
        kind: SaveTargetType::File,
        cloud_path: None,
        excluded_folders: Vec::new(),
    };
    let entry = GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: "BatchFailGame".into(),
            executable_path: "/games/batch".into(),
            save_targets: vec![t1.clone(), t2.clone()],
            ..EntryCommon::default()
        },
        wait_for_process: None,
        cover_path: None,
    });
    let game = SyncGame {
        name: "BatchFailGame".into(),
        entry,
        rom_path: None,
        effective_title: "BatchFailGame".into(),
        save_targets: vec![t1, t2],
    };

    let res = synchronizer
        .play_with_sync(game, &MockSetup::default())
        .await;
    assert_eq!(res, Ok(()));

    // Game launched (pre-sync transport fallback allowed offline play)
    assert_eq!(
        *launcher.launched_games.lock().unwrap(),
        vec!["BatchFailGame"]
    );

    // In PostGameSync, prepare_batch_cloud_state failed
    let failure = failure_store
        .lock()
        .unwrap()
        .clone()
        .expect("PostSyncFailed must be emitted");

    assert_eq!(failure.completed_save_count, 0);
    assert_eq!(
        failure.pending_save_count, 2,
        "All configured targets must be marked as pending"
    );
    assert!(failure.reason.contains("listing failed"));

    // Now restore list_files in drive and retry
    drive.fail_list.store(false, Ordering::SeqCst);
    events.lock().unwrap().clear();

    let retry_res = synchronizer.retry_post_sync(&failure).await;
    assert_eq!(retry_res, Ok(()));

    // Both files uploaded!
    let mut uploaded = tracked.uploads.lock().unwrap().clone();
    uploaded.sort();
    assert_eq!(uploaded, vec!["/saves/s1.sav", "/saves/s2.sav"]);

    let retry_events = events.lock().unwrap().clone();
    assert!(retry_events.contains(&SyncEvent::Idle));
}
