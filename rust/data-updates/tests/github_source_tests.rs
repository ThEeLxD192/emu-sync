use emusync_data_updates::{
    GitHubAsset, GitHubUpdateSource, find_best_asset_for_platform, is_newer_version,
};
use emusync_domain::update::UpdateSource;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn correctly_compares_semantic_versions() {
    assert!(is_newer_version("0.1.0", "0.1.1"));
    assert!(is_newer_version("0.1.0", "0.2.0"));
    assert!(is_newer_version("0.9.9", "1.0.0"));
    assert!(is_newer_version("0.1.0", "0.1.0.1"));

    assert!(!is_newer_version("0.1.0", "0.1.0"));
    assert!(!is_newer_version("0.1.1", "0.1.0"));
    assert!(!is_newer_version("1.0.0", "0.9.9"));
    assert!(!is_newer_version("0.2.0", "0.1.9"));
}

#[test]
fn missing_trailing_version_components_are_equivalent_to_zero() {
    assert!(!is_newer_version("1.2", "1.2.0"));
    assert!(!is_newer_version("1.2.0", "1.2"));
    assert!(is_newer_version("1.2", "1.2.0.1"));
    assert!(!is_newer_version("1.2.0.1", "1.2"));
}

#[test]
fn asset_priority_for_linux_and_windows() {
    let assets = vec![
        GitHubAsset {
            name: "EmuSync.deb".into(),
            browser_download_url: "https://example.com/deb".into(),
            size: 100,
        },
        GitHubAsset {
            name: "EmuSync.tar.gz".into(),
            browser_download_url: "https://example.com/tar".into(),
            size: 200,
        },
        GitHubAsset {
            name: "EmuSync-x86_64.AppImage".into(),
            browser_download_url: "https://example.com/appimage".into(),
            size: 300,
        },
        GitHubAsset {
            name: "EmuSync.zip".into(),
            browser_download_url: "https://example.com/zip".into(),
            size: 400,
        },
        GitHubAsset {
            name: "EmuSync.exe".into(),
            browser_download_url: "https://example.com/exe".into(),
            size: 500,
        },
        GitHubAsset {
            name: "EmuSync.msi".into(),
            browser_download_url: "https://example.com/msi".into(),
            size: 600,
        },
    ];

    // On Linux: AppImage > tar.gz > deb
    let linux_best = find_best_asset_for_platform(&assets, false).expect("linux asset");
    assert_eq!(linux_best.name, "EmuSync-x86_64.AppImage");

    // On Windows: msi > exe > zip
    let win_best = find_best_asset_for_platform(&assets, true).expect("windows asset");
    assert_eq!(win_best.name, "EmuSync.msi");
}

#[tokio::test]
async fn check_for_updates_detects_newer_release_and_appimage_on_linux() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let body = r#"{
                "tag_name": "v0.3.0",
                "name": "EmuSync 0.3.0",
                "body": "Bug fixes and improvements",
                "assets": [
                    {
                        "name": "EmuSync-x86_64.AppImage",
                        "browser_download_url": "https://example.com/download/EmuSync-x86_64.AppImage",
                        "size": 1024
                    },
                    {
                        "name": "EmuSync-0.3.0.msi",
                        "browser_download_url": "https://example.com/download/EmuSync-0.3.0.msi",
                        "size": 2048
                    }
                ]
            }"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let client = reqwest::Client::new();
    let source = GitHubUpdateSource::new(client, "owner", "repo", "0.2.0", false)
        .with_base_url(format!("http://127.0.0.1:{port}"));

    let update = source.check().await.unwrap().expect("update available");
    assert_eq!(update.version, "0.3.0");
    assert_eq!(update.tag_name, "v0.3.0");
    assert_eq!(update.asset_name, "EmuSync-x86_64.AppImage");
    assert_eq!(
        update.download_url,
        "https://example.com/download/EmuSync-x86_64.AppImage"
    );
    assert_eq!(update.asset_size, 1024);
}

#[tokio::test]
async fn check_for_updates_detects_newer_release_and_msi_on_windows() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let body = r#"{
                "tag_name": "v0.3.0",
                "name": "EmuSync 0.3.0",
                "body": "Bug fixes and improvements",
                "assets": [
                    {
                        "name": "EmuSync-x86_64.AppImage",
                        "browser_download_url": "https://example.com/download/EmuSync-x86_64.AppImage",
                        "size": 1024
                    },
                    {
                        "name": "EmuSync-0.3.0.msi",
                        "browser_download_url": "https://example.com/download/EmuSync-0.3.0.msi",
                        "size": 2048
                    }
                ]
            }"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let client = reqwest::Client::new();
    let source = GitHubUpdateSource::new(client, "owner", "repo", "0.2.0", true)
        .with_base_url(format!("http://127.0.0.1:{port}"));

    let update = source.check().await.unwrap().expect("update available");
    assert_eq!(update.version, "0.3.0");
    assert_eq!(update.tag_name, "v0.3.0");
    assert_eq!(update.asset_name, "EmuSync-0.3.0.msi");
    assert_eq!(
        update.download_url,
        "https://example.com/download/EmuSync-0.3.0.msi"
    );
    assert_eq!(update.asset_size, 2048);
}
