use emusync_application::{OperationCoordinator, OperationEvent};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mutual_exclusion_rejects_concurrent_operations() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);
    let coordinator = Arc::new(OperationCoordinator::with_listener(move |event| {
        events_clone.lock().unwrap().push(event);
    }));

    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();

    let coord_clone = Arc::clone(&coordinator);
    let h1 = tokio::spawn(async move {
        coord_clone
            .run_if_idle(async move {
                let _ = tx1.send(());
                let _ = rx2.await;
                42
            })
            .await
    });

    rx1.await.unwrap();
    assert!(coordinator.is_busy());

    // Concurrent operation must be rejected with None
    let rejected = coordinator.run_if_idle(async { 99 }).await;
    assert_eq!(rejected, None);

    // Release first operation
    let _ = tx2.send(());
    let res = h1.await.unwrap();
    assert_eq!(res, Some(42));

    assert!(!coordinator.is_busy());
    // Generation was incremented at start (1) and at finish (2)
    assert_eq!(coordinator.revision(), 2);

    // After completion, next operation succeeds
    let next = coordinator.run_if_idle(async { 100 }).await;
    assert_eq!(next, Some(100));
    assert_eq!(coordinator.revision(), 4);

    let recorded_events = events.lock().unwrap().clone();
    assert_eq!(
        recorded_events,
        vec![
            OperationEvent::Started,
            OperationEvent::Finished,
            OperationEvent::Started,
            OperationEvent::Finished,
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_task_guard_drop_increments_revision_publishes_cancelled_and_clears_busy() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);
    let coordinator = Arc::new(OperationCoordinator::with_listener(move |event| {
        events_clone.lock().unwrap().push(event);
    }));

    let (started_tx, started_rx) = tokio::sync::oneshot::channel();

    let coord_clone = Arc::clone(&coordinator);
    let task = tokio::spawn(async move {
        coord_clone
            .run_if_idle(async move {
                let _ = started_tx.send(());
                tokio::time::sleep(Duration::from_secs(10)).await;
            })
            .await
    });

    started_rx.await.unwrap();
    assert!(coordinator.is_busy());
    assert_eq!(coordinator.revision(), 1);

    // Abort the task (simulating cooperative cancel drop or tokio abort)
    task.abort();
    let _ = task.await;

    // Guard drop must clear busy and advance revision
    assert!(!coordinator.is_busy());
    assert_eq!(coordinator.revision(), 2);

    // Subsequent operation is accepted
    let success = coordinator.run_if_idle(async { "ok" }).await;
    assert_eq!(success, Some("ok"));
    assert_eq!(coordinator.revision(), 4);

    let recorded_events = events.lock().unwrap().clone();
    assert_eq!(
        recorded_events,
        vec![
            OperationEvent::Started,
            OperationEvent::Cancelled,
            OperationEvent::Finished,
            OperationEvent::Started,
            OperationEvent::Finished,
        ]
    );
}

#[tokio::test]
async fn coordinator_default_runs_without_listener() {
    let coordinator = OperationCoordinator::new();
    assert!(!coordinator.is_busy());
    assert_eq!(coordinator.revision(), 0);

    let res = coordinator.run_if_idle(async { 123 }).await;
    assert_eq!(res, Some(123));
    assert_eq!(coordinator.revision(), 2);
    assert!(!coordinator.is_busy());
}

#[tokio::test]
async fn panic_in_task_unwinds_safely_and_clears_busy() {
    let coordinator = Arc::new(OperationCoordinator::new());
    let coord_clone = Arc::clone(&coordinator);

    let res = tokio::spawn(async move {
        coord_clone
            .run_if_idle(async {
                panic!("intentional panic inside task");
            })
            .await
    })
    .await;

    assert!(res.is_err());
    assert!(
        !coordinator.is_busy(),
        "busy must be cleared after task panic"
    );
    assert_eq!(coordinator.revision(), 2);

    let ok = coordinator.run_if_idle(async { "recovered" }).await;
    assert_eq!(ok, Some("recovered"));
    assert_eq!(coordinator.revision(), 4);
}

#[tokio::test]
async fn panic_in_listener_unwinds_safely_and_clears_busy() {
    let coordinator = Arc::new(OperationCoordinator::with_listener(|event| {
        if event == OperationEvent::Started {
            panic!("intentional panic on Started event");
        }
    }));

    let coord_clone = Arc::clone(&coordinator);
    let res = tokio::spawn(async move { coord_clone.run_if_idle(async { 42 }).await }).await;

    assert!(res.is_err());
    assert!(
        !coordinator.is_busy(),
        "busy must be cleared if listener panics on Started"
    );
}
