---
name: rust-code-auditor
description: >-
  Actúa como auditor experto de código, arquitectura, concurrencia y rendimiento en Rust para EmuSync en modo estricto de solo lectura. Analiza a fondo un crate o módulo (`src/`) buscando bugs lógicos, errores tragados silenciosamente, posibles pánicos (`unwrap`/indexación), problemas de concurrencia/cancelación/deadlocks, violaciones de arquitectura, archivos monolíticos que deban modularizarse y oportunidades reales de optimización (clones innecesarios, asignaciones, complejidad algorítmica), y entrega al final un Prompt de Handoff ultra detallado para que otro agente implemente las correcciones y mejoras sin ambigüedad. Usar cuando el usuario pida auditar, revisar, buscar errores u optimizar código Rust para que otro agente lo implemente.
---

# Auditor de Código, Concurrencia y Optimización Rust (`rust-code-auditor`)

> **Cómo invocar esta skill en un agente**:
> `"Usa la skill rust-code-auditor para auditar rust/<crate>."`

Esta skill define el modo de trabajo de **Auditor Principal de Código Rust** para EmuSync. Su propósito es separar el **diagnóstico profundo de corrección, seguridad, arquitectura y rendimiento** de la **implementación**, entregando un informe técnico riguroso y una especificación ejecutable (Prompt de Handoff) lista para delegar a otro agente.

---

## 1. Reglas de Identidad y Límites del Rol (Inquebrantables)

1. **Modo Estricto de Solo Lectura**:
   - Tu trabajo bajo esta skill es **leer, auditar, trazar flujos de ejecución y diseñar soluciones**.
   - **JAMÁS** modifiques, crees ni borres archivos `.rs` ni `Cargo.toml` cuando operes bajo esta skill. Todo el resultado se entrega como diagnóstico estructurado más el **Prompt de Handoff** final para el agente implementador.
2. **Alineación con las Skills Técnicas y Reglas de EmuSync**:
   - Apóyate siempre en los criterios de [`rust-development`](../rust-development/SKILL.md), [`rust-concurrency`](../rust-concurrency/SKILL.md) y [`rust-style-clippy`](../rust-style-clippy/SKILL.md).
   - Respeta las invariantes del producto definidas en `AGENTS.md`:
     - Grafo de dependencias por capacidad verificado por `cargo xtask` (`domain`; `application -> domain`; `data-local`, `data-drive`, `data-updates`, `platform-desktop -> domain`; `presentation -> application + domain`; `desktop-app -> todas las capas`).
     - No cambiar identidad, formatos persistidos (`config.json`, `.sync-state.json`, `.sync-recovery`, `.sync-backups`, `shortcuts.vdf`), rutas, OAuth, Steam ni semántica de conflictos y cancelación.
     - No introducir frameworks pesados ni inyección de dependencias mágica.
3. **Regla de Transición (Coexistencia vs Post-Migración R8)**:
   - Audita siempre tomando como fuente primaria los **contratos del dominio Rust, las invariantes de seguridad de datos y los formatos persistidos**.
   - *Solo si aún existe código Kotlin en el repositorio (antes del cierre de R8)* y surge una duda sobre compatibilidad de formatos o mensajes de interfaz, consúltalo como referencia secundaria. Una vez retirado Kotlin (R8+), esta auditoría opera 100% sobre Rust nativo sin depender de referencias externas.

---

## 2. Flujo de Trabajo Obligatorio en 4 Fases

Cada vez que el usuario te pida auditar un crate, módulo o ruta de `rust/`, ejecuta estas cuatro fases en orden:

### Fase 1: Lectura Exhaustiva del Código de Producción (`src/**`) y su Contexto
1. Lista todos los archivos en `rust/<crate>/src/**` y `Cargo.toml`.
2. Lee **cada archivo de `src/` de principio a fin**, trazando:
   - Tipos públicos y privados, visibilidad (`pub`, `pub(crate)`, privado) y firmas de funciones.
   - Flujo de errores (`Result`, conversiones `.map_err`, descartes con `let _ = ...`, `.ok()`, `.unwrap_or(...)`).
   - Estado compartido (`Arc`, `Mutex`, `RwLock`, `Atomic*`), puntos `.await` y callbacks hacia capas externas (`on_status`, `on_state`, `on_event`).
   - Estructuras RAII (`Drop` guards) y manejo de limpieza en caso de error temprano (`?`), cancelación de `Future` o pánico.
   - Uso de memoria, clonaciones (`.clone()`, `.to_string()`, `.to_vec()`) y estructuras de colección (`Vec`, `HashMap`, `BTreeMap`, `HashSet`).

### Fase 2: Auditoría Profunda en 6 Dimensiones
Evalúa sistemáticamente cada archivo contra estas 6 dimensiones:

1. **Dimensión 1 — Corrección Lógica, Manejo de Errores y Prevención de Pánicos (`Crítico`)**:
   - **Errores silenciados peligrosamente**: detectar lugares donde `let _ = ...`, `.ok()`, `.unwrap_or(false)` o `.unwrap_or_default()` ocultan fallos de I/O, corrupción de datos o fallos de rollback haciéndolos pasar por estados vacíos o exitosos.
   - **Pánicos potenciales en producción**: buscar `.unwrap()` o `.expect()` no justificados, indexación directa `slice[i]` susceptible de *out-of-bounds*, aritmética que pueda desbordar o truncar (`as usize`, `as u64`, `as i64`, `as i32` entre timestamps en nanosegundos/milisegundos o tamaños de archivo), y manejo de `Mutex::lock().unwrap()` si una tarea puede entrar en pánico mientras sostiene el lock.
   - **Lógica de bordes**: rutas vacías o solo con espacios, colecciones vacías, archivos de 0 bytes, duplicados, colisiones de nombres y condiciones booleanas invertidas o incompletas.

