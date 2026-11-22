#![cfg(unix)]
//! Whether a folder counts as portable depends on what the real user can write, checked
//! like access(2) with W_OK. The location is read from the process (working directory,
//! executable and environment), so each test relaunches this binary as a child with that
//! state and the child asserts the resolved location.

use emusync_data_local::{portable_install_directory, resolve_config_location};
use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

/// Set only in the relaunched child; the test that sees it asserts instead of preparing.
const CHILD_MARKER: &str = "EMUSYNC_TEST_LOCATION_WRITABILITY_CHILD";
/// Path the child must resolve, chosen by the parent from the folders it prepared.
const EXPECTED_LOCATION: &str = "EMUSYNC_TEST_EXPECTED_LOCATION";
/// Expected result of `portable_install_directory` in the child; empty means `None`.
const EXPECTED_PORTABLE_DIRECTORY: &str = "EMUSYNC_TEST_EXPECTED_PORTABLE_DIRECTORY";
const SYNTHETIC_XDG: &str = "/synthetic/xdg";
const PLATFORM_LOCATION: &str = "/synthetic/xdg/emusync/config.json";

fn is_child() -> bool {
    std::env::var_os(CHILD_MARKER).is_some()
}

/// Root passes W_OK whatever the permission bits say, so read-only cases cannot be built.
fn runs_as_root() -> bool {
    rustix::process::getuid().is_root()
}

/// Asserts in the child that the configuration resolves to the expected path.
fn assert_child_resolves_expected_location() {
    let expected = PathBuf::from(std::env::var_os(EXPECTED_LOCATION).unwrap());
    assert_eq!(resolve_config_location(None).unwrap(), expected);
}

