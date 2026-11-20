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

/// The limit the user has to finish the authorization in the browser.
const AUTHORIZATION_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

#[tokio::test(start_paused = true)]
async fn an_authorization_never_completed_in_the_browser_times_out_and_frees_the_callback_port() {
    let (_dir, store, config) = setup(None);
    let stored_before = store.load().unwrap();
    let reservation = common::listener_outside_ephemeral_range();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);
    let (token_listener, token_url) = unused_token_endpoint();
    let opened = Arc::new(AtomicBool::new(false));
    let oauth = InteractiveOAuth::with_endpoints(
        common::direct_client(),
        Shared(store.clone()),
        Arc::new(SpyBrowser {
            opened: opened.clone(),
        }),
        token_url,
        Url::parse("https://example.invalid/auth").unwrap(),
        callback,
        format!("http://{callback}"),
    );
    let started = tokio::time::Instant::now();

    // The clock is paused: it only moves when nothing else can run, so the limit passes
    // at once in real time while the browser never calls back.
    let result = tokio::time::timeout(
        AUTHORIZATION_LIMIT * 2,
        oauth.authorize_scoped(&config, true),
    )
    .await
    .expect("the authorization must give up on its own");

    assert!(
        matches!(result, Err(OAuthError::TimedOut)),
        "an authorization left unfinished times out, got {result:?}"
    );
    // The limit covers the whole interactive step, opening the browser included (the
    // desktop launcher only starts it and returns at once).
    assert_eq!(started.elapsed(), AUTHORIZATION_LIMIT);
    assert!(opened.load(Ordering::SeqCst), "the browser was opened");
    assert!(
        matches!(token_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "nothing is exchanged without a code"
    );
    assert_eq!(store.load().unwrap(), stored_before);
    assert!(
        TcpListener::bind(callback).is_ok(),
        "the callback port is released when the authorization times out"
    );
}

/// Keeps the paused clock from moving on its own while the test waits on real sockets or
/// threads: tokio does not auto-advance time while a blocking task runs. The clock only
/// moves through `tokio::time::advance` until the returned sender is dropped.
fn hold_paused_clock() -> std::sync::mpsc::Sender<()> {
    let (release, released) = std::sync::mpsc::channel::<()>();
    drop(tokio::task::spawn_blocking(move || {
        let _ = released.recv();
    }));
    release
}

/// A browser that reports the `state` of each authorization URL it is asked to open and
/// otherwise does nothing; the test plays the provider's redirect itself. With
/// `launch_gate`, `open` only returns once the gate receives a message or its sender is
/// dropped, like a launcher slow to start the browser (or one that never returns).
struct ObservedBrowser {
    opened_states: tokio::sync::mpsc::UnboundedSender<String>,
    launch_gate: Option<Mutex<std::sync::mpsc::Receiver<()>>>,
}
impl BrowserLauncher for ObservedBrowser {
    type Error = std::io::Error;
    fn open(&self, url: &str) -> Result<(), Self::Error> {
        let state = Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        self.opened_states.send(state).unwrap();
        if let Some(gate) = &self.launch_gate {
            let _ = gate.lock().unwrap().recv();
        }
        Ok(())
    }
}

fn observed_oauth(
    store: &Arc<ConfigManager>,
    token_url: Url,
    callback: SocketAddr,
    launch_gate: Option<std::sync::mpsc::Receiver<()>>,
) -> (
    InteractiveOAuth<Shared, ObservedBrowser>,
    tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    let (opened_states, received_states) = tokio::sync::mpsc::unbounded_channel();
    let oauth = InteractiveOAuth::with_endpoints(
        common::direct_client(),
        Shared(store.clone()),
        Arc::new(ObservedBrowser {
            opened_states,
            launch_gate: launch_gate.map(Mutex::new),
        }),
        token_url,
        Url::parse("https://example.invalid/auth").unwrap(),
        callback,
        format!("http://{callback}"),
    );
    (oauth, received_states)
}

/// A loopback address no other test binds, free when this returns.
fn free_callback_address() -> SocketAddr {
    let reservation = common::listener_outside_ephemeral_range();
    let callback = reservation.local_addr().unwrap();
    drop(reservation);
    callback
}

/// The form fields of each request a scripted token endpoint answered, and its listener.
type AnsweredTokenRequests = (Vec<HashMap<String, String>>, TcpListener);

