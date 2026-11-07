use emusync_data_local::{SyncHistoryStore, TrackedFileSync, file_sha256};
use emusync_domain::sync::{CloudSaveTransfer, Decision, LocalSaveFiles, RemoteState};
use std::{fs, path::Path, sync::Arc};

struct Transfer {
    remote: RemoteState,
    source_to_change: Option<std::path::PathBuf>,
}

impl CloudSaveTransfer for Transfer {
    type Error = std::io::Error;
    async fn download(
        &self,
        _: &str,
        _: &str,
        destination: &str,
        _: Option<i128>,
        _: &str,
    ) -> Result<(), Self::Error> {
        fs::write(destination, b"remote")
    }
    async fn upload(
        &self,
        _: &str,
        source: &str,
        _: &str,
        _: &str,
    ) -> Result<RemoteState, Self::Error> {
        assert_eq!(fs::read(source)?, b"local");
        if let Some(path) = &self.source_to_change {
            fs::write(path, b"newer")?;
        }
        Ok(self.remote.clone())
    }
    async fn replace(
        &self,
        token: &str,
        _: &RemoteState,
        source: &str,
        name: &str,
    ) -> Result<RemoteState, Self::Error> {
        self.upload(token, source, name, "").await
    }
}

#[test]
fn tracked_transfers_verify_content_record_history_and_keep_local_changes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.dat");
        fs::write(&path, b"local").unwrap();
        let hash = file_sha256(&path).unwrap();
        let remote = RemoteState {
            id: "id".into(),
            modified: 1_780_000_000_000_000_000,
            sha256: Some(hash),
            version: Some("1".into()),
        };
        let history = Arc::new(SyncHistoryStore::new(&dir.path().join("config.json")).unwrap());
        let sync = TrackedFileSync::new(
            Transfer {
                remote: remote.clone(),
                source_to_change: Some(path.clone()),
            },
            history.clone(),
        );
        let name = path.to_str().unwrap();
        assert!(
            sync.upload("token", name, None, "folder", "save.dat")
                .await
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"newer");
        let key = history.key(Path::new(name), "folder", "save.dat").unwrap();
        assert!(history.get(&key).unwrap().is_some());
        assert_eq!(
            sync.plan(name, Some(&remote), "folder", "save.dat")
                .await
                .unwrap(),
            Decision::UploadLocal
        );
        assert!(
            sync.download("token", &remote, name, "folder", "save.dat")
                .await
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"newer");
        let remote_file = dir.path().join("remote");
        fs::write(&remote_file, b"remote").unwrap();
        let remote = RemoteState {
            sha256: Some(file_sha256(&remote_file).unwrap()),
            ..remote
        };
        sync.download("token", &remote, name, "folder", "save.dat")
            .await
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"remote");
        assert_eq!(
            sync.plan(name, Some(&remote), "folder", "save.dat")
                .await
                .unwrap(),
            Decision::InSync
        );
    });
}
