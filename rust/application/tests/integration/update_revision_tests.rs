//! Announcements of the update coordinator that reach the interface out of order.
//!
//! Each announcement runs outside the coordinator's lock, so a slow one can arrive after
//! newer ones. The interface keeps the announcement with the highest revision; these tests
//! hold one announcement back at the exact moment it is made, let a newer operation run to
//! its end, and then deliver the held one last.

use emusync_application::{DownloadFailure, UpdateCoordinator, UpdateRevision, UpdateState};
use emusync_domain::update::{UpdateFiles, UpdateInfo, UpdateInstaller, UpdateSource};
use std::future::Future;
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

const HARNESS_LIMIT: Duration = Duration::from_secs(10);
const DESTINATION: &str = "/tmp/revision-update.AppImage";

fn release() -> UpdateInfo {
    UpdateInfo {
        version: "2.0.0".into(),
        tag_name: "v2.0.0".into(),
        release_notes: "New release".into(),
        download_url: "https://example.com/EmuSync.AppImage".into(),
        asset_size: 1024,
        asset_name: "EmuSync.AppImage".into(),
        manifest_url: String::new(),
        signature_url: String::new(),
    }
}

fn ready_to_restart() -> UpdateState {
    UpdateState::ReadyToRestart {
        downloaded_path: DESTINATION.into(),
        is_steam_game_mode: false,
    }
}

/// A release source whose checks and downloads can be made to wait forever.
#[derive(Clone, Default)]
struct ScriptedSource {
    release: Arc<Mutex<Option<UpdateInfo>>>,
    is_check_stalled: Arc<AtomicBool>,
    is_download_stalled: Arc<AtomicBool>,
    download_error: Arc<Mutex<Option<String>>>,
}

impl UpdateSource for ScriptedSource {
    type Error = String;

    async fn check(&self) -> Result<Option<UpdateInfo>, Self::Error> {
        if self.is_check_stalled.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        Ok(self.release.lock().unwrap().clone())
    }

    async fn download(
        &self,
        _info: &UpdateInfo,
        _destination: &str,
        mut on_progress: impl FnMut(f32) + Send,
    ) -> Result<(), Self::Error> {
        on_progress(0.5);
        if self.is_download_stalled.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        if let Some(error) = self.download_error.lock().unwrap().clone() {
            return Err(error);
        }
        on_progress(1.0);
        Ok(())
    }
}

/// Downloads always land at `DESTINATION` with the size the release announces.
struct ExactSizeFiles;

impl UpdateFiles for ExactSizeFiles {
    type Error = String;

    async fn prepare_destination(&self, _info: &UpdateInfo) -> Result<String, Self::Error> {
        Ok(DESTINATION.into())
    }

    fn size(&self, _path: &str) -> Result<u64, Self::Error> {
        Ok(1024)
    }

