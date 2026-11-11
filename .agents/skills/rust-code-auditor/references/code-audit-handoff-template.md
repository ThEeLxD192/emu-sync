# Plantilla Maestra de Informe y Prompt de Handoff (`rust-code-auditor`)

Usa esta estructura al responder al usuario: primero entrega el diagnóstico técnico (Secciones A y B) y al final el bloque copiable para el agente implementador (Sección C).

---

## A. Plantilla de Tabla de Hallazgos de Auditoría

Presenta todos los hallazgos ordenados de mayor a menor criticidad, con enlaces clicables a los archivos y líneas exactas:

| ID | Severidad | Categoría | Archivo y Líneas | Problema Detectado | Impacto en Producción / Rendimiento |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **F-01** | 🔴 Crítico | Bug / Datos / Pánico | `src/modulo.rs#L40-L55` | Descripción concisa del bug, error silenciado o pánico posible. | Pérdida de estado, corrupción, crash o comportamiento incorrecto. |
| **F-02** | 🟠 Alto | Concurrencia / Cancelación | `src/coord.rs#L80-L110` | Lock retenido durante callback/await, carrera TOCTOU o limpieza incompleta en `Drop`. | Deadlock reentrante, estado inconsistente tras cancelar o condición de carrera. |
| **F-03** | 🟡 Medio | Arquitectura / Modularidad | `src/grande.rs#L1-L600` | Archivo monolítico con múltiples responsabilidades o visibilidad `pub` innecesaria. | Acoplamiento alto y dificultad de mantenimiento. |
| **F-04** | 🔵 Bajo | Optimización / Idiomático | `src/mapa.rs#L120-L160` | Búsqueda $O(N^2)$ en bucle o `.clone()` innecesarios de estructuras grandes. | Asignaciones de memoria redundantes y costo de CPU evitable. |

---

## B. Criterios para Filtrar Hallazgos (Cero Ruido)

1. **No inventar problemas artificiales**: Si un archivo ya es correcto, seguro, claro y eficiente, dilo explícitamente en el informe y no propongas cambios cosméticos sobre él.
2. **Preservar contratos externos**: Ninguna optimización o refactorización propuesta puede cambiar formatos de archivos (`config.json`, `.sync-state.json`, `.sync-recovery`, `shortcuts.vdf`), rutas en la nube, semántica de errores observables por el usuario ni romper el grafo de dependencias de `cargo xtask`.
3. **Soluciones concretas, no quejas genéricas**: Cada hallazgo debe ir acompañado de cómo debe escribirse exactamente la solución en Rust idiomático.

---

## C. Plantilla del Bloque Copiable (Prompt de Handoff para el Agente Implementador)

Entrega siempre el siguiente bloque dentro de cuatro acentos graves (```` ````markdown ... ```` ````) para que el usuario pueda copiarlo de una sola vez y pasárselo al agente que modificará el código:

````markdown
# Prompt de Implementación: Correcciones, Refactorización y Optimización en `rust/<crate>/`

Antes de modificar código, lee y aplica las reglas de `AGENTS.md`, `.agents/skills/rust-development/SKILL.md`, `.agents/skills/rust-concurrency/SKILL.md` y `.agents/skills/rust-style-clippy/SKILL.md`.

## 1. Reglas Inquebrantables de Alcance
- **Límite de archivos permitidos**: Solo debes modificar los archivos dentro de `rust/<crate>/` detallados en esta especificación.
- **Preservación de contratos y compatibilidad**:
  - Mantén intactos los formatos persistidos, mensajes observables por el usuario, semántica de conflictos/cancelación y firmas públicas requeridas por los demás crates del workspace.
  - Respeta estrictamente las dependencias permitidas para `rust/<crate>` verificadas por `cargo xtask`.
- **Calidad estática y pruebas**:
  - No introduzcas `unwrap()`, `expect()` sin invariante demostrada, `#[allow(...)]` injustificados ni bloqueos a través de `.await`.
  - Si una corrección de bug requiere actualizar o añadir una prueba de regresión en `rust/<crate>/tests/`, hazlo únicamente dentro de `rust/<crate>/tests/`.

---

## 2. Cambios a Implementar Paso a Paso por Archivo

### A. Archivo: `rust/<crate>/src/<ruta_archivo>.rs`

#### Cambio A.1: [Título claro del bug, mejora de concurrencia u optimización] (Hallazgo F-XX)
- **Ubicación**: Función/estructura `<nombre>` (alrededor de las líneas `Lxx-Lyy`).
- **Problema actual**:
  [Explicar con precisión qué hace hoy el código, por qué es incorrecto, inseguro o ineficiente, y en qué escenario falla o desperdicia recursos].
- **Solución requerida**:
  [Explicar paso a paso qué cambio exacto realizar en el código, incluyendo firmas, manejo de errores, orden de adquisición/liberación de locks, uso de referencias `&str`/`&[T]`, `HashSet`, `with_capacity`, o guards RAII].
- **Invariante que debe cumplirse tras el cambio**:
  [Qué queda garantizado después de aplicar la modificación].

---

### B. Separación Modular (Solo si aplica a archivos monolíticos de `src/`)
- **Archivo origen**: `rust/<crate>/src/<monolito>.rs`
- **Estructura objetivo**:
  - Extraer `<responsabilidad_1>` a `rust/<crate>/src/<submodulo_1>.rs` con visibilidad `pub(crate)` (o privada).
  - Re-exportar únicamente los símbolos públicos necesarios desde `<monolito>.rs` o `mod.rs` para no romper la API pública del crate.

---

## 3. Pruebas de Regresión y Verificación Requerida

1. **Verificación de regresiones en `rust/<crate>/tests/`**:
   - [Indicar si algún cambio de bug requiere añadir o ajustar una aserción en `rust/<crate>/tests/<suite>_tests.rs` y cómo probarlo].
2. **Ejecución obligatoria de comandos de validación**:
   - `cargo test --locked -p <nombre-del-crate>`
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets --locked -- -D warnings`
   - `cargo xtask quality` (o `cargo xtask check` si el cambio toca bordes públicos entre crates)
````
