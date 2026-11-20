use crate::common;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use emusync_data_drive::{AuthorizationCode, OAuthError, loopback_code};
use reqwest::Url;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, net::SocketAddr, sync::mpsc, time::Duration};
use tokio::{net::TcpListener, task::JoinHandle};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_ignores_wrong_state_and_accepts_matching_code() {
    let listener = listener_outside_ephemeral_range();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(
        query.get("scope").unwrap(),
        "https://www.googleapis.com/auth/drive.file"
    );
    assert_eq!(query.get("redirect_uri").unwrap(), "http://localhost:8089");
    assert_eq!(query.get("access_type").unwrap(), "offline");
    let client = common::direct_client();
    assert_eq!(
        client
            .get(format!("http://{address}/?state=wrong&code=bad"))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert!(!task.is_finished());
    assert_eq!(
        client
            .get(format!(
                "http://{address}/?state={}&code=synthetic",
                query["state"]
            ))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let authorization = task.await.unwrap().unwrap();
    assert_eq!(authorization.code, "synthetic");
    // RFC 7636: a 43-character base64url verifier whose S256 challenge went to the browser.
    assert_eq!(authorization.code_verifier.len(), 43);
    assert!(
        authorization
            .code_verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    );
    assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
    assert_eq!(
        query.get("code_challenge").unwrap(),
        &URL_SAFE_NO_PAD.encode(Sha256::digest(authorization.code_verifier.as_bytes()))
    );
    assert_ne!(authorization.code_verifier, query["state"]);
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_releases_listener() {
    let listener = listener_outside_ephemeral_range();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "client",
            Url::parse("https://example.invalid/").unwrap(),
            "http://localhost:8089",
            |_| async move {
                tx.send(()).unwrap();
                Ok(())
            },
        )
        .await
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    task.abort();
    let _ = task.await;
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_rejects_truncated_http_request_and_continues_listening() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = listener_outside_ephemeral_range();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let truncated = format!("GET /?state={state}&code=partial");
    stream.write_all(truncated.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response);
    assert!(
        response_str.starts_with("HTTP/1.1 400 Bad Request"),
        "expected 400 Bad Request but got: {response_str}"
    );

    assert!(!task.is_finished());

    let client = common::direct_client();
    assert_eq!(
        client
            .get(format!("http://{address}/?state={state}&code=valid_code"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(task.await.unwrap().unwrap().code, "valid_code");
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oauth_error_callback_with_matching_state_terminates_and_returns_provider_error_description()
 {
    use emusync_data_drive::OAuthError;

    let listener = listener_outside_ephemeral_range();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let client = common::direct_client();

    let post_resp = client
        .post(format!("http://{address}/?state={state}&code=abc"))
        .send()
        .await
        .unwrap();
    assert_eq!(post_resp.status(), 400);
    assert!(!task.is_finished());

    let err_resp = client
        .get(format!(
            "http://{address}/?error=access_denied&error_description=User+denied+access&state={state}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(err_resp.status(), 400);

    let err = task.await.unwrap().unwrap_err();
    assert!(matches!(err, OAuthError::Provider(ref msg) if msg.contains("User denied access")));
    assert!(TcpListener::bind(address).await.is_ok());

    let listener = listener_outside_ephemeral_range();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let err_resp = client
        .get(format!(
            "http://{address}/?error=access_denied&error_description=%20%20&state={state}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(err_resp.status(), 400);

    let err = task.await.unwrap().unwrap_err();
    assert!(matches!(err, OAuthError::Provider(ref msg) if msg == "access_denied"));
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_request_headers_and_unspecified_bind_address_are_rejected_safely() {
    use emusync_data_drive::OAuthError;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let launched = std::sync::atomic::AtomicBool::new(false);
    let result = loopback_code(
        listener,
        "client",
        Url::parse("https://example.invalid/").unwrap(),
        "http://localhost:8089",
        |_| async {
            launched.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
    )
    .await;
    assert!(matches!(result, Err(OAuthError::Provider(ref msg)) if msg.contains("loopback")));
    assert!(!launched.load(std::sync::atomic::Ordering::SeqCst));

    let listener = listener_outside_ephemeral_range();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let url = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = &query["state"];

    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let oversized = format!(
        "GET /?state={state}&code=x HTTP/1.1\r\nHost: localhost\r\nX-Oversized: {}\r\n\r\n",
        "a".repeat(10240)
    );
    stream.write_all(oversized.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response_str = String::from_utf8_lossy(&response);
    assert!(
        response_str.starts_with("HTTP/1.1 400 Bad Request"),
        "expected 400 Bad Request but got: {response_str}"
    );
    assert!(!task.is_finished());

    let client = common::direct_client();
    let resp = client
        .get(format!("http://{address}/?state={state}&code=good-code"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    assert_eq!(task.await.unwrap().unwrap().code, "good-code");
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consecutive_authorizations_send_different_challenges_and_return_different_verifiers() {
    let mut attempts = Vec::new();
    for code in ["first-code", "second-code"] {
        let (address, browser_url, task) = start_authorization(listener_outside_ephemeral_range());
        let query = query_of(&browser_url);
        let response =
            raw_callback(address, &format!("/?state={}&code={code}", query["state"])).await;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        let authorization = task.await.unwrap().unwrap();
        assert_eq!(authorization.code, code);
        assert_eq!(query["code_challenge"], s256(&authorization.code_verifier));
        attempts.push((query["code_challenge"].clone(), authorization.code_verifier));
    }
    let [
        (first_challenge, first_verifier),
        (second_challenge, second_verifier),
    ] = <[_; 2]>::try_from(attempts).unwrap();
    assert_ne!(first_verifier, second_verifier);
    assert_ne!(first_challenge, second_challenge);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verifier_never_appears_in_the_browser_url_or_the_callback_page() {
    let (address, browser_url, task) = start_authorization(listener_outside_ephemeral_range());
    let query = query_of(&browser_url);
    let response = raw_callback(
        address,
        &format!("/?state={}&code=page-code", query["state"]),
    )
    .await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let authorization = task.await.unwrap().unwrap();

    assert!(!browser_url.as_str().contains(&authorization.code_verifier));
    assert!(!response.contains(&authorization.code_verifier));
    assert!(!query.contains_key("code_verifier"));
    // The browser carries exactly one S256 challenge, never a plain one or the verifier.
    let challenge_parameters: Vec<_> = browser_url
        .query_pairs()
        .filter(|(key, _)| key.starts_with("code_challenge"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        challenge_parameters,
        [
            (
                "code_challenge".to_owned(),
                s256(&authorization.code_verifier)
            ),
            ("code_challenge_method".to_owned(), "S256".to_owned()),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn callback_with_valid_state_but_no_usable_code_is_rejected_and_keeps_waiting() {
    let (address, browser_url, task) = start_authorization(listener_outside_ephemeral_range());
    let state = query_of(&browser_url)["state"].clone();
    for target in [
        format!("/?state={state}"),
        format!("/?state={state}&code="),
        format!("/?state={state}&code=%20%09"),
        format!("/?state={state}&code=&error="),
    ] {
        let response = raw_callback(address, &target).await;
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request"),
            "{target}: {response}"
        );
        assert!(!task.is_finished(), "{target} ended the authorization");
    }
    // Still listening: the next real callback is the one that completes it.
    let response = raw_callback(address, &format!("/?state={state}&code=real-code")).await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let authorization = task.await.unwrap().unwrap();
    assert_eq!(authorization.code, "real-code");
    assert_eq!(
        query_of(&browser_url)["code_challenge"],
        s256(&authorization.code_verifier)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn callback_with_both_code_and_error_returns_the_error_and_no_code() {
    let (address, browser_url, task) = start_authorization(listener_outside_ephemeral_range());
    let state = query_of(&browser_url)["state"].clone();
    let response = raw_callback(
        address,
        &format!("/?state={state}&code=smuggled-code&error=access_denied"),
    )
    .await;
    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "{response}"
    );
    match task.await.unwrap() {
        Err(OAuthError::Provider(message)) => assert_eq!(message, "access_denied"),
        other => panic!("expected the provider error, got {other:?}"),
    }
    assert!(TcpListener::bind(address).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verifier_or_challenge_sent_in_the_callback_is_ignored() {
    let (address, browser_url, task) = start_authorization(listener_outside_ephemeral_range());
    let query = query_of(&browser_url);
    let attacker_verifier = "attacker-chosen-verifier-attacker-chosen-ve";
    let response = raw_callback(
        address,
        &format!(
            "/?state={}&code=real-code&code_verifier={attacker_verifier}&code_challenge={}",
            query["state"],
            s256(attacker_verifier)
        ),
    )
    .await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let authorization = task.await.unwrap().unwrap();
    assert_ne!(authorization.code_verifier, attacker_verifier);
    assert_eq!(query["code_challenge"], s256(&authorization.code_verifier));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborted_authorization_leaves_its_state_and_verifier_unusable_on_the_next_attempt() {
    let listener = listener_outside_ephemeral_range();
    let address = listener.local_addr().unwrap();
    let (_, abandoned_url, abandoned) = start_authorization(listener);
    abandoned.abort();
    assert!(abandoned.await.unwrap_err().is_cancelled());
    let abandoned_query = query_of(&abandoned_url);

    let listener = TcpListener::bind(address).await.unwrap();
    let (_, browser_url, task) = start_authorization(listener);
    let query = query_of(&browser_url);
    assert_ne!(query["state"], abandoned_query["state"]);
    assert_ne!(query["code_challenge"], abandoned_query["code_challenge"]);

    // A late callback of the abandoned attempt reaches the new listener and is refused.
    let response = raw_callback(
        address,
        &format!("/?state={}&code=stale-code", abandoned_query["state"]),
    )
    .await;
    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "{response}"
    );
    assert!(!task.is_finished());

    let response = raw_callback(
        address,
        &format!("/?state={}&code=fresh-code", query["state"]),
    )
    .await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let authorization = task.await.unwrap().unwrap();
    assert_eq!(authorization.code, "fresh-code");
    assert_eq!(query["code_challenge"], s256(&authorization.code_verifier));
    assert_ne!(
        abandoned_query["code_challenge"],
        s256(&authorization.code_verifier)
    );
}

/// Starts `loopback_code` on `listener` and returns its address, the URL it asked the
/// browser to open and the running authorization.
fn start_authorization(
    listener: TcpListener,
) -> (
    SocketAddr,
    Url,
    JoinHandle<Result<AuthorizationCode, OAuthError>>,
) {
    let address = listener.local_addr().unwrap();
    let (browser_tx, browser_rx) = mpsc::channel();
    let task = tokio::spawn(async move {
        loopback_code(
            listener,
            "synthetic client",
            Url::parse("https://example.invalid/authorize").unwrap(),
            "http://localhost:8089",
            |url| async move {
                browser_tx.send(url).unwrap();
                Ok(())
            },
        )
        .await
    });
    let browser_url = browser_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    (address, browser_url, task)
}

fn query_of(url: &Url) -> HashMap<String, String> {
    url.query_pairs().into_owned().collect()
}

/// Sends one browser callback to the loopback and returns its whole HTTP response.
async fn raw_callback(address: SocketAddr, target: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8(response).unwrap()
}

/// The RFC 7636 S256 challenge, computed here independently of the crate.
fn s256(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

/// Every test here binds its listener's address again after `loopback_code` releases it;
/// see `common::listener_outside_ephemeral_range`.
fn listener_outside_ephemeral_range() -> TcpListener {
    let listener = common::listener_outside_ephemeral_range();
    listener.set_nonblocking(true).unwrap();
    TcpListener::from_std(listener).unwrap()
}