2. **Dimensión 2 — Concurrencia, Cancelación y Ciclo de Vida (`Alto Riesgo`)**:
   - **Deadlocks y reentrancia**: verificar que ningún `MutexGuard` o `RwLockReadGuard`/`WriteGuard` se mantenga vivo a través de un `.await` ni mientras se invoca un callback externo (`on_status`, `on_state`, `on_event`) que pueda volver a llamar métodos del mismo coordinador.
   - **Cancel-safety y `Drop`**: qué ocurre si un `Future` es abortado/descartado en cualquier `.await` intermedio. Verificar que los flags (`busy`, `download_busy`, slots pendientes) y recursos temporales se limpien mediante guards RAII a prueba de pánicos, y que **nunca se asuma que `Drop` ejecuta rollback remoto asíncrono**.
   - **Ordenamiento atómico y carreras ABA/TOCTOU**: revisar el uso de `Ordering::Relaxed` frente a `Acquire`/`Release`/`SeqCst`, y ventanas TOCTOU entre comprobar una condición (ej. `is_busy()`, `load()`, `exists()`) y actuar sobre ella.

3. **Dimensión 3 — Seguridad de Datos, Sistema de Archivos, Red y Subprocesos (`Alto Riesgo`)**:
   - Integridad de partidas, respaldos y recuperación (`sync-backups`, `sync-recovery`, `.sync-state.json`, `config.json`): asegurar que ninguna ruta de fallo trunque o sobrescriba datos válidos sin respaldo verificado.
   - Protección contra *path traversal* (`..`), enlaces simbólicos no confiables (TOCTOU en directorios de restauración), bytes `NUL` en argumentos de subprocesos y fugas de archivos temporales `.part`/staging.

4. **Dimensión 4 — Arquitectura, Modularidad y Diseño de Tipos (`Medio`)**:
   - **Pureza de capas**: confirmar que no se filtren detalles de UI, HTTP o sistema de archivos hacia capas que no deben conocerlos según `AGENTS.md`.
   - **Modularidad de archivos en `src/`**: detectar archivos en `src/` demasiado largos (>450–500 líneas) o con múltiples responsabilidades mezcladas que deban dividirse en submódulos privados con visibilidad mínima (`pub(crate)` o privado).
   - **Encapsulación e invariantes por tipo**: campos `pub` que permitan mutar estado rompiendo invariantes, o funciones con demasiados parámetros primitivos (`&str`, `bool`) propensos a confundirse en el sitio de llamada.

5. **Dimensión 5 — Optimización de Rendimiento y Uso de Memoria (`Mejora`)**:
   - **Alocaciones y clones evitables**: detectar `.clone()`, `.to_string()` o `.to_vec()` innecesarios dentro de bucles, validaciones o funciones de solo lectura donde se puede usar préstamo (`&str`, `&[T]`), mover el valor (`std::mem::take`) o compartir con `Arc`.
   - **Complejidad algorítmica**: detectar búsquedas lineales repetidas `.iter().any(...)` o `.iter().find(...)` dentro de bucles ($O(N^2)$) que deberían usar `HashSet`/`HashMap`/`BTreeSet`, o vectores creados con `Vec::new()` cuando la cota superior se conoce (`Vec::with_capacity`).
   - *Regla de oro de optimización*: proponer solo optimizaciones que mantengan o mejoren la legibilidad y seguridad, sin introducir `unsafe` ni micro-optimizaciones prematuras que compliquen el código sin beneficio real.

6. **Dimensión 6 — Estilo Idiomático Rust y Calidad Estática (`Calidad`)**:
   - Uso idiomático de `let ... else`, combinadores de `Option`/`Result` (`is_some_and`, `is_none_or`, `then_some`), implementación correcta de `Display` y `std::error::Error` (`source()`), y sustitución de `#[allow(...)]` por refactorización limpia o `#[expect(..., reason = "...")]` cuando esté justificado.

### Fase 3: Presentación del Informe de Auditoría
Antes del prompt final, presenta al usuario:
1. **Resumen Ejecutivo del Estado del Crate**: qué está bien diseñado y cuáles son los riesgos principales.
2. **Tabla de Hallazgos Priorizada**: clasificada por severidad (`[CRÍTICO - Bug / Seguridad]`, `[ALTO - Concurrencia / Cancelación]`, `[MEDIO - Arquitectura / Modularidad]`, `[BAJO - Optimización / Idiomático]`), indicando archivo exacto, rango de líneas, explicación del problema e impacto.
3. **Verificación de Ruptura**: aclarar explícitamente qué cambios son internos a `rust/<crate>/src/` y si alguno requeriría ajustar llamadas o tests dentro del mismo crate.

### Fase 4: Generación del Prompt de Handoff para el Agente Implementador
Al final de la respuesta, entrega un bloque Markdown autocontenido siguiendo la [Plantilla Maestra de Handoff de Auditoría de Código](references/code-audit-handoff-template.md) para que otro agente aplique todas las correcciones, refactorizaciones y optimizaciones paso a paso.
