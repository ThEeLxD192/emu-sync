use emusync_desktop_app::{CloseState, Closing, OperationFailure, RuntimeOwner};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

fn finish<E: Send + 'static>(owner: &mut RuntimeOwner<E>) -> CloseState {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = owner.poll_close();
        if status != CloseState::Pending {
            return status;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}
#[test]
fn close_waits_for_cleanup_and_keeps_runtime_alive_until_failure_is_resolved() {
    let mut owner = RuntimeOwner::<&'static str>::new().unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let receiver = owner
        .with_supervisor(|supervisor| {
            supervisor.start(move |cancel| async move {
                started_tx.send(()).unwrap();
                cancel.cancelled().await;
                release_rx.await.unwrap();
                Err::<(), _>("recovery required")
            })
        })
        .unwrap()
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.begin_close();
    assert_eq!(owner.poll_close(), CloseState::Pending);
    assert_eq!(owner.with_supervisor(|_| ()), Err(Closing));
    release_tx.send(()).unwrap();
    assert_eq!(finish(&mut owner), CloseState::RecoveryRequired);
    assert_eq!(receiver.blocking_recv().unwrap(), None);
    owner
        .with_supervisor(|supervisor| {
            let failure = supervisor.failure().unwrap();
            assert!(matches!(
                *failure.lock().unwrap(),
                OperationFailure::Operation("recovery required")
            ));
            assert!(!supervisor.resolve());
            drop(failure);
            assert!(supervisor.resolve());
        })
        .unwrap();
    owner.begin_close();
    assert_eq!(finish(&mut owner), CloseState::Ready);
}
#[test]
fn clean_close_waits_for_successful_worker() {
    let mut owner = RuntimeOwner::<()>::new().unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let receiver = owner
        .with_supervisor(|supervisor| {
            supervisor.start(move |cancel| async move {
                started_tx.send(()).unwrap();
                cancel.cancelled().await;
                Ok::<_, ()>(42)
            })
        })
        .unwrap()
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.begin_close();
    assert_eq!(finish(&mut owner), CloseState::Ready);
    assert_eq!(receiver.blocking_recv().unwrap(), Some(42));
}

#[test]
fn asynchronous_recovery_keeps_close_pending_until_cleanup_finishes() {
    let mut owner = RuntimeOwner::<&'static str>::new().unwrap();
    let failed = owner
        .with_supervisor(|s| s.start(|_| async { Err::<(), _>("recover") }))
        .unwrap()
        .unwrap();
    assert!(failed.blocking_recv().unwrap().is_none());
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let recovery = owner
        .start_recovery(move |supervisor| {
            Box::pin(async move {
                started_tx.send(()).unwrap();
                release_rx.await.unwrap();
                assert!(supervisor.failure().is_some());
                supervisor.resolve()
            })
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    owner.begin_close();
    assert_eq!(owner.poll_close(), CloseState::Pending);
    assert_eq!(owner.with_supervisor(|_| ()), Err(Closing));
    release_tx.send(()).unwrap();
    assert!(recovery.blocking_recv().unwrap().unwrap());
    assert_eq!(finish(&mut owner), CloseState::Ready);
}
#[test]
fn panicking_recovery_keeps_close_blocked_until_acknowledged() {
    use emusync_desktop_app::RecoveryPanicked;
    let mut owner = RuntimeOwner::<()>::new().unwrap();
    let failed = owner
        .with_supervisor(|s| s.start(|_| async { Err::<(), _>(()) }))
        .unwrap()
        .unwrap();
    assert!(failed.blocking_recv().unwrap().is_none());
    fn fail() -> bool {
        panic!("synthetic recovery panic")
    }
    let recovery = owner
        .start_recovery(|_| Box::pin(async { fail() }))
        .unwrap();
    assert_eq!(recovery.blocking_recv().unwrap(), Err(RecoveryPanicked));
    owner.begin_close();
    assert_eq!(finish(&mut owner), CloseState::RecoveryRequired);
    owner.with_supervisor(|s| assert!(s.resolve())).unwrap();
    owner.acknowledge_recovery_panic().unwrap();
    owner.begin_close();
    assert_eq!(finish(&mut owner), CloseState::Ready);
}

#[test]
fn start_recovery_enforces_closing_busy_and_no_failure_preconditions() {
    use emusync_desktop_app::{OperationFailure, RecoveryStartError};

    let mut owner = RuntimeOwner::<&'static str>::new().unwrap();

    // 1. Clean state without failure -> NoFailure
    let err1 = owner.start_recovery(|_| Box::pin(async {}));
    assert!(matches!(err1, Err(RecoveryStartError::NoFailure)));

    // Retain a failure in supervisor
    owner
        .with_supervisor(|s| {
            assert!(s.retain_failure(OperationFailure::Operation("err")));
        })
        .unwrap();

    // 2. Start recovery blocked on channel
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel();
    let _rx = owner
        .start_recovery(move |_| {
            Box::pin(async move {
                let _ = started_tx.send(());
                let _ = release_rx.await;
            })
        })
        .unwrap();

    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    // While recovery is busy, subsequent start_recovery returns Busy
    let err2 = owner.start_recovery(|_| Box::pin(async {}));
    assert!(matches!(err2, Err(RecoveryStartError::Busy)));

    // acknowledge_recovery_panic returns Err(Closing) while recovery is busy
    assert_eq!(owner.acknowledge_recovery_panic(), Err(Closing));

    // Release the first recovery
    let _ = release_tx.send(());

    // Wait until recovery finishes in the runtime
    while let Err(RecoveryStartError::Busy) = owner.start_recovery(|_| Box::pin(async {})) {
        thread::yield_now();
    }

    // Now call begin_close
    owner.begin_close();

    // 3. After begin_close, start_recovery returns Closing
    let err3 = owner.start_recovery(|_| Box::pin(async {}));
    assert!(matches!(err3, Err(RecoveryStartError::Closing)));

    // acknowledge_recovery_panic also returns Err(Closing) when closing is true
    assert_eq!(owner.acknowledge_recovery_panic(), Err(Closing));
}
