use emusync_desktop_app::coordinator::interaction::{
    ConflictResolver, InteractionControls, RetryRunner,
};
use emusync_domain::sync::{Decision, PostSyncFailure, RequestId, SaveConflict};
use std::sync::{Arc, Mutex, mpsc::channel};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn consecutive_conflicts_are_resolved_in_sequence() {
    let (tx, _rx) = channel();
    let controls = InteractionControls::new(tx, Arc::new(|| {}));

    let decisions = Arc::new(Mutex::new(Vec::new()));
    let decisions_clone = decisions.clone();

    let resolver: ConflictResolver = Arc::new(move |conflict, decision| {
        decisions_clone
            .lock()
            .unwrap()
            .push((conflict.request_id, decision));
    });

    controls.set_conflict_resolver(resolver);

    // First conflict
    let conflict1 = SaveConflict {
        request_id: RequestId(1),
        game_name: "Game".into(),
        local_modified_ms: 100,
        cloud_modified_ms: 200,
    };
    controls.on_conflict(conflict1);

    // Resolve first conflict
    assert!(controls.resolve_conflict(Decision::UploadLocal));

    // Second conflict under same operation
    let conflict2 = SaveConflict {
        request_id: RequestId(2),
        game_name: "Game".into(),
        local_modified_ms: 300,
        cloud_modified_ms: 400,
    };
    controls.on_conflict(conflict2);

    // Resolve second conflict
    assert!(controls.resolve_conflict(Decision::DownloadCloud));

    // Verify both decisions were processed in order
    let recorded = decisions.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![
            (RequestId(1), Decision::UploadLocal),
            (RequestId(2), Decision::DownloadCloud)
        ]
    );

    // A late extra response is rejected
    assert!(!controls.resolve_conflict(Decision::UploadLocal));
}

#[tokio::test]
async fn cancel_active_notifies_cancellation_token() {
    let (tx, _rx) = channel();
    let controls = InteractionControls::new(tx, Arc::new(|| {}));

    let cancel_token = CancellationToken::new();
    controls.set_cancel_token(cancel_token.clone());

    assert!(!cancel_token.is_cancelled());
    controls.cancel_active();
    assert!(cancel_token.is_cancelled());
}

#[tokio::test]
async fn cancel_conflict_resolves_with_decision_conflict() {
    let (tx, _rx) = channel();
    let controls = InteractionControls::new(tx, Arc::new(|| {}));

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let rec = recorded.clone();
    let resolver: ConflictResolver = Arc::new(move |conflict, decision| {
        rec.lock().unwrap().push((conflict.request_id, decision));
    });
    controls.set_conflict_resolver(resolver);

    let conflict = SaveConflict {
        request_id: RequestId(99),
        game_name: "ConflictGame".into(),
        local_modified_ms: 100,
        cloud_modified_ms: 200,
    };
    controls.on_conflict(conflict);

    // Cancel conflict
    controls.cancel_conflict();

    let rec_list = recorded.lock().unwrap().clone();
    assert_eq!(rec_list, vec![(RequestId(99), Decision::Conflict)]);
    // Conflict cleared
    assert!(controls.current_conflict.lock().unwrap().is_none());
}

#[tokio::test]
async fn retry_failure_runner_can_be_installed_and_taken() {
    let (tx, _rx) = channel();
    let controls = InteractionControls::new(tx, Arc::new(|| {}));

    let received_req = Arc::new(Mutex::new(None));
    let rec_req = received_req.clone();
    let runner: RetryRunner = Arc::new(move |failure: PostSyncFailure| {
        let flag = rec_req.clone();
        Box::pin(async move {
            *flag.lock().unwrap() = Some(failure.request_id);
        })
    });

    let dummy_failure = PostSyncFailure {
        request_id: RequestId(42),
        game_name: "TestGame".into(),
        reason: "Temporary network hiccup".into(),
        completed_save_count: 1,
        pending_save_count: 1,
    };

    assert!(!controls.has_retry());
    controls.set_retry(dummy_failure.clone(), runner);
    assert!(controls.has_retry());

    let taken = controls.take_retry();
    assert!(taken.is_some());
    let (failure, run_fn) = taken.unwrap();
    assert_eq!(failure.request_id, RequestId(42));
    assert_eq!(failure.game_name, "TestGame");

    run_fn(failure).await;
    assert_eq!(*received_req.lock().unwrap(), Some(RequestId(42)));

    // After taking, it is None and has_retry is false
    assert!(!controls.has_retry());
    assert!(controls.take_retry().is_none());
}
