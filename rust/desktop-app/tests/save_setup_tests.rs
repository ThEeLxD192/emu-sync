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

fn controls_with_events() -> (InteractionControls, std::sync::mpsc::Receiver<AppEvent>) {
    let (tx, rx) = channel();
    (InteractionControls::new(tx, Arc::new(|| {})), rx)
}

fn request_for(name: &str) -> SaveSetupRequest {
    SaveSetupRequest {
        game: test_game(name),
        cloud_file_names: vec![format!("{name}.sav")],
    }
}

/// Starts a request and returns its id once the UI event shows it is pending.
async fn start_request(
    controls: &InteractionControls,
    rx: &std::sync::mpsc::Receiver<AppEvent>,
    name: &str,
    closed: &mut Vec<u64>,
) -> (
    u64,
    tokio::task::JoinHandle<Result<emusync_domain::sync::SaveSetupResult, String>>,
) {
    let bridge = AsyncSaveSetupBridge {
        controls: controls.clone(),
    };
    let req = request_for(name);
    let task = tokio::spawn(async move { bridge.request(req).await });
    loop {
        match rx.try_recv() {
            Ok(AppEvent::SaveSetupRequested { request }) => return (request.id, task),
            Ok(AppEvent::SaveSetupClosed { request_id }) => closed.push(request_id),
            Ok(_) => {}
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(5)).await,
        }
    }
}

fn closed_ids(rx: &std::sync::mpsc::Receiver<AppEvent>) -> Vec<u64> {
    rx.try_iter()
        .filter_map(|e| match e {
            AppEvent::SaveSetupClosed { request_id } => Some(request_id),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn taking_the_request_closes_it_once_and_lets_the_caller_answer() {
    let (controls, rx) = controls_with_events();
    let (id, task) = start_request(&controls, &rx, "Zelda", &mut Vec::new()).await;

    let pending = controls.take_save_setup(id).expect("request is pending");
    assert_eq!(closed_ids(&rx), vec![id]);
    assert!(controls.take_save_setup(id).is_none(), "claimed only once");

    let target = SaveTarget {
        path: "/saves/zelda.sav".to_string(),
        kind: SaveTargetType::File,
        cloud_path: Some("zelda.sav".to_string()),
        excluded_folders: Vec::new(),
    };
    pending.complete(emusync_domain::sync::SaveSetupResult {
        game: test_game("Zelda"),
        targets: vec![target.clone()],
    });
    let result = task.await.unwrap().expect("bridge succeeds");
    assert_eq!(result.targets, vec![target]);
    assert!(closed_ids(&rx).is_empty(), "no second close");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_save_setup_errors_the_bridge_and_closes_once() {
    let (controls, rx) = controls_with_events();
    let (id, task) = start_request(&controls, &rx, "Metroid", &mut Vec::new()).await;

    controls.cancel_save_setup();
    controls.cancel_save_setup();

    assert!(task.await.unwrap().is_err());
    assert_eq!(closed_ids(&rx), vec![id]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_for_the_matching_id_closes_once_and_another_id_keeps_the_request() {
    let (controls, rx) = controls_with_events();
    let (id, task) = start_request(&controls, &rx, "Metroid", &mut Vec::new()).await;

    controls.cancel_save_setup_for(id + 1);
    assert!(closed_ids(&rx).is_empty(), "other id emits nothing");
    assert!(!task.is_finished(), "request stays pending");

    controls.cancel_save_setup_for(id);
    assert!(task.await.unwrap().is_err());
    assert_eq!(closed_ids(&rx), vec![id]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_request_for_the_same_game_supersedes_the_first() {
    let (controls, rx) = controls_with_events();
    let (first, first_task) = start_request(&controls, &rx, "Zelda", &mut Vec::new()).await;
    let mut closed = Vec::new();
    let (second, second_task) = start_request(&controls, &rx, "Zelda", &mut closed).await;

    assert_ne!(first, second, "same game, distinct ids");
    assert!(first_task.await.unwrap().is_err());
    closed.extend(closed_ids(&rx));
    assert_eq!(closed, vec![first]);
    assert!(!second_task.is_finished(), "second stays pending");
    assert!(controls.take_save_setup(first).is_none());
    assert!(controls.take_save_setup(second).is_some());
}

#[test]
fn taking_an_absent_request_returns_none_and_emits_nothing() {
    let (controls, rx) = controls_with_events();
    assert!(controls.take_save_setup(3).is_none());
    assert!(closed_ids(&rx).is_empty());
}

// Needs `current_thread`: the cancel must land while the bridge is parked reading targets.
#[tokio::test(flavor = "current_thread")]
async fn a_request_cancelled_before_its_announcement_is_never_requested() {
    let (controls, rx) = controls_with_events();
    let bridge = AsyncSaveSetupBridge {
        controls: controls.clone(),
    };
    let req = request_for("Zelda");
    let task = tokio::spawn(async move { bridge.request(req).await });

    // Single-threaded runtime: the bridge is parked on reading targets, and the cancel
    // below runs before it is polled again.
    tokio::task::yield_now().await;
    controls.cancel_save_setup();

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("bridge did not finish: the cancel must land after it registers its request and before it announces it");
    assert!(result.unwrap().is_err());
    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::SaveSetupRequested { .. })),
        "{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, AppEvent::SaveSetupClosed { .. }))
            .count(),
        1
    );
}
