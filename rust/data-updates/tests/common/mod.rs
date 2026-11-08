#![allow(dead_code)]

use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const MAX_REQUEST_HEAD_BYTES: usize = 16 * 1024;

/// Independent upper bound for harness waits. Production timers must never be
/// the only thing preventing a test from hanging forever when they disappear.
pub const HARNESS_WAIT_LIMIT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestRecord {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl RequestRecord {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug)]
pub enum ServerError {
    Accept(std::io::Error),
    Read(std::io::Error),
    Write(std::io::Error),
    IncompleteRequest,
    RequestTooLarge,
    InvalidRequest,
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accept(error) => write!(formatter, "server accept failed: {error}"),
            Self::Read(error) => write!(formatter, "server read failed: {error}"),
            Self::Write(error) => write!(formatter, "server write failed: {error}"),
            Self::IncompleteRequest => {
                write!(
                    formatter,
                    "connection closed before complete request headers"
                )
            }
            Self::RequestTooLarge => write!(formatter, "request headers exceeded size limit"),
            Self::InvalidRequest => write!(formatter, "request headers were not valid HTTP/1.1"),
        }
    }
}

impl std::error::Error for ServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Accept(error) | Self::Read(error) | Self::Write(error) => Some(error),
            Self::IncompleteRequest | Self::RequestTooLarge | Self::InvalidRequest => None,
        }
    }
}

pub struct TestServer {
    listener: Option<std::net::TcpListener>,
    addr: SocketAddr,
}

impl TestServer {
    pub fn bind() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        listener
            .set_nonblocking(true)
            .expect("set loopback listener nonblocking");
        let addr = listener
            .local_addr()
            .expect("read loopback listener address");
        Self {
            listener: Some(listener),
            addr,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }

    pub fn assert_no_connection(&self) {
        let listener = self
            .listener
            .as_ref()
            .expect("listener is available before spawning the server");
        match listener.accept() {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("failed to check for unexpected loopback connection: {error}"),
            Ok((_stream, peer)) => {
                panic!("unexpected loopback connection established from {peer}")
            }
        }
    }

    pub fn spawn<F, Fut>(mut self, handler: F) -> ServerTask
    where
        F: FnOnce(ServerConnection) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ServerError>> + Send + 'static,
    {
        let listener = self
            .listener
            .take()
            .expect("listener is available before spawning the server");
        let (request_tx, request_rx) = oneshot::channel();
        let (finish_tx, finish_rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let listener = TcpListener::from_std(listener).map_err(ServerError::Accept)?;
            let (mut stream, _) = listener.accept().await.map_err(ServerError::Accept)?;
            let request = read_request(&mut stream).await?;
            let _ = request_tx.send(request.clone());

            let connection = ServerConnection {
                stream,
                request: request.clone(),
            };
            handler(connection).await?;

            let mut requests = vec![request];
            let mut finish_rx = finish_rx;
            loop {
                tokio::select! {
                    biased;
                    accepted = listener.accept() => {
                        let (mut stream, _) = accepted.map_err(ServerError::Accept)?;
                        let extra = read_request(&mut stream).await?;
                        requests.push(extra);
                        stream.shutdown().await.map_err(ServerError::Write)?;
                    }
                    _ = &mut finish_rx => break,
                }
            }
            Ok(requests)
        });

        ServerTask {
            request_rx,
            finish_tx: Some(finish_tx),
            handle,
        }
    }
}

pub struct ServerTask {
    request_rx: oneshot::Receiver<RequestRecord>,
    finish_tx: Option<oneshot::Sender<()>>,
    handle: JoinHandle<Result<Vec<RequestRecord>, ServerError>>,
}

impl ServerTask {
    /// Wait for the primary request with an independent deadline. The download
    /// task is observed too, so a download that ends before sending HTTP fails
    /// with a diagnostic instead of waiting forever.
    pub async fn wait_for_request<T>(
        &mut self,
        download_task: &mut JoinHandle<T>,
        context: &str,
    ) -> RequestRecord {
        let request = tokio::time::timeout(HARNESS_WAIT_LIMIT, async {
            tokio::select! {
                biased;
                request = &mut self.request_rx => Some(request),
                _ = &mut *download_task => None,
            }
        })
        .await;

        match request {
            Ok(Some(Ok(request))) => request,
            Ok(Some(Err(_))) => {
                abort_harness_tasks(download_task, self).await;
                panic!("{context}: server ended before capturing the request");
            }
            Ok(None) => {
                self.abort().await;
                panic!("{context}: download task ended before sending the HTTP request");
            }
            Err(_) => {
                abort_harness_tasks(download_task, self).await;
                panic!(
                    "{context}: harness wait limit {HARNESS_WAIT_LIMIT:?} exceeded waiting for the request; tasks aborted"
                );
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    pub async fn finish(mut self) -> Vec<RequestRecord> {
        if let Some(finish_tx) = self.finish_tx.take() {
            let _ = finish_tx.send(());
        }
        match tokio::time::timeout(HARNESS_WAIT_LIMIT, &mut self.handle).await {
            Ok(Ok(Ok(requests))) => requests,
            Ok(Ok(Err(error))) => panic!("test server failed: {error}"),
            Ok(Err(error)) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Ok(Err(error)) => panic!("test server task ended unexpectedly: {error}"),
            Err(_) => {
                self.handle.abort();
                let _ = self.handle.await;
                panic!(
                    "test server did not finish within harness wait limit {HARNESS_WAIT_LIMIT:?}; task aborted"
                );
            }
        }
    }

    pub async fn abort(&mut self) {
        self.handle.abort();
        match (&mut self.handle).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => panic!("test server failed before abort: {error}"),
            Err(error) if error.is_cancelled() => {}
            Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Err(error) => panic!("test server task failed: {error}"),
        }
        self.finish_tx.take();
    }

    pub async fn abort_and_wait(mut self) {
        self.abort().await;
    }
}

async fn abort_harness_tasks<T>(task: &mut JoinHandle<T>, server_task: &mut ServerTask) {
    task.abort();
    let _ = task.await;
    server_task.abort().await;
}

/// Wait for a value without allowing a missing production callback or timer to
/// leave the test hanging forever. On timeout the download and server tasks are
/// aborted and awaited before the diagnostic panic.
pub async fn recv_with_limit<T, U>(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<T>,
    task: &mut JoinHandle<U>,
    server_task: &mut ServerTask,
    context: &str,
) -> T {
    match tokio::time::timeout(HARNESS_WAIT_LIMIT, receiver.recv()).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            abort_harness_tasks(task, server_task).await;
            panic!("{context}: channel closed before a value arrived");
        }
        Err(_) => {
            abort_harness_tasks(task, server_task).await;
            panic!(
                "{context}: harness wait limit {HARNESS_WAIT_LIMIT:?} exceeded waiting for a value"
            );
        }
    }
}

