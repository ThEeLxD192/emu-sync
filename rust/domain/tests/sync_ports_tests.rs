use emusync_domain::sync::{Decision, DirectoryDownload, LocalSaveFiles, RemoteState};
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

struct Recorder(Mutex<Vec<[String; 5]>>);

impl LocalSaveFiles for Recorder {
    type Error = ();

    async fn plan(
        &self,
        _local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<Decision, ()> {
        unreachable!("plan is not part of this contract")
    }
    async fn remember(
        &self,
        _local: &str,
        _remote: &RemoteState,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), ()> {
        unreachable!("remember is not part of this contract")
    }
    async fn download(
        &self,
        token: &str,
        remote: &RemoteState,
        local: &str,
        folder: &str,
        remote_name: &str,
    ) -> Result<(), ()> {
        self.0.lock().unwrap().push([
            token.to_owned(),
            remote.id.clone(),
            local.to_owned(),
            folder.to_owned(),
            remote_name.to_owned(),
        ]);
        Ok(())
    }
    async fn upload(
        &self,
        _token: &str,
        _local: &str,
        _remote: Option<&RemoteState>,
        _folder: &str,
        _remote_name: &str,
    ) -> Result<(), ()> {
        unreachable!("upload is not part of this contract")
    }
}

#[test]
fn directory_download_default_forwards_exact_arguments_to_download() {
    let recorder = Recorder(Mutex::new(Vec::new()));
    let remote = RemoteState {
        id: "rid".into(),
        modified: 0,
        sha256: None,
        version: None,
    };
    let request = DirectoryDownload {
        token: "tok",
        remote: &remote,
        local: "/root/sub/save.dat",
        root: "/root",
        relative: "sub/save.dat",
        folder: "folder-id",
        remote_name: "remote-name",
    };
    let mut future = std::pin::pin!(recorder.download_in_directory(request));
    let mut context = Context::from_waker(Waker::noop());
    assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(Ok(())));
    assert_eq!(
        *recorder.0.lock().unwrap(),
        vec![[
            "tok".to_owned(),
            "rid".to_owned(),
            "/root/sub/save.dat".to_owned(),
            "folder-id".to_owned(),
            "remote-name".to_owned(),
        ]]
    );
}
