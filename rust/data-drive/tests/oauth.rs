use emusync_data_drive::{OAuth, OAuthError};
use emusync_data_local::ConfigManager;
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, Credentials},
    library::{GameEntry, NativeGame},
};
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
};

fn setup(refresh: Option<&str>) -> (tempfile::TempDir, Arc<ConfigManager>, AppConfig) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ConfigManager::new(&dir.path().join("config.json")).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
            refresh_token: refresh.map(str::to_owned),
        }),
        entries: vec![],
    };
    store.save(&config).unwrap();
    (dir, store, config)
}
// The wrapper keeps the real file-backed store shared with the test.
struct Shared(Arc<ConfigManager>);
impl emusync_domain::config::CredentialStore for Shared {
    type Error = std::io::Error;
    fn snapshot(
        &self,
        config: &AppConfig,
    ) -> Result<emusync_domain::config::CredentialSnapshot, Self::Error> {
        self.0.snapshot(config)
    }
    fn compare_and_set(
        &self,
        old: &emusync_domain::config::CredentialSnapshot,
        new: Option<Credentials>,
        config: &AppConfig,
    ) -> Result<bool, Self::Error> {
        self.0.compare_and_set(old, new, config)
    }
}
fn server(
    status: &'static str,
    body: &'static str,
) -> (
    Url,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
    thread::JoinHandle<()>,
) {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/token", socket.local_addr().unwrap())).unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = socket.accept().unwrap();
        let mut data = [0; 4096];
        let n = stream.read(&mut data).unwrap();
        assert!(String::from_utf8_lossy(&data[..n]).starts_with("POST /token"));
        seen_tx.send(()).unwrap();
        go_rx.recv().unwrap();
        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    (url, seen_rx, go_tx, handle)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exchange_keeps_concurrent_library_edits() {
    let (_dir, store, config) = setup(None);
    let (url, seen, go, server) = server(
        "200 OK",
        r#"{"access_token":"access","refresh_token":"new"}"#,
    );
    let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);
    let task = tokio::spawn(async move {
        oauth
            .authorize(&config, true, |_| async { Ok("code".into()) })
            .await
    });
    seen.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    store
        .update(|mut c| {
            c.entries.push(GameEntry::Native(NativeGame::default()));
            c
        })
        .unwrap();
    go.send(()).unwrap();
    assert_eq!(task.await.unwrap().unwrap(), "access");
    server.join().unwrap();
    let result = store.load().unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(
        result.credentials.unwrap().refresh_token.as_deref(),
        Some("new")
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_response_is_discarded_after_disconnect_and_reconnect() {
    let (_dir, store, config) = setup(Some("old"));
    let (url, seen, go, server) = server("200 OK", r#"{"access_token":"stale"}"#);
    let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);
    let task = tokio::spawn(async move {
        oauth
            .authorize(&config, false, |_| async {
                panic!("interactive flow must not start")
            })
            .await
    });
    seen.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let original = store.load().unwrap().credentials;
    store.update_credentials(|_| None).unwrap();
    store.update_credentials(|_| original).unwrap();
    go.send(()).unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(OAuthError::ConnectionChanged)
    ));
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_response_is_discarded_after_generic_configuration_aba() {
    let (_dir, store, config) = setup(Some("old"));
    let (url, seen, go, server) = server("200 OK", r#"{"access_token":"stale"}"#);
    let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);
    let task_config = config.clone();
    let task = tokio::spawn(async move {
        oauth
            .authorize(&task_config, false, |_| async {
                panic!("interactive flow must not start")
            })
            .await
    });
    seen.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    store
        .update(|mut current| {
            current.credentials = None;
            current
        })
        .unwrap();
    store
        .update(|mut current| {
            current.credentials = config.credentials.clone();
            current
        })
        .unwrap();
    go.send(()).unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(OAuthError::ConnectionChanged)
    ));
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_grant_clears_only_current_refresh_token() {
    let (_dir, store, config) = setup(Some("old"));
    let (url, seen, go, server) = server("400 Bad Request", r#"{"error":"invalid_grant"}"#);
    let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);
    let task = tokio::spawn(async move {
        oauth
            .authorize(&config, false, |_| async {
                panic!("interactive flow must not start")
            })
            .await
    });
    seen.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    go.send(()).unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(OAuthError::AuthenticationRequired)
    ));
    server.join().unwrap();
    assert_eq!(
        store.load().unwrap().credentials.unwrap().refresh_token,
        None
    );
}

struct SyntheticBrowser {
    callback: std::net::SocketAddr,
}
impl emusync_domain::execution::BrowserLauncher for SyntheticBrowser {
    type Error = std::io::Error;
    fn open(&self, url: &str) -> Result<(), Self::Error> {
        let url = Url::parse(url).unwrap();
        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let address = self.callback;
        thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            write!(stream, "GET /?state={state}&code=synthetic-code HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200"));
        });
        Ok(())
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn composed_browser_callback_exchanges_and_persists_tokens() {
    use emusync_data_drive::InteractiveOAuth;
    use emusync_domain::config::CredentialStore;
    let (_dir, store, config) = setup(None);
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let token_url =
        Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let n = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..n]).starts_with("POST /token"));
        let body = r#"{"access_token":"access","refresh_token":"refresh"}"#;
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let oauth = InteractiveOAuth::with_endpoints(
        Client::new(),
        Shared(store.clone()),
        Arc::new(SyntheticBrowser { callback }),
        token_url,
        Url::parse("https://example.invalid/auth").unwrap(),
        callback,
        format!("http://{callback}"),
    );
    let (access, revision) = oauth.authorize_scoped(&config, true).await.unwrap();
    assert_eq!(revision, store.snapshot(&config).unwrap());
    assert_eq!(access, "access");
    assert_eq!(
        store
            .load()
            .unwrap()
            .credentials
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("refresh")
    );
    server.join().unwrap();
}
