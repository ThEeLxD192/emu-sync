#![allow(dead_code)]
//! Helpers compartidos por las suites de `emusync-platform-desktop`.
//!
//! Ninguna espera usa `sleep`: la sincronización se hace con contadores, flags y canales.

use emusync_domain::{
    execution::SteamLaunchPreparation,
    library::{Emulator, GameEntry},
};
use emusync_platform_desktop::{ProcessPolling, SteamShortcut, SteamShortcutManager};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

pub type ProbeFn = Arc<dyn Fn(&str) -> HashSet<u64> + Send + Sync>;
pub type AliveFn = Arc<dyn Fn(u64) -> bool + Send + Sync>;

pub fn linux_manager(dir: &Path) -> SteamShortcutManager {
    SteamShortcutManager::new()
        .with_custom_shortcuts_file(dir.join("config/shortcuts.vdf"))
        .with_custom_launcher_dir(dir.join("launchers"))
        .with_simulate_windows(false)
}

/// Atajo sintético; `name` es el `AppName` literal (con o sin prefijo `[EmuSync] `).
pub fn shortcut(id: i32, name: &str) -> SteamShortcut {
    SteamShortcut {
        app_id: id,
        app_name: name.into(),
        exe: "\"/old/launcher\"".into(),
        ..SteamShortcut::default()
    }
}

pub fn emulator_entry(name: &str, exe: &str) -> GameEntry {
    GameEntry::Emulator(Emulator::new(name, exe, "/roms", vec![]))
}

/// Escribe `body` y lo marca como ejecutable (0o755 en Unix).
pub fn write_script(path: &Path, body: &str) -> PathBuf {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path.to_path_buf()
}

/// Un `fork` concurrente de otro test puede heredar el descriptor de escritura del script
/// recién creado y provocar `ETXTBSY`: se reintenta sin dormir.
pub fn spawn_retry(cmd: &mut Command) -> io::Result<Child> {
    let mut attempts = 0;
    loop {
        match cmd.spawn() {
            Err(err) if err.kind() == io::ErrorKind::ExecutableFileBusy && attempts < 50 => {
                attempts += 1;
                std::thread::yield_now();
            }
            other => return other,
        }
    }
}

/// Ejecuta un launcher generado con bash. Si el `exec` interno del script choca con `ETXTBSY`
/// bash termina con 126 sin haber ejecutado nada: se repite la ejecución.
#[cfg(unix)]
pub fn run_bash(script: &Path, cwd: &Path) -> std::process::Output {
    let mut attempts = 0;
    loop {
        let output = Command::new("bash")
            .arg(script)
            .current_dir(cwd)
            .output()
            .unwrap();
        if output.status.code() == Some(126) && attempts < 50 {
            attempts += 1;
            std::thread::yield_now();
            continue;
        }
        return output;
    }
}

/// Intervalos mínimos: son esperas de producción, no de test.
pub fn fast_polling() -> ProcessPolling {
    ProcessPolling {
        appearance_timeout: Duration::from_secs(30),
        appearance_interval: Duration::from_millis(1),
        exit_interval: Duration::from_millis(1),
        reuse_confirmation_delay: Duration::ZERO,
    }
}

pub fn polling_with_timeout(timeout: Duration) -> ProcessPolling {
    ProcessPolling {
        appearance_timeout: timeout,
        ..fast_polling()
    }
}

/// Nombre de proceso único que el sondeo real de `/proc` no encuentra.
pub fn unique_process_name(tag: &str) -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    format!("emusync-test-{}-{tag}-{n}", std::process::id())
}

pub async fn guard<F: std::future::Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("test future hung for 10 s")
}

/// Sondeo de procesos guiado por un guion: cada llamada saca el siguiente conjunto y, cuando
/// solo queda uno, lo repite. Registra cada llamada en un log compartido.
#[derive(Clone)]
pub struct ScriptedProbe {
    queue: Arc<Mutex<VecDeque<HashSet<u64>>>>,
    log: Arc<Mutex<Vec<String>>>,
    names: Arc<Mutex<Vec<String>>>,
    calls: Arc<AtomicUsize>,
}

impl ScriptedProbe {
    pub fn new(script: Vec<Vec<u64>>) -> Self {
        Self::with_log(script, Arc::new(Mutex::new(Vec::new())))
    }

    pub fn with_log(script: Vec<Vec<u64>>, log: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(
                script
                    .into_iter()
                    .map(|set| set.into_iter().collect())
                    .collect(),
            )),
            log,
            names: Arc::new(Mutex::new(Vec::new())),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn names(&self) -> Vec<String> {
        self.names.lock().unwrap().clone()
    }

    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    pub fn matching_fn(&self) -> ProbeFn {
        let probe = self.clone();
        Arc::new(move |name: &str| {
            probe.calls.fetch_add(1, Ordering::SeqCst);
            probe.names.lock().unwrap().push(name.to_string());
            probe.log.lock().unwrap().push("match".to_string());
            let mut queue = probe.queue.lock().unwrap();
            if queue.len() > 1 {
                queue.pop_front().unwrap()
            } else {
                queue.front().cloned().unwrap_or_default()
            }
        })
    }
}

/// Sondeo de vida por PID con presupuesto de respuestas `true`; después responde `false`.
#[derive(Clone, Default)]
pub struct AliveProbe {
    budget: Arc<Mutex<HashMap<u64, usize>>>,
    log: Arc<Mutex<Vec<u64>>>,
}

impl AliveProbe {
    pub fn new(budgets: &[(u64, usize)]) -> Self {
        Self {
            budget: Arc::new(Mutex::new(budgets.iter().copied().collect())),
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn calls_for(&self, pid: u64) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|p| **p == pid)
            .count()
    }

    pub fn total_calls(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    pub fn remaining(&self, pid: u64) -> usize {
        self.budget.lock().unwrap().get(&pid).copied().unwrap_or(0)
    }

    pub fn alive_fn(&self) -> AliveFn {
        let probe = self.clone();
        Arc::new(move |pid| {
            probe.log.lock().unwrap().push(pid);
            let mut budget = probe.budget.lock().unwrap();
            match budget.get_mut(&pid) {
                Some(left) if *left > 0 => {
                    *left -= 1;
                    true
                }
                _ => false,
            }
        })
    }
}

/// Preparación de Steam que registra la ROM, cuenta llamadas y anota `prepare` en un log.
#[derive(Clone, Default)]
pub struct MockSteamPreparation {
    pub prepared_rom: Arc<Mutex<Option<String>>>,
    pub calls: Arc<AtomicUsize>,
    pub log: Option<Arc<Mutex<Vec<String>>>>,
}

impl MockSteamPreparation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_log(log: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            log: Some(log),
            ..Self::default()
        }
    }
}

impl SteamLaunchPreparation for MockSteamPreparation {
    type Error = io::Error;

    fn prepare(&self, _entry: &Emulator, rom_path: &str) -> Result<(), Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(log) = &self.log {
            log.lock().unwrap().push("prepare".to_string());
        }
        *self.prepared_rom.lock().unwrap() = Some(rom_path.to_string());
        Ok(())
    }
}

/// `true` si el proceso de test puede escribir aunque el directorio no tenga permisos (root).
#[cfg(unix)]
pub fn permissions_are_not_enforced() -> bool {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let writable = fs::write(locked.join("probe"), b"").is_ok();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    writable
}

pub fn read_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

pub fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
