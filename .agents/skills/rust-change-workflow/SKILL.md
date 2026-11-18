---
name: rust-change-workflow
description: Flujo obligatorio para cerrar un cambio de código Rust de EmuSync (`rust/`, `xtask/`) con subagentes —formato y Clippy, dos revisiones independientes sin el contexto de la sesión, verificación de hallazgos por el agente principal y pruebas adversariales escritas por otro subagente—. Usar al terminar cualquier cambio de código Rust, antes de dar el trabajo por cerrado o proponer un commit.
---

# Flujo de cambios Rust con subagentes

El agente principal escribe el cambio; otros agentes, que no conocen la sesión, lo revisan y lo atacan con pruebas; el agente principal verifica cada hallazgo antes de aceptarlo. Así se detectan los errores que el autor no ve por conocer su propia intención, y se descartan las alucinaciones de los revisores.

## Cuándo aplica

| Cambio | Etapas |
| --- | --- |
| Comportamiento: lógica, errores, concurrencia, cancelación, persistencia, red, procesos, estado de la interfaz | 1 a 5 |
| Refactor sin cambio de comportamiento (mover, renombrar, extraer) | 1, 2, 3 y 5; pruebas solo si un revisor señala un contrato sin cubrir |
| Solo documentación, comentarios, textos de `texts.rs` o metadatos de Cargo | 1 y 5, sin subagentes |

En la duda, tratarlo como cambio de comportamiento. Un cambio pedido por el usuario sigue este flujo aunque no lo mencione.

## Etapa 1: formato y Clippy

```sh
cargo fmt --all
CARGO_BUILD_JOBS=2 nice -n 19 cargo clippy --workspace --all-targets --locked -- -D warnings
```

Corregir todo aviso sin silenciarlo (`#[expect(..., reason = "...")]` solo si está justificado, según `rust-style-clippy`). Repetir esta etapa después de cada corrección de las etapas siguientes.

## Etapa 2: revisión independiente (2 subagentes en paralelo)

Preparar el material, que es lo único que verán los revisores:

```sh
mkdir -p target/agent-review
git diff "$(git merge-base main HEAD)" -- rust xtask > target/agent-review/change.diff
git ls-files --others --exclude-standard -- rust xtask   # archivos nuevos: se leen enteros
```

Lanzar a la vez dos revisores (en Claude Code, el agente `rust-reviewer`), uno por enfoque, con este encargo:

```text
Enfoque: <A | B>
Diff: target/agent-review/change.diff
Archivos nuevos (leer completos): <lista o «ninguno»>
Intención del cambio: <1–3 frases: qué debe hacer el código después del cambio>
```

- **Enfoque A, lógica y datos:** corrección lógica y casos borde, errores tragados (`let _ =`, `.ok()`, `unwrap_or_default`), pánicos, desbordamientos y conversiones `as`, integridad de partidas, historial, backups y recuperación, formatos persistidos y compatibilidad con instalaciones existentes, rutas (traversal, symlinks, mayúsculas en Windows).
- **Enfoque B, concurrencia y diseño:** locks a través de `.await` o callbacks, cancelación y rollback asíncrono, carreras TOCTOU/ABA, `Drop` que asume I/O, trabajo bloqueante en el hilo de la interfaz o redibujado continuo, grafo de capas de `AGENTS.md`, visibilidad, errores tipados, textos visibles sin `i18n::tr`, y nombres poco intuitivos (regla de `rust-development`).

La intención dice **qué** debe hacer el código, no cómo se decidió ni dónde se sospecha un fallo. Nunca pasar al revisor el razonamiento de la sesión, hipótesis propias ni los hallazgos del otro revisor: cada uno debe llegar a sus conclusiones solo.

Cada hallazgo vuelve con este formato; sin escenario concreto no se reporta:

```text
### H<n> [crítico | alto | medio | bajo] <título>
- Ubicación: <archivo>:<línea>
- Defecto: <una frase>
- Escenario de fallo: <estado o entrada concreta> → <resultado incorrecto observable>
- Evidencia: <líneas citadas que lo demuestran>
- Confianza: <alta | media>
```

## Etapa 3: verificación por el agente principal

Para cada hallazgo, leer el código citado y seguir el flujo real hasta clasificarlo:

- **Confirmado:** el escenario ocurre. Corregir y volver a la etapa 1.
- **Descartado:** el escenario no ocurre; anotar la razón concreta (p. ej. «la guardia de `foo.rs:120` lo impide»).
- **Dudoso:** no se puede decidir leyendo. Resolverlo con una prueba mínima que reproduzca el escenario, o preguntar al usuario si depende de qué comportamiento se quiere.

Si hubo correcciones, regenerar el diff y lanzar **un** revisor nuevo sobre el delta de la corrección, con el enfoque del hallazgo. Máximo dos rondas de re-revisión: si siguen apareciendo hallazgos, parar y presentárselos al usuario en lugar de seguir el ciclo.

## Etapa 4: pruebas adversariales (1 subagente; 2 si el riesgo es alto)

Lanzar el escritor de pruebas (en Claude Code, el agente `rust-adversarial-tester`) cuando las etapas 2 y 3 quedaron cerradas. Usar dos cuando el cambio toque concurrencia, cancelación, recuperación, formatos persistidos, Drive u OAuth, y repartirles los vectores del playbook en lugar de duplicar el encargo.

```text
Crate y carpeta de pruebas: rust/<crate>/tests/
Diff: target/agent-review/change.diff
Archivos nuevos (leer completos): <lista o «ninguno»>
Contrato que debe cumplirse: <promesas observables del cambio, sin detalles de implementación>
Vectores asignados: <todos | lista, si hay dos escritores>
```

El escritor solo crea o edita archivos de pruebas, nunca código de producción ni `Cargo.toml`. Si una prueba falla, no la debilita para que pase: la deja y la reporta como posible bug con el formato de hallazgos y el comando que la reproduce.

El agente principal, al recibir el informe:

1. Comprueba con `git status` que solo cambiaron archivos de pruebas.
2. Para cada prueba que falla, decide si el esperado sale del contrato (bug real: corregir el código y repetir etapas 1 a 3 para la corrección) o si la prueba está mal (corregirla o quitarla, anotando por qué).
3. Descarta pruebas que solo reflejan la implementación o duplican otras (`rust-valuable-tests`). En las críticas, confirma su valor invirtiendo temporalmente la guardia que protegen y viendo que fallan; después restaurar el código y comprobarlo con `git diff`.
4. Pasa las pruebas nuevas por las etapas 1 a 3 como cualquier otro cambio, salvo que la revisión de pruebas basta con un revisor.

## Etapa 5: cierre

```sh
CARGO_BUILD_JOBS=2 nice -n 19 cargo xtask check
```

Informar al usuario: qué cambió, hallazgos confirmados y corregidos, hallazgos descartados con su razón, pruebas añadidas y lo que queda pendiente (incluida la validación en Windows nativo, que este flujo no cubre). Las reglas de commit de `AGENTS.md` no cambian: confirmación explícita antes de cada commit.

## Sin subagentes

Si la herramienta no puede lanzar subagentes, decírselo al usuario antes de cerrar el cambio. No sustituir en silencio la revisión independiente por una autorrevisión; una alternativa es abrir una sesión nueva con los mismos encargos.

## Adaptadores

- Claude Code: `.claude/agents/rust-reviewer.md` (solo lectura: sin edición ni shell) y `.claude/agents/rust-adversarial-tester.md`.
- Otros agentes: usar su mecanismo de subagentes con los mismos encargos y formato de hallazgos.
