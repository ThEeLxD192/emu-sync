use std::collections::HashMap;

/// Removes AppImage runtime variables before starting unrelated child applications.
pub struct AppImageEnvironment;

impl AppImageEnvironment {
    pub(crate) fn clean_command(command: &mut std::process::Command) {
        let app_dir = std::env::var_os("APPDIR")
            .filter(|value| !value.is_empty() && value != "/" && value != "\\")
            .map(std::path::PathBuf::from);
        for name in ["APPDIR", "ARGV0", "OWD"] {
            command.env_remove(name);
        }
        if let Some(library_path) = std::env::var_os("LD_LIBRARY_PATH") {
            let retained: Vec<_> = std::env::split_paths(&library_path)
                .filter(|path| {
                    let in_app_dir = app_dir.as_ref().is_some_and(|root| path.starts_with(root));
                    let in_mount = path
                        .to_string_lossy()
                        .split(['/', '\\'])
                        .any(|segment| segment.starts_with(".mount_"));
                    !in_app_dir && !in_mount
                })
                .collect();
            command.env_remove("LD_LIBRARY_PATH");
            if !retained.is_empty()
                && let Ok(joined) = std::env::join_paths(retained)
            {
                command.env("LD_LIBRARY_PATH", joined);
            }
        }
    }

    pub fn clean(environment: &mut HashMap<String, String>) {
        let app_dir = environment
            .get("APPDIR")
            .cloned()
            .unwrap_or_default()
            .trim_end_matches('/')
            .trim_end_matches('\\')
            .to_string();
        environment.remove("APPDIR");
        environment.remove("ARGV0");
        environment.remove("OWD");

        if let Some(ld_path) = environment.get("LD_LIBRARY_PATH") {
            let separator = if cfg!(windows) { ';' } else { ':' };
            let cleaned: Vec<&str> = ld_path
                .split(separator)
                .filter(|path| {
                    let path_str = *path;
                    let is_app_dir = !app_dir.is_empty()
                        && (path_str == app_dir
                            || path_str.starts_with(&format!("{app_dir}/"))
                            || path_str.starts_with(&format!("{app_dir}\\")));
                    let has_mount = path_str
                        .split(['/', '\\'])
                        .any(|seg| seg.starts_with(".mount_"));
                    !is_app_dir && !has_mount
                })
                .collect();

            if cleaned.is_empty() {
                environment.remove("LD_LIBRARY_PATH");
            } else {
                environment.insert(
                    "LD_LIBRARY_PATH".to_string(),
                    cleaned.join(&separator.to_string()),
                );
            }
        }
    }
}
