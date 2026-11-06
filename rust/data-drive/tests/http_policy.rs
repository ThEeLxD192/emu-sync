use emusync_data_drive::DriveHttpPolicy;
use std::{
    io::{Read, Write},
    net::TcpListener,
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
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 1024];
        assert!(stream.read(&mut request).unwrap() > 0);
        thread::sleep(Duration::from_millis(300));
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
        thread::sleep(Duration::from_millis(400));
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
    server.join().unwrap();
}
