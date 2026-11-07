//! Keeps a local transfer failure alive until the operation owner takes it for recovery.
use emusync_domain::sync::{Decision, DirectoryDownload, LocalSaveFiles, RemoteState};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Public errors contain no transfer details: the original error remains owned here.
#[derive(Debug, PartialEq, Eq)]
pub enum RetainedFilesError {
    TransferFailed,
    RecoveryRequired,
}

/// Operation-scoped adapter. Its owner must keep the supervisor's exclusion permit
/// while this adapter and its retained failure are being recovered.
pub struct RetainedFiles<T: LocalSaveFiles> {
    inner: Arc<T>,
    failure: Arc<Mutex<Option<T::Error>>>,
}

impl<T: LocalSaveFiles> Clone for RetainedFiles<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            failure: self.failure.clone(),
        }
    }
}

impl<T: LocalSaveFiles> RetainedFiles<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
            failure: Arc::new(Mutex::new(None)),
        }
    }

    /// Transfer the failure to the owner only after every operation clone has gone.
    /// An active future borrows a clone, so a successful finish cannot race it.
    pub fn finish(self) -> Result<Option<T::Error>, Self> {
        match Arc::try_unwrap(self.failure) {
            Ok(failure) => Ok(failure.into_inner()),
            Err(failure) => Err(Self {
                inner: self.inner,
                failure,
            }),
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
        let mut failure = self.failure.lock().await;
        if failure.is_some() {
            return Err(RetainedFilesError::RecoveryRequired);
        }
        match self.inner.plan(local, remote, folder, remote_name).await {
            Ok(value) => Ok(value),
            Err(error) => {
                *failure = Some(error);
                Err(RetainedFilesError::TransferFailed)
            }
        }
    }

    async fn remember(
        &self,
        local: &str,
        remote: &RemoteState,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let mut failure = self.failure.lock().await;
        if failure.is_some() {
            return Err(RetainedFilesError::RecoveryRequired);
        }
        match self
            .inner
            .remember(local, remote, folder, remote_name)
            .await
        {
            Ok(value) => Ok(value),
            Err(error) => {
                *failure = Some(error);
                Err(RetainedFilesError::TransferFailed)
            }
        }
    }

    async fn download(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let mut failure = self.failure.lock().await;
        if failure.is_some() {
            return Err(RetainedFilesError::RecoveryRequired);
        }
        match self
            .inner
            .download(token, remote, local, folder, remote_name)
            .await
        {
            Ok(value) => Ok(value),
            Err(error) => {
                *failure = Some(error);
                Err(RetainedFilesError::TransferFailed)
            }
        }
    }

    async fn upload(
        &self,
        token: &str,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        let mut failure = self.failure.lock().await;
        if failure.is_some() {
            return Err(RetainedFilesError::RecoveryRequired);
        }
        match self
            .inner
            .upload(token, local, remote, folder, remote_name)
            .await
        {
            Ok(value) => Ok(value),
            Err(error) => {
                *failure = Some(error);
                Err(RetainedFilesError::TransferFailed)
            }
        }
    }

    async fn download_in_directory(
        &self,
        request: DirectoryDownload<'_>,
    ) -> Result<(), Self::Error> {
        let mut failure = self.failure.lock().await;
        if failure.is_some() {
            return Err(RetainedFilesError::RecoveryRequired);
        }
        match self.inner.download_in_directory(request).await {
            Ok(value) => Ok(value),
            Err(error) => {
                *failure = Some(error);
                Err(RetainedFilesError::TransferFailed)
            }
        }
    }
}
