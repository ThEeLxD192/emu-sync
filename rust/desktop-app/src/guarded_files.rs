//! Bind the authorized connection revision to transfers made by the orchestrator.
use emusync_data_local::{RecoveryCredentials, TrackedError, TrackedFileSync, UploadRequest};
use emusync_domain::{
    config::CredentialTransaction,
    sync::{CloudSaveTransfer, Decision, DirectoryDownload, LocalSaveFiles, RemoteState},
};
use std::{io, sync::Arc};

type FailureClassifier<E> = dyn Fn(&E) -> bool + Send + Sync;

pub struct GuardedFiles<T: CloudSaveTransfer, S> {
    tracked: Arc<TrackedFileSync<T>>,
    credentials: RecoveryCredentials<S>,
    definitive_failure: Arc<FailureClassifier<T::Error>>,
}

impl<T: CloudSaveTransfer, S> Clone for GuardedFiles<T, S> {
    fn clone(&self) -> Self {
        Self {
            tracked: self.tracked.clone(),
            credentials: self.credentials.clone(),
            definitive_failure: self.definitive_failure.clone(),
        }
    }
}

impl<T: CloudSaveTransfer, S> GuardedFiles<T, S> {
    pub fn new(
        tracked: Arc<TrackedFileSync<T>>,
        credentials: RecoveryCredentials<S>,
        definitive_failure: impl Fn(&T::Error) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            tracked,
            credentials,
            definitive_failure: Arc::new(definitive_failure),
        }
    }
}

impl<T, S> LocalSaveFiles for GuardedFiles<T, S>
where
    T: CloudSaveTransfer + Send + Sync,
    T::Error: Send,
    S: CredentialTransaction<Error = io::Error> + Send + Sync + 'static,
{
    type Error = TrackedError<T::Error>;
    async fn plan(
        &self,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> Result<Decision, Self::Error> {
        self.tracked.plan(local, remote, folder, remote_name).await
    }
    async fn remember(
        &self,
        local: &str,
        remote: &RemoteState,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        self.tracked
            .remember_guarded(local, remote, folder, remote_name, self.credentials.clone())
            .await
            .map_err(TrackedError::Local)
    }
    async fn download(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        self.tracked
            .download_guarded(
                token,
                remote,
                local,
                folder,
                remote_name,
                self.credentials.clone(),
            )
            .await
    }
    async fn upload(
        &self,
        token: &str,
        local: &str,
        remote: Option<&RemoteState>,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), Self::Error> {
        self.tracked
            .upload_guarded_classified(
                UploadRequest {
                    token,
                    local,
                    remote,
                    folder,
                    remote_name,
                },
                self.credentials.clone(),
                |error| (self.definitive_failure)(error),
            )
            .await
    }

    async fn download_in_directory(
        &self,
        request: DirectoryDownload<'_>,
    ) -> Result<(), Self::Error> {
        #[cfg(target_os = "linux")]
        {
            self.tracked
                .download_directory_guarded(request, self.credentials.clone())
                .await
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.tracked
                .download_guarded(
                    request.token,
                    request.remote,
                    request.local,
                    request.folder,
                    request.remote_name,
                    self.credentials.clone(),
                )
                .await
        }
    }
}
