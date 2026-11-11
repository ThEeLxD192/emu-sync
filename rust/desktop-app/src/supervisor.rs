use futures_util::FutureExt;
use std::{
    future::Future,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, oneshot},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

/// Failure is retained with its operation permit until explicitly resolved.
#[derive(Debug)]
pub enum OperationFailure<E> {
    Operation(E),
    Panic,
}
struct Retained<E> {
    failure: Arc<Mutex<OperationFailure<E>>>,
    _permit: OwnedSemaphorePermit,
}
#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    Busy,
}
/// Owns one supervised operation. Keep this owner alive through shutdown and recovery.
pub struct OperationSupervisor<E> {
    gate: Arc<Semaphore>,
    retained: Arc<Mutex<Option<Retained<E>>>>,
    task: Option<JoinHandle<()>>,
    cancel: Option<CancellationToken>,
}
impl<E> Default for OperationSupervisor<E> {
    fn default() -> Self {
        Self {
            gate: Arc::new(Semaphore::new(1)),
            retained: Arc::new(Mutex::new(None)),
            task: None,
            cancel: None,
        }
    }
}
impl<E: Send + 'static> OperationSupervisor<E> {
    pub fn new() -> Self {
        Self::default()
    }
    /// A dropped receiver never aborts the operation or discards an unresolved error.
    pub fn start<T, F, Fut>(
        &mut self,
        operation: F,
    ) -> Result<oneshot::Receiver<Option<T>>, StartError>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        if self.task.as_ref().is_some_and(|task| !task.is_finished()) {
            return Err(StartError::Busy);
        }
        let permit = self
            .gate
            .clone()
            .try_acquire_owned()
            .map_err(|_| StartError::Busy)?;
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let retained = self.retained.clone();
        let (sender, receiver) = oneshot::channel();
        self.task = Some(tokio::spawn(async move {
            match AssertUnwindSafe(async move { operation(signal).await })
                .catch_unwind()
                .await
            {
                Ok(Ok(value)) => {
                    drop(permit);
                    let _ = sender.send(Some(value));
                }
                outcome => {
                    let failure = match outcome {
                        Ok(Err(error)) => OperationFailure::Operation(error),
                        Err(_) => OperationFailure::Panic,
                        Ok(Ok(_)) => unreachable!(),
                    };
                    // No await between retaining the failure and notifying the receiver.
                    let mut guard = retained.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = Some(Retained {
                        failure: Arc::new(Mutex::new(failure)),
                        _permit: permit,
                    });
                    drop(guard);
                    let _ = sender.send(None);
                }
            }
        }));
        self.cancel = Some(cancel);
        Ok(receiver)
    }
    pub fn is_running(&self) -> bool {
        self.task.as_ref().is_some_and(|task| !task.is_finished())
    }
    pub fn cancel(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }
    /// Retain a failure externally with an acquired permit.
    pub fn retain_failure(&mut self, failure: OperationFailure<E>) -> bool {
        let permit = match self.gate.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => return false,
        };
        let mut guard = self.retained.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(Retained {
            failure: Arc::new(Mutex::new(failure)),
            _permit: permit,
        });
        true
    }
    /// Wait for cooperative cleanup; a failure remains retained after this returns.
    pub async fn shutdown(&mut self) {
        self.cancel();
        if let Some(task) = self.task.as_mut() {
            let _ = task.await;
        }
        self.task.take();
        self.cancel = None;
    }
    /// The owner may inspect or copy recovery information, without releasing exclusion.
    pub fn failure(&self) -> Option<Arc<Mutex<OperationFailure<E>>>> {
        self.retained
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|r| r.failure.clone())
    }
    /// Call only after recovery has completed and the retained resources can be released.
    pub fn resolve(&mut self) -> bool {
        let mut guard = self.retained.lock().unwrap_or_else(|e| e.into_inner());
        if guard
            .as_ref()
            .is_some_and(|r| Arc::strong_count(&r.failure) == 1)
        {
            *guard = None;
            true
        } else {
            false
        }
    }
}
