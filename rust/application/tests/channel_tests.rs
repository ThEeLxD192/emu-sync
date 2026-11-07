use emusync_application::sync::oneshot;
use std::time::Duration;

#[tokio::test]
async fn send_before_await_delivers_value() {
    let (tx, rx) = oneshot();
    assert_eq!(tx.send(123), Ok(()));
    assert_eq!(rx.await, Some(123));
}

#[tokio::test]
async fn receive_before_send_wakes_and_delivers_value() {
    let (tx, rx) = oneshot();
    let task = tokio::spawn(rx);

    // Allow the receiver task to be polled once and register its waker
    tokio::time::sleep(Duration::from_millis(5)).await;

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
