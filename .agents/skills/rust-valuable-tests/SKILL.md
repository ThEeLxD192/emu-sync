---
name: rust-valuable-tests
description: Diseña, revisa o audita pruebas Rust de EmuSync y trazabilidad de migración Kotlin según contratos observables y riesgos de producción. No usar para cambios triviales o solo documentación.
---

# Pruebas Rust con valor

Identificar contrato observable, fallo plausible y punto de entrada. Consultar el seguimiento y el catálogo Kotlin antes de sustituir una garantía. Registrar escenario fuente, prueba Rust, plataforma y evidencia; se pueden conservar, combinar o reescribir pruebas si su protección equivalente es identificable. No imponer cuotas de cobertura ni igual número de pruebas.

Priorizar partidas, historial y backups; rutas y JSON legacy; OAuth/CAS; Drive condicional y dos escritores; cancelación, rollback y cierre; procesos, Steam y actualizaciones. Usar esperados literales independientes y componentes reales cuando el límite sea precisamente el riesgo. Aislar archivos, puertos y procesos; probar red real mediante servidores controlados cuando importe el transporte.

Para concurrencia, observar estados y efectos, no depender solo de sleeps. Comprobar cancelación cooperativa y finalización de rollback. Separar pruebas portables y Windows/Linux nativas, registrar omisiones y no presentar simulaciones o resultados cacheados como evidencia nueva.

No añadir pruebas solo por movimientos o documentación. Ejecutar suites afectadas y registrar comandos/resultados. Consultar rust-testing para mecánicas y rust-concurrency para riesgos async; `AGENTS.md` y el usuario prevalecen sobre guías genéricas.
