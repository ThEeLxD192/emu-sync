//! `ProcessMonitor` sobre procesos reales y controlados (solo Linux: lectura de `/proc`).

#![cfg(target_os = "linux")]

use emusync_platform_desktop::ProcessMonitor;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn exited_unreaped_child_is_not_alive() {
    let mut child = Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap();
    let pid = u64::from(child.id());
    // `try_wait` no se usa: se espera a que `/proc` muestre el zombi sin recogerlo.
    let state_path = format!("/proc/{pid}/status");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = std::fs::read_to_string(&state_path).unwrap_or_default();
        if status
            .lines()
            .any(|l| l.starts_with("State:") && l.contains('Z'))
        {
            break;
        }
        assert!(Instant::now() < deadline, "child never became a zombie");
        std::thread::yield_now();
    }
    assert!(!ProcessMonitor::is_process_alive(pid));
    child.wait().unwrap();
    assert!(!ProcessMonitor::is_process_alive(pid));
}

#[test]
fn running_process_is_alive() {
    // `read` mantiene vivo el hijo hasta que se cierra stdin o se le mata.
    let mut child = Command::new("sh")
        .args(["-c", "read _"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(ProcessMonitor::is_process_alive(u64::from(child.id())));
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn matching_process_ids_finds_process_by_argument_name_variants() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let stem = format!("emusync-probe-{}-{nanos}", std::process::id());
    let name = format!("{stem}.exe");
    let dir = tempfile::tempdir().unwrap();
    let marker_path = dir.path().join(&name);

    // `$0` del shell es el nombre buscado; `read` mantiene vivo el proceso hasta cerrar stdin.
    let mut child = Command::new("sh")
        .args(["-c", "read _"])
        .arg(&marker_path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = u64::from(child.id());

    let variants = [
        name.clone(),
        format!("\"/otra/ruta/{name}\""),
        name.to_uppercase(),
        stem.clone(),
    ];
    for variant in &variants {
        assert!(
            ProcessMonitor::matching_process_ids(variant).contains(&pid),
            "variant {variant:?} did not match"
        );
    }
    assert!(ProcessMonitor::is_process_running(&name));

    drop(child.stdin.take());
    child.wait().unwrap();

    for variant in &variants {
        assert!(
            !ProcessMonitor::matching_process_ids(variant).contains(&pid),
            "variant {variant:?} still matches an exited process"
        );
    }
    assert!(!ProcessMonitor::is_process_running(&name));
}

#[test]
fn blank_or_quoted_empty_names_match_nothing() {
    for name in ["", "   ", "\"\""] {
        assert!(
            ProcessMonitor::matching_process_ids(name).is_empty(),
            "{name:?}"
        );
    }
    assert!(!ProcessMonitor::is_process_running("  "));
}
