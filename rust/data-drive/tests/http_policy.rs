use emusync_data_drive::DriveHttpPolicy;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};
fn policy() -> DriveHttpPolicy {
    DriveHttpPolicy {
        connect: Duration::from_secs(2),
        metadata: Duration::from_millis(150),
        inactivity: Duration::from_millis(600),
    }
}
#[tokio::test]
async fn metadata_request_has_total_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        assert!(stream.read(&mut request).unwrap() > 0);
        let _ = release_rx.recv();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    });
    let clients = policy().build().unwrap();
    let error = clients
        .metadata
        .get(format!("http://{address}/"))
        .send()
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    let _ = release_tx.send(());
    server.join().unwrap();
}
#[tokio::test]
async fn active_transfer_can_exceed_the_metadata_request_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        assert!(stream.read(&mut request).unwrap() > 0);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        stream.flush().unwrap();
        for _ in 0..4 {
            thread::sleep(Duration::from_millis(80));
            stream.write_all(b"a").unwrap();
            stream.flush().unwrap();
        }
    });
    let clients = policy().build().unwrap();
    let bytes = clients
        .transfer
        .get(format!("http://{address}/"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(bytes.as_ref(), b"aaaa");
    server.join().unwrap();
}
#[tokio::test]
async fn inactivity_stops_a_stalled_transfer() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        assert!(stream.read(&mut request).unwrap() > 0);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\na"
        )
        .unwrap();
        stream.flush().unwrap();
        let _ = release_rx.recv();
    });
    let mut policy = policy();
    policy.inactivity = Duration::from_millis(150);
    let clients = policy.build().unwrap();
    let error = clients
        .transfer
        .get(format!("http://{address}/"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap_err();
    assert!(error.is_timeout() || error.is_body());
    let _ = release_tx.send(());
    server.join().unwrap();
}

#[tokio::test]
async fn all_policy_clients_refuse_to_follow_http_redirects_to_trap_origin() {
    let trap_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    trap_listener.set_nonblocking(true).unwrap();
    let trap_url = format!("http://{}/trap", trap_listener.local_addr().unwrap());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_trap_url = trap_url.clone();
    let server = thread::spawn(move || {
        for _ in 0..4 {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0; 1024];
            let _ = s.read(&mut buf).unwrap();
            let response = format!(
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: {server_trap_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            s.write_all(response.as_bytes()).unwrap();
        }
    });

    let clients = DriveHttpPolicy::default().build().unwrap();

    let meta_resp = clients
        .metadata
        .get(format!("http://{server_addr}/meta"))
        .send()
        .await
        .unwrap();
    assert_eq!(meta_resp.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);

    let transfer_resp = clients
        .transfer
        .get(format!("http://{server_addr}/transfer"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        transfer_resp.status(),
        reqwest::StatusCode::TEMPORARY_REDIRECT
    );

    let upload_resp = clients
        .upload
        .put(format!("http://{server_addr}/upload"))
        .body(vec![1, 2, 3])
        .send()
        .await
        .unwrap();
    assert_eq!(
        upload_resp.status(),
        reqwest::StatusCode::TEMPORARY_REDIRECT
    );

    let empty_resp = clients
        .empty_upload
        .put(format!("http://{server_addr}/empty"))
        .send()
        .await
        .unwrap();
    assert_eq!(empty_resp.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);

    assert_eq!(
        trap_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );

    server.join().unwrap();
}