    fn delete(&self, _path: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// An installer that never applies the update, so every restart fails.
struct RefusingInstaller;

impl UpdateInstaller for RefusingInstaller {
    type Error = String;

    fn is_steam_game_mode(&self) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn restart(&self, _path: &str) -> Result<bool, Self::Error> {
        Ok(false)
    }
}

type RevisionCoordinator = UpdateCoordinator<ScriptedSource, ExactSizeFiles, RefusingInstaller>;
/// An announcement as it reached the interface.
type Arrival = (UpdateRevision, UpdateState);
type Arrivals = Arc<Mutex<Vec<Arrival>>>;

/// What an interface that keeps the highest revision shows after `arrivals`, starting,
/// like the application window, from the default revision and `Idle`.
fn shown_after(arrivals: &[Arrival]) -> UpdateState {
    let mut shown_revision = UpdateRevision::default();
    let mut shown_state = UpdateState::Idle;
    for (revision, state) in arrivals {
        if *revision > shown_revision {
            shown_revision = *revision;
            shown_state = state.clone();
        }
    }
    shown_state
}

/// Two announcements never share a revision: the interface would drop the second one.
fn assert_revisions_are_distinct(arrivals: &[Arrival]) {
    let mut revisions: Vec<UpdateRevision> =
        arrivals.iter().map(|(revision, _)| *revision).collect();
    revisions.sort();
    revisions.dedup();
    assert_eq!(
        revisions.len(),
        arrivals.len(),
        "each announcement has a revision of its own: {arrivals:?}"
    );
}

/// Polls `step` once, as nothing in these fakes waits for a wake-up.
fn poll_once<T>(step: Pin<&mut impl Future<Output = T>>) -> Poll<T> {
    step.poll(&mut Context::from_waker(Waker::noop()))
}

/// Runs `step` to its end inside the current call; the fakes only wait when stalled.
fn finish_without_waiting<T>(step: impl Future<Output = T>) -> T {
    let mut step = pin!(step);
    match poll_once(step.as_mut()) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the step had to wait"),
    }
}

/// Starts `step`, which must stop at a stalled fake, and then drops it, as an aborted task.
fn interrupt_at_first_wait<T>(step: impl Future<Output = T>) {
    let mut step = pin!(step);
    assert!(
        poll_once(step.as_mut()).is_pending(),
        "the step must stop at the stalled fake before it is interrupted"
    );
}

/// Runs `operation` on a thread of its own and waits for it with a wall-clock limit, so an
/// announcement made under the coordinator's lock fails the test instead of hanging it.
fn on_own_thread_within_limit(operation: impl FnOnce() + Send + 'static) {
    let (finished_tx, finished_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        operation();
        let _ = finished_tx.send(());
    });
    finished_rx
        .recv_timeout(HARNESS_LIMIT)
        .expect("the operation did not finish within the harness limit");
    worker.join().expect("the operation panicked");
}

/// Whether an announced state is the one to hold back.
type HeldStatePredicate = fn(&UpdateState) -> bool;

/// Holds back the first announcement that matches the armed predicate until released.
struct Hold {
    is_held: Mutex<Option<HeldStatePredicate>>,
    reached_tx: mpsc::Sender<()>,
    release_rx: Mutex<mpsc::Receiver<()>>,
}

impl Hold {
    fn takes(&self, state: &UpdateState) -> bool {
        let mut is_held = self.is_held.lock().unwrap();
        if is_held.is_some_and(|matches| matches(state)) {
            *is_held = None;
            return true;
        }
        false
    }
}

/// A coordinator whose announcements are recorded as they reach the interface, one of
/// which can be held back.
struct Crossing {
    coordinator: Arc<RevisionCoordinator>,
    source: ScriptedSource,
    arrivals: Arrivals,
    hold: Arc<Hold>,
    reached_rx: mpsc::Receiver<()>,
    release_tx: mpsc::Sender<()>,
}

