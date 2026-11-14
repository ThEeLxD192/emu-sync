use emusync_data_drive::{InteractiveOAuth, OAuthError};
use emusync_data_local::ConfigManager;
use emusync_domain::{
    config::{AppConfig, CloudAuthorization, ConfigurationStore, CredentialStore, Credentials},
    execution::BrowserLauncher,
};
use reqwest::{Client, Url};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

#[derive(Clone)]
struct Shared(Arc<ConfigManager>);
impl CredentialStore for Shared {
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

fn setup(refresh: Option<&str>) -> (tempfile::TempDir, Arc<ConfigManager>, AppConfig) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ConfigManager::new(&dir.path().join("config.json")).unwrap());
    let config = AppConfig {
        language: Default::default(),
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

struct SyntheticBrowser {
    callback: SocketAddr,
}
impl BrowserLauncher for SyntheticBrowser {
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
            write!(
                stream,
                "GET /?state={state}&code=synthetic-code HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200"));
        });
        Ok(())
    }
}

struct SpyBrowser {
    opened: Arc<AtomicBool>,
}
impl BrowserLauncher for SpyBrowser {
    type Error = std::io::Error;
    fn open(&self, _url: &str) -> Result<(), Self::Error> {
        self.opened.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct FailingBrowser;
impl BrowserLauncher for FailingBrowser {
    type Error = std::io::Error;
    fn open(&self, _url: &str) -> Result<(), Self::Error> {
        Err(std::io::Error::other("no browser found"))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn composed_browser_callback_exchanges_and_persists_tokens() {
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
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_loopback_listener_address_is_rejected_without_opening_browser_or_network() {
    let (_dir, store, config) = setup(None);
    let opened = Arc::new(AtomicBool::new(false));
    let token_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    token_listener.set_nonblocking(true).unwrap();
    let token_url = Url::parse(&format!(
        "http://{}/token",
        token_listener.local_addr().unwrap()
    ))
    .unwrap();

    let non_loopback: SocketAddr = "0.0.0.0:8089".parse().unwrap();
    let oauth = InteractiveOAuth::with_endpoints(
        Client::new(),
        Shared(store.clone()),
        Arc::new(SpyBrowser {
            opened: opened.clone(),
        }),
        token_url,
        Url::parse("https://example.invalid/auth").unwrap(),
        non_loopback,
        format!("http://{non_loopback}"),
    );

    let result = oauth.authorize_scoped(&config, true).await;
    match result {
        Err(OAuthError::Provider(msg)) => {
            assert!(
                msg.contains("loopback"),
                "expected error message to mention loopback, got: {msg}"
            );
        }
        other => panic!("expected Err(OAuthError::Provider), got: {other:?}"),
    }

    assert!(!opened.load(Ordering::SeqCst));
    assert_eq!(
        token_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn occupied_loopback_port_fails_cleanly_without_opening_browser() {
    let (_dir, store, config) = setup(None);
    let opened = Arc::new(AtomicBool::new(false));

    let occupying_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let occupied_addr = occupying_listener.local_addr().unwrap();

    let oauth = InteractiveOAuth::with_endpoints(
        Client::new(),
        Shared(store.clone()),
        Arc::new(SpyBrowser {
            opened: opened.clone(),
        }),
        Url::parse("https://example.invalid/token").unwrap(),
        Url::parse("https://example.invalid/auth").unwrap(),
        occupied_addr,
        format!("http://{occupied_addr}"),
    );

    let result = CloudAuthorization::authorize(&oauth, &config, true).await;
    assert!(matches!(result, Err(OAuthError::Provider(_))));
    assert!(!opened.load(Ordering::SeqCst));

    drop(occupying_listener);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_launcher_failure_aborts_authorization_and_releases_loopback_port() {
    let (_dir, store, config) = setup(None);

    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);

    let oauth = InteractiveOAuth::with_endpoints(
        Client::new(),
        Shared(store.clone()),
        Arc::new(FailingBrowser),
        Url::parse("https://example.invalid/token").unwrap(),
        Url::parse("https://example.invalid/auth").unwrap(),
        callback,
        format!("http://{callback}"),
    );

    let result = oauth.authorize_scoped(&config, true).await;
    match result {
        Err(OAuthError::Provider(msg)) => {
            assert!(
                msg.contains("no browser found"),
                "expected error message to contain 'no browser found', got: {msg}"
            );
        }
        other => panic!("expected Err(OAuthError::Provider), got: {other:?}"),
    }

    let rebind = tokio::net::TcpListener::bind(callback).await;
    assert!(rebind.is_ok());
}