/// Wait for a spawned task with an independent limit and clean up both the task
/// and its server if the limit is exceeded.
pub async fn join_with_limit<T>(
    task: &mut JoinHandle<T>,
    server_task: &mut ServerTask,
    context: &str,
) -> T {
    match tokio::time::timeout(HARNESS_WAIT_LIMIT, &mut *task).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            server_task.abort().await;
            panic!("{context}: task join failed: {error}");
        }
        Err(_) => {
            abort_harness_tasks(task, server_task).await;
            panic!(
                "{context}: harness wait limit {HARNESS_WAIT_LIMIT:?} exceeded; task aborted and server cancelled"
            );
        }
    }
}

pub struct ServerConnection {
    stream: TcpStream,
    request: RequestRecord,
}

impl ServerConnection {
    pub fn request(&self) -> &RequestRecord {
        &self.request
    }

    pub async fn write_all(&mut self, bytes: &[u8]) -> Result<(), ServerError> {
        self.stream
            .write_all(bytes)
            .await
            .map_err(ServerError::Write)
    }

    pub async fn flush(&mut self) -> Result<(), ServerError> {
        self.stream.flush().await.map_err(ServerError::Write)
    }

    pub async fn shutdown(&mut self) -> Result<(), ServerError> {
        self.stream.shutdown().await.map_err(ServerError::Write)
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<RequestRecord, ServerError> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            if position > MAX_REQUEST_HEAD_BYTES {
                return Err(ServerError::RequestTooLarge);
            }
            return parse_request(&buffer[..position]);
        }
        if buffer.len() >= MAX_REQUEST_HEAD_BYTES {
            return Err(ServerError::RequestTooLarge);
        }
        let read = stream.read(&mut chunk).await.map_err(ServerError::Read)?;
        if read == 0 {
            return Err(ServerError::IncompleteRequest);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn parse_request(head: &[u8]) -> Result<RequestRecord, ServerError> {
    let head = std::str::from_utf8(head).map_err(|_| ServerError::InvalidRequest)?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or(ServerError::InvalidRequest)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or(ServerError::InvalidRequest)?.to_owned();
    let target = parts.next().ok_or(ServerError::InvalidRequest)?.to_owned();
    let version = parts.next().ok_or(ServerError::InvalidRequest)?;
    if !version.starts_with("HTTP/") {
        return Err(ServerError::InvalidRequest);
    }

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or(ServerError::InvalidRequest)?;
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
    }

    Ok(RequestRecord {
        method,
        target,
        headers,
    })
}

pub fn http_response(status_line: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut response = Vec::new();
    response.extend_from_slice(format!("HTTP/1.1 {status_line}\r\n").as_bytes());
    for (name, value) in headers {
        response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(body);
    response
}

pub fn response_with_content_length(status_line: &str, body: &[u8]) -> Vec<u8> {
    let length = body.len().to_string();
    http_response(
        status_line,
        &[("Content-Length", length.as_str()), ("Connection", "close")],
        body,
    )
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("build no-proxy reqwest client")
}

pub fn directory_entries(path: &Path) -> Vec<String> {
    let mut entries: Vec<String> = std::fs::read_dir(path)
        .expect("read directory inventory")
        .map(|entry| {
            entry
                .expect("read directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    entries.sort();
    entries
}

pub fn part_entries(path: &Path) -> Vec<String> {
    directory_entries(path)
        .into_iter()
        .filter(|name| name.ends_with(".part"))
        .collect()
}

pub fn assert_directory_entries(path: &Path, expected: &[&str]) {
    let actual = directory_entries(path);
    let expected: Vec<String> = expected.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(
        actual,
        expected,
        "unexpected directory inventory in {}",
        path.display()
    );
}
