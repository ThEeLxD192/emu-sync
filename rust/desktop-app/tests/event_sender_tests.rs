//! The UI draws only when woken, so every event must wake it after it can be received.

use emusync_desktop_app::{AppEvent, EventSender};
use std::sync::{
    Arc, Mutex,
    mpsc::{TryRecvError, channel},
};

#[test]
fn each_event_is_receivable_when_the_ui_is_woken() {
    let (tx, rx) = channel();
    let rx = Arc::new(Mutex::new(rx));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let wake = {
        let rx = rx.clone();
        let seen = seen.clone();
        // The frame a wake starts drains the queue, as the UI does.
        Arc::new(move || {
            let event = rx.lock().unwrap().try_recv();
            seen.lock().unwrap().push(event);
        })
    };
    let sender = EventSender::new(tx, wake);

    sender.send(AppEvent::Toast("first".into())).unwrap();
    sender.clone().send(AppEvent::ExitRequested).unwrap();

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            Ok(AppEvent::Toast("first".into())),
            Ok(AppEvent::ExitRequested)
        ]
    );
    assert_eq!(rx.lock().unwrap().try_recv(), Err(TryRecvError::Empty));
}

#[test]
fn an_event_the_ui_can_no_longer_receive_does_not_wake_it() {
    let (tx, rx) = channel();
    drop(rx);
    let wakes = Arc::new(Mutex::new(0));
    let counter = wakes.clone();
    let sender = EventSender::new(tx, Arc::new(move || *counter.lock().unwrap() += 1));

    assert!(sender.send(AppEvent::ExitRequested).is_err());
    assert_eq!(*wakes.lock().unwrap(), 0);
}
