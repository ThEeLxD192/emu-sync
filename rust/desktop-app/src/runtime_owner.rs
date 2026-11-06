use crate::OperationSupervisor;
use futures_util::FutureExt;
use std::{
    future::Future,
    io,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    runtime::{Builder, Runtime},
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseState {
    Pending,
    RecoveryRequired,
    Ready,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closing;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryStartError {
    Closing,
    Busy,
    NoFailure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryPanicked;
/// Owns the runtime and supervisor; the window may exit only after `Ready`.
pub struct RuntimeOwner<E> {
    runtime: Runtime,
    supervisor: Arc<Mutex<OperationSupervisor<E>>>,
    recovery: Option<JoinHandle<()>>,
    recovery_panicked: Arc<AtomicBool>,
    closing: bool,
}
impl<E: Send + 'static> RuntimeOwner<E> {
    pub fn new() -> io::Result<Self> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;
        Ok(Self {
            runtime,
            supervisor: Arc::new(Mutex::new(OperationSupervisor::new())),
            recovery: None,
            recovery_panicked: Arc::new(AtomicBool::new(false)),
            closing: false,
        })
    }
    pub fn handle(&self) -> &tokio::runtime::Handle {
        self.runtime.handle()
    }
    fn reap_recovery(&mut self) {
        if self
            .recovery
            .as_ref()
            .is_some_and(|task| task.is_finished())
            && let Some(task) = self.recovery.take()
            && self.runtime.block_on(task).is_err()
        {
            self.recovery_panicked.store(true, Ordering::Release);
        }
    }
    /// Runs composition under the owned runtime without blocking on operation work.
    pub fn with_supervisor<R>(
        &mut self,
        action: impl FnOnce(&mut OperationSupervisor<E>) -> R,
    ) -> Result<R, Closing> {
        self.reap_recovery();
        if self.closing || self.recovery.is_some() {
            return Err(Closing);
        }
        let mut supervisor = self.supervisor.try_lock().map_err(|_| Closing)?;
        let _entered = self.runtime.enter();
        Ok(action(&mut supervisor))
    }
    /// Run recovery off the UI thread, retaining the supervisor until it finishes.
    pub fn start_recovery<R, F>(
        &mut self,
        action: F,
    ) -> Result<oneshot::Receiver<Result<R, RecoveryPanicked>>, RecoveryStartError>
    where
        R: Send + 'static,
        F: for<'a> FnOnce(
                &'a mut OperationSupervisor<E>,
            ) -> Pin<Box<dyn Future<Output = R> + Send + 'a>>
            + Send
            + 'static,
    {
        self.reap_recovery();
        if self.closing {
            return Err(RecoveryStartError::Closing);
        }
        if self.recovery.is_some() {
            return Err(RecoveryStartError::Busy);
        }
        let guard = self
            .supervisor
            .try_lock()
            .map_err(|_| RecoveryStartError::Busy)?;
        if guard.failure().is_none() {
            return Err(RecoveryStartError::NoFailure);
        }
        drop(guard);
        let supervisor = self.supervisor.clone();
        let panicked = self.recovery_panicked.clone();
        let (tx, rx) = oneshot::channel();
        self.recovery = Some(self.runtime.spawn(async move {
            let mut guard = supervisor.lock().await;
            let result = AssertUnwindSafe(async { action(&mut guard).await })
                .catch_unwind()
                .await;
            if result.is_err() {
                panicked.store(true, Ordering::Release);
            }
            let _ = tx.send(result.map_err(|_| RecoveryPanicked));
        }));
        Ok(rx)
    }
    /// Clear the retained panic only after the caller has resolved its consequences.
    pub fn acknowledge_recovery_panic(&mut self) -> Result<(), Closing> {
        self.reap_recovery();
        if self.closing || self.recovery.is_some() {
            return Err(Closing);
        }
        self.recovery_panicked.store(false, Ordering::Release);
        Ok(())
    }
    pub fn begin_close(&mut self) {
        self.closing = true;
        if let Ok(supervisor) = self.supervisor.try_lock() {
            supervisor.cancel();
        }
    }
    /// Nonblocking on active work. A retained failure or recovery panic prevents shutdown.
    pub fn poll_close(&mut self) -> CloseState {
        self.reap_recovery();
        if !self.closing || self.recovery.is_some() {
            return CloseState::Pending;
        }
        let Ok(mut supervisor) = self.supervisor.try_lock() else {
            return CloseState::Pending;
        };
        supervisor.cancel();
        if supervisor.is_running() {
            return CloseState::Pending;
        }
        self.runtime.block_on(supervisor.shutdown());
        if supervisor.failure().is_some() || self.recovery_panicked.load(Ordering::Acquire) {
            self.closing = false;
            CloseState::RecoveryRequired
        } else {
            CloseState::Ready
        }
    }
}
