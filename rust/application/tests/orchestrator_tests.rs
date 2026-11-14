mod common;

use common::*;
use emusync_application::{SyncOrchestratorFactory, SyncServices};
use emusync_domain::sync::SyncMessage;
use emusync_domain::sync::{Decision, EntrySyncStatus, PostSyncFailure, RequestId, SyncEvent};
use std::sync::Arc;

#[tokio::test]
async fn test_orchestrator_delegates_sync_entry_and_renames() {
    let fs = MockFs::default();
    fs.add_file("/saves/game.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["Game".into()], "f_game");
    drive.add_file("f_game", sample_remote_file("rf1", "game.sav", 1000));

    let tracked = MockTrackedSync::default();
    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    let config = sample_app_config();

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

#[tokio::test]
async fn test_orchestrator_delegates_play_check_conflict_and_retry() {
    let fs = MockFs::default();
    fs.add_file("/saves/game.sav", 1000);

    let drive = MockDrive::default();
    drive.add_folder(&["Game".into()], "f_game");
    drive.add_file("f_game", sample_remote_file("rf1", "game.sav", 1000));

    let tracked = MockTrackedSync::default();
    tracked
        .planned_decisions
        .lock()
        .unwrap()
        .insert("/saves/game.sav".into(), Decision::Conflict);

    let auth = MockAuth::default();
    *auth.token.lock().unwrap() = Some("token".into());

    let launcher = MockLauncher::default();
    let tracked_clone = tracked.clone();
    *launcher.on_launch_hook.lock().unwrap() = Some(Box::new(move || {
        tracked_clone
            .planned_decisions
            .lock()
            .unwrap()
            .insert("/saves/game.sav".into(), Decision::UploadLocal);
    }));

    let (conflict_tx, mut conflict_rx) = tokio::sync::mpsc::unbounded_channel();
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
    let synchronizer = Arc::new(factory.create(config, move |ev| {
        if let SyncEvent::Conflict(req) = ev {
            let _ = conflict_tx.send(req);
        }
    }));

    let game = sample_native_game("Game", "/saves/game.sav");

    // 1. check_entry delegation
    let status = synchronizer.check_entry(&game.entry).await;
    assert_eq!(status, Ok(EntrySyncStatus::Conflict));

    // 2. play_with_sync delegation with deterministic conflict resolution
    let sync_clone = Arc::clone(&synchronizer);
    let game_clone = game.clone();
    let play_task = tokio::spawn(async move {
        sync_clone
            .play_with_sync(game_clone, &MockSetup::default())
            .await
    });

    let conflict = conflict_rx
        .recv()
        .await
        .expect("Conflict request must be received");

    // 3. resolve_conflict delegation
    synchronizer.resolve_conflict(&conflict, Decision::UploadLocal);

    let play_res = play_task.await.unwrap();
    assert_eq!(play_res, Ok(()));

    // 4. retry_post_sync delegation
    let failure = PostSyncFailure {
        request_id: RequestId(12345),
        game_name: "Game".into(),
        reason: SyncMessage::Detail("Test".into()),
        completed_save_count: 0,
        pending_save_count: 1,
    };
    let retry_res = synchronizer.retry_post_sync(&failure).await;
    assert_eq!(retry_res, Ok(()));
}
