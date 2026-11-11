//! `ExternalProcessTracker`: aparición, renovación por shaders, reutilización y seguimiento de salida.
//!
//! Los sondeos se guían por contadores (`ScriptedProbe`, `AliveProbe`); no hay `sleep` ni
//! aserciones sobre tiempo transcurrido.

mod common;

use common::{AliveProbe, ScriptedProbe, fast_polling, guard, polling_with_timeout};
use emusync_platform_desktop::{ExternalProcessTracker, ProcessLaunch};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn launch(existing: &[u64]) -> ProcessLaunch {
    ProcessLaunch {
        name: "game.exe".into(),
        existing_ids: existing.iter().copied().collect(),
        request_exit_code: None,
        request_is_alive: true,
        desktop_handoff_succeeded: false,
    }
}

fn tracker(matching: &ScriptedProbe, alive: &AliveProbe) -> ExternalProcessTracker {
    ExternalProcessTracker::new(matching.matching_fn(), Arc::new(|| false), alive.alive_fn())
}

#[tokio::test]
async fn appearance_within_timeout_and_exit_tracking() {
    let matching = ScriptedProbe::new(vec![vec![], vec![], vec![42], vec![42], vec![]]);
    let alive = AliveProbe::new(&[(42, 2)]);
    let tracker = tracker(&matching, &alive);

    let success = guard(tracker.wait_for_process(launch(&[]), fast_polling())).await;

    assert!(success);
    assert_eq!(
        alive.calls_for(42),
        3,
        "two live answers and the final death"
    );
    assert_eq!(alive.remaining(42), 0);
}

#[tokio::test]
async fn timeout_renewed_while_shader_compilation_is_active() {
    // Márgenes amplios (50 ms de timeout frente a 200 ms de shaders) para que la carga del
    // host no pueda agotar el timeout entre dos iteraciones.
    let first_call: Arc<std::sync::Mutex<Option<Instant>>> = Arc::default();
    let shader_done = Arc::new(AtomicBool::new(false));
    let shader_calls = Arc::new(AtomicUsize::new(0));

    let tracker = {
        let (first_call, shader_done, shader_calls) = (
            Arc::clone(&first_call),
            Arc::clone(&shader_done),
            Arc::clone(&shader_calls),
        );
        let done_for_matching = Arc::clone(&shader_done);
        ExternalProcessTracker::new(
            Arc::new(move |_| {
                if done_for_matching.load(Ordering::SeqCst) {
                    HashSet::from([99])
                } else {
                    HashSet::new()
                }
            }),
            Arc::new(move || {
                shader_calls.fetch_add(1, Ordering::SeqCst);
                let t0 = *first_call.lock().unwrap().get_or_insert_with(Instant::now);
                if t0.elapsed() < Duration::from_millis(200) {
                    true
                } else {
                    shader_done.store(true, Ordering::SeqCst);
                    false
                }
            }),
            AliveProbe::new(&[]).alive_fn(),
        )
    };

    let success = guard(
        tracker.wait_for_process(launch(&[]), polling_with_timeout(Duration::from_millis(50))),
    )
    .await;

    assert!(success);
    assert!(shader_done.load(Ordering::SeqCst));
    assert!(shader_calls.load(Ordering::SeqCst) > 1);
}

#[tokio::test]
async fn failed_launch_request_aborts_without_waiting_for_existing_processes() {
    let matching = ScriptedProbe::new(vec![vec![100]]);
    let alive = AliveProbe::new(&[(100, usize::MAX)]);
    let shader_calls = Arc::new(AtomicUsize::new(0));
    let tracker = {
        let shader_calls = Arc::clone(&shader_calls);
        ExternalProcessTracker::new(
            matching.matching_fn(),
            Arc::new(move || {
                shader_calls.fetch_add(1, Ordering::SeqCst);
                false
            }),
            alive.alive_fn(),
        )
    };
    let failed = ProcessLaunch {
        request_exit_code: Some(1),
        request_is_alive: false,
        ..launch(&[100])
    };

    let success =
        guard(tracker.wait_for_process(failed, polling_with_timeout(Duration::from_secs(3600))))
            .await;

    assert!(!success);
    assert_eq!(matching.calls(), 1);
    assert_eq!(shader_calls.load(Ordering::SeqCst), 0);
    assert_eq!(alive.total_calls(), 0);
}

#[tokio::test]
async fn exit_wait_ignores_preexisting_same_name_process_that_never_exits() {
    // El PID 100 ya existía y sigue vivo para siempre; solo el 42 pertenece al lanzamiento.
    let matching = ScriptedProbe::new(vec![vec![100, 42], vec![100, 42], vec![100, 42], vec![100]]);
    let alive = AliveProbe::new(&[(42, 2), (100, usize::MAX)]);
    let tracker = tracker(&matching, &alive);

    let success = guard(tracker.wait_for_process(launch(&[100]), fast_polling())).await;

    assert!(success);
    assert_eq!(alive.calls_for(42), 3);
    assert_eq!(alive.calls_for(100), 0);
}

