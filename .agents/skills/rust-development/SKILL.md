---
name: rust-development
description: Diseña, implementa o revisa código Rust de EmuSync durante su migración Kotlin, incluidos ownership, límites de módulos, errores y compatibilidad. No usar para cambios solo Kotlin.
---

# Desarrollo Rust de EmuSync

Leer `AGENTS.md` y el hito vigente en `RUST_MIGRATION.md`. Elegir tipos y ownership para representar invariantes; no traducir clases o patrones JVM uno a uno. Mantener domain y application independientes de adaptadores concretos; convertir DTO en los límites y restringir visibilidad. Usar errores tipados y preservar formatos y semántica existentes.

Diseñar quién posee y cierra cada cliente, tarea y runtime. Las operaciones cancelables deben completar limpieza y rollback remoto antes de devolver control; `Drop` no ejecuta I/O asíncrono. No bloquear el hilo de UI ni mantener locks sin justificar durante esperas. Aislar `unsafe` cuando sea necesario y documentar sus invariantes.

Consultar [Rust Book](https://doc.rust-lang.org/book/), [Cargo](https://doc.rust-lang.org/cargo/) y [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) para detalles; contrastar APIs sensibles a versión con la toolchain fijada. Usar rust-concurrency para concurrencia y rust-style-clippy para calidad. Ejecutar comprobaciones afectadas disponibles, respetando la etapa real de migración.
