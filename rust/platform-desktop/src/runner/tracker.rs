use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct ProcessLaunch {
    pub name: String,
    pub existing_ids: HashSet<u64>,
    pub request_exit_code: Option<i32>,
    pub request_is_alive: bool,
    pub desktop_handoff_succeeded: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessPolling {
    pub appearance_timeout: Duration,
    pub appearance_interval: Duration,
    pub exit_interval: Duration,
    pub reuse_confirmation_delay: Duration,
}

impl Default for ProcessPolling {
    fn default() -> Self {
        Self {
            appearance_timeout: Duration::from_secs(60),
            appearance_interval: Duration::from_secs(1),
            exit_interval: Duration::from_secs(2),
            reuse_confirmation_delay: Duration::from_secs(1),
        }
    }
}

pub type MatchingProcessIdsFn = Arc<dyn Fn(&str) -> HashSet<u64> + Send + Sync>;
pub type IsShaderCompilingFn = Arc<dyn Fn() -> bool + Send + Sync>;
pub type IsProcessAliveFn = Arc<dyn Fn(u64) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct ExternalProcessTracker {
    matching_process_ids: MatchingProcessIdsFn,
    is_steam_shader_compiling: IsShaderCompilingFn,
    is_process_alive: IsProcessAliveFn,
}

struct AppearanceState {
    started_at: Instant,
    successful_request_at: Option<Instant>,
}

impl ExternalProcessTracker {
    // Los sondeos del sistema bloquean (lectura de /proc, procesos externos): se ejecutan
    // fuera del executor. Un `JoinError` se trata como "sin coincidencias"/"no vivo".
    async fn matching(&self, name: &str) -> HashSet<u64> {
        let probe = Arc::clone(&self.matching_process_ids);
        let name = name.to_string();
        tokio::task::spawn_blocking(move || probe(&name))
            .await
            .unwrap_or_default()
    }

    async fn shader_compiling(&self) -> bool {
        let probe = Arc::clone(&self.is_steam_shader_compiling);
        tokio::task::spawn_blocking(move || probe())
            .await
            .unwrap_or(false)
    }

    async fn retain_alive(&self, pids: HashSet<u64>) -> HashSet<u64> {
        let probe = Arc::clone(&self.is_process_alive);
        tokio::task::spawn_blocking(move || pids.into_iter().filter(|pid| probe(*pid)).collect())
            .await
            .unwrap_or_default()
    }

    pub fn new(
        matching_process_ids: MatchingProcessIdsFn,
        is_steam_shader_compiling: IsShaderCompilingFn,
        is_process_alive: IsProcessAliveFn,
    ) -> Self {
        Self {
            matching_process_ids,
            is_steam_shader_compiling,
            is_process_alive,
        }
    }

    pub async fn wait_for_process(&self, launch: ProcessLaunch, polling: ProcessPolling) -> bool {
        let tracked = self.await_appearance(&launch, &polling).await;
        let appeared = !tracked.is_empty();
        let mut tracked_mut = tracked;
        self.await_exit(&launch, &mut tracked_mut, polling.exit_interval)
            .await;
        appeared
    }

    async fn await_appearance(
        &self,
        launch: &ProcessLaunch,
        polling: &ProcessPolling,
    ) -> HashSet<u64> {
        let mut state = AppearanceState {
            started_at: Instant::now(),
            successful_request_at: None,
        };

        let mut outcome: Option<HashSet<u64>> = None;

        while outcome.is_none() && state.started_at.elapsed() < polling.appearance_timeout {
            outcome = self
                .observe_appearance(launch, &mut state, polling.reuse_confirmation_delay)
                .await;
            if outcome.is_none() {
                if self.shader_compiling().await {
                    state.started_at = Instant::now();
                }
                tokio::time::sleep(polling.appearance_interval).await;
            }
        }

        outcome.unwrap_or_default()
    }

    async fn observe_appearance(
        &self,
        launch: &ProcessLaunch,
        state: &mut AppearanceState,
        confirmation_delay: Duration,
    ) -> Option<HashSet<u64>> {
        let current_matches = self.matching(&launch.name).await;
        let new_matches: HashSet<u64> = current_matches
            .difference(&launch.existing_ids)
            .copied()
            .collect();

        if !new_matches.is_empty() {
            return Some(new_matches);
        }

        let exit_code = if !launch.request_is_alive {
            launch.request_exit_code
        } else {
            None
        };

        if let Some(code) = exit_code
            && code != 0
        {
            return Some(HashSet::new());
        }

        self.confirmed_reuse(launch, state, confirmation_delay, exit_code)
            .await
    }

    async fn confirmed_reuse(
        &self,
        launch: &ProcessLaunch,
        state: &mut AppearanceState,
        confirmation_delay: Duration,
        exit_code: Option<i32>,
    ) -> Option<HashSet<u64>> {
        if !launch.desktop_handoff_succeeded && exit_code != Some(0) {
            state.successful_request_at = None;
            return None;
        }

        let now = Instant::now();
        let confirmed_at = match state.successful_request_at {
            Some(t) => t,
            None => {
                state.successful_request_at = Some(now);
                now
            }
        };

        if now.duration_since(confirmed_at) < confirmation_delay {
            None
        } else {
            let current_matches = self.matching(&launch.name).await;
            let reused: HashSet<u64> = current_matches
                .intersection(&launch.existing_ids)
                .copied()
                .collect();
            if !reused.is_empty() {
                Some(reused)
            } else {
                None
            }
        }
    }

    async fn await_exit(
        &self,
        launch: &ProcessLaunch,
        tracked: &mut HashSet<u64>,
        interval: Duration,
    ) {
        while !tracked.is_empty() {
            let current_matches = self.matching(&launch.name).await;
            let active_matches: HashSet<u64> = current_matches
                .into_iter()
                .filter(|pid| !launch.existing_ids.contains(pid) || tracked.contains(pid))
                .collect();

            tracked.extend(active_matches);
            let candidates = std::mem::take(tracked);
            *tracked = self.retain_alive(candidates).await;

            if !tracked.is_empty() {
                tokio::time::sleep(interval).await;
            }
        }
    }
}
