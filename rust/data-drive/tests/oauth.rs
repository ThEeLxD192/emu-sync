use emusync_data_drive::{OAuth, OAuthError};
use emusync_data_local::ConfigManager;
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, CredentialStore, Credentials},
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_grant_with_interactive_allowed_clears_stale_refresh_and_completes_interactive_exchange()
 {
    let (_dir, store, config) = setup(Some("old"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut data = [0; 4096];
        let n = stream.read(&mut data).unwrap();
        let req = String::from_utf8_lossy(&data[..n]);
        assert!(req.starts_with("POST /token"));
        assert!(req.contains("grant_type=refresh_token"));
        let body = r#"{"error":"invalid_grant"}"#;
        write!(
            stream,
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();

        let (mut stream, _) = listener.accept().unwrap();
        let n = stream.read(&mut data).unwrap();
        let req = String::from_utf8_lossy(&data[..n]);
        assert!(req.starts_with("POST /token"));
        assert!(req.contains("grant_type=authorization_code"));
        let body = r#"{"access_token":"fresh-access","refresh_token":"fresh-refresh"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });

    let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);
    let (access, snapshot) = oauth
        .authorize_scoped(&config, true, |_| async { Ok("interactive-code".into()) })
        .await
        .unwrap();

    assert_eq!(access, "fresh-access");
    assert_eq!(snapshot, store.snapshot(&config).unwrap());
    assert_eq!(snapshot.revision, 3);
    assert_eq!(
        store
            .load()
            .unwrap()
            .credentials
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("fresh-refresh")
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_invalid_grant_refresh_failure_preserves_existing_refresh_token_and_never_starts_interactive()
 {
    let (_dir, store, config) = setup(Some("old"));
    let (url, seen, go, server) =
        server("400 Bad Request", r#"{"error":"temporarily_unavailable"}"#);
    let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);

    let interactive_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let spy = interactive_started.clone();

    let task = tokio::spawn(async move {
        oauth
            .authorize(&config, true, |_| {
                spy.store(true, std::sync::atomic::Ordering::SeqCst);
                async { Ok("code".into()) }
            })
            .await
    });

    seen.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    go.send(()).unwrap();

    let result = task.await.unwrap();
    assert!(matches!(result, Err(OAuthError::Provider(_))));
    assert!(!interactive_started.load(std::sync::atomic::Ordering::SeqCst));
    server.join().unwrap();

    assert_eq!(
        store
            .load()
            .unwrap()
            .credentials
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("old")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interactive_exchange_aborts_on_concurrent_credential_mutation_and_rejects_blank_access_tokens()
 {
    {
        let (_dir, store, config) = setup(None);
        let (url, seen, go, server) = server(
            "200 OK",
            r#"{"access_token":"fresh","refresh_token":"new-refresh"}"#,
        );
        let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);

        let task_config = config.clone();
        let task = tokio::spawn(async move {
            oauth
                .authorize(&task_config, true, |_| async { Ok("code".into()) })
                .await
        });

        seen.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        store
            .update(|mut current| {
                current.credentials = Some(Credentials {
                    client_id: "mutated-client".into(),
                    client_secret: "mutated-secret".into(),
                    refresh_token: Some("mutated-refresh".into()),
                });
                current
            })
            .unwrap();
        go.send(()).unwrap();

        let result = task.await.unwrap();
        assert!(matches!(result, Err(OAuthError::ConnectionChanged)));
        server.join().unwrap();

        let loaded = store.load().unwrap().credentials.unwrap();
        assert_eq!(loaded.client_id, "mutated-client");
        assert_eq!(loaded.refresh_token.as_deref(), Some("mutated-refresh"));
    }

    {
        let (_dir, store, _) = setup(None);
        let oauth = OAuth::new(
            Client::new(),
            Shared(store.clone()),
            Url::parse("http://127.0.0.1:1/token").unwrap(),
        );

        for (id, secret) in [("", "secret"), ("client", "  "), (" ", " ")] {
            let config = AppConfig {
                credentials: Some(Credentials {
                    client_id: id.into(),
                    client_secret: secret.into(),
                    refresh_token: None,
                }),
                entries: vec![],
            };
            let result = oauth
                .authorize(&config, true, |_| async { Ok("code".into()) })
                .await;
            assert!(matches!(result, Err(OAuthError::MissingCredentials)));
        }
    }

    {
        let (_dir, store, config) = setup(None);
        let (url, seen, go, server) = server(
            "200 OK",
            r#"{"access_token":"   ","refresh_token":"should-not-persist"}"#,
        );
        let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);

        let task = tokio::spawn(async move {
            oauth
                .authorize(&config, true, |_| async { Ok("code".into()) })
                .await
        });

        seen.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        go.send(()).unwrap();

        let result = task.await.unwrap();
        match result {
            Err(OAuthError::Provider(msg)) => {
                assert_eq!(msg, "missing access token");
            }
            other => panic!("expected Provider('missing access token'), got: {other:?}"),
        }
        server.join().unwrap();

        assert_eq!(
            store.load().unwrap().credentials.unwrap().refresh_token,
            None
        );
    }

    {
        let (_dir, store, _) = setup(None);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
        let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);

        let stale_config = AppConfig {
            credentials: Some(Credentials {
                client_id: "stale-client".into(),
                client_secret: "stale-secret".into(),
                refresh_token: None,
            }),
            entries: vec![],
        };

        let result = oauth
            .authorize_scoped(&stale_config, true, |_| async { Ok("code".into()) })
            .await;
        assert!(matches!(result, Err(OAuthError::ConnectionChanged)));

        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    {
        let (_dir, store, config) = setup(Some("old"));
        let (url, seen, go, server) = server("200 OK", r#"{"access_token":"   "}"#);
        let oauth = OAuth::new(Client::new(), Shared(store.clone()), url);

        let task = tokio::spawn(async move {
            oauth
                .authorize(&config, true, |_| async { Ok("code".into()) })
                .await
        });

        seen.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        go.send(()).unwrap();

        let result = task.await.unwrap();
        match result {
            Err(OAuthError::Provider(msg)) => {
                assert_eq!(msg, "missing access token");
            }
            other => panic!("expected Provider('missing access token'), got: {other:?}"),
        }
        server.join().unwrap();
    }
}