#[tokio::test]
async fn exit_wait_follows_respawned_process_with_new_pid() {
    let matching = ScriptedProbe::new(vec![vec![42], vec![42], vec![43]]);
    let alive = AliveProbe::new(&[(42, 1), (43, 2)]);
    let tracker = tracker(&matching, &alive);

    let success = guard(tracker.wait_for_process(launch(&[]), fast_polling())).await;

    assert!(success);
    assert_eq!(
        alive.remaining(43),
        0,
        "the new PID was followed until it died"
    );
    assert_eq!(alive.calls_for(43), 3);
    assert_eq!(alive.remaining(42), 0);
}

#[tokio::test]
async fn reused_existing_process_is_tracked_after_successful_handoff() {
    let matching = ScriptedProbe::new(vec![vec![100]]);
    let alive = AliveProbe::new(&[(100, 1)]);
    let tracker = tracker(&matching, &alive);
    let handed_off = ProcessLaunch {
        desktop_handoff_succeeded: true,
        ..launch(&[100])
    };

    let success = guard(tracker.wait_for_process(handed_off, fast_polling())).await;

    assert!(success);
    assert_eq!(alive.calls_for(100), 2);
}

#[tokio::test]
async fn reuse_is_not_confirmed_before_delay_and_times_out() {
    let matching = ScriptedProbe::new(vec![vec![100]]);
    let alive = AliveProbe::new(&[(100, usize::MAX)]);
    let tracker = tracker(&matching, &alive);
    let handed_off = ProcessLaunch {
        desktop_handoff_succeeded: true,
        ..launch(&[100])
    };
    let polling = emusync_platform_desktop::ProcessPolling {
        appearance_timeout: Duration::from_millis(20),
        reuse_confirmation_delay: Duration::from_secs(3600),
        ..fast_polling()
    };

    let success = guard(tracker.wait_for_process(handed_off, polling)).await;

    assert!(!success);
    assert_eq!(alive.total_calls(), 0);
}

#[tokio::test]
async fn existing_process_is_not_reused_when_handoff_failed() {
    let matching = ScriptedProbe::new(vec![vec![100]]);
    let alive = AliveProbe::new(&[(100, usize::MAX)]);
    let tracker = tracker(&matching, &alive);
    let no_handoff = ProcessLaunch {
        desktop_handoff_succeeded: false,
        request_is_alive: true,
        ..launch(&[100])
    };

    let success = guard(
        tracker.wait_for_process(no_handoff, polling_with_timeout(Duration::from_millis(20))),
    )
    .await;

    assert!(!success);
    assert_eq!(alive.total_calls(), 0);
}

#[tokio::test]
async fn nonzero_exit_code_is_ignored_while_request_is_still_alive() {
    let matching = ScriptedProbe::new(vec![vec![], vec![], vec![7]]);
    let alive = AliveProbe::new(&[]);
    let tracker = tracker(&matching, &alive);
    let still_running = ProcessLaunch {
        request_exit_code: Some(1),
        request_is_alive: true,
        ..launch(&[])
    };

    let success = guard(tracker.wait_for_process(still_running, fast_polling())).await;

    assert!(success, "a non-zero code of a live request must not abort");
    assert!(matching.calls() >= 3);
    assert_eq!(alive.calls_for(7), 1);
}

#[tokio::test]
async fn zero_exit_code_allows_reuse_even_without_desktop_handoff() {
    let matching = ScriptedProbe::new(vec![vec![100]]);
    let alive = AliveProbe::new(&[(100, 1)]);
    let tracker = tracker(&matching, &alive);
    let finished = ProcessLaunch {
        request_exit_code: Some(0),
        request_is_alive: false,
        desktop_handoff_succeeded: false,
        ..launch(&[100])
    };

    let success = guard(tracker.wait_for_process(finished, fast_polling())).await;

    assert!(success);
    assert_eq!(alive.calls_for(100), 2);
}

#[tokio::test]
async fn panicking_probes_degrade_to_no_match_instead_of_crashing() {
    let panicking_match = ExternalProcessTracker::new(
        Arc::new(|_| panic!("synthetic matching panic")),
        Arc::new(|| false),
        AliveProbe::new(&[]).alive_fn(),
    );
    let success = guard(
        panicking_match
            .wait_for_process(launch(&[]), polling_with_timeout(Duration::from_millis(20))),
    )
    .await;
    assert!(!success);

    let alive_calls = Arc::new(AtomicUsize::new(0));
    let panicking_alive = {
        let alive_calls = Arc::clone(&alive_calls);
        ExternalProcessTracker::new(
            Arc::new(|_| HashSet::from([5])),
            Arc::new(|| false),
            Arc::new(move |_| {
                alive_calls.fetch_add(1, Ordering::SeqCst);
                panic!("synthetic liveness panic")
            }),
        )
    };
    let success = guard(panicking_alive.wait_for_process(launch(&[]), fast_polling())).await;
    assert!(success);
    assert_eq!(alive_calls.load(Ordering::SeqCst), 1);
}
