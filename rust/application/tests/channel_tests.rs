use emusync_application::sync::oneshot;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

struct TestWaker {
    woken: AtomicBool,
}

impl Wake for TestWaker {
    fn wake(self: Arc<Self>) {
        self.woken.store(true, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn send_before_await_delivers_value() {
    let (tx, rx) = oneshot();
    assert_eq!(tx.send(123), Ok(()));
    assert_eq!(rx.await, Some(123));
}

#[tokio::test]
async fn receive_before_send_wakes_and_delivers_value() {
    let (tx, rx) = oneshot();
    let (polled_tx, polled_rx) = tokio::sync::oneshot::channel();

    let task = tokio::spawn(async move {
        let mut rx = std::pin::pin!(rx);
        let mut polled_tx = Some(polled_tx);
        std::future::poll_fn(move |cx| {
            let res = rx.as_mut().poll(cx);
            if let Some(tx) = polled_tx.take() {
                let _ = tx.send(());
            }
            res
        })
        .await
    });

    polled_rx.await.expect("task must poll receiver first");
    assert_eq!(tx.send("hello".to_string()), Ok(()));
    assert_eq!(task.await.unwrap(), Some("hello".to_string()));
}

#[tokio::test]
async fn sender_dropped_without_sending_delivers_none() {
    let (tx, rx) = oneshot::<i32>();
    drop(tx);
    assert_eq!(rx.await, None);
}

#[tokio::test]
async fn receiver_dropped_before_send_returns_error_with_value() {
    let (tx, rx) = oneshot::<String>();
    drop(rx);
    assert_eq!(tx.send("lost".into()), Err("lost".into()));
}

#[test]
fn receiver_waker_replacement_and_concurrent_sender_drop_wake_deterministically_without_sleep() {
    // Subcase A: Waker replacement followed by successful send
    {
        let (tx, mut rx) = oneshot::<&str>();

        let waker_1 = Arc::new(TestWaker {
            woken: AtomicBool::new(false),
        });
        let w1 = Waker::from(Arc::clone(&waker_1));
        let mut cx1 = Context::from_waker(&w1);

        assert_eq!(Pin::new(&mut rx).poll(&mut cx1), Poll::Pending);
        assert!(!waker_1.woken.load(Ordering::SeqCst));

        // Replace waker with waker_2
        let waker_2 = Arc::new(TestWaker {
            woken: AtomicBool::new(false),
        });
        let w2 = Waker::from(Arc::clone(&waker_2));
        let mut cx2 = Context::from_waker(&w2);

        assert_eq!(Pin::new(&mut rx).poll(&mut cx2), Poll::Pending);
        assert!(!waker_1.woken.load(Ordering::SeqCst));
        assert!(!waker_2.woken.load(Ordering::SeqCst));

        // Send value: waker_2 should be woken, waker_1 should NOT be woken
        assert_eq!(tx.send("hello"), Ok(()));
        assert!(!waker_1.woken.load(Ordering::SeqCst));
        assert!(waker_2.woken.load(Ordering::SeqCst));

        // Subsequent poll delivers the value
        assert_eq!(Pin::new(&mut rx).poll(&mut cx2), Poll::Ready(Some("hello")));
    }

    // Subcase B: Waker replacement followed by sender drop
    {
        let (tx, mut rx) = oneshot::<i32>();

        let waker_1 = Arc::new(TestWaker {
            woken: AtomicBool::new(false),
        });
        let w1 = Waker::from(Arc::clone(&waker_1));
        let mut cx1 = Context::from_waker(&w1);

        assert_eq!(Pin::new(&mut rx).poll(&mut cx1), Poll::Pending);
        assert!(!waker_1.woken.load(Ordering::SeqCst));

        let waker_2 = Arc::new(TestWaker {
            woken: AtomicBool::new(false),
        });
        let w2 = Waker::from(Arc::clone(&waker_2));
        let mut cx2 = Context::from_waker(&w2);

        assert_eq!(Pin::new(&mut rx).poll(&mut cx2), Poll::Pending);
        assert!(!waker_1.woken.load(Ordering::SeqCst));
        assert!(!waker_2.woken.load(Ordering::SeqCst));

        // Drop sender without sending
        drop(tx);
        assert!(!waker_1.woken.load(Ordering::SeqCst));
        assert!(waker_2.woken.load(Ordering::SeqCst));

        // Subsequent poll delivers None
        assert_eq!(Pin::new(&mut rx).poll(&mut cx2), Poll::Ready(None));
    }
}