/// Restores write permission so the temporary folder can be removed even if an assertion fails.
struct WritableAgain(Vec<PathBuf>);
impl Drop for WritableAgain {
    fn drop(&mut self) {
        for path in &self.0 {
            let mode = if path.is_dir() { 0o700 } else { 0o600 };
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
        }
    }
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// A temporary folder with its symlinks resolved, as the child sees it in `current_dir`.
fn canonical_dir(root: &Path, name: &str) -> PathBuf {
    let directory = root.join(name);
    fs::create_dir(&directory).unwrap();
    directory.canonicalize().unwrap()
}

/// Runs the test `test_name` of this module in `executable`, from `working_directory`, with
/// no configuration override, the synthetic XDG folder and the given extra variables.
fn run_child(
    executable: &Path,
    test_name: &str,
    working_directory: &Path,
    variables: &[(&str, &OsStr)],
) {
    // The child filters by the full name inside the test binary (`location_writability::…`);
    // a filter that matched nothing would pass without testing.
    let module = module_path!().split_once("::").unwrap().1;
    let mut command = Command::new(executable);
    command
        .arg(format!("{module}::{test_name}"))
        .arg("--exact")
        .current_dir(working_directory)
        .env(CHILD_MARKER, "1")
        .env("XDG_CONFIG_HOME", SYNTHETIC_XDG)
        .env_remove("EMUSYNC_CONFIG")
        .env_remove("APPIMAGE");
    for (key, value) in variables {
        command.env(key, value);
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "child failed:\n{stdout}\n{stderr}");
    assert!(
        stdout.contains("test result: ok. 1 passed;"),
        "child did not run the test: {stdout}"
    );
}

fn run_this_binary(test_name: &str, working_directory: &Path, variables: &[(&str, &OsStr)]) {
    run_child(
        &std::env::current_exe().unwrap(),
        test_name,
        working_directory,
        variables,
    );
}

#[test]
fn a_read_only_config_in_a_writable_folder_is_portable() {
    if is_child() {
        return assert_child_resolves_expected_location();
    }
    if runs_as_root() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let working_directory = canonical_dir(root.path(), "cwd");
    let config = working_directory.join("config.json");
    fs::write(&config, b"{}").unwrap();
    let _restore = WritableAgain(vec![config.clone()]);
    set_mode(&config, 0o400);

    run_this_binary(
        "a_read_only_config_in_a_writable_folder_is_portable",
        &working_directory,
        &[(EXPECTED_LOCATION, config.as_os_str())],
    );
}

#[test]
fn a_writable_config_in_a_read_only_folder_is_portable() {
    if is_child() {
        return assert_child_resolves_expected_location();
    }
    let root = tempfile::tempdir().unwrap();
    let working_directory = canonical_dir(root.path(), "cwd");
    let config = working_directory.join("config.json");
    fs::write(&config, b"{}").unwrap();
    let _restore = WritableAgain(vec![working_directory.clone()]);
    set_mode(&config, 0o600);
    set_mode(&working_directory, 0o500);

    run_this_binary(
        "a_writable_config_in_a_read_only_folder_is_portable",
        &working_directory,
        &[(EXPECTED_LOCATION, config.as_os_str())],
    );
}

#[test]
fn an_appimage_folder_with_read_only_config_and_folder_yields_to_a_portable_working_directory() {
    if is_child() {
        return assert_child_resolves_expected_location();
    }
    if runs_as_root() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let appimage_folder = canonical_dir(root.path(), "appimage");
    let appimage_config = appimage_folder.join("config.json");
    fs::write(&appimage_config, b"{}").unwrap();
    let working_directory = canonical_dir(root.path(), "cwd");
    let working_config = working_directory.join("config.json");
    fs::write(&working_config, b"{}").unwrap();
    let _restore = WritableAgain(vec![appimage_folder.clone(), appimage_config.clone()]);
    set_mode(&appimage_config, 0o400);
    set_mode(&appimage_folder, 0o500);

    let appimage = appimage_folder.join("EmuSync.AppImage");
    run_this_binary(
        "an_appimage_folder_with_read_only_config_and_folder_yields_to_a_portable_working_directory",
        &working_directory,
        &[
            ("APPIMAGE", appimage.as_os_str()),
            (EXPECTED_LOCATION, working_config.as_os_str()),
        ],
    );
}

#[test]
fn an_appimage_folder_with_a_read_only_config_but_writable_folder_is_portable() {
    if is_child() {
        return assert_child_resolves_expected_location();
    }
    if runs_as_root() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let appimage_folder = canonical_dir(root.path(), "appimage");
    let appimage_config = appimage_folder.join("config.json");
    fs::write(&appimage_config, b"{}").unwrap();
    // A portable working directory comes later; it must not win over the AppImage folder.
    let working_directory = canonical_dir(root.path(), "cwd");
    fs::write(working_directory.join("config.json"), b"{}").unwrap();
    let _restore = WritableAgain(vec![appimage_config.clone()]);
    set_mode(&appimage_config, 0o400);

    let appimage = appimage_folder.join("EmuSync.AppImage");
    run_this_binary(
        "an_appimage_folder_with_a_read_only_config_but_writable_folder_is_portable",
        &working_directory,
        &[
            ("APPIMAGE", appimage.as_os_str()),
            (EXPECTED_LOCATION, appimage_config.as_os_str()),
        ],
    );
}

/// Asserts in the child the portable folder of its own executable and where the
/// configuration resolves.
fn assert_child_portable_install() {
    let expected_directory = std::env::var_os(EXPECTED_PORTABLE_DIRECTORY).unwrap();
    let expected_directory =
        (!expected_directory.is_empty()).then(|| PathBuf::from(expected_directory));
    assert_eq!(portable_install_directory(), expected_directory);
    assert_child_resolves_expected_location();
}

/// Hard-links this test binary into a new folder holding the portable marker, as an
/// unpacked portable installation, and returns the folder and the link inside it. The link
/// shares the file of the real test binary: never change its mode or contents.
fn link_this_binary_into_a_portable_folder(root: &Path) -> (PathBuf, PathBuf) {
    let install = canonical_dir(root, "install");
    fs::write(install.join("portable.txt"), b"").unwrap();
    let executable = install.join("emusync-test");
    // A hard link, not a copy: a copy is open for writing while other tests of this process
    // spawn children, and running it could then fail with "Text file busy". The temporary
    // folder is in the target directory, so it is on the same file system as the binary.
    fs::hard_link(std::env::current_exe().unwrap(), &executable)
        .expect("link the test binary into the portable folder");
    (install, executable)
}

#[test]
fn a_marker_beside_the_executable_in_a_writable_folder_makes_a_portable_install() {
    if is_child() {
        return assert_child_portable_install();
    }
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (install, executable) = link_this_binary_into_a_portable_folder(root.path());
    let working_directory = canonical_dir(root.path(), "cwd");

    run_child(
        &executable,
        "a_marker_beside_the_executable_in_a_writable_folder_makes_a_portable_install",
        &working_directory,
        &[
            (EXPECTED_PORTABLE_DIRECTORY, install.as_os_str()),
            (EXPECTED_LOCATION, install.join("config.json").as_os_str()),
        ],
    );
}

#[test]
fn a_marker_beside_the_executable_in_a_read_only_folder_is_not_a_portable_install() {
    if is_child() {
        return assert_child_portable_install();
    }
    if runs_as_root() {
        return;
    }
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (install, executable) = link_this_binary_into_a_portable_folder(root.path());
    let working_directory = canonical_dir(root.path(), "cwd");
    let _restore = WritableAgain(vec![install.clone()]);
    set_mode(&install, 0o500);

    run_child(
        &executable,
        "a_marker_beside_the_executable_in_a_read_only_folder_is_not_a_portable_install",
        &working_directory,
        &[
            (EXPECTED_PORTABLE_DIRECTORY, OsStr::new("")),
            (EXPECTED_LOCATION, OsStr::new(PLATFORM_LOCATION)),
        ],
    );
}
