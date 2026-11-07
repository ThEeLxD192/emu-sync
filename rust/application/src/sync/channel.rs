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
    state: Option<Arc<Mutex<State<T>>>>,
}

pub struct OneshotReceiver<T> {
    state: Arc<Mutex<State<T>>>,
}

impl<T> OneshotSender<T> {
    pub fn send(mut self, value: T) -> Result<(), T> {
        if let Some(state_arc) = self.state.take() {
            let waker = {
                let mut state = state_arc.lock().unwrap();
                if state.closed {
                    return Err(value);
                }
                state.value = Some(value);
                state.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
            Ok(())
        } else {
            Err(value)
        }
    }
}

impl<T> Drop for OneshotSender<T> {
    fn drop(&mut self) {
        if let Some(state_arc) = self.state.take() {
            let waker = {
                let mut state = state_arc.lock().unwrap();
                state.closed = true;
                state.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
}

impl<T> Drop for OneshotReceiver<T> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        state.waker = None;
        state.value = None;
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
            match &mut state.waker {
                Some(w) if w.will_wake(cx.waker()) => {}
                _ => state.waker = Some(cx.waker().clone()),
            }
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
            state: Some(Arc::clone(&state)),
        },
        OneshotReceiver { state },
    )
}
