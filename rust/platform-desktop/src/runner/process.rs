#[cfg(not(target_os = "windows"))]
use crate::util::AppImageEnvironment;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(not(target_os = "windows"))]
use std::process::Command;
use std::{collections::HashSet, path::Path};

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessMonitor;

impl ProcessMonitor {
    pub fn is_steam_shader_compiling() -> bool {
        Self::is_process_running("fossilize_replay")
            || Self::is_process_running("fossilize_replay64")
    }

    pub fn is_process_running(name: &str) -> bool {
        let clean = name.trim().trim_matches('"');
        if clean.is_empty() {
            return false;
        }
        !Self::matching_process_ids(clean).is_empty()
    }

    pub fn matching_process_ids(name: &str) -> HashSet<u64> {
        let clean_name = Path::new(name.trim().trim_matches('"'))
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if clean_name.is_empty() {
            return HashSet::new();
        }

        // Windows matches the image name in a process snapshot; other systems compare stems.
        #[cfg(not(target_os = "windows"))]
        let target_stem = Path::new(&clean_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        #[cfg(target_os = "linux")]
        {
            let mut matches = Self::scan_proc_linux(&clean_name, &target_stem);
            if matches.is_empty() {
                matches = Self::fallback_pgrep(&target_stem);
            }
            matches
        }

        #[cfg(target_os = "windows")]
        {
            Self::scan_snapshot_windows(&clean_name)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Self::fallback_pgrep(&target_stem)
        }
    }

    pub fn is_process_alive(pid: u64) -> bool {
        #[cfg(target_os = "linux")]
        {
            // Un zombi (`Z`) o proceso muerto (`X`) ya no cuenta como vivo.
            match fs::read_to_string(format!("/proc/{pid}/status")) {
                Ok(status) => status
                    .lines()
                    .find_map(|line| line.strip_prefix("State:"))
                    .and_then(|state| state.trim().chars().next())
                    .is_some_and(|c| c != 'Z' && c != 'X'),
                Err(_) => false,
            }
        }
        #[cfg(all(unix, not(target_os = "linux")))]
        {
            Path::new(&format!("/proc/{pid}")).exists()
        }
        #[cfg(windows)]
        {
            // As `tasklist` did: alive while the snapshot lists it; unknown counts as gone.
            super::windows_processes::running_processes()
                .is_some_and(|processes| processes.iter().any(|p| u64::from(p.pid) == pid))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = pid;
            false
        }
    }

    #[cfg(target_os = "linux")]
    fn scan_proc_linux(clean_name: &str, target_stem: &str) -> HashSet<u64> {
        let mut pids = HashSet::new();
        let proc_dir = match fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return pids,
        };

        for entry in proc_dir.flatten() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            let pid = match name_str.parse::<u64>() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let cmdline_path = entry.path().join("cmdline");
            let cmdline = match fs::read(cmdline_path) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };

            let args: Vec<&str> = cmdline
                .split(|&b| b == 0)
                .filter_map(|slice| std::str::from_utf8(slice).ok())
                .filter(|s| !s.is_empty())
                .collect();

            let matches = args.iter().any(|arg| {
                let trimmed = arg.trim().trim_matches('"');
                let arg_name = Path::new(trimmed)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                let arg_stem = Path::new(arg_name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                arg_name.eq_ignore_ascii_case(clean_name) || arg_stem == target_stem
            });

            if matches {
                pids.insert(pid);
            }
        }

        pids
    }

    #[cfg(not(target_os = "windows"))]
    fn fallback_pgrep(target_stem: &str) -> HashSet<u64> {
        let mut command = Command::new("pgrep");
        command.args(["-x", target_stem]);
        AppImageEnvironment::clean_command(&mut command);
        let output = match command.output() {
            Ok(out) => out,
            Err(_) => return HashSet::new(),
        };

        let mut pids = HashSet::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Ok(pid) = line.trim().parse::<u64>() {
                pids.insert(pid);
            }
        }
        pids
    }

    #[cfg(target_os = "windows")]
    fn scan_snapshot_windows(clean_name: &str) -> HashSet<u64> {
        let target_exe = if clean_name.to_lowercase().ends_with(".exe") {
            clean_name.to_string()
        } else {
            format!("{clean_name}.exe")
        };
        super::windows_processes::running_processes()
            .unwrap_or_default()
            .into_iter()
            .filter(|process| process.exe.eq_ignore_ascii_case(&target_exe))
            .map(|process| u64::from(process.pid))
            .collect()
    }
}
