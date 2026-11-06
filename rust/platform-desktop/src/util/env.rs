use std::collections::HashMap;

/// Removes AppImage runtime variables before starting unrelated child applications.
pub struct AppImageEnvironment;

impl AppImageEnvironment {
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
