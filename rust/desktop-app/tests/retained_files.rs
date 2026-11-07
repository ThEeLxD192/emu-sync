use emusync_desktop_app::{RetainedFiles, RetainedFilesError};
use emusync_domain::sync::{Decision, LocalSaveFiles, RemoteState};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct SnapshotFailure {
    snapshot: tempfile::TempDir,
}
struct FailingTransfer {
    calls: Arc<AtomicUsize>,
    snapshot: PathBuf,
}

impl LocalSaveFiles for FailingTransfer {
    type Error = SnapshotFailure;
    async fn plan(
        &self,
        _: &str,
        _: Option<&RemoteState>,
        _: &str,
        _: &str,
    ) -> Result<Decision, Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Decision::InSync)
    }
    async fn remember(
        &self,
        _: &str,
        _: &RemoteState,
        _: &str,
        _: &str,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
    async fn download(
        &self,
        _: &str,
        _: &RemoteState,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
    async fn upload(
        &self,
        _: &str,
        _: &str,
        _: Option<&RemoteState>,
        _: &str,
        _: &str,
    ) -> Result<(), Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let snapshot = tempfile::tempdir_in(&self.snapshot).unwrap();
        fs::write(snapshot.path().join("save"), b"save before upload").unwrap();
        Err(SnapshotFailure { snapshot })
    }
}

#[tokio::test]
async fn failure_retains_snapshot_blocks_following_work_and_transfers_only_after_clones_drop() {
    let parent = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let files = RetainedFiles::new(FailingTransfer {
        calls: calls.clone(),
        snapshot: parent.path().to_path_buf(),
    });
    let operation = files.clone();
    assert_eq!(
        operation
            .upload("token", "save", None, "folder", "save")
            .await,
        Err(RetainedFilesError::TransferFailed)
    );
    assert_eq!(
        files.plan("save", None, "folder", "save").await,
        Err(RetainedFilesError::RecoveryRequired)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let files = match files.finish() {
        Ok(_) => panic!("operation still owns a clone"),
        Err(files) => files,
    };
    drop(operation);
    let failure = match files.finish() {
        Ok(Some(failure)) => failure,
        _ => panic!("failure must be retained"),
    };
    let path = failure.snapshot.path().join("save");
    assert_eq!(fs::read(&path).unwrap(), b"save before upload");
    drop(failure);
    assert!(!path.exists());
}

#[tokio::test]
async fn supervised_operation_retains_snapshot_and_exclusion_until_recovered() {
    use emusync_desktop_app::{OperationFailure, OperationSupervisor, StartError};
    let parent = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut supervisor = OperationSupervisor::<SnapshotFailure>::new();
    let transfer = FailingTransfer {
        calls,
        snapshot: parent.path().to_path_buf(),
    };
    let result = supervisor
        .start(move |_| async move {
            let files = RetainedFiles::new(transfer);
            let operation = files.clone();
            let status = operation
                .upload("token", "save", None, "folder", "save")
                .await;
            assert_eq!(status, Err(RetainedFilesError::TransferFailed));
            drop(operation);
            match files.finish() {
                Ok(Some(failure)) => Err(failure),
                _ => panic!("operation must transfer its failure to the supervisor"),
            }
        })
        .unwrap();
    assert_eq!(result.await.unwrap(), None::<()>);
    assert!(matches!(
        supervisor.start(|_| async { Ok(()) }),
        Err(StartError::Busy)
    ));
    let failure = supervisor.failure().unwrap();
    let path = {
        let guard = failure.lock().unwrap();
        match &*guard {
            OperationFailure::Operation(failure) => failure.snapshot.path().join("save"),
            OperationFailure::Panic => panic!("unexpected panic"),
        }
    };
    assert_eq!(fs::read(&path).unwrap(), b"save before upload");
    assert!(!supervisor.resolve());
    drop(failure);
    // A real caller must reconcile the remote outcome before releasing the permit.
    assert!(supervisor.resolve());
    assert!(!path.exists());
    assert!(supervisor.start(|_| async { Ok(()) }).is_ok());
    supervisor.shutdown().await;
}
