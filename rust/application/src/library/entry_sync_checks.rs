//! Tracks freshness of entry sync checks against operations, connection changes, and edits.

use crate::operation::OperationCoordinator;
use emusync_domain::config::AppConfig;
use emusync_domain::library::GameEntry;
use emusync_domain::sync::{EntrySyncStatus, SaveSynchronizer, SaveSynchronizerFactory};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncCheckRequest {
    pub entry: GameEntry,
    pub config: AppConfig,
    pub generation: u64,
    pub operation_revision: u64,
}

pub struct EntrySyncChecks<F> {
    operations: Arc<OperationCoordinator>,
    synchronizers: F,
    generation: AtomicU64,
    checks: Mutex<HashMap<String, u64>>,
}

impl<F: SaveSynchronizerFactory> EntrySyncChecks<F> {
    pub fn new(operations: Arc<OperationCoordinator>, synchronizers: F) -> Self {
        Self {
            operations,
            synchronizers,
            generation: AtomicU64::new(0),
            checks: Mutex::new(HashMap::new()),
        }
    }

    pub fn begin(&self, entry: &GameEntry, config: &AppConfig) -> SyncCheckRequest {
        let request = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.checks
            .lock()
            .unwrap()
            .insert(entry.common().name.clone(), request);
        SyncCheckRequest {
            entry: entry.clone(),
            config: config.clone(),
            generation: request,
            operation_revision: self.operations.revision(),
        }
    }

    pub fn is_current(&self, request: &SyncCheckRequest, current: Option<&AppConfig>) -> bool {
        let checks = self.checks.lock().unwrap();
        checks.get(&request.entry.common().name) == Some(&request.generation)
            && self.operations.revision() == request.operation_revision
            && !self.operations.is_busy()
            && current.map(|c| &c.credentials) == Some(&request.config.credentials)
            && current.is_some_and(|c| c.entries.iter().any(|e| e == &request.entry))
    }

    pub async fn read(
        &self,
        request: &SyncCheckRequest,
    ) -> Result<EntrySyncStatus, <F::Synchronizer as SaveSynchronizer>::Error> {
        let synchronizer = self.synchronizers.create(request.config.clone(), |_| {});
        synchronizer.check_entry(&request.entry).await
    }
}
