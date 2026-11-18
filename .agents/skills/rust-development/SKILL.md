---
name: rust-development
description: Diseña, implementa o revisa código Rust de EmuSync, incluidos ownership, límites de módulos, errores y compatibilidad.
---

# Desarrollo Rust de EmuSync

Leer `AGENTS.md`. Elegir tipos y ownership para representar invariantes. Mantener domain y application independientes de adaptadores concretos; convertir DTO en los límites y restringir visibilidad. Usar errores tipados y preservar formatos y semántica existentes.

Diseñar quién posee y cierra cada cliente, tarea y runtime. Las operaciones cancelables deben completar limpieza y rollback remoto antes de devolver control; `Drop` no ejecuta I/O asíncrono. No bloquear el hilo de UI ni mantener locks sin justificar durante esperas. Aislar `unsafe` cuando sea necesario y documentar sus invariantes.

Nombres que se entiendan sin buscar su definición, también en pruebas. Palabras completas del dominio (`save_path`, `remote_file`, `pending_uploads`), no letras sueltas ni abreviaturas (`p`, `rf`, `tmp2`, `res`, `cfg_m`), salvo convenciones universales: `id`, `url`, `io`, `ctx` de egui, `i` en un bucle corto, `e` en un cierre de una línea. Nombrar por lo que representa, no por su tipo (`save_path`, no `path_buf`). Booleanos como predicado (`is_cancelled`, `has_conflict`, `should_retry`). Unidades en el nombre cuando el tipo no las da (`timeout_ms`, `size_bytes`). Las pruebas se nombran por escenario y resultado esperado.

Consultar [Rust Book](https://doc.rust-lang.org/book/), [Cargo](https://doc.rust-lang.org/cargo/) y [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) para detalles; contrastar APIs sensibles a versión con la toolchain fijada. Usar rust-concurrency para concurrencia y rust-style-clippy para calidad. Ejecutar las comprobaciones afectadas.
