use emusync_desktop_app::{OperationFailure, OperationSupervisor, StartError};
use tokio::sync::oneshot;

#[tokio::test]
async fn cancellation_waits_for_cleanup_and_retains_exclusion_until_recovery() {
    let mut supervisor = OperationSupervisor::<&'static str>::new();
    let (started_tx, started_rx) = oneshot::channel();
    let (cleanup_tx, cleanup_rx) = oneshot::channel();
    let result = supervisor
        .start(move |cancel| async move {
            started_tx.send(()).unwrap();
            cancel.cancelled().await;
            cleanup_rx.await.unwrap();
            Err::<(), _>("rollback needed")
        })
        .unwrap();
    started_rx.await.unwrap();
    supervisor.cancel();
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, &str>(()) }),
        Err(StartError::Busy)
    ));
    assert!(supervisor.failure().is_none());
    cleanup_tx.send(()).unwrap();
    assert_eq!(result.await.unwrap(), None);
    supervisor.shutdown().await;
    let failure = supervisor.failure().unwrap();
    assert!(matches!(
        *failure.lock().unwrap(),
        OperationFailure::Operation("rollback needed")
    ));
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, &str>(()) }),
        Err(StartError::Busy)
    ));
    assert!(!supervisor.resolve());
    drop(failure);
    assert!(supervisor.resolve());
    assert_eq!(
        supervisor
            .start(|_| async { Ok::<_, &str>(42) })
            .unwrap()
            .await
            .unwrap(),
        Some(42)
    );
    supervisor.shutdown().await;
}

#[tokio::test]
async fn abandoned_receiver_does_not_discard_failure_and_panic_keeps_exclusion() {
    let mut supervisor = OperationSupervisor::<()>::new();
    fn fail() -> Result<(), ()> {
        panic!("synthetic failure")
    }
    let receiver = supervisor.start(|_| async { fail() }).unwrap();
    drop(receiver);
    supervisor.shutdown().await;
    let failure = supervisor.failure().unwrap();
    assert!(matches!(*failure.lock().unwrap(), OperationFailure::Panic));
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, ()>(()) }),
        Err(StartError::Busy)
    ));
    drop(failure);
    assert!(supervisor.resolve());
}

#[tokio::test]
async fn interrupted_shutdown_can_be_awaited_again_without_losing_worker() {
    use std::time::Duration;
    let mut supervisor = OperationSupervisor::<()>::new();
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let result = supervisor
        .start(move |cancel| async move {
            started_tx.send(()).unwrap();
            cancel.cancelled().await;
            release_rx.await.unwrap();
            Ok::<_, ()>(7)
        })
        .unwrap();
    started_rx.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(25), supervisor.shutdown())
            .await
            .is_err()
    );
    assert!(matches!(
        supervisor.start(|_| async { Ok::<_, ()>(()) }),
        Err(StartError::Busy)
    ));
    release_tx.send(()).unwrap();
    supervisor.shutdown().await;
    assert_eq!(result.await.unwrap(), Some(7));
    assert!(supervisor.start(|_| async { Ok::<_, ()>(()) }).is_ok());
    supervisor.shutdown().await;
}