impl Crossing {
    fn new() -> Self {
        let source = ScriptedSource::default();
        let arrivals: Arrivals = Arc::default();
        let (reached_tx, reached_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let hold = Arc::new(Hold {
            is_held: Mutex::new(None),
            reached_tx,
            release_rx: Mutex::new(release_rx),
        });
        let coordinator = Arc::new(UpdateCoordinator::new(
            source.clone(),
            ExactSizeFiles,
            RefusingInstaller,
            {
                let arrivals = Arc::clone(&arrivals);
                let hold = Arc::clone(&hold);
                move |state: &UpdateState, revision| {
                    if hold.takes(state) {
                        let _ = hold.reached_tx.send(());
                        hold.release_rx
                            .lock()
                            .unwrap()
                            .recv_timeout(HARNESS_LIMIT)
                            .expect("the held announcement was never released");
                    }
                    // Recorded once delivered, as an event sent after the slow part.
                    arrivals.lock().unwrap().push((revision, state.clone()));
                }
            },
        ));
        Self {
            coordinator,
            source,
            arrivals,
            hold,
            reached_rx,
            release_tx,
        }
    }

    /// Holds back the next announcement for which `is_held` is true.
    fn hold_next(&self, is_held: HeldStatePredicate) {
        *self.hold.is_held.lock().unwrap() = Some(is_held);
    }

    /// Runs `held_operation` on its own thread until its held announcement is made, runs
    /// `overtaking_operation` to its end meanwhile, and only then delivers the held one.
    fn cross(
        &self,
        held_operation: impl FnOnce(&RevisionCoordinator) + Send + 'static,
        overtaking_operation: impl FnOnce(&RevisionCoordinator),
    ) {
        let (finished_tx, finished_rx) = mpsc::channel();
        let coordinator = Arc::clone(&self.coordinator);
        let held_thread = std::thread::spawn(move || {
            held_operation(&coordinator);
            let _ = finished_tx.send(());
        });
        self.reached_rx
            .recv_timeout(HARNESS_LIMIT)
            .expect("the held announcement never reached the interface");
        overtaking_operation(&self.coordinator);
        self.release_tx
            .send(())
            .expect("the held announcement is waiting");
        finished_rx
            .recv_timeout(HARNESS_LIMIT)
            .expect("the held operation did not finish once released");
        held_thread.join().expect("the held operation panicked");
    }

    fn arrivals(&self) -> Vec<Arrival> {
        self.arrivals.lock().unwrap().clone()
    }

    /// The held announcement arrived last, after newer ones, and the interface still shows
    /// `expected`, which is the coordinator's state.
    fn assert_newest_on_screen(&self, held: &UpdateState, expected: &UpdateState) {
        let arrivals = self.arrivals();
        assert_eq!(
            arrivals.last().map(|(_, state)| state),
            Some(held),
            "the held announcement must arrive last for the announcements to cross: {arrivals:?}"
        );
        assert_ne!(
            held, expected,
            "the held announcement is older than the outcome"
        );
        assert_revisions_are_distinct(&arrivals);
        assert_eq!(
            shown_after(&arrivals),
            *expected,
            "the interface must keep the newest state: {arrivals:?}"
        );
        assert_eq!(self.coordinator.state(), *expected);
    }
}

#[test]
fn a_checking_announcement_delivered_after_a_whole_download_leaves_ready_to_restart_on_screen() {
    let crossing = Crossing::new();
    *crossing.source.release.lock().unwrap() = Some(release());
    crossing.hold_next(|state| *state == UpdateState::Checking);

    crossing.cross(
        |coordinator| finish_without_waiting(coordinator.check_for_updates()),
        |coordinator| finish_without_waiting(coordinator.download(&release())),
    );

    crossing.assert_newest_on_screen(&UpdateState::Checking, &ready_to_restart());
}

#[test]
fn a_cancelled_check_whose_restored_state_is_delivered_after_a_download_leaves_ready_to_restart() {
    let crossing = Crossing::new();
    *crossing.source.release.lock().unwrap() = Some(release());
    finish_without_waiting(crossing.coordinator.check_for_updates());
    assert_eq!(
        crossing.coordinator.state(),
        UpdateState::Available(release())
    );
    crossing
        .source
        .is_check_stalled
        .store(true, Ordering::SeqCst);
    crossing.hold_next(|state| matches!(state, UpdateState::Available(_)));

    crossing.cross(
        // The cancelled check puts back the release it replaced with `Checking`.
        |coordinator| interrupt_at_first_wait(coordinator.check_for_updates()),
        |coordinator| finish_without_waiting(coordinator.download(&release())),
    );

    crossing.assert_newest_on_screen(&UpdateState::Available(release()), &ready_to_restart());
}

#[test]
fn a_second_check_that_finds_the_state_already_checking_overtakes_the_first_checking() {
    let crossing = Crossing::new();
    *crossing.source.release.lock().unwrap() = Some(release());
    crossing.hold_next(|state| *state == UpdateState::Checking);

    crossing.cross(
        |coordinator| finish_without_waiting(coordinator.check_for_updates()),
        // `Checking` is already the state, so this check only announces its result.
        |coordinator| finish_without_waiting(coordinator.check_for_updates()),
    );

    crossing.assert_newest_on_screen(&UpdateState::Checking, &UpdateState::Available(release()));
    assert_eq!(
        crossing
            .arrivals()
            .iter()
            .map(|(_, state)| state.clone())
            .collect::<Vec<_>>(),
        vec![UpdateState::Available(release()), UpdateState::Checking],
        "the second check announces no `Checking` of its own"
    );
}

#[test]
fn an_interrupted_download_whose_release_is_delivered_after_a_newer_check_leaves_its_result() {
    let crossing = Crossing::new();
    crossing
        .source
        .is_download_stalled
        .store(true, Ordering::SeqCst);
    crossing.hold_next(|state| matches!(state, UpdateState::Available(_)));

    crossing.cross(
        // The interrupted download offers the release again.
        |coordinator| interrupt_at_first_wait(coordinator.download(&release())),
        // Meanwhile a check finds no newer release.
        |coordinator| finish_without_waiting(coordinator.check_for_updates()),
    );

    crossing.assert_newest_on_screen(&UpdateState::Available(release()), &UpdateState::Idle);
}

#[test]
fn a_download_failure_delivered_after_a_newer_check_leaves_the_found_release_on_screen() {
    let crossing = Crossing::new();
    *crossing.source.release.lock().unwrap() = Some(release());
    *crossing.source.download_error.lock().unwrap() = Some("Network cut".into());
    crossing.hold_next(|state| matches!(state, UpdateState::DownloadFailed(_)));

    crossing.cross(
        |coordinator| finish_without_waiting(coordinator.download(&release())),
        |coordinator| finish_without_waiting(coordinator.check_for_updates()),
    );

    crossing.assert_newest_on_screen(
        &UpdateState::DownloadFailed(DownloadFailure::Transfer("Network cut".into())),
        &UpdateState::Available(release()),
    );
}

#[test]
fn a_restart_failure_delivered_after_a_newer_check_leaves_the_check_result_on_screen() {
    let crossing = Crossing::new();
    finish_without_waiting(crossing.coordinator.download(&release()));
    assert_eq!(crossing.coordinator.state(), ready_to_restart());
    crossing.hold_next(|state| matches!(state, UpdateState::RestartFailed { .. }));

    crossing.cross(
        |coordinator| assert!(!coordinator.restart(DESTINATION, false)),
        // The header offers a new check once the restart failed; it finds nothing newer.
        |coordinator| finish_without_waiting(coordinator.check_for_updates()),
    );

    crossing.assert_newest_on_screen(
        &UpdateState::RestartFailed {
            downloaded_path: DESTINATION.into(),
        },
        &UpdateState::Idle,
    );
}

/// A coordinator whose interface reacts to each announcement before delivering it, so
/// the states its reaction causes arrive first.
fn coordinator_reacting_before_delivery(
    source: ScriptedSource,
    react: impl Fn(&RevisionCoordinator, &UpdateState) + Send + Sync + 'static,
) -> (Arc<RevisionCoordinator>, Arrivals) {
    let arrivals: Arrivals = Arc::default();
    let recorder = Arc::clone(&arrivals);
    let coordinator = Arc::new_cyclic(|this: &std::sync::Weak<RevisionCoordinator>| {
        let this = this.clone();
        UpdateCoordinator::new(
            source,
            ExactSizeFiles,
            RefusingInstaller,
            move |state: &UpdateState, revision| {
                if let Some(coordinator) = this.upgrade() {
                    react(&coordinator, state);
                }
                recorder.lock().unwrap().push((revision, state.clone()));
            },
        )
    });
    (coordinator, arrivals)
}

#[test]
fn states_announced_from_inside_an_announcement_and_delivered_before_it_leave_the_newest_on_screen()
{
    // (case, failing download, reaction, outcome the reaction leads to)
    type Reaction = fn(&RevisionCoordinator, &UpdateState);
    let check_on_failure: Reaction = |coordinator, state| {
        if matches!(state, UpdateState::DownloadFailed(_)) {
            finish_without_waiting(coordinator.check_for_updates());
        }
    };
    let restart_when_ready: Reaction = |coordinator, state| {
        if let UpdateState::ReadyToRestart {
            downloaded_path, ..
        } = state
        {
            assert!(!coordinator.restart(downloaded_path, false));
        }
    };
    let cases = [
        (
            "a check started when the failure shows",
            Some("Network cut"),
            check_on_failure,
            UpdateState::DownloadFailed(DownloadFailure::Transfer("Network cut".into())),
            UpdateState::Available(release()),
        ),
        (
            "a restart pressed as soon as it is offered",
            None,
            restart_when_ready,
            ready_to_restart(),
            UpdateState::RestartFailed {
                downloaded_path: DESTINATION.into(),
            },
        ),
    ];
    for (case, download_error, reaction, delivered_last, expected) in cases {
        let source = ScriptedSource::default();
        *source.release.lock().unwrap() = Some(release());
        *source.download_error.lock().unwrap() = download_error.map(str::to_owned);
        let (coordinator, arrivals) = coordinator_reacting_before_delivery(source, reaction);

        on_own_thread_within_limit({
            let coordinator = Arc::clone(&coordinator);
            move || finish_without_waiting(coordinator.download(&release()))
        });

        let arrivals = arrivals.lock().unwrap().clone();
        assert_eq!(
            arrivals.last().map(|(_, state)| state),
            Some(&delivered_last),
            "{case}: the announcement that caused the reaction arrives last: {arrivals:?}"
        );
        assert_revisions_are_distinct(&arrivals);
        assert_eq!(shown_after(&arrivals), expected, "{case}: {arrivals:?}");
        assert_eq!(coordinator.state(), expected, "{case}");
    }
}

/// Marks the child process that runs `first_announcement_of_the_process_child`.
const FIRST_ANNOUNCEMENT_CHILD_VARIABLE: &str = "EMUSYNC_FIRST_UPDATE_ANNOUNCEMENT_CHILD";

#[test]
fn the_first_state_announced_in_a_process_is_newer_than_the_initial_revision_of_the_window() {
    // Revisions may be shared by every coordinator of the process, so the very first
    // announcement only happens in a process of its own: the child runs that test alone.
    let module = module_path!().split_once("::").unwrap().1;
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("{module}::first_announcement_of_the_process_child"),
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(FIRST_ANNOUNCEMENT_CHILD_VARIABLE, "1")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child failed:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("test result: ok. 1 passed;"),
        "the child did not run the test:\n{stdout}"
    );
}

