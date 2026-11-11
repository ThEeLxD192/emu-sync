//! Synchronization context holding active operations, dependencies, and conflict resolutions.

use super::channel::{OneshotSender, oneshot};
use super::migrator::CloudFolderMigrator;
use super::target_mapping::SaveTargetMapping;
use super::target_resolver::SaveTargetResolver;
use emusync_domain::config::{AppConfig, CloudAuthorization};
use emusync_domain::execution::GameLauncher;
use emusync_domain::sync::{
    CloudFileListing, CloudFolderStore, Decision, LocalSaveAccess, LocalSaveFiles, LocalSavePath,
    PostSyncFailure, RequestId, SaveConflict, SyncEvent,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SyncServices<A, L, F, T, Auth, R> {
    pub files: A,
    pub search: L,
    pub folders: F,
    pub tracked_sync: T,
    pub authorization: Auth,
    pub runner: R,
}

type RetryFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + 'static>;

struct CleanupGuard<'a> {
    pending: &'a Mutex<Option<(RequestId, OneshotSender<Decision>)>>,
    req_id: RequestId,
}

impl<'a> Drop for CleanupGuard<'a> {
    fn drop(&mut self) {
        let mut lock = self.pending.lock().unwrap();
        if lock.as_ref().is_some_and(|(id, _)| *id == self.req_id) {
            *lock = None;
        }
    }
}

pub struct SyncContext<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess,
{
    pub(crate) config: AppConfig,
    pub(crate) services: SyncServices<A, L, F, T, Auth, R>,
    pub(crate) on_status: Arc<dyn Fn(SyncEvent) + Send + Sync + 'static>,
    pub(crate) migrator: CloudFolderMigrator<F, L>,
    pub(crate) mapping: SaveTargetMapping<A>,
    pub(crate) resolver: SaveTargetResolver<A>,
    request_counter: AtomicU64,
    pending_conflict: Mutex<Option<(RequestId, OneshotSender<Decision>)>>,
    pending_retry: Mutex<Option<(RequestId, RetryFn)>>,
}

impl<A, L, F, T, Auth, R> SyncContext<A, L, F, T, Auth, R>
where
    A: LocalSaveAccess + Clone + Send + Sync + 'static,
    A::Path: Clone + Send + Sync + 'static,
    A::Error: std::fmt::Debug + Send + Sync + 'static,
    <A::Path as LocalSavePath>::Error: std::fmt::Debug + Send + Sync + 'static,
    L: CloudFileListing + Clone + Send + Sync + 'static,
    L::Error: std::fmt::Debug + Send + Sync + 'static,
    F: CloudFolderStore + Clone + Send + Sync + 'static,
    F::Error: std::fmt::Debug + Send + Sync + 'static,
    T: LocalSaveFiles + Clone + Send + Sync + 'static,
    Auth: CloudAuthorization + Clone + Send + Sync + 'static,
    R: GameLauncher + Clone + Send + Sync + 'static,
{
    pub fn new(
        config: AppConfig,
        services: SyncServices<A, L, F, T, Auth, R>,
        on_status: impl Fn(SyncEvent) + Send + Sync + 'static,
    ) -> Self {
        let migrator = CloudFolderMigrator::new(services.folders.clone(), services.search.clone());
        let mapping = SaveTargetMapping::new(services.files.clone());
        let resolver = SaveTargetResolver::new(services.files.clone());
        Self {
            config,
            services,
            on_status: Arc::new(on_status),
            migrator,
            mapping,
            resolver,
            request_counter: AtomicU64::new(0),
            pending_conflict: Mutex::new(None),
            pending_retry: Mutex::new(None),
        }
    }

    pub fn next_request_id(&self) -> RequestId {
        RequestId(self.request_counter.fetch_add(1, Ordering::Relaxed) + 1)
    }

    pub async fn request_conflict(
        &self,
        local_modified_ms: i64,
        cloud_modified_ms: i64,
        game_name: String,
    ) -> Decision {
        let req_id = self.next_request_id();
        let conflict = SaveConflict {
            request_id: req_id,
            local_modified_ms,
            cloud_modified_ms,
            game_name,
        };
        let (tx, rx) = oneshot();
        {
            let mut lock = self.pending_conflict.lock().unwrap();
            *lock = Some((req_id, tx));
        }

        let _guard = CleanupGuard {
            pending: &self.pending_conflict,
            req_id,
        };

        (self.on_status)(SyncEvent::Conflict(conflict));

        rx.await.unwrap_or(Decision::Conflict)
    }

    pub fn resolve_conflict(&self, request: &SaveConflict, decision: Decision) {
        let tx = {
            let mut lock = self.pending_conflict.lock().unwrap();
            if lock
                .as_ref()
                .is_some_and(|(id, _)| *id == request.request_id)
            {
                lock.take().map(|(_, tx)| tx)
            } else {
                None
            }
        };
        if let Some(tx) = tx {
            let _ = tx.send(decision);
        }
    }

    pub fn report_post_sync_failure<Fut>(
        &self,
        failure: PostSyncFailure,
        retry: impl FnOnce() -> Fut + Send + 'static,
    ) where
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let req_id = failure.request_id;
        let boxed: RetryFn = Box::new(
            move || -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
                Box::pin(retry())
            },
        );
        *self.pending_retry.lock().unwrap() = Some((req_id, boxed));
        (self.on_status)(SyncEvent::PostSyncFailed(failure));
    }

    pub async fn retry_post_sync(&self, failure: &PostSyncFailure) -> Result<(), String> {
        let retry_fn = {
            let mut lock = self.pending_retry.lock().unwrap();
            if lock
                .as_ref()
                .is_some_and(|(id, _)| *id == failure.request_id)
            {
                lock.take().map(|(_, f)| f)
            } else {
                None
            }
        };
        if let Some(f) = retry_fn {
            f().await
        } else {
            Ok(())
        }
    }
}
