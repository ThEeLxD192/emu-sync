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

struct InFlightMockTransfer {
    download_pause: Arc<tokio::sync::Notify>,
    download_resume: Arc<tokio::sync::Notify>,
    calls: Arc<AtomicUsize>,
}

impl LocalSaveFiles for InFlightMockTransfer {
    type Error = &'static str;
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.download_pause.notify_one();
        self.download_resume.notified().await;
        Err("second_error")
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
        Err("first_error")
    }
}

#[tokio::test]
async fn quarantine_and_take_error_waits_for_in_flight_operations_and_preserves_first_error() {
    let download_pause = Arc::new(tokio::sync::Notify::new());
    let download_resume = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));

    let mock = InFlightMockTransfer {
        download_pause: download_pause.clone(),
        download_resume: download_resume.clone(),
        calls: calls.clone(),
    };
    let files = RetainedFiles::new(mock);

    let dl_files = files.clone();
    let remote = RemoteState {
        id: "r1".into(),
        modified: 100,
        sha256: None,
        version: None,
    };
    let dl_remote = remote.clone();
    let dl_handle = tokio::spawn(async move {
        dl_files
            .download("token", &dl_remote, "local", "root", "rel")
            .await
    });

    // Wait until download has entered the mock (in_flight == 1)
    download_pause.notified().await;

    // Fail upload with first_error
    let up_res = files.upload("token", "local", None, "folder", "save").await;
    assert_eq!(up_res, Err(RetainedFilesError::TransferFailed));
    assert!(files.has_error());

    // Spawn quarantine_and_take_error in a task
    let q_files = files.clone();
    let q_handle = tokio::spawn(async move { q_files.quarantine_and_take_error().await });

    // Verify q_handle is still pending because download is in_flight
    tokio::task::yield_now().await;
    assert!(!q_handle.is_finished());

    // Resume download, which fails with "second_error"
    download_resume.notify_one();
    let dl_res = dl_handle.await.unwrap();
    assert_eq!(dl_res, Err(RetainedFilesError::TransferFailed));

    // Now quarantine_and_take_error completes and returns "first_error"
    let taken_error = q_handle.await.unwrap();
    assert_eq!(taken_error, Some("first_error"));

    // Any new call on any clone immediately fails with RecoveryRequired
    let plan_res = files.plan("local", None, "folder", "save").await;
    assert_eq!(plan_res, Err(RetainedFilesError::RecoveryRequired));

    let up_res2 = files.upload("token", "local", None, "folder", "save").await;
    assert_eq!(up_res2, Err(RetainedFilesError::RecoveryRequired));

    let dl_res2 = files
        .download("token", &remote, "local", "root", "rel")
        .await;
    assert_eq!(dl_res2, Err(RetainedFilesError::RecoveryRequired));

    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
