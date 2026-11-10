# Guía de trabajo de EmuSync

## Estado y alcance

EmuSync es actualmente Kotlin/Compose Desktop para Linux y Windows. La rama `rewrite/rust-native` prepara una migración manual a Rust nativo por hitos; la versión Kotlin sigue operativa y sirve de referencia hasta verificar equivalencia. R1 introduce el workspace Cargo y una ventana Rust de prueba; consultar el seguimiento para su estado y validaciones pendientes. La base y el historial, incluidos los pendientes de validación nativa Windows, están en [RUST_MIGRATION.md](RUST_MIGRATION.md). Documentar un hito no lo ejecuta.

Mantener los flujos y distribución visual de EmuSync, sin JVM ni WebView en el producto Rust. No cambiar identidad, formatos, rutas, partidas, historial, backups, OAuth, Steam ni semántica de conflictos y cancelación. No introducir un framework de aplicación o DI; usar bibliotecas Rust cuando hagan falta.

## Arquitectura objetivo

Workspace Cargo por capacidad: domain; application → domain; data-local, data-drive, data-updates y platform-desktop → domain; presentation → application + domain; desktop-app → todas las capas necesarias. Dominio no depende de UI, HTTP ni implementaciones de archivos/procesos. Adaptadores implementan contratos pequeños inyectados; DTO se convierten en sus límites. Composición, runtime y cierre de recursos pertenecen a desktop-app.

Interfaz con egui, winit y wgpu; Tokio para coordinación fuera de dominio, reqwest/Rustls para HTTP y Serde en adaptadores. Organizar por funcionalidad, con visibilidad mínima, errores tipados y estado observable de solo lectura. Mantener UI y operaciones bloqueantes separadas.

Cancelación de operaciones debe esperar limpieza y rollback asíncrono antes de liberar exclusión o cerrar recursos. No asumir que abortar un futuro o `Drop` realiza rollback remoto.

## Skills

- [rust-development](.agents/skills/rust-development/SKILL.md): diseño y desarrollo Rust de EmuSync.
- [rust-code-auditor](.agents/skills/rust-code-auditor/SKILL.md): auditoría profunda de código Rust (`src/`) en solo lectura (bugs, concurrencia, arquitectura y optimización) con entrega de prompt para otro agente (ejemplo: *"Usa la skill `rust-code-auditor` para auditar `rust/<crate>`"*).
- [rust-valuable-tests](.agents/skills/rust-valuable-tests/SKILL.md): pruebas y trazabilidad de migración por riesgos y contratos.
- [rust-adversarial-test-architect](.agents/skills/rust-adversarial-test-architect/SKILL.md): auditoría `src/` vs `tests/`, separación modular y diseño de pruebas adversariales en solo lectura con entrega de prompt para otro agente (ejemplo: *"Usa la skill `rust-adversarial-test-architect` para auditar `rust/<crate>`, los tests van en `rust/<crate>/tests/`"*).
- [rust-testing](.agents/skills/rust-testing/SKILL.md): técnicas de pruebas Rust; usar solo técnicas aplicables.
- [rust-concurrency](.agents/skills/rust-concurrency/SKILL.md): concurrencia, cancelación y ciclo de vida.
- [rust-style-clippy](.agents/skills/rust-style-clippy/SKILL.md): formato y análisis estático.
- [kotlin-valuable-tests](.agents/skills/kotlin-valuable-tests/SKILL.md): usar mientras exista código o pruebas Kotlin; conserva también la matriz y el catálogo históricos.

Las skills externas proceden de [full-stack-skills/rust-skills](https://github.com/full-stack-skills/rust-skills) revisión `b8a6af74af28ee86b8cbbbc3075cde0b31a5155e`, Apache-2.0; cada copia incluye su licencia. Las reglas del usuario y esta guía prevalecen sobre ejemplos, cuotas, flujos automáticos y recomendaciones de las skills. No instalar herramientas, imponer cobertura ni ejecutar todas las combinaciones de features sin necesidad.

## Calidad y pruebas

Durante la coexistencia se conservan `./gradlew qualityCheck --no-daemon` y `./gradlew check --no-daemon`. Para Rust: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` y `cargo test --workspace --locked`, agregados con el verificador de arquitectura mediante `cargo xtask quality` y `cargo xtask check`.

Al ejecutar localmente pruebas, Clippy o `cargo xtask` en Rust, limitar la CPU: anteponer `nice -n 19` (y `ionice -c3` si el disco se satura) y usar `CARGO_BUILD_JOBS=2`; para pruebas, `-- --test-threads=2`. Ejemplo: `CARGO_BUILD_JOBS=2 nice -n 19 cargo test --workspace --locked -- --test-threads=2`. Esto aplica también a agentes y subagentes; no es requisito de CI.

Mantener CI y resultados por módulo/plataforma, distinguir ejecución nueva de caché o informes históricos. Ejecutar pruebas y análisis afectados; ejecutar el check completo al cerrar integraciones. No agregar pruebas por movimientos mecánicos o documentación. Las pruebas deben proteger contratos y riesgos, no reflejar implementación ni cumplir cuotas.

El hook valida una exportación aislada del índice y no formatea, prepara archivos ni ejecuta pruebas. No usar stash ni reset para hacerlo pasar. Durante la coexistencia disparar calidad Kotlin y Rust según archivos preparados. Mantener Gradle, scripts y distribución Kotlin funcionales hasta R8.

## Compatibilidad y trabajo

Seguir [RUST_MIGRATION.md](RUST_MIGRATION.md) en orden; registrar evidencia, fechas, comandos, omisiones y resultados Linux/Windows. Una validación Linux no cierra Windows. Mantener el catálogo histórico y trazar cada garantía Kotlin hacia pruebas Rust con justificación para equivalencias o consolidaciones.

Kotlin y Rust no comparten locks entre procesos: usar datos y Drive sintéticos aislados en desarrollo y no ejecutar ambos sobre los mismos datos al mismo tiempo. El actualizador de desarrollo usa fuentes controladas. No versionar credenciales, configuración personal, partidas, historial ni backups.

Solicitar confirmación explícita antes de cada commit, mostrando cambios y mensaje propuesto en inglés. Una aprobación anterior no autoriza el siguiente commit. No publicar releases ni hacer push sin autorización explícita.
