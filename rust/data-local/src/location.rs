use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

/// Resolves the same portable and platform locations used by the Kotlin application.
/// `explicit` is the replacement for the JVM property and takes priority over the environment.
pub fn resolve_config_location(explicit: Option<&OsStr>) -> io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    resolve_with(
        explicit,
        |key| std::env::var_os(key),
        &cwd,
        std::env::consts::OS,
        portable_install_directory().as_deref(),
    )
}

/// A file with this name beside the executable makes the installation portable: its data
/// lives in that folder instead of the user's profile.
const PORTABLE_MARKER: &str = "portable.txt";

/// The folder of the running executable when it is a portable installation, that is, it holds
/// the marker file and can be written to. Data and updates both depend on this.
pub fn portable_install_directory() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    portable_directory(executable.parent()?)
}

fn portable_directory(directory: &Path) -> Option<PathBuf> {
    (directory.join(PORTABLE_MARKER).is_file() && writable(directory))
        .then(|| directory.to_path_buf())
}

fn nonblank(value: Option<OsString>) -> Option<OsString> {
    value.filter(|v| !v.to_string_lossy().trim().is_empty())
}
fn resolve_with(
    explicit: Option<&OsStr>,
    env: impl Fn(&str) -> Option<OsString>,
    cwd: &Path,
    os: &str,
    portable_install: Option<&Path>,
) -> io::Result<PathBuf> {
    if let Some(path) =
        nonblank(explicit.map(OsStr::to_os_string)).or_else(|| nonblank(env("EMUSYNC_CONFIG")))
    {
        return Ok(path.into());
    }
    if let Some(directory) = portable_install {
        return Ok(directory.join("config.json"));
    }
    if let Some(image) = nonblank(env("APPIMAGE")) {
        let image = Path::new(&image);
        if let Some(parent) = image.parent().filter(|p| !p.as_os_str().is_empty()) {
            let path = parent.join("config.json");
            if portable(&path, parent) {
                return Ok(path);
            }
        }
    }
    let path = cwd.join("config.json");
    if portable(&path, cwd) {
        return Ok(path);
    }
    let home = nonblank(env(if os == "windows" {
        "USERPROFILE"
    } else {
        "HOME"
    }))
    .unwrap_or_default();
    let base = if os == "windows" {
        nonblank(env("APPDATA"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join("AppData/Roaming"))
    } else if os == "macos" {
        PathBuf::from(home).join("Library/Application Support")
    } else {
        nonblank(env("XDG_CONFIG_HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".config"))
    };
    Ok(base.join("emusync/config.json"))
}
fn portable(path: &Path, parent: &Path) -> bool {
    path.exists() && (writable(path) || writable(parent))
}
#[cfg(unix)]
fn writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: CString supplies a valid, NUL-terminated path for the duration of the call.
    // access consults the real process credentials, as Java File.canWrite does on Unix.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}
#[cfg(not(unix))]
fn writable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| !m.permissions().readonly())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_and_portable_locations_precede_platform() {
        let dir = tempfile::tempdir().unwrap();
        let portable = dir.path().join("portable");
        let cwd = dir.path().join("cwd");
        std::fs::create_dir(&portable).unwrap();
        std::fs::create_dir(&cwd).unwrap();
        std::fs::write(portable.join("config.json"), "{}").unwrap();
        std::fs::write(cwd.join("config.json"), "{}").unwrap();
        let env = |key: &str| match key {
            "EMUSYNC_CONFIG" => Some(OsString::from("environment.json")),
            "APPIMAGE" => Some(portable.join("app.AppImage").into_os_string()),
            _ => None,
        };
        assert_eq!(
            resolve_with(Some(OsStr::new("custom.json")), env, &cwd, "linux", None).unwrap(),
            PathBuf::from("custom.json")
        );
        assert_eq!(
            resolve_with(None, env, &cwd, "linux", None).unwrap(),
            PathBuf::from("environment.json")
        );
        let env = |key: &str| {
            if key == "APPIMAGE" {
                Some(portable.join("app.AppImage").into_os_string())
            } else {
                None
            }
        };
        assert_eq!(
            resolve_with(None, env, &cwd, "linux", None).unwrap(),
            portable.join("config.json")
        );
        std::fs::remove_file(portable.join("config.json")).unwrap();
        assert_eq!(
            resolve_with(None, env, &cwd, "linux", None).unwrap(),
            cwd.join("config.json")
        );
        std::fs::remove_file(cwd.join("config.json")).unwrap();
        let env = |key: &str| {
            if key == "XDG_CONFIG_HOME" {
                Some(OsString::from("/synthetic/xdg"))
            } else {
                None
            }
        };
        assert_eq!(
            resolve_with(None, env, &cwd, "linux", None).unwrap(),
            PathBuf::from("/synthetic/xdg/emusync/config.json")
        );
    }

    #[test]
    fn a_portable_install_keeps_its_data_beside_the_executable_unless_overridden() {
        let dir = tempfile::tempdir().unwrap();
        let none = |_: &str| None;
        assert_eq!(
            resolve_with(None, none, dir.path(), "windows", Some(dir.path())).unwrap(),
            dir.path().join("config.json")
        );
        let env = |key: &str| (key == "EMUSYNC_CONFIG").then(|| OsString::from("override.json"));
        assert_eq!(
            resolve_with(None, env, dir.path(), "windows", Some(dir.path())).unwrap(),
            PathBuf::from("override.json")
        );
    }

    #[test]
    fn the_marker_file_alone_makes_a_folder_a_portable_install() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(portable_directory(dir.path()), None);
        std::fs::write(dir.path().join(PORTABLE_MARKER), "").unwrap();
        assert_eq!(
            portable_directory(dir.path()),
            Some(dir.path().to_path_buf())
        );
        // A directory with that name is not the marker.
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir(other.path().join(PORTABLE_MARKER)).unwrap();
        assert_eq!(portable_directory(other.path()), None);
    }
}
