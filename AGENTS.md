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

Las skills externas proceden de [full-stack-skills/rust-skills](https://github.com/full-stack-skills/rust-skills) revisión `b8a6af74af28ee86b8cbbbc3075cde0b31a5155e`, Apache-2.0; cada copia incluye su licencia. Las reglas del usuario y esta guía prevalecen sobre ejemplos, cuotas, flujos automáticos y recomendaciones de las skills. No instalar herramientas, imponer cobertura ni ejecutar todas las combinaciones de features sin necesidad.

## Calidad y pruebas

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` y `cargo test --workspace --locked`, agregados con el verificador de arquitectura mediante `cargo xtask quality` (sin pruebas) y `cargo xtask check`. Los scripts tienen pruebas en `scripts/tests/`.

Al ejecutar localmente pruebas, Clippy o `cargo xtask`, limitar la CPU: anteponer `nice -n 19` (y `ionice -c3` si el disco se satura) y usar `CARGO_BUILD_JOBS=2`; para pruebas, `-- --test-threads=2`. Ejemplo: `CARGO_BUILD_JOBS=2 nice -n 19 cargo test --workspace --locked -- --test-threads=2`. Esto aplica también a agentes y subagentes; no es requisito de CI.

Mantener CI y resultados por plataforma, distinguir ejecución nueva de caché o informes históricos. Ejecutar pruebas y análisis afectados; ejecutar el check completo al cerrar integraciones. No agregar pruebas por movimientos mecánicos o documentación. Las pruebas deben proteger contratos y riesgos, no reflejar implementación ni cumplir cuotas.

El hook valida una exportación aislada del índice con `cargo xtask quality` y no formatea, prepara archivos ni ejecuta pruebas. No usar stash ni reset para hacerlo pasar.

## Empaquetado y publicación

- Linux: `scripts/build-appimage-container.sh` genera `EmuSync-x86_64.AppImage` íntegramente en Docker (Debian Bookworm, `Dockerfile` por etapas `build`, `tools` y `package`; `appimagetool` y runtime fijados por versión y SHA-256). El anfitrión solo necesita Docker y Git. `scripts/build-appimage.sh` empaqueta dentro del contenedor o con herramientas locales; usa `packaging/linux/AppRun`. Recursos en `packaging/assets/` y `packaging/linux/`.
- Windows: `scripts/build-windows-installers.sh` (WiX 4, `packaging/windows/`) es un borrador sin validar; no forma parte de la release.
- `scripts/bump-version.sh` actualiza `workspace.package.version` y `Cargo.lock`. El workflow de release publica solo el AppImage. Conservar `io.github.ThEeLxD192.EmuSync`, el UpgradeCode Windows `a7b3c291-5f4e-4b28-98e1-d63f5e824c90` y los nombres de artefactos que busca el actualizador.

## Pendientes Windows

Ninguna validación Linux cierra Windows. Pendiente de validación nativa (no compilado ni ejecutado en Windows nativo; Wine no cuenta):

- CI Windows de `cargo xtask check` y pruebas de scripts; nunca se ha ejecutado en remoto.
- Rutas UNC, nombres cortos 8.3, dispositivos reservados (`CON`, `PRN`, `NUL`) y bloqueos de archivos en persistencia local y restauración.
- Procesos: `tasklist` con `CREATE_NO_WINDOW` y sondeos en `spawn_blocking`; PID exacto leyendo el segundo campo CSV de `tasklist /FO CSV /NH`; seguimiento de procesos de juego.
- Selector `rfd` sin ventana padre (que no quede detrás de la app).
- Lanzadores Steam con PowerShell y manifiesto JSON.
- Instaladores WiX: compilación, licencia RTF, instalación limpia, actualización desde el MSI Kotlin 1.2.0 (mismo UpgradeCode), transición MSI/EXE, desinstalación y conservación de datos en `%APPDATA%\emusync`. Al validarlo, añadir el job Windows a `release.yml`; hasta entonces los usuarios Windows no reciben actualizaciones.

Registrar aquí fecha, comandos y resultado de cada validación Windows y retirar el punto cuando quede cerrado.

## Trabajo

Usar datos y Drive sintéticos aislados en desarrollo (`--config` y una cuenta o raíz Drive de prueba). El actualizador de desarrollo usa fuentes controladas. No versionar credenciales, configuración personal, partidas, historial ni backups.

Solicitar confirmación explícita antes de cada commit, mostrando cambios y mensaje propuesto en inglés. Una aprobación anterior no autoriza el siguiente commit. No publicar releases ni hacer push sin autorización explícita.
