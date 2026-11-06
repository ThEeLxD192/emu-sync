//! Single-producer single-consumer oneshot channel using standard library primitives.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

struct State<T> {
    value: Option<T>,
    waker: Option<Waker>,
    closed: bool,
}

pub struct OneshotSender<T> {
    state: Arc<Mutex<State<T>>>,
}

pub struct OneshotReceiver<T> {
    state: Arc<Mutex<State<T>>>,
}

impl<T> OneshotSender<T> {
    pub fn send(self, value: T) -> Result<(), T> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(value);
        }
        state.value = Some(value);
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
        Ok(())
    }
}

impl<T> Drop for OneshotSender<T> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Future for OneshotReceiver<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();
        if let Some(val) = state.value.take() {
            Poll::Ready(Some(val))
        } else if state.closed {
            Poll::Ready(None)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    let state = Arc::new(Mutex::new(State {
        value: None,
        waker: None,
        closed: false,
    }));
    (
        OneshotSender {
            state: Arc::clone(&state),
        },
        OneshotReceiver { state },
    )
}
