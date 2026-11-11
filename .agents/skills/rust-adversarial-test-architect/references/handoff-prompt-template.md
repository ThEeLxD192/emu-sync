# Plantilla Maestra de Auditoría y Prompt de Handoff (`rust-adversarial-test-architect`)

Usa esta estructura al responder al usuario: primero presenta el diagnóstico (Secciones A y B) y después entrega el bloque copiable para el agente implementador (Sección C).

---

## A. Plantilla de Tabla de Paridad `src/` vs `tests/`

Presenta siempre un inventario completo que permita ver de un vistazo qué falta y qué archivo de test está sobrecargado:

| Archivo en `rust/<crate>/src/` | Archivo en `rust/<crate>/tests/` | Estado y Brechas Adversariales Detectadas |
| :--- | :--- | :--- |
| `modulo_a.rs` | `modulo_a_tests.rs` | ✅ Cubierto / ⚠️ Falta caso X / ❌ Sin pruebas de rollback |
| `submodulo_b.rs` | ❌ *(Sin archivo propio; mezclado en `monolito_tests.rs`)* | ❌ Separar en `tests/submodulo_b_tests.rs` y añadir ataques Y, Z |

---

## B. Criterios para Separar Archivos de Test Monolíticos

Cuando un archivo en `tests/` acumule pruebas de varios submódulos de `src/` o supere ~500–600 líneas:
1. **Un archivo de integración por submódulo lógico**:
   - Ejemplo en `emusync-application`: en lugar de concentrar todo en `orchestrator_tests.rs`, separar en:
     - `lifecycle_tests.rs` (para `src/sync/lifecycle.rs`)
     - `post_game_tests.rs` (para `src/sync/post_game.rs`)
     - `status_tests.rs` (para `src/sync/status.rs`)
     - `target_resolver_tests.rs` (para `src/sync/target_resolver.rs`, separado de `target_mapping_tests.rs`)
     - `orchestrator_tests.rs` (solo delegación e integración de `SyncOrchestrator` y `SyncOrchestratorFactory`)
2. **Mocks compartidos en `tests/common/mod.rs`**:
   - En Cargo, cualquier archivo directo `tests/*.rs` se compila como un crate de test independiente, mientras que los submódulos en subdirectorios como `tests/common/mod.rs` pueden importarse con `mod common;` desde múltiples suites sin duplicar cientos de líneas de fakes (`MockFs`, `MockDrive`, `MockTrackedSync`, etc.) y sin generar un binario vacío en el test runner.

---

## C. Plantilla del Bloque Copiable (Prompt de Handoff para el Agente Implementador)

Todo el bloque siguiente debe entregarse dentro de un bloque de código de cuatro acentos graves (```` ````markdown ... ```` ````) para que el usuario pueda copiarlo de una sola vez:

````markdown
# Prompt de Implementación: Separación Modular y Suite de Pruebas Adversariales en `rust/<crate>/tests/`

Antes de escribir código, lee y aplica las reglas de `.agents/skills/rust-valuable-tests/SKILL.md` (y su `references/adversarial-testing-playbook.md`), `.agents/skills/rust-testing/SKILL.md` y `.agents/skills/rust-style-clippy/SKILL.md`.

## 1. Reglas Inquebrantables
- **Restricción estricta de carpeta**: TODOS los archivos nuevos o modificados deben estar única y exclusivamente dentro de `rust/<crate>/tests/`. Está terminantemente prohibido añadir `#[cfg(test)]` en `rust/<crate>/src/` o crear/modificar archivos fuera de `rust/<crate>/tests/`. No modifiques código de producción en `rust/<crate>/src/` (salvo que descubras un bug real, en cuyo caso debes reportarlo).
- **Separación modular de suites**:
  - [Listar aquí qué archivos monolíticos de `tests/` deben dividirse, qué archivos nuevos `tests/<nombre>_tests.rs` deben crearse, qué pruebas existentes se mueven a cada uno, y el uso opcional de `tests/common/mod.rs` para compartir fakes].
- **Aislamiento de capa**:
  - [Especificar qué dependencias e infraestructura están permitidas en este crate según `AGENTS.md`: ej. solo fakes en memoria de `emusync-domain` para `application`, `tempfile::tempdir()` para `data-local`, `TcpListener` loopback `127.0.0.1:0` para `data-drive`, etc.].
- **Sincronización determinista (Cero `sleep`)**:
  - Prohibido usar `tokio::time::sleep` o `std::thread::sleep` para esperar ventanas de concurrencia. Usa primitivas deterministas (`tokio::sync::oneshot`, `tokio::sync::Notify`, `std::sync::Barrier`, `AtomicBool` o `Future::poll` manual con `Waker`). Elimina también los `sleep` existentes en `[listar archivos con sleep]`.
- **Verificación completa del post-estado**:
  - Cada test debe comprobar: (1) el valor de retorno exacto, (2) los eventos/estados emitidos, y (3) la ausencia de efectos colaterales prohibidos (stores intactos tras error, procesos no lanzados tras fallo de pre-sync, archivos ya completados no re-subidos en retry, temporales eliminados tras fallo o cancelación).

---

## 2. Nuevos Archivos de Test a Crear en `rust/<crate>/tests/`

### A. Nuevo archivo: `rust/<crate>/tests/<nuevo_archivo>_tests.rs` (para `src/<ruta>.rs`)
[Indicar qué tests existentes se trasladan aquí desde el archivo monolítico y detallar cada nuevo test adversarial:]

1. **`<nombre_descriptivo_del_test_en_snake_case>`**
   - **Contrato e invariante**: [Explicar qué regla o promesa del código en `src/<ruta>.rs` protege este test y qué daño ocurriría en producción si se rompe].
   - **Precondición y preparación de fakes**: [Detallar el estado inicial de configuración, archivos, carpetas, hashes, timestamps y mocks].
   - **Inyección adversarial (cómo probarlo)**: [Detallar paso a paso el estímulo hostil: ej. fallo en el archivo N + fallo intermedio en rollback, duplicados remotos sin SHA-256, corte a mitad de operación, cancelación sincronizada con `oneshot`].
   - **Post-estado esperado (aserciones exactas)**:
     1. [Valor de retorno exacto].
     2. [Eventos emitidos o estado final observable].
     3. [Efectos colaterales verificados: logs de llamadas exactos, ausencia de mutaciones en stores, archivos conservados o limpiados].

---

## 3. Pruebas Adversariales a Añadir en Archivos Existentes de `rust/<crate>/tests/`

### B. En `rust/<crate>/tests/<archivo_existente>_tests.rs` (para `src/<ruta>.rs`)
1. **`<nombre_descriptivo_del_test_en_snake_case>`**
   - **Contrato e invariante**: [...]
   - **Precondición y preparación de fakes**: [...]
   - **Inyección adversarial (cómo probarlo)**: [...]
   - **Post-estado esperado (aserciones exactas)**:
     1. [...]
     2. [...]
     3. [...]

---

## 4. Verificación Requerida al Finalizar
1. Ejecutar `cargo test --locked -p <nombre-del-crate>` y comprobar que todas las suites en `rust/<crate>/tests/` compilan y pasan en verde.
2. Verificar mediante mutación manual rápida (sin dejar cambios en `src/`) que si se comenta o invierte temporalmente la guardia correspondiente en `src/`, el test nuevo falla exactamente por la aserción esperada; luego restaurar `src/` intacto.
3. Confirmar con `git status` / `git diff` que solo existen cambios dentro de `rust/<crate>/tests/`.
4. Ejecutar `cargo fmt --all -- --check` y `cargo clippy --workspace --all-targets --locked -- -D warnings`.
````
