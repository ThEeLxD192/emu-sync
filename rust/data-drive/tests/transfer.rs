mod common;

use common::{read_request, reply};
use emusync_data_drive::{
    ConditionalError, CreatedFile, DriveTransfer, ResponseMetadataError, TransferError,
    UploadError, UploadedFile,
};
use emusync_domain::sync::{CloudSaveTransfer, RemoteState};
use reqwest::{Client, StatusCode, Url};
use std::{net::TcpListener, sync::Arc, thread};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn missing_timestamp_after_success_retains_created_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        common::reply_with_headers(&mut s, "200 OK", "Location: /session\r\n", "");
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        let body = r#"{"id":"created-id","name":"save.dat"}"#;
        reply(&mut s, "200 OK", body);
    });
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"abc").unwrap();
    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );
    let result = CloudSaveTransfer::upload(
        &transfer,
        "synthetic",
        save.to_str().unwrap(),
        "save.dat",
        "folder",
    )
    .await;
    assert!(
        matches!(result, Err(TransferError::InvalidCreatedTimestamp(file)) if file.id == "created-id")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn transfer_error_leaves_remote_unchanged_classification() {
    let reqwest_err = Client::new()
        .get("http://127.0.0.1:1")
        .send()
        .await
        .unwrap_err();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/upload", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        use std::io::Write;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nLocation: /session\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();

        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        drop(s);

        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot");
    std::fs::write(&path, b"test").unwrap();

    let error = emusync_data_drive::ResumableUpload::new(Client::new(), url)
        .upload(
            "synthetic",
            &path,
            "test.dat",
            "folder",
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    let session = match error {
        UploadError::RecoveryFailed { session, .. } => *session,
        other => panic!("expected RecoveryFailed, got: {other:?}"),
    };
    server.join().unwrap();

    let uploaded_file = || {
        serde_json::from_str::<UploadedFile>(
            r#"{"id":"f1","title":"t1","modifiedDate":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap()
    };

    let created_file = CreatedFile {
        id: "c1".into(),
        name: "test.dat".into(),
        modified_time: "2026-01-01T00:00:00Z".into(),
        sha256_checksum: None,
        version: None,
        parents: vec![],
    };

    let pre_write_failures = [
        TransferError::Cancelled,
        TransferError::Upload(UploadError::Cancelled),
        TransferError::Upload(UploadError::Io(std::io::Error::other("io error"))),
        TransferError::Upload(UploadError::InvalidSession),
        TransferError::Upload(UploadError::Transport(
            Client::new()
                .get("http://127.0.0.1:1")
                .send()
                .await
                .unwrap_err(),
        )),
        TransferError::Upload(UploadError::Http(StatusCode::UNAUTHORIZED)),
        TransferError::Conditional(ConditionalError::Changed),
        TransferError::Conditional(ConditionalError::MissingStrongEtag),
        TransferError::Conditional(ConditionalError::ControlMetadata(
            ResponseMetadataError::Inactive,
        )),
        TransferError::Conditional(ConditionalError::InvalidEndpoint),
        TransferError::Conditional(ConditionalError::Cancelled),
        TransferError::Conditional(ConditionalError::Transport(
            Client::new()
                .get("http://127.0.0.1:1")
                .send()
                .await
                .unwrap_err(),
        )),
        TransferError::Conditional(ConditionalError::Io(std::io::Error::other("io error"))),
        TransferError::Conditional(ConditionalError::Http(StatusCode::SERVICE_UNAVAILABLE)),
    ];

    for failure in &pre_write_failures {
        assert!(
            failure.leaves_remote_unchanged(),
            "expected {failure:?} to leave remote unchanged"
        );
    }

    let uncertain_outcome_failures = [
        TransferError::Upload(UploadError::OutcomeUnknown(Box::new(session.clone()))),
        TransferError::Upload(UploadError::RecoveryFailed {
            session: Box::new(session),
            cause: Box::new(UploadError::NoProgress),
        }),
        TransferError::Conditional(ConditionalError::OutcomeUnknown(reqwest_err)),
        TransferError::Conditional(ConditionalError::HeadersTimedOut),
        TransferError::Conditional(ConditionalError::CancelledOutcomeUnknown),
        TransferError::Conditional(ConditionalError::ServerOutcomeUnknown(
            StatusCode::INTERNAL_SERVER_ERROR,
        )),
        TransferError::Conditional(ConditionalError::ResponseMetadata(
            ResponseMetadataError::Inactive,
        )),
        TransferError::Conditional(ConditionalError::UnexpectedResponse(Box::new(
            uploaded_file(),
        ))),
        TransferError::InvalidCreatedTimestamp(Box::new(created_file)),
        TransferError::InvalidUpdatedTimestamp(Box::new(uploaded_file())),
    ];

    for failure in &uncertain_outcome_failures {
        assert!(
            !failure.leaves_remote_unchanged(),
            "expected {failure:?} to NOT leave remote unchanged"
        );
    }

    assert!(std::error::Error::source(&TransferError::Cancelled).is_none());
    assert!(
        std::error::Error::source(&TransferError::Upload(UploadError::InvalidSession)).is_some()
    );
    assert!(std::error::Error::source(&UploadError::InvalidSession).is_none());
}

#[tokio::test]
async fn invalid_updated_timestamp_after_conditional_replace_retains_uploaded_file_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = Url::parse(&format!("http://{}/files/", listener.local_addr().unwrap())).unwrap();
    let upload_base = Url::parse(&format!(
        "http://{}/upload/",
        listener.local_addr().unwrap()
    ))
    .unwrap();

    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        let get_body = r#"{"id":"file-1","title":"save.dat","modifiedDate":"2026-01-01T00:00:00Z","version":"1","etag":"\"v1\""}"#;
        reply(&mut s, "200 OK", get_body);

        let (mut s, _) = listener.accept().unwrap();
        let (headers, body) = read_request(&mut s);
        assert!(headers.starts_with("PUT /upload/file-1?"));
        assert_eq!(body, b"test-save");
        let put_body = r#"{"id":"file-1","title":"save.dat","modifiedDate":"not-a-valid-rfc3339","version":"2","etag":"\"v2\""}"#;
        reply(&mut s, "200 OK", put_body);
    });

    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    std::fs::write(&save, b"test-save").unwrap();

    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        upload_base.clone(),
        base.clone(),
        upload_base,
        CancellationToken::new(),
        Arc::new(|_| {}),
    );

    let observed = RemoteState {
        id: "file-1".into(),
        modified: 1_700_000_000,
        sha256: None,
        version: Some("1".into()),
    };

    let result = CloudSaveTransfer::replace(
        &transfer,
        "synthetic",
        &observed,
        save.to_str().unwrap(),
        "save.dat",
    )
    .await;

    match result {
        Err(err) => {
            assert!(!err.leaves_remote_unchanged());
            match err {
                TransferError::InvalidUpdatedTimestamp(file) => {
                    assert_eq!(file.id, "file-1");
                    assert_eq!(file.version.as_deref(), Some("2"));
                }
                other => panic!("expected InvalidUpdatedTimestamp, got: {other:?}"),
            }
        }
        other => panic!("expected Err(InvalidUpdatedTimestamp), got: {other:?}"),
    }

    server.join().unwrap();
}

#[tokio::test]
async fn pre_cancelled_drive_transfer_replace_aborts_without_network_and_leaves_remote_unchanged() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = Url::parse(&format!("http://{}/files/", listener.local_addr().unwrap())).unwrap();

    let cancel = CancellationToken::new();
    cancel.cancel();

    let transfer = DriveTransfer::new(
        Client::new(),
        base.clone(),
        base.clone(),
        base.clone(),
        base,
        cancel,
        Arc::new(|_| {}),
    );

    let observed = RemoteState {
        id: "file-1".into(),
        modified: 1_700_000_000,
        sha256: None,
        version: Some("1".into()),
    };

    let result = CloudSaveTransfer::replace(
        &transfer,
        "synthetic",
        &observed,
        "nonexistent-path",
        "save.dat",
    )
    .await;

    match result {
        Err(err) => {
            assert!(err.leaves_remote_unchanged());
            assert!(matches!(err, TransferError::Cancelled));
        }
        other => panic!("expected Cancelled, got: {other:?}"),
    }

    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