/// A token endpoint that answers its first requests with `responses` (status and JSON body)
/// in order, then returns their forms and the listener, left non-blocking so the test can
/// check that no further request arrived.
fn scripted_token_endpoint(
    responses: Vec<(&'static str, &'static str)>,
) -> (Url, thread::JoinHandle<AnsweredTokenRequests>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let mut forms = Vec::new();
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let (head, form) = common::read_request(&mut stream);
            assert!(head.starts_with("POST /token"), "{head}");
            common::reply_json(&mut stream, status, body);
            let form = String::from_utf8(form).unwrap();
            forms.push(
                Url::parse(&format!("http://form.invalid/?{form}"))
                    .unwrap()
                    .query_pairs()
                    .into_owned()
                    .collect(),
            );
        }
        listener.set_nonblocking(true).unwrap();
        (forms, listener)
    });
    (url, server)
}

/// Plays the provider's redirect to the loopback with `query` and returns the raw response.
async fn send_callback(callback: SocketAddr, query: &str) -> std::io::Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(callback).await?;
    stream
        .write_all(
            format!("GET /?{query} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

fn assert_nothing_more_was_requested(token_listener: &TcpListener) {
    assert!(
        matches!(token_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "the token endpoint received a request it should not have"
    );
}

#[tokio::test(start_paused = true)]
async fn a_callback_arriving_a_millisecond_before_the_limit_completes_the_authorization() {
    let (_dir, store, config) = setup(None);
    let callback = free_callback_address();
    let (token_url, token_server) = scripted_token_endpoint(vec![(
        "200 OK",
        r#"{"access_token":"in-time-access","refresh_token":"in-time-refresh"}"#,
    )]);
    let (oauth, mut opened_states) = observed_oauth(&store, token_url, callback, None);
    let _clock_hold = hold_paused_clock();
    let task_config = config.clone();
    let authorization =
        tokio::spawn(async move { oauth.authorize_scoped(&task_config, true).await });

    let state = opened_states.recv().await.expect("the browser is opened");
    tokio::time::advance(AUTHORIZATION_LIMIT - Duration::from_millis(1)).await;
    let response = send_callback(callback, &format!("state={state}&code=in-time-code"))
        .await
        .expect("the loopback still listens before the limit");

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let (access, _) = authorization
        .await
        .unwrap()
        .expect("a callback inside the limit completes the authorization");
    assert_eq!(access, "in-time-access");
    let (forms, token_listener) = token_server.join().unwrap();
    assert_eq!(forms[0]["grant_type"], "authorization_code");
    assert_eq!(forms[0]["code"], "in-time-code");
    assert_nothing_more_was_requested(&token_listener);
    assert_eq!(
        store.load().unwrap().credentials,
        Some(Credentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
            refresh_token: Some("in-time-refresh".into()),
        })
    );
}

#[tokio::test(start_paused = true)]
async fn after_a_timeout_the_late_callback_is_refused_and_a_new_attempt_on_the_same_port_redeems_only_its_own_code()
 {
    let (_dir, store, config) = setup(None);
    let callback = free_callback_address();
    let (token_url, token_server) = scripted_token_endpoint(vec![(
        "200 OK",
        r#"{"access_token":"second-access","refresh_token":"second-refresh"}"#,
    )]);
    let (oauth, mut opened_states) = observed_oauth(&store, token_url, callback, None);
    let oauth = Arc::new(oauth);
    let _clock_hold = hold_paused_clock();

    let first_oauth = Arc::clone(&oauth);
    let first_config = config.clone();
    let first_attempt =
        tokio::spawn(async move { first_oauth.authorize_scoped(&first_config, true).await });
    let stale_state = opened_states.recv().await.expect("the browser is opened");
    tokio::time::advance(AUTHORIZATION_LIMIT).await;
    let first_result = first_attempt.await.unwrap();
    assert!(
        matches!(first_result, Err(OAuthError::TimedOut)),
        "the first attempt times out, got {first_result:?}"
    );
    // The user finishes in the browser of the first attempt only now.
    let late_callback =
        send_callback(callback, &format!("state={stale_state}&code=stale-code")).await;
    assert!(
        matches!(&late_callback, Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused),
        "nobody listens for a callback after the limit, got {late_callback:?}"
    );

    let second_config = config.clone();
    let second_attempt =
        tokio::spawn(async move { oauth.authorize_scoped(&second_config, true).await });
    let fresh_state = opened_states
        .recv()
        .await
        .expect("the browser is opened again");
    assert_ne!(fresh_state, stale_state);
    // The first attempt's tab is reloaded: its code must not be redeemed by the new attempt.
    let stale_response = send_callback(callback, &format!("state={stale_state}&code=stale-code"))
        .await
        .expect("the second attempt listens on the same port");
    assert!(
        stale_response.starts_with("HTTP/1.1 400"),
        "{stale_response}"
    );
    assert!(!second_attempt.is_finished());
    let fresh_response = send_callback(callback, &format!("state={fresh_state}&code=fresh-code"))
        .await
        .unwrap();
    assert!(
        fresh_response.starts_with("HTTP/1.1 200"),
        "{fresh_response}"
    );

    let (access, _) = second_attempt
        .await
        .unwrap()
        .expect("the new attempt completes");
    assert_eq!(access, "second-access");
    let (forms, token_listener) = token_server.join().unwrap();
    assert_eq!(forms.len(), 1);
    assert_eq!(forms[0]["code"], "fresh-code");
    assert_nothing_more_was_requested(&token_listener);
    assert_eq!(
        store
            .load()
            .unwrap()
            .credentials
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("second-refresh")
    );
}

#[tokio::test(start_paused = true)]
async fn a_timeout_after_the_provider_rejected_the_stored_refresh_token_keeps_it_cleared_and_exchanges_nothing()
 {
    let (_dir, store, config) = setup(Some("revoked-refresh"));
    let callback = free_callback_address();
    let (token_url, token_server) =
        scripted_token_endpoint(vec![("400 Bad Request", r#"{"error":"invalid_grant"}"#)]);
    let (oauth, mut opened_states) = observed_oauth(&store, token_url, callback, None);
    let _clock_hold = hold_paused_clock();
    let task_config = config.clone();
    let authorization =
        tokio::spawn(async move { oauth.authorize_scoped(&task_config, true).await });

    opened_states.recv().await.expect("the browser is opened");
    tokio::time::advance(AUTHORIZATION_LIMIT).await;
    let result = authorization.await.unwrap();

    assert!(
        matches!(result, Err(OAuthError::TimedOut)),
        "the unfinished authorization times out, got {result:?}"
    );
    let (forms, token_listener) = token_server.join().unwrap();
    assert_eq!(forms[0]["grant_type"], "refresh_token");
    assert_eq!(forms[0]["refresh_token"], "revoked-refresh");
    assert_nothing_more_was_requested(&token_listener);
    // The rejected refresh token stays cleared: the timeout neither restores it nor stores
    // anything else.
    assert_eq!(
        store.load().unwrap().credentials,
        Some(Credentials {
            client_id: "client".into(),
            client_secret: "secret".into(),
            refresh_token: None,
        })
    );
    assert!(
        TcpListener::bind(callback).is_ok(),
        "the callback port is released"
    );
}

#[tokio::test(start_paused = true)]
async fn a_browser_launcher_that_never_returns_still_times_out_and_frees_the_callback_port() {
    let (_dir, store, config) = setup(None);
    let stored_before = store.load().unwrap();
    let callback = free_callback_address();
    let (token_listener, token_url) = unused_token_endpoint();
    let (launch_release, launch_gate) = std::sync::mpsc::channel();
    let (oauth, mut opened_states) = observed_oauth(&store, token_url, callback, Some(launch_gate));
    let task_config = config.clone();
    let authorization =
        tokio::spawn(async move { oauth.authorize_scoped(&task_config, true).await });

    // The launcher runs as a blocking task, so the paused clock only moves when told to.
    opened_states.recv().await.expect("the launcher is called");
    tokio::time::advance(AUTHORIZATION_LIMIT).await;
    for _ in 0..100 {
        if authorization.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        authorization.is_finished(),
        "a launcher stuck past the limit must not keep the authorization waiting"
    );
    let result = authorization.await.unwrap();

    assert!(
        matches!(result, Err(OAuthError::TimedOut)),
        "got {result:?}"
    );
    assert_eq!(
        token_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(store.load().unwrap(), stored_before);
    assert!(
        TcpListener::bind(callback).is_ok(),
        "the callback port is released while the launcher is still stuck"
    );
    drop(launch_release);
}
