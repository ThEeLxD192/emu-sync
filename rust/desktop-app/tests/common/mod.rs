#![allow(dead_code)]

use emusync_data_local::{ConfigManager, RecoveryCredentials};
use emusync_desktop_app::{AppEvent, DesktopCoordinator, DriveEndpoints};
use emusync_domain::{
    config::{AppConfig, CredentialStore, Credentials},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
};
use std::{
    io::{Read, Write},
    net::SocketAddr,
    sync::{Arc, mpsc::Receiver},
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::Notify;

pub struct TestHarness {
    pub dir: TempDir,
    pub manager: ConfigManager,
    pub coordinator: DesktopCoordinator,
    pub event_rx: Receiver<AppEvent>,
    pub redraw_notify: Arc<Notify>,
}

impl TestHarness {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let manager = ConfigManager::new(&config_path).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let redraw_notify = Arc::new(Notify::new());
        let notify_clone = redraw_notify.clone();
        let redraw = Arc::new(move || {
            notify_clone.notify_waiters();
            notify_clone.notify_one();
        });
        let coordinator = DesktopCoordinator::new(
            manager.clone(),
            tx,
            redraw,
            tokio::runtime::Handle::current(),
        );
        Self {
            dir,
            manager,
            coordinator,
            event_rx: rx,
            redraw_notify,
        }
    }

    pub fn with_endpoints(mut self, endpoints: DriveEndpoints) -> Self {
        self.coordinator = self.coordinator.with_endpoints(endpoints);
        self
    }

    pub async fn wait_for_active_tasks(&self) {
        wait_for_active_tasks(&self.coordinator).await;
    }

    pub async fn recv_event_matching<F>(&self, predicate: F) -> AppEvent
    where
        F: FnMut(&AppEvent) -> bool,
    {
        recv_event_matching(&self.event_rx, &self.redraw_notify, predicate).await
    }
}

pub async fn wait_for_active_tasks(coordinator: &DesktopCoordinator) {
    loop {
        let handles: Vec<_> = {
            let mut tasks = coordinator.active_tasks.lock().unwrap();
            tasks.drain(..).collect()
        };
        if handles.is_empty() {
            tokio::task::yield_now().await;
            let tasks = coordinator.active_tasks.lock().unwrap();
            if tasks.is_empty() {
                break;
            }
            continue;
        }
        for handle in handles {
            let _ = handle.await;
        }
    }
}

pub async fn recv_event_matching<F>(
    rx: &Receiver<AppEvent>,
    notify: &Notify,
    mut predicate: F,
) -> AppEvent
where
    F: FnMut(&AppEvent) -> bool,
{
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            while let Ok(event) = rx.try_recv() {
                if predicate(&event) {
                    return event;
                }
            }
            notify.notified().await;
        }
    })
    .await
    .expect("timed out waiting for matching AppEvent")
}

pub fn make_drive_endpoints(port: u16) -> DriveEndpoints {
    let url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).unwrap();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    DriveEndpoints {
        files_v3: url.join("/files").unwrap(),
        upload_v3: url.join("/upload").unwrap(),
        files_v2: url.join("/v2/files").unwrap(),
        upload_v2: url.join("/v2/upload").unwrap(),
        token_endpoint: url.join("/token").unwrap(),
        authorization_endpoint: url.join("/auth").unwrap(),
        oauth_address: addr,
        redirect_uri: format!("http://127.0.0.1:{port}/callback"),
    }
}

pub fn synthetic_credentials(root: &std::path::Path) -> RecoveryCredentials<ConfigManager> {
    let store = Arc::new(ConfigManager::new(&root.join("config.json")).unwrap());
    let config = AppConfig {
        credentials: Some(Credentials {
            client_id: "client".into(),
            client_secret: "synthetic".into(),
            refresh_token: Some("refresh".into()),
        }),
        entries: vec![],
    };
    store.save(&config).unwrap();
    let snapshot = store.snapshot(&config).unwrap();
    RecoveryCredentials::new(store, snapshot, config)
}

/// Executable and arguments of a process that exits at once with `code` on each platform.
pub fn shell_exit(code: i32) -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), format!("exit {code}")],
        )
    } else {
        (
            "/bin/sh".to_string(),
            vec!["-c".to_string(), format!("exit {code}")],
        )
    }
}

pub fn test_native_entry(name: &str) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: shell_exit(0).0,
            arguments: shell_exit(0).1,
            save_paths: vec!["/save.dat".to_string()],
            save_targets: vec![SaveTarget {
                path: "/save.dat".to_string(),
                cloud_path: Some("save.dat".to_string()),
                kind: SaveTargetType::File,
                excluded_folders: Vec::new(),
            }],
            ..Default::default()
        },
        ..Default::default()
    })
}

pub fn test_emulator_entry(name: &str) -> GameEntry {
    GameEntry::Emulator(Emulator {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/usr/bin/retroarch".to_string(),
            arguments: vec!["{ROM}".to_string()],
            save_paths: vec!["/saves".to_string()],
            save_targets: vec![SaveTarget {
                path: "/saves".to_string(),
                cloud_path: Some("saves".to_string()),
                kind: SaveTargetType::Directory,
                excluded_folders: Vec::new(),
            }],
            ..Default::default()
        },
        roms_directory: "/roms".to_string(),
        extensions: vec!["sfc".to_string(), "smc".to_string()],
        ..Default::default()
    })
}

pub fn read_http_request(stream: &mut std::net::TcpStream) -> std::io::Result<(String, Vec<u8>)> {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let end = loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "EOF reading HTTP headers",
            ));
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|b| b == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8_lossy(&data[..end]).to_string();
    let len = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    while data.len() < end + len {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let body_end = (end + len).min(data.len());
    Ok((headers, data[end..body_end].to_vec()))
}

pub fn write_http_json(
    stream: &mut std::net::TcpStream,
    status: u16,
    reason: &str,
    body: &str,
) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

pub async fn read_http_request_async(
    stream: &mut tokio::net::TcpStream,
) -> std::io::Result<(String, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let end = loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "EOF reading HTTP headers",
            ));
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|b| b == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8_lossy(&data[..end]).to_string();
    let len = headers
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    while data.len() < end + len {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let body_end = (end + len).min(data.len());
    Ok((headers, data[end..body_end].to_vec()))
}

pub async fn write_http_json_async(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    reason: &str,
    body: &str,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await
}