/// The first announcement of a fresh process, which the parent test above checks.
#[test]
#[ignore = "re-executed as a child process in which no coordinator announced anything yet"]
fn first_announcement_of_the_process_child() {
    if std::env::var_os(FIRST_ANNOUNCEMENT_CHILD_VARIABLE).is_none() {
        return;
    }
    let crossing = Crossing::new();

    assert!(!crossing.coordinator.restart(DESTINATION, false));

    let arrivals = crossing.arrivals();
    let failed = UpdateState::RestartFailed {
        downloaded_path: DESTINATION.into(),
    };
    assert_eq!(
        arrivals.iter().map(|(_, state)| state).collect::<Vec<_>>(),
        vec![&failed]
    );
    assert!(
        arrivals[0].0 > UpdateRevision::default(),
        "the window starts at the default revision and ignores anything not newer: {arrivals:?}"
    );
    assert_eq!(shown_after(&arrivals), failed);
}

#[test]
fn a_coordinator_created_to_replace_another_announces_states_the_window_still_shows() {
    let replaced = Crossing::new();
    let replacement = Crossing::new();
    *replacement.source.release.lock().unwrap() = Some(release());

    // The replaced coordinator announced several states before it was replaced, more than
    // the replacement announces afterwards.
    finish_without_waiting(replaced.coordinator.download(&release()));
    assert!(!replaced.coordinator.restart(DESTINATION, false));
    finish_without_waiting(replacement.coordinator.check_for_updates());

    let replaced_arrivals = replaced.arrivals();
    let replacement_arrivals = replacement.arrivals();
    assert_eq!(
        replacement_arrivals
            .iter()
            .map(|(_, state)| state.clone())
            .collect::<Vec<_>>(),
        vec![UpdateState::Checking, UpdateState::Available(release())]
    );
    assert!(
        replaced_arrivals.len() > replacement_arrivals.len(),
        "{replaced_arrivals:?}"
    );
    // One window receives both: first everything from the replaced coordinator.
    let window_arrivals: Vec<Arrival> = replaced_arrivals
        .iter()
        .chain(&replacement_arrivals)
        .cloned()
        .collect();
    assert_eq!(
        shown_after(&window_arrivals),
        UpdateState::Available(release()),
        "the replacement's states must not be taken for older ones: {window_arrivals:?}"
    );
}
