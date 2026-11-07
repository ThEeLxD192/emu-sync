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
        let request = {
            let mut checks = self.checks.lock().unwrap();
            let req = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            checks.insert(entry.common().name.clone(), req);
            req
        };
        SyncCheckRequest {
            entry: entry.clone(),
            config: config.clone(),
            generation: request,
            operation_revision: self.operations.revision(),
        }
    }

    pub fn is_current(&self, request: &SyncCheckRequest, current: Option<&AppConfig>) -> bool {
        if self.operations.is_busy() || self.operations.revision() != request.operation_revision {
            return false;
        }

        let Some(current_config) = current else {
            return false;
        };

        if current_config.credentials != request.config.credentials {
            return false;
        }

        let is_latest = self
            .checks
            .lock()
            .unwrap()
            .get(&request.entry.common().name)
            .is_some_and(|&g| g == request.generation);
        if !is_latest {
            return false;
        }

        current_config
            .entries
            .iter()
            .find(|e| e.common().name == request.entry.common().name)
            .is_some_and(|e| e == &request.entry)
    }

    pub async fn read(
        &self,
        request: &SyncCheckRequest,
    ) -> Result<EntrySyncStatus, <F::Synchronizer as SaveSynchronizer>::Error> {
        let synchronizer = self.synchronizers.create(request.config.clone(), |_| {});
        synchronizer.check_entry(&request.entry).await
    }
}
