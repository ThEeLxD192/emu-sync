use emusync_platform_desktop::{ExternalProcessTracker, ProcessLaunch, ProcessPolling};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[tokio::test]
async fn appearance_within_timeout_and_exit_tracking() {
    let pids = Arc::new(Mutex::new(HashSet::new()));
    let pids_clone = pids.clone();

    let is_alive_pids = pids.clone();
    let tracker = ExternalProcessTracker::new(
        Arc::new(move |_| pids_clone.lock().unwrap().clone()),
        Arc::new(|| false),
        Arc::new(move |pid| is_alive_pids.lock().unwrap().contains(&pid)),
    );

    let launch = ProcessLaunch {
        name: "game.exe".into(),
        existing_ids: HashSet::new(),
        request_exit_code: None,
        request_is_alive: true,
        desktop_handoff_succeeded: false,
    };

    let polling = ProcessPolling {
        appearance_timeout: Duration::from_millis(500),
        appearance_interval: Duration::from_millis(20),
        exit_interval: Duration::from_millis(20),
        reuse_confirmation_delay: Duration::from_millis(50),
    };

    // Simulate process appearing after 50ms and exiting after 150ms
    let pids_sim = pids.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        pids_sim.lock().unwrap().insert(42);
        tokio::time::sleep(Duration::from_millis(100)).await;
        pids_sim.lock().unwrap().remove(&42);
    });

    let success = tracker.wait_for_process(launch, polling).await;
    assert!(success);
}

#[tokio::test]
async fn timeout_renewed_while_shader_compilation_is_active() {
    let shader_compiling = Arc::new(AtomicBool::new(true));
    let shader_clone = shader_compiling.clone();

    let pids = Arc::new(Mutex::new(HashSet::new()));
    let pids_clone = pids.clone();

    let tracker = ExternalProcessTracker::new(
        Arc::new(move |_| pids_clone.lock().unwrap().clone()),
        Arc::new(move || shader_clone.load(Ordering::SeqCst)),
        Arc::new(|_| false),
    );

    let launch = ProcessLaunch {
        name: "game.exe".into(),
        existing_ids: HashSet::new(),
        request_exit_code: None,
        request_is_alive: true,
        desktop_handoff_succeeded: false,
    };

    // Timeout is 100ms, but shader compiling is active for 200ms
    let polling = ProcessPolling {
        appearance_timeout: Duration::from_millis(100),
        appearance_interval: Duration::from_millis(20),
        exit_interval: Duration::from_millis(20),
        reuse_confirmation_delay: Duration::from_millis(50),
    };

    let pids_sim = pids.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        shader_compiling.store(false, Ordering::SeqCst);
        pids_sim.lock().unwrap().insert(99);
    });

    let success = tracker.wait_for_process(launch, polling).await;
    assert!(success);
}

#[tokio::test]
async fn failed_launch_request_aborts_without_waiting_for_existing_processes() {
    let mut existing = HashSet::new();
    existing.insert(100);

    let pids = Arc::new(Mutex::new(existing.clone()));
    let tracker = ExternalProcessTracker::new(
        Arc::new(move |_| pids.lock().unwrap().clone()),
        Arc::new(|| false),
        Arc::new(|_| true),
    );

    let launch = ProcessLaunch {
        name: "game.exe".into(),
        existing_ids: existing,
        request_exit_code: Some(1), // Failed launch exit code
        request_is_alive: false,
        desktop_handoff_succeeded: false,
    };

    let polling = ProcessPolling {
        appearance_timeout: Duration::from_secs(5),
        appearance_interval: Duration::from_millis(20),
        exit_interval: Duration::from_millis(20),
        reuse_confirmation_delay: Duration::from_millis(50),
    };

    let start = std::time::Instant::now();
    let success = tracker.wait_for_process(launch, polling).await;
    assert!(!success);
    // Should abort immediately without waiting the full 5 second timeout
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[cfg(target_os = "linux")]
#[test]
fn exited_unreaped_child_is_not_alive() {
    use emusync_platform_desktop::ProcessMonitor;
    let mut child = std::process::Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn()
        .unwrap();
    let pid = u64::from(child.id());
    // `try_wait` no se usa: se espera a que `/proc` muestre el zombi sin recogerlo.
    let state_path = format!("/proc/{pid}/status");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let status = std::fs::read_to_string(&state_path).unwrap_or_default();
        if status
            .lines()
            .any(|l| l.starts_with("State:") && l.contains('Z'))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "child never became a zombie"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!ProcessMonitor::is_process_alive(pid));
    child.wait().unwrap();
    assert!(!ProcessMonitor::is_process_alive(pid));
}

#[cfg(target_os = "linux")]
#[test]
fn running_process_is_alive() {
    use emusync_platform_desktop::ProcessMonitor;
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    assert!(ProcessMonitor::is_process_alive(u64::from(child.id())));
    child.kill().unwrap();
    child.wait().unwrap();
}
