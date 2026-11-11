use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

/// Compares semantic versions (e.g. 0.1.0 vs 0.1.1, 1.2 vs 1.2.0, etc.).
/// Missing trailing version components are treated as 0.
pub fn is_newer_version(current: &str, remote: &str) -> bool {
    let curr_parts: Vec<i32> = current
        .split('.')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .collect();
    let remote_parts: Vec<i32> = remote
        .split('.')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .collect();

    let max_len = curr_parts.len().max(remote_parts.len());
    for i in 0..max_len {
        let c = curr_parts.get(i).copied().unwrap_or(0);
        let r = remote_parts.get(i).copied().unwrap_or(0);
        match r.cmp(&c) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => continue,
        }
    }
    false
}

/// Selects the most appropriate asset from a release based on platform priority.
/// Windows: .msi > .exe > .zip
/// Linux: .AppImage > .tar.gz > .deb
pub fn find_best_asset_for_platform(
    assets: &[GitHubAsset],
    is_windows: bool,
) -> Option<&GitHubAsset> {
    if is_windows {
        assets
            .iter()
            .find(|a| a.name.to_lowercase().ends_with(".msi"))
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_lowercase().ends_with(".exe"))
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_lowercase().ends_with(".zip"))
            })
    } else {
        assets
            .iter()
            .find(|a| a.name.to_lowercase().ends_with(".appimage"))
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_lowercase().ends_with(".tar.gz"))
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_lowercase().ends_with(".deb"))
            })
    }
}
