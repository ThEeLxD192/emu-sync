//! Exclusive operation coordinator tracking revisions and cancellation.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationEvent {
    Started,
    Cancelled,
    Finished,
}

/// Serializes mutually exclusive operations and increments generation before and after each run.
pub struct OperationCoordinator {
    generation: AtomicU64,
    busy: AtomicBool,
    on_event: Option<Box<dyn Fn(OperationEvent) + Send + Sync + 'static>>,
}

struct OperationGuard<'a> {
    coordinator: &'a OperationCoordinator,
    completed: bool,
}

impl<'a> Drop for OperationGuard<'a> {
    fn drop(&mut self) {
        struct BusyRelease<'b>(&'b OperationCoordinator);
        impl<'b> Drop for BusyRelease<'b> {
            fn drop(&mut self) {
                self.0.generation.fetch_add(1, Ordering::SeqCst);
                self.0.busy.store(false, Ordering::SeqCst);
            }
        }

        let _release = BusyRelease(self.coordinator);
        if !self.completed {
            self.coordinator.emit(OperationEvent::Cancelled);
        }
        self.coordinator.emit(OperationEvent::Finished);
    }
}

impl OperationCoordinator {
    pub const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            busy: AtomicBool::new(false),
            on_event: None,
        }
    }

    pub fn with_listener(on_event: impl Fn(OperationEvent) + Send + Sync + 'static) -> Self {
        Self {
            generation: AtomicU64::new(0),
            busy: AtomicBool::new(false),
            on_event: Some(Box::new(on_event)),
        }
    }

    fn emit(&self, event: OperationEvent) {
        if let Some(ref listener) = self.on_event {
            listener(event);
        }
    }

    pub fn revision(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub async fn run_if_idle<T>(&self, fut: impl Future<Output = T>) -> Option<T> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }

        let mut guard = OperationGuard {
            coordinator: self,
            completed: false,
        };

        self.generation.fetch_add(1, Ordering::SeqCst);
        self.emit(OperationEvent::Started);

        let result = fut.await;
        guard.completed = true;
        Some(result)
    }
}

impl Default for OperationCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
