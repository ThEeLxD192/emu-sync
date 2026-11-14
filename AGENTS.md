# Guía de trabajo de EmuSync

## Estado y alcance

EmuSync es una aplicación de escritorio Rust nativa para Linux y Windows (workspace Cargo en `rust/` y `xtask/`). La migración desde Kotlin/Compose terminó y el código Kotlin se retiró; su historial queda en Git. Linux está validado; Windows tiene validaciones nativas pendientes (ver «Pendientes Windows»).

Mantener los flujos y distribución visual de EmuSync, sin JVM ni WebView. No cambiar identidad, formatos, rutas, partidas, historial, backups, OAuth, Steam ni semántica de conflictos y cancelación: las instalaciones existentes deben seguir leyendo sus datos. No introducir un framework de aplicación o DI; usar bibliotecas Rust cuando hagan falta.

## Arquitectura

Workspace Cargo por capacidad: domain; application → domain; data-local, data-drive, data-updates y platform-desktop → domain; presentation → application + domain; desktop-app → todas las capas necesarias. Dominio no depende de UI, HTTP ni implementaciones de archivos/procesos. Adaptadores implementan contratos pequeños inyectados; DTO se convierten en sus límites. Composición, runtime y cierre de recursos pertenecen a desktop-app.

Interfaz con egui, winit y wgpu; Tokio para coordinación fuera de dominio, reqwest/Rustls para HTTP y Serde en adaptadores. Organizar por funcionalidad, con visibilidad mínima, errores tipados y estado observable de solo lectura. Mantener UI y operaciones bloqueantes separadas.

Cancelación de operaciones debe esperar limpieza y rollback asíncrono antes de liberar exclusión o cerrar recursos. No asumir que abortar un futuro o `Drop` realiza rollback remoto.

## Skills

- [rust-development](.agents/skills/rust-development/SKILL.md): diseño y desarrollo Rust de EmuSync.
- [rust-code-auditor](.agents/skills/rust-code-auditor/SKILL.md): auditoría profunda de código Rust (`src/`) en solo lectura (bugs, concurrencia, arquitectura y optimización) con entrega de prompt para otro agente (ejemplo: *"Usa la skill `rust-code-auditor` para auditar `rust/<crate>`"*).
- [rust-valuable-tests](.agents/skills/rust-valuable-tests/SKILL.md): pruebas por riesgos y contratos.
- [rust-adversarial-test-architect](.agents/skills/rust-adversarial-test-architect/SKILL.md): auditoría `src/` vs `tests/`, separación modular y diseño de pruebas adversariales en solo lectura con entrega de prompt para otro agente (ejemplo: *"Usa la skill `rust-adversarial-test-architect` para auditar `rust/<crate>`, los tests van en `rust/<crate>/tests/`"*).
- [rust-testing](.agents/skills/rust-testing/SKILL.md): técnicas de pruebas Rust; usar solo técnicas aplicables.
- [rust-concurrency](.agents/skills/rust-concurrency/SKILL.md): concurrencia, cancelación y ciclo de vida.
- [rust-style-clippy](.agents/skills/rust-style-clippy/SKILL.md): formato y análisis estático.

Lectura obligatoria antes de editar (completa, en cada sesión y en cada subagente):

- Código Rust de producción (`rust/`, `xtask/`): `rust-development` y `rust-style-clippy`; además `rust-concurrency` si el cambio toca tareas, locks, atómicos, canales o cancelación.
- Pruebas (`tests/`, `*_tests.rs`, `tests.rs` o módulos `#[cfg(test)]`): `rust-valuable-tests` y `rust-testing`.

`.agents/hooks/require_skills.py` bloquea la edición hasta que se hayan leído, con la misma regla para Claude Code (`.claude/settings.json`), Codex (`.codex/hooks.json`), Antigravity (`.agents/hooks.json`) y OpenCode (`.opencode/plugins/`); sus pruebas están en `scripts/tests/agent-skill-hooks-test.sh`. Un agente sin hooks sigue obligado por esta regla.

