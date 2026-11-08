//! Keeps a local transfer failure alive until the operation owner takes it for recovery.
use emusync_domain::sync::{Decision, DirectoryDownload, LocalSaveFiles, RemoteState};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// Public errors contain no transfer details: the original error remains owned here.
#[derive(Debug, PartialEq, Eq)]
pub enum RetainedFilesError {
    TransferFailed,
    RecoveryRequired,
}

struct RetainedState<E> {
    error: Option<E>,
    quarantined: bool,
    in_flight: usize,
}

struct InFlightGuard<E> {
    state: Arc<Mutex<RetainedState<E>>>,
    notify: Arc<Notify>,
}

impl<E> Drop for InFlightGuard<E> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.in_flight = state.in_flight.saturating_sub(1);
        if state.in_flight == 0 {
            self.notify.notify_waiters();
        }
    }
}

/// Operation-scoped adapter. Its owner must keep the supervisor's exclusion permit
/// while this adapter and its retained failure are being recovered.
pub struct RetainedFiles<T: LocalSaveFiles> {
    inner: Arc<T>,
    state: Arc<Mutex<RetainedState<T::Error>>>,
    notify: Arc<Notify>,
}

impl<T: LocalSaveFiles> Clone for RetainedFiles<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            state: self.state.clone(),
            notify: self.notify.clone(),
        }
    }
}

impl<T: LocalSaveFiles> RetainedFiles<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
            state: Arc::new(Mutex::new(RetainedState {
                error: None,
                quarantined: false,
                in_flight: 0,
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Transfer the failure to the owner only after every operation clone has gone.
    /// An active future borrows a clone, so a successful finish cannot race it.
    pub fn finish(self) -> Result<Option<T::Error>, Self> {
        let state = match Arc::try_unwrap(self.state) {
            Ok(state) => state,
            Err(state) => {
                return Err(Self {
                    inner: self.inner,
                    state,
                    notify: self.notify,
                });
            }
        };
        Ok(state.into_inner().unwrap().error)
    }

    /// Check if a transfer error has been recorded without quarantining yet.
    pub fn has_error(&self) -> bool {
        self.state.lock().unwrap().error.is_some()
    }

    /// Quarantines this adapter and extracts the error for recovery even if clones still exist.
    /// Waits until any active in-flight transfer has finished. All existing and future calls on
    /// any clone will continue to reject operations with `RecoveryRequired`.
    pub async fn quarantine_and_take_error(&self) -> Option<T::Error> {
        loop {
            let notified = self.notify.notified();
            {
                let mut state = self.state.lock().unwrap();
                state.quarantined = true;
                if state.in_flight == 0 {
                    return state.error.take();
                }
            }
            notified.await;
        }
    }

    fn check_admission(&self) -> Result<InFlightGuard<T::Error>, RetainedFilesError> {
        let mut state = self.state.lock().unwrap();
        if state.quarantined || state.error.is_some() {
            return Err(RetainedFilesError::RecoveryRequired);
        }
        state.in_flight += 1;
        Ok(InFlightGuard {
            state: self.state.clone(),
            notify: self.notify.clone(),
        })
    }

    fn record_result<R>(&self, result: Result<R, T::Error>) -> Result<R, RetainedFilesError> {
        match result {
            Ok(val) => Ok(val),
            Err(err) => {
                let mut state = self.state.lock().unwrap();
                if state.error.is_none() {
                    state.error = Some(err);
                }
                Err(RetainedFilesError::TransferFailed)
            }
        }
    }
}

impl<T> LocalSaveFiles for RetainedFiles<T>
where
    T: LocalSaveFiles + Send + Sync,
    T::Error: Send,
{
    type Error = RetainedFilesError;

    async fn plan(
        &self,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> Result<Decision, Self::Error> {
        let _guard = self.check_admission()?;
        let res = self.inner.plan(local, remote, folder, remote_name).await;
        self.record_result(res)
    }

    async fn remember(
        &self,
        local: &str,
        remote: &RemoteState,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let _guard = self.check_admission()?;
        let res = self
            .inner
            .remember(local, remote, folder, remote_name)
            .await;
        self.record_result(res)
    }

    async fn download(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let _guard = self.check_admission()?;
        let res = self
            .inner
            .download(token, remote, local, folder, remote_name)
            .await;
        self.record_result(res)
    }

    async fn upload(
        &self,
        token: &str,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let _guard = self.check_admission()?;
        let res = self
            .inner
            .upload(token, local, remote, folder, remote_name)
            .await;
        self.record_result(res)
    }

    async fn download_in_directory(
        &self,
        request: DirectoryDownload<'_>,
    ) -> Result<(), Self::Error> {
        let _guard = self.check_admission()?;
        let res = self.inner.download_in_directory(request).await;
        self.record_result(res)
    }
}
