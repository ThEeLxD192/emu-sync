use emusync_data_updates::{GitHubAsset, find_best_asset_for_platform, is_newer_version};

fn asset(name: &str, url: &str, size: i64) -> GitHubAsset {
    GitHubAsset {
        name: name.to_owned(),
        browser_download_url: url.to_owned(),
        size,
    }
}

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
        asset("EmuSync.deb", "https://example.com/deb", 100),
        asset("EmuSync.tar.gz", "https://example.com/tar", 200),
        asset(
            "EmuSync-x86_64.AppImage",
            "https://example.com/appimage",
            300,
        ),
        asset("EmuSync.zip", "https://example.com/zip", 400),
        asset("EmuSync.exe", "https://example.com/exe", 500),
        asset("EmuSync.msi", "https://example.com/msi", 600),
    ];

    let linux_best = find_best_asset_for_platform(&assets, false).expect("linux asset");
    assert_eq!(linux_best.name, "EmuSync-x86_64.AppImage");
    assert_eq!(
        linux_best.browser_download_url,
        "https://example.com/appimage"
    );

    let win_best = find_best_asset_for_platform(&assets, true).expect("windows asset");
    assert_eq!(win_best.name, "EmuSync.msi");
    assert_eq!(win_best.browser_download_url, "https://example.com/msi");

    let windows_without_msi = vec![
        asset("EmuSync.deb", "https://example.com/deb", 100),
        asset("EmuSync.tar.gz", "https://example.com/tar", 200),
        asset(
            "EmuSync-x86_64.AppImage",
            "https://example.com/appimage",
            300,
        ),
        asset("EmuSync.zip", "https://example.com/zip", 400),
        asset("EmuSync-Setup.exe", "https://example.com/exe-fallback", 500),
    ];
    let exe = find_best_asset_for_platform(&windows_without_msi, true).expect("exe fallback");
    assert_eq!(exe.name, "EmuSync-Setup.exe");
    assert_eq!(exe.browser_download_url, "https://example.com/exe-fallback");

    let windows_only_zip = vec![
        asset("EmuSync.deb", "https://example.com/deb", 100),
        asset("EmuSync.tar.gz", "https://example.com/tar", 200),
        asset(
            "EmuSync-x86_64.AppImage",
            "https://example.com/appimage",
            300,
        ),
        asset("EmuSync.zip", "https://example.com/zip-fallback", 400),
    ];
    let zip = find_best_asset_for_platform(&windows_only_zip, true).expect("zip fallback");
    assert_eq!(zip.name, "EmuSync.zip");
    assert_eq!(zip.browser_download_url, "https://example.com/zip-fallback");

    let linux_without_appimage = vec![
        asset("EmuSync.msi", "https://example.com/msi", 600),
        asset("EmuSync.zip", "https://example.com/zip", 400),
        asset("EmuSync.exe", "https://example.com/exe", 500),
        asset("EmuSync.deb", "https://example.com/deb", 100),
        asset("EmuSync.tar.gz", "https://example.com/tar-fallback", 200),
    ];
    let tar = find_best_asset_for_platform(&linux_without_appimage, false).expect("tar fallback");
    assert_eq!(tar.name, "EmuSync.tar.gz");
    assert_eq!(tar.browser_download_url, "https://example.com/tar-fallback");

    let linux_only_deb = vec![
        asset("EmuSync.msi", "https://example.com/msi", 600),
        asset("EmuSync.zip", "https://example.com/zip", 400),
        asset("EmuSync.exe", "https://example.com/exe", 500),
        asset("EmuSync.deb", "https://example.com/deb-fallback", 100),
    ];
    let deb = find_best_asset_for_platform(&linux_only_deb, false).expect("deb fallback");
    assert_eq!(deb.name, "EmuSync.deb");
    assert_eq!(deb.browser_download_url, "https://example.com/deb-fallback");
}

#[test]
fn asset_selection_is_case_insensitive_and_preserves_first_equal_priority() {
    let windows_assets = vec![
        asset("EmuSync.zip", "https://example.com/zip", 100),
        asset("EmuSync-Setup.EXE", "https://example.com/exe", 200),
        asset("EmuSync-First.MsI", "https://example.com/first-msi", 300),
        asset("EmuSync-Second.MSI", "https://example.com/second-msi", 400),
    ];
    let first_msi = find_best_asset_for_platform(&windows_assets, true).expect("first windows MSI");
    assert_eq!(first_msi.name, "EmuSync-First.MsI");
    assert_eq!(
        first_msi.browser_download_url,
        "https://example.com/first-msi"
    );

    let linux_assets = vec![
        asset("EmuSync.deb", "https://example.com/deb", 100),
        asset("EmuSync.tar.gz", "https://example.com/tar", 200),
        asset(
            "EmuSync-First.aPpImAgE",
            "https://example.com/first-appimage",
            300,
        ),
        asset(
            "EmuSync-Second.AppImage",
            "https://example.com/second-appimage",
            400,
        ),
    ];
    let first_appimage =
        find_best_asset_for_platform(&linux_assets, false).expect("first linux AppImage");
    assert_eq!(first_appimage.name, "EmuSync-First.aPpImAgE");
    assert_eq!(
        first_appimage.browser_download_url,
        "https://example.com/first-appimage"
    );
}

#[test]
fn asset_selection_returns_none_without_a_supported_extension() {
    let cases = vec![
        (Vec::new(), true),
        (Vec::new(), false),
        (
            vec![
                asset("release.txt", "https://example.com/notes", 1),
                asset("release.sha256", "https://example.com/sum", 2),
            ],
            true,
        ),
        (
            vec![
                asset("release.txt", "https://example.com/notes", 1),
                asset("release.sha256", "https://example.com/sum", 2),
            ],
            false,
        ),
        (
            vec![
                asset("update.msi.sha256", "https://example.com/msi-sum", 1),
                asset("update.AppImage.sig", "https://example.com/appimage-sig", 2),
            ],
            true,
        ),
        (
            vec![
                asset("update.msi.sha256", "https://example.com/msi-sum", 1),
                asset("update.AppImage.sig", "https://example.com/appimage-sig", 2),
            ],
            false,
        ),
        (
            vec![asset("EmuSync.msi", "https://example.com/msi", 1)],
            false,
        ),
        (
            vec![asset(
                "EmuSync-x86_64.AppImage",
                "https://example.com/appimage",
                1,
            )],
            true,
        ),
    ];

    for (assets, is_windows) in cases {
        assert!(
            find_best_asset_for_platform(&assets, is_windows).is_none(),
            "unexpected asset selected for is_windows={is_windows}: {assets:?}"
        );
    }
}

#[test]
fn version_component_overflow_matches_kotlin_int_parsing() {
    // Kotlin drops components outside i32, so an overflowing component is absent
    // and therefore equivalent to zero rather than an enormous version number.
    assert!(is_newer_version("0", "2147483647.0"));
    assert!(!is_newer_version("0", "2147483648.0"));
    assert!(!is_newer_version("2147483648.0", "0"));
}
