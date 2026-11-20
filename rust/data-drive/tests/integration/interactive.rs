use crate::common;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use emusync_data_drive::{InteractiveOAuth, OAuthError};
use emusync_data_local::ConfigManager;
use emusync_domain::{
    config::{AppConfig, CloudAuthorization, ConfigurationStore, CredentialStore, Credentials},
    execution::BrowserLauncher,
};
use reqwest::Url;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
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
    /// The PKCE challenge of the authorization URL it was asked to open.
    code_challenge: Arc<Mutex<Option<String>>>,
}
impl BrowserLauncher for SyntheticBrowser {
    type Error = std::io::Error;
    fn open(&self, url: &str) -> Result<(), Self::Error> {
        let url = Url::parse(url).unwrap();
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(query["code_challenge_method"], "S256");
        *self.code_challenge.lock().unwrap() = Some(query["code_challenge"].clone());
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
    let reservation = common::listener_outside_ephemeral_range();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let token_url =
        Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (head, form) = common::read_request(&mut stream);
        assert!(head.starts_with("POST /token"));
        let body = r#"{"access_token":"access","refresh_token":"refresh"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        // The form body is a query string; parse it as one.
        let form = String::from_utf8(form).unwrap();
        Url::parse(&format!("http://form.invalid/?{form}"))
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect::<HashMap<String, String>>()
    });
    let code_challenge = Arc::new(Mutex::new(None));
    let oauth = InteractiveOAuth::with_endpoints(
        common::direct_client(),
        Shared(store.clone()),
        Arc::new(SyntheticBrowser {
            callback,
            code_challenge: Arc::clone(&code_challenge),
        }),
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
    let exchange = server.join().unwrap();
    assert_eq!(exchange["grant_type"], "authorization_code");
    assert_eq!(exchange["code"], "synthetic-code");
    // PKCE: the exchange proves it comes from whoever started the authorization, by
    // sending the verifier whose S256 challenge the browser carried.
    assert_eq!(
        code_challenge.lock().unwrap().as_deref(),
        Some(
            URL_SAFE_NO_PAD
                .encode(Sha256::digest(exchange["code_verifier"].as_bytes()))
                .as_str()
        )
    );
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
        common::direct_client(),
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
        common::direct_client(),
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

    let reservation = common::listener_outside_ephemeral_range();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);

    let oauth = InteractiveOAuth::with_endpoints(
        common::direct_client(),
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

/// A browser whose provider answers the authorization with `?state=<state><query_suffix>`
/// and reports the loopback's response.
struct CallbackBrowser {
    callback: SocketAddr,
    query_suffix: &'static str,
    responses: std::sync::mpsc::Sender<String>,
}
impl BrowserLauncher for CallbackBrowser {
    type Error = std::io::Error;
    fn open(&self, url: &str) -> Result<(), Self::Error> {
        let state = Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let address = self.callback;
        let query_suffix = self.query_suffix;
        let responses = self.responses.clone();
        thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            write!(
                stream,
                "GET /?state={state}{query_suffix} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            responses.send(response).unwrap();
        });
        Ok(())
    }
}

/// A token endpoint that must never be called; `accept` reports `WouldBlock` while it was not.
fn unused_token_endpoint() -> (TcpListener, Url) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
    (listener, url)
}

fn callback_oauth(
    store: &Arc<ConfigManager>,
    token_url: Url,
    callback: SocketAddr,
    query_suffix: &'static str,
) -> (
    InteractiveOAuth<Shared, CallbackBrowser>,
    std::sync::mpsc::Receiver<String>,
) {
    let (responses, received) = std::sync::mpsc::channel();
    let oauth = InteractiveOAuth::with_endpoints(
        common::direct_client(),
        Shared(store.clone()),
        Arc::new(CallbackBrowser {
            callback,
            query_suffix,
            responses,
        }),
        token_url,
        Url::parse("https://example.invalid/auth").unwrap(),
        callback,
        format!("http://{callback}"),
    );
    (oauth, received)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn error_callback_ends_authorization_without_contacting_the_token_endpoint() {
    let (_dir, store, config) = setup(None);
    let reservation = common::listener_outside_ephemeral_range();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);
    let (token_listener, token_url) = unused_token_endpoint();
    let (oauth, responses) = callback_oauth(
        &store,
        token_url,
        callback,
        "&error=access_denied&code=smuggled-code",
    );

    let result = oauth.authorize_scoped(&config, true).await;

    match result {
        Err(OAuthError::Provider(message)) => assert_eq!(message, "access_denied"),
        other => panic!("expected the provider error, got {other:?}"),
    }
    let response = responses.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "{response}"
    );
    assert_eq!(
        token_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(store.load().unwrap().credentials, config.credentials);
    assert!(tokio::net::TcpListener::bind(callback).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoning_authorization_after_a_callback_without_code_never_contacts_the_token_endpoint()
{
    let (_dir, store, config) = setup(None);
    let reservation = common::listener_outside_ephemeral_range();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);
    let (token_listener, token_url) = unused_token_endpoint();
    let (oauth, responses) = callback_oauth(&store, token_url, callback, "");

    let task_config = config.clone();
    let task = tokio::spawn(async move { oauth.authorize_scoped(&task_config, true).await });
    let response = tokio::task::spawn_blocking(move || {
        responses.recv_timeout(Duration::from_secs(5)).unwrap()
    })
    .await
    .unwrap();
    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "{response}"
    );
    assert!(!task.is_finished());

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        token_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(store.load().unwrap().credentials, config.credentials);
    assert!(tokio::net::TcpListener::bind(callback).await.is_ok());
}