Las skills externas proceden de [full-stack-skills/rust-skills](https://github.com/full-stack-skills/rust-skills) revisión `b8a6af74af28ee86b8cbbbc3075cde0b31a5155e`, Apache-2.0; cada copia incluye su licencia. Las reglas del usuario y esta guía prevalecen sobre ejemplos, cuotas, flujos automáticos y recomendaciones de las skills. No instalar herramientas, imponer cobertura ni ejecutar todas las combinaciones de features sin necesidad.

## Calidad y pruebas

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` y `cargo test --workspace --locked`, agregados con el verificador de arquitectura mediante `cargo xtask quality` (sin pruebas) y `cargo xtask check`. Los scripts tienen pruebas en `scripts/tests/`.

Al ejecutar localmente pruebas, Clippy o `cargo xtask`, limitar la CPU: anteponer `nice -n 19` (y `ionice -c3` si el disco se satura) y usar `CARGO_BUILD_JOBS=2`; para pruebas, `-- --test-threads=2`. Ejemplo: `CARGO_BUILD_JOBS=2 nice -n 19 cargo test --workspace --locked -- --test-threads=2`. Esto aplica también a agentes y subagentes; no es requisito de CI.

Mantener CI y resultados por plataforma, distinguir ejecución nueva de caché o informes históricos. Ejecutar pruebas y análisis afectados; ejecutar el check completo al cerrar integraciones. No agregar pruebas por movimientos mecánicos o documentación. Las pruebas deben proteger contratos y riesgos, no reflejar implementación ni cumplir cuotas.

El hook valida una exportación aislada del índice con `cargo xtask quality` y no formatea, prepara archivos ni ejecuta pruebas. No usar stash ni reset para hacerlo pasar.

## Rendimiento

Medir antes y después de cada cambio que pueda afectar al rendimiento, en la misma máquina y sin carga pesada en paralelo. Las compilaciones siguen las limitaciones de CPU de arriba; las medidas se ejecutan sin `nice`. Comparar contra una referencia tomada con el mismo código de medición.

- Benchmarks Criterion (`rust/data-local/benches/hashing.rs`, `rust/presentation/benches/library_frame.rs`): `cargo bench -p <crate> --bench <nombre> -- --save-baseline <ref>` en la referencia y `-- --baseline <ref>` con el cambio; Criterion indica el cambio y si es significativo. Siempre con `--bench`, o la opción llega también a los tests y falla. No se ejecutan en CI (los runners compartidos dan demasiado ruido); Clippy con `--all-targets` los compila.
- App real (`cargo xtask perf`, solo Linux): `run --label <nombre> [--runs N] [--bin <binario>] [library|scroll|fling|spinner]` compila en release (salvo con `--bin`), ejecuta cada escenario sobre una biblioteca sintética de 200 juegos con portadas y guarda la mediana en `target/perf/<label>/`; `compare <a> <b>` imprime la tabla. La ventana se abre en el escritorio y debe quedar visible y sin tocar. Métricas útiles: `cpu_in_window_pct` (reposo o spinner), `slow_frames` y `ui_max_ms` (tirones), `covers_ready_ms`, `peak_rss_mib` y `texture_max_mib` (memoria de vídeo de las portadas, que `peak_rss_mib` no ve). Los percentiles con pocos frames (escenario `library`, que queda en reposo) no son comparables.
- Drive real (`cargo xtask perf drive --config <config> --entry <nombre> [--files N] [--runs N]`): copia la entrada y la conexión a una configuración aislada en `target/perf/drive`, mide subida inicial, sincronización sin cambios y diez archivos cambiados, y borra de Drive los archivos `perf/…` antes y después; la configuración copiada se elimina al terminar. Usar solo la entrada de pruebas «Test»; depende de la red, repetir y usar la mediana.
- La app solo registra métricas con `EMUSYNC_PERF_LOG=<archivo>` (una línea CSV por frame); `EMUSYNC_PERF_SCENARIO` (`spinner`, `scroll`, `fling`, `sync`) la conduce para que cada build mida el mismo trabajo.

## Empaquetado y publicación

- Linux: `scripts/build-appimage-container.sh` genera `EmuSync-x86_64.AppImage` íntegramente en Docker (Debian Bookworm, `Dockerfile` por etapas `build`, `tools` y `package`; `appimagetool` y runtime fijados por versión y SHA-256). El anfitrión solo necesita Docker y Git. `scripts/build-appimage.sh` empaqueta dentro del contenedor o con herramientas locales; usa `packaging/linux/AppRun`. Recursos en `packaging/assets/` y `packaging/linux/`.
- Windows: `scripts/build-windows-installers.sh` (WiX 4, `packaging/windows/`) genera `EmuSync-x86_64.msi` y `EmuSync-x86_64-portable.zip`; `scripts/verify-windows-installers.ps1` los instala, actualiza desde el MSI Kotlin 1.2.1, reinstala y desinstala en el runner. Forman parte de la release.
- `scripts/bump-version.sh` actualiza `workspace.package.version` y `Cargo.lock`. El workflow de release publica el AppImage, `EmuSync-x86_64.msi` y `EmuSync-x86_64-portable.zip`, y no publica nada si falla la construcción o la verificación de Windows (`windows-packages.yml`). Conservar `io.github.ThEeLxD192.EmuSync`, el UpgradeCode Windows `a7b3c291-5f4e-4b28-98e1-d63f5e824c90` y los nombres de artefactos que busca el actualizador.

## Pendientes Windows

Ninguna validación Linux cierra Windows. Pendiente de validación nativa (no compilado ni ejecutado en Windows nativo; Wine no cuenta):

- Rutas UNC, nombres cortos 8.3 (`PROGRA~1`: la descarga confinada no los resuelve) y bloqueos de archivos de terceros (antivirus, indexador) en persistencia local y restauración.
- Identidad de rutas y mayúsculas: `data-local::path_identity` (candados, registro de `ConfigManager`, `RecoveryStore::has_unresolved`, comparación de destino de descargas de carpeta y `relative_to`), `is_excluded`, `canonical_cloud_files` y el emparejamiento local/nube de `manual.rs` pliegan mayúsculas solo cuando `cfg!(windows)`. Las pruebas `#[cfg(windows)]` de `recovery_store.rs` y `restore_paths.rs` pasan en CI, pero el emparejamiento de `manual.rs` y los candados no tienen prueba en Windows. Pendiente: la clave de `SyncHistoryStore::key` sigue dependiendo de la grafía de la ruta (cambiarla rompería el formato existente; hace falta migrar al leer) y `reserved_sibling_names` se compara con grafía exacta.
- Un fallo retenido que no es una subida incierta (p. ej. una descarga fallida) deja al supervisor con un fallo que impide cerrar la app («A sync operation needs recovery before closing»); `Coordinator::reconcile_active` no tiene llamador en la interfaz y solo entiende subidas, así que hay que matar el proceso. No es específico de Windows.
- Binario Windows: es de subsistema consola (abre una ventana negra), sin icono ni versión incrustados ni CRT estático.
- Procesos: `tasklist` con `CREATE_NO_WINDOW` y sondeos en `spawn_blocking`; PID exacto leyendo el segundo campo CSV de `tasklist /FO CSV /NH`; seguimiento de procesos de juego.
- Selector `rfd` sin ventana padre (que no quede detrás de la app).
- Lanzadores Steam con PowerShell y manifiesto JSON.
- Instaladores WiX: la reapertura tras actualizar (`AUTOLAUNCH=1`) y el asistente con una versión más nueva que la instalada solo se prueban con dos versiones reales; falta firma de código (SmartScreen avisará). Nunca se ha publicado una release con el MSI nuevo.

Registrar aquí fecha, comandos y resultado de cada validación Windows y retirar el punto cuando quede cerrado.

Validaciones Windows registradas:

- 2026-11-12, CI `windows-latest` (run 43309575999, commit `00e0662`): `scripts/verify-windows-installers.ps1` instala el MSI Kotlin 1.2.1, actualiza al MSI nuevo (un solo producto registrado, sin restos de la versión anterior), reinstala y desinstala conservando `%APPDATA%\emusync`; el ZIP portable trae `EmuSync.exe`, `portable.txt` y `LICENSE`; el ejecutable es GUI, con versión y CRT estático. En un PC Windows 11 nativo se instalaron, abrieron y desinstalaron el MSI y el ZIP portable sin problemas; la ventana carecía de icono propio (corregido en `90444d3`, sin reverificar).
- 2026-11-12, CI `windows-latest` (run 43304104505, commit `b3f11d5`): `cargo xtask check` y `cargo build --release --locked -p emusync-desktop-app` pasan, incluidas las pruebas `#[cfg(windows)]` de `pinned_path_windows.rs`, `recovery_store.rs`, `restore_paths.rs` y las de reemplazo del temporal de `staging.rs`. Es la primera ejecución de `xtask check` en Windows remoto.
- 2026-11-12, Windows 11 nativo (PC de prueba), `.exe` del job `windows-exe` del mismo run, con `--config` aislado y una raíz Drive sintética: se restauran desde la nube un archivo suelto borrado en local y subcarpetas de una partida de tipo carpeta; el reemplazo conserva la copia anterior en `.sync-backups`; la app cierra sin pedir recovery. Cierra la descarga de carpetas y el reemplazo del temporal en Windows. Un archivo creado a mano en Drive con la web no se lista (alcance `drive.file`, comportamiento esperado).

## Trabajo

Usar datos y Drive sintéticos aislados en desarrollo (`--config` y una cuenta o raíz Drive de prueba). El actualizador de desarrollo usa fuentes controladas. No versionar credenciales, configuración personal, partidas, historial ni backups.

Solicitar confirmación explícita antes de cada commit, mostrando cambios y mensaje propuesto en inglés. Una aprobación anterior no autoriza el siguiente commit. No publicar releases ni hacer push sin autorización explícita.
