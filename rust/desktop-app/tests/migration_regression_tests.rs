use emusync_data_local::ConfigManager;
use emusync_desktop_app::{AppEvent, DesktopCoordinator, DriveEndpoints};
use emusync_domain::{
    config::{AppConfig, ConfigurationStore, Credentials},
    library::{EntryCommon, GameEntry, NativeGame},
};
use emusync_presentation::{actions::UiAction, model::AppStatus};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, mpsc::channel},
};
use tempfile::tempdir;

fn mock_failing_token_server() -> (DriveEndpoints, std::sync::mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop_tx, stop_rx) = channel();

    std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while stop_rx.try_recv().is_err() {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream
                    .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    let url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).unwrap();
    let endpoints = DriveEndpoints {
        files_v3: url.join("/files").unwrap(),
        upload_v3: url.join("/upload").unwrap(),
        files_v2: url.join("/v2/files").unwrap(),
        upload_v2: url.join("/v2/upload").unwrap(),
        token_endpoint: url.join("/token").unwrap(),
        authorization_endpoint: url.join("/auth").unwrap(),
        oauth_address: format!("127.0.0.1:{port}").parse().unwrap(),
        redirect_uri: format!("http://127.0.0.1:{port}/callback"),
    };
    (endpoints, stop_tx)
}

fn test_native(name: &str, group: Option<&str>, cloud_folder: Option<&str>) -> GameEntry {
    GameEntry::Native(NativeGame {
        common: EntryCommon {
            name: name.to_string(),
            executable_path: "/bin/true".to_string(),
            arguments: vec![],
            save_paths: vec!["/tmp/saves".to_string()],
            save_targets: vec![],
            group: group.map(str::to_string),
            cloud_folder: cloud_folder.map(str::to_string),
            drive_file_id: None,
            steam_app_id: None,
        },
        cover_path: None,
        wait_for_process: None,
    })
}

#[tokio::test]
async fn token_server_failure_leaves_config_intact_and_emits_error() {
    let (endpoints, _stop) = mock_failing_token_server();
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry = test_native("OriginalGame", None, None);
    let initial_config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry.clone()],
    };
    manager.update(|_| initial_config).unwrap();
    let original_config = manager.load().unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(entry.clone()),
        new: Box::new(test_native("RenamedGame", None, None)),
    });

    let mut saw_syncing = false;
    let mut saw_error = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::StatusChanged(AppStatus::Syncing { .. })) {
                saw_syncing = true;
            }
            if matches!(event, AppEvent::StatusChanged(AppStatus::Error { .. })) {
                saw_error = true;
                break;
            }
        }
        if saw_error {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(saw_syncing, "Expected syncing event for cloud migration");
    assert!(saw_error, "Expected migration to fail with Error status");

    // Config must remain identical to original
    assert_eq!(manager.load().unwrap(), original_config);

    // No success toast was emitted
    let events: Vec<_> = rx.try_iter().collect();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::Toast(msg) if msg.contains("updated")))
    );
}

#[tokio::test]
async fn cloud_folder_change_without_name_change_triggers_migration() {
    let (endpoints, _stop) = mock_failing_token_server();
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry_old = test_native("SameName", None, None);
    let entry_new = test_native("SameName", None, Some("custom_folder"));

    let original_config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry_old.clone()],
    };
    manager.update(|_| original_config.clone()).unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(entry_old),
        new: Box::new(entry_new),
    });

    // Effective cloud folder changed from "SameName" to "custom_folder", so migration is triggered
    let mut saw_syncing = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::StatusChanged(AppStatus::Syncing { .. })) {
                saw_syncing = true;
                break;
            }
        }
        if saw_syncing {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        saw_syncing,
        "Expected migration to be triggered by cloud folder change"
    );
}

#[tokio::test]
async fn renaming_grouped_native_game_triggers_migration() {
    let (endpoints, _stop) = mock_failing_token_server();
    let dir = tempdir().unwrap();
    let manager = ConfigManager::new(&dir.path().join("config.json")).unwrap();
    let entry_old = test_native("OldGame", Some("GroupG"), None);
    let entry_new = test_native("NewGame", Some("GroupG"), None);

    let original_config = AppConfig {
        credentials: Some(Credentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            refresh_token: Some("token".into()),
        }),
        entries: vec![entry_old.clone()],
    };
    manager.update(|_| original_config.clone()).unwrap();

    let (tx, rx) = channel();
    let coordinator = DesktopCoordinator::new(
        manager.clone(),
        tx,
        Arc::new(|| {}),
        tokio::runtime::Handle::current(),
    )
    .with_endpoints(endpoints);

    coordinator.handle_action(UiAction::EditEntry {
        old: Box::new(entry_old),
        new: Box::new(entry_new),
    });

    // Cloud path segments changed from ["GroupG", "OldGame"] to ["GroupG", "NewGame"],
    // so migration is triggered even though effective_cloud_folder() is "GroupG" for both!
    let mut saw_syncing = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if matches!(event, AppEvent::StatusChanged(AppStatus::Syncing { .. })) {
                saw_syncing = true;
                break;
            }
        }
        if saw_syncing {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        saw_syncing,
        "Expected cloud migration to be triggered by renaming grouped native game"
    );
}
