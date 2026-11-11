mod common;

use emusync_desktop_app::{
    AppEvent,
    coordinator::interaction::{AsyncSaveSetupBridge, InteractionControls},
};
use emusync_domain::{
    library::{EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
    sync::{SaveSetup, SaveSetupRequest, SyncGame},
};
use std::sync::{Arc, mpsc::channel};

fn test_game(name: &str) -> SyncGame {
    SyncGame {
        name: name.to_string(),
        entry: GameEntry::Native(NativeGame {
            common: EntryCommon {
                name: name.to_string(),
                executable_path: "/bin/true".to_string(),
                arguments: vec![],
                save_paths: vec![],
                save_targets: vec![],
                group: None,
                cloud_folder: None,
                drive_file_id: None,
                steam_app_id: None,
            },
            cover_path: None,
            wait_for_process: None,
        }),
        rom_path: None,
        effective_title: name.to_string(),
        save_targets: vec![],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_emits_requested_event_and_resolves_on_confirmation() {
    let (tx, rx) = channel();
    let notify = Arc::new(tokio::sync::Notify::new());
    let notify_clone = notify.clone();
    let controls = InteractionControls::new(
        tx,
        Arc::new(move || {
            notify_clone.notify_waiters();
            notify_clone.notify_one();
        }),
    );
    let bridge = AsyncSaveSetupBridge {
        controls: controls.clone(),
    };

    let game = test_game("Zelda");
    let req = SaveSetupRequest {
        game: game.clone(),
        cloud_file_names: vec!["zelda.sav".to_string()],
    };

    let bridge_task = tokio::spawn(async move { bridge.request(req).await });

    let event = common::recv_event_matching(&rx, &notify, |e| {
        matches!(e, AppEvent::SaveSetupRequested { .. })
    })
    .await;
    match event {
        AppEvent::SaveSetupRequested { request } => {
            assert_eq!(request.item.name, "Zelda");
            assert_eq!(request.cloud_file_names, vec!["zelda.sav".to_string()]);
        }
        other => panic!("Unexpected event: {other:?}"),
    }

    // 2. UI confirms setup
    let target = SaveTarget {
        path: "/saves/zelda.sav".to_string(),
        kind: SaveTargetType::File,
        cloud_path: Some("zelda.sav".to_string()),
    };
    let resolved = controls.complete_save_setup_response("Zelda", game, vec![target.clone()]);
    assert!(resolved, "Confirmation must be accepted for matching game");

    let result = bridge_task.await.unwrap().expect("Bridge should succeed");
    assert_eq!(result.targets, vec![target]);
}

#[tokio::test]
async fn bridge_returns_error_on_cancellation() {
    let (tx, _rx) = channel();
    let controls = InteractionControls::new(tx, Arc::new(|| {}));
    let bridge = AsyncSaveSetupBridge {
        controls: controls.clone(),
    };

    let game = test_game("Metroid");
    let req = SaveSetupRequest {
        game,
        cloud_file_names: vec!["metroid.dat".to_string()],
    };

    let bridge_task = tokio::spawn(async move { bridge.request(req).await });

    // Cancel setup from UI / dialog close
    tokio::task::yield_now().await;
    controls.cancel_save_setup();

    let result = bridge_task.await.unwrap();
    assert!(result.is_err(), "Cancelled setup must return error");
}

#[tokio::test]
async fn mismatched_game_name_is_rejected() {
    let (tx, _rx) = channel();
    let controls = InteractionControls::new(tx, Arc::new(|| {}));
    let bridge = AsyncSaveSetupBridge {
        controls: controls.clone(),
    };

    let game = test_game("Castlevania");
    let req = SaveSetupRequest {
        game: game.clone(),
        cloud_file_names: vec!["save.bin".to_string()],
    };

    let _bridge_task = tokio::spawn(async move { bridge.request(req).await });
    tokio::task::yield_now().await;

    // Responding with wrong game name
    let wrong = controls.complete_save_setup_response("WrongGame", game, vec![]);
    assert!(!wrong, "Mismatched game name must return false");
}

#[tokio::test]
async fn cancel_for_another_game_keeps_the_pending_request() {
    let (events, _events_rx) = channel();
    let controls = InteractionControls::new(events, Arc::new(|| {}));
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    *controls.pending_save_setup.lock().unwrap() = Some(("Metroid".to_string(), tx));

    controls.cancel_save_setup_for("Zelda");
    assert!(rx.try_recv().is_err(), "another game's cancel is ignored");

    controls.cancel_save_setup_for("Metroid");
    assert!(matches!(rx.await, Ok(Err(_))));
}
