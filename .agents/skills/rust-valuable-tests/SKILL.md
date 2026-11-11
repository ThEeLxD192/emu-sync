---
name: rust-valuable-tests
description: Diseña, revisa, implementa y audita pruebas Rust de EmuSync —incluidas pruebas adversariales, de inyección de fallos, cancelación/rollback y compatibilidad de formatos persistidos— según contratos observables y riesgos reales de producción. No usar para cambios triviales o solo documentación.
---

# Pruebas Rust con Valor y Testing Adversarial en EmuSync

Proteger los contratos observables y las invariantes críticas de EmuSync  El valor de una prueba no se mide por cobertura porcentual ni cantidad de casos, sino por su capacidad de **detectar un fallo plausible o un estado hostil que causaría pérdida de partidas, corrupción de configuración/historial, bloqueo colgado o incompatibilidad silenciosa con datos existentes**.

---

## 1. Elegir qué comprobar y mentalidad adversarial

Antes de escribir o modificar una prueba, definir tres elementos:
1. **Contrato observable e invariante**: qué promete el sistema al usuario o a la capa consumidora (por ejemplo, *"ninguna partida local se sobrescribe sin respaldo verificable y revisión OAuth vigente"*).
2. **Fallo plausible o vector adversarial**: qué interrupción, condición de carrera, entrada malformada o error parcial podría romper esa promesa.
3. **Punto de entrada con fidelidad al riesgo**: el límite más pequeño que conserve el riesgo real (regla pura en `domain`, caso de uso en `application`, adaptador real con socket/disco aislado en `data-*`/`platform-desktop`, o composición completa en `desktop-app`).

### Prioridad de riesgos en EmuSync
1. **Integridad de partidas, historial y recuperación (`sync-backups`, `sync-recovery`, `.sync-state.json`)**: nunca perder ni truncar una partida local; nunca registrar como sincronizada una transferencia fallida, truncada o de otra cuenta; bloquear nuevas subidas (`ResourceBusy`) mientras exista una operación incierta sin reconciliar.
2. **Costuras de composición y ciclo de vida (`DesktopCoordinator` ↔ `RuntimeOwner` ↔ `SyncOrchestrator` ↔ `RetainedFiles` ↔ `GuardedFiles`)**: no descartar errores retenidos ni snapshots al cruzar de adaptadores al orquestador; impedir que el cierre de ventana (`CloseRequested`) mate transferencias activas o ignore estados `RecoveryRequired`.
3. **Cancelación cooperativa y rollback asíncrono**: señalar cancelación (`CancellationToken` / `UiAction::ClearStatus`) debe esperar a que el worker termine de limpiar temporales y ejecutar el rollback remoto inverso antes de liberar la exclusión mutua o declarar `Idle`. Nunca asumir que `Drop` o abortar un `Future` ejecuta rollback remoto.
4. **Google Drive condicional, sesiones reanudables y dos escritores**: precondición `If-Match` con ETag fuerte v2 sin fallback incondicional; pérdida de respuesta tras `PUT` tratada como resultado incierto (nunca reenviar ciegamente creando duplicados); validación de rangos `308`, checksums SHA-256 e identidad remota.
5. **OAuth, CAS y problemas ABA**: preservar ediciones concurrentes de biblioteca durante intercambios OAuth; invalidar tokens o commits si las credenciales cambiaron o se desvincularon mientras la transferencia estaba en vuelo (`CredentialTransaction`).
6. **Seguridad de rutas, TOCTOU y compatibilidad multiplataforma**: rechazo de traversal (`..`), bytes `NUL`, enlaces simbólicos no confiables y sustitución de directorios tras staging (`PinnedRemoteDestination`); manejo de nombres reservados de Windows y rutas 8.3.
7. **Compatibilidad con datos existentes**: ordenación UTF-16 frente a UTF-8, esquemas JSON legacy y campos desconocidos en `config.json` y `shortcuts.vdf`, seguimiento de procesos reales, zombies y renovación de timeout durante compilación de shaders en Steam.

No añadir pruebas por mover código mecánicamente, cambiar comentarios o declarar structs/getters sin lógica contractual propia.

---

## 2. Taxonomía de pruebas adversariales

Al diseñar pruebas, aplicar sistemáticamente estas cinco dimensiones (detalladas con patrones e implementación en el [playbook de pruebas adversariales](references/adversarial-testing-playbook.md)):

1. **Sabotaje de costuras de integración (Seam & Composition Adversary)**:
   - Atacar la unión real entre capas (por ejemplo, `DesktopCoordinator` ejecutando `SyncOrchestrator` sobre `RetainedFiles<GuardedFiles<...>>` bajo `RuntimeOwner`), verificando que los errores enriquecidos, snapshots y bloqueos de cierre no se pierden al traducirse a eventos de UI.
2. **Inyección controlada de fallos (Fault Injection / Chaos Loopback)**:
   - Usar servidores TCP/HTTP locales (`127.0.0.1:0`) y directorios temporales reales que fallen en el instante más dañino: corte de conexión tras recibir el cuerpo del `PUT`, respuestas HTTP `412`/`429`/`500`/`503`, cuerpos truncados a mitad de descarga, metadatos con hash distinto o permisos revocados en staging.
3. **Concurrencia hostil, cancelación y muerte de proceso**:
   - Sincronizar mediante barreras o notificaciones (`Barrier`, `Notify`, canales) el momento exacto en que una operación tiene recursos abiertos o bytes parciales escritos, y disparar entonces: cancelación del usuario, segundo escritor concurrente, rotación de credenciales OAuth, solicitud de cierre de ventana o `SIGKILL` sobre un subproceso real.
4. **Entorno hostil y ataques de frontera (Boundary & TOCTOU)**:
   - Intercambiar un subdirectorio por un symlink apuntando fuera de la raíz entre la validación y la escritura; inyectar manifiestos corruptos o de otra cuenta en `.sync-recovery`; pasar metacaracteres de shell, rutas no canónicas o VDF truncados.
5. **Oráculo con esperados independientes**:
   - Contrastar entradas extremas y ambiguas contra esperados literales independientes (`rust/domain/tests/fixtures/comparison.tsv`, fixtures JSON v1–v3 y archivos persistidos por versiones anteriores).

---

## 3. Reglas de diseño y propiedad por capa

Respetar estrictamente el grafo de dependencias verificado por `cargo xtask`:

| Crate / Capa | Qué probar aquí | Cómo aislar los límites |
| --- | --- | --- |
| `emusync-domain` | Planificación temporal y por checkpoint, conflictos, duplicados, rutas lógicas, títulos UTF-16 y argumentos. | Pruebas puras sin red, reloj del sistema ni I/O; contrastadas con literales y `comparison.tsv`. |
| `emusync-application` | Orquestación (`SyncOrchestrator`), migración con rollback inverso, frescura de chequeos, exclusión (`OperationCoordinator`) y reintentos por `RequestId`. | Fakes pequeños e inyectados que implementen los traits de `domain` y permitan inyectar fallos deterministas o pausar en puntos `await`. |
| `emusync-data-local` | `ConfigManager` (CAS/legacy), `SyncHistoryStore`, `TrackedFileSync`, `StagedRestore`, `PinnedRemoteDestination` y `RecoveryStore`. | `tempfile::tempdir()` real, descriptores e inodos reales, permisos y symlinks reales en el sistema de archivos. |
| `emusync-data-drive` / `emusync-data-updates` | OAuth loopback, `ConditionalUpdate`, `ResumableUpload`, `DriveDownload`, `DriveListing`, `DriveFolders` y `UpdateDownloader`. | Clientes `reqwest` reales contra sockets `TcpListener` loopback locales que hablen HTTP/1.1 controlado; nunca Google Drive ni GitHub reales. |
| `emusync-platform-desktop` | `GameRunner`, `ProcessMonitor`, `BinaryVdf`, `SteamShortcutManager`, `scan_roms`, `DesktopCoverFiles` y `AppImageInstaller`. | Subprocesos locales controlados, entornos sintéticos aislados y archivos binarios VDF/AppImage en directorios temporales. |
| `emusync-presentation` | Estado inmutable (`AppUiState`), validación de formularios, transiciones de diálogos y emisión de `UiAction` en egui headless. | Contexto `egui::Context` headless e inputs sintéticos sin levantar ventana nativa ni tocar disco/red. |
| `emusync-desktop-app` | Composición real (`DesktopCoordinator`, `RuntimeOwner`, `OperationSupervisor`, `GuardedFiles`, `RetainedFiles`) y recuperación entre procesos. | Almacenamiento temporal real + servidores HTTP loopback + runtime Tokio real, verificando la costura completa de principio a fin. |

### Reglas de implementación de las pruebas
- **Esperados independientes**: obtener siempre el valor esperado de un literal, especificación o fixture independiente; jamás recalcularlo invocando la misma función o helper bajo prueba.
- **Sincronización determinista**: coordinar hilos y tareas async con primitivas explícitas (`Barrier`, `Notify`, `oneshot`, lecturas bloqueantes de socket). No depender de `sleep` arbitrarios para acertar una ventana de carrera.
- **Aserciones completas sobre el post-estado**: cuando el contrato involucra seguridad de datos, comprobar en la misma prueba: (1) el resultado/error devuelto, (2) que la partida original sigue intacta byte a byte, (3) que los temporales de staging desaparecieron (o que el snapshot de recuperación persiste si el resultado fue incierto), y (4) que el historial y la exclusión quedaron en el estado correcto.
- **Sin herramientas ni cuotas innecesarias**: no añadir dependencias de producción ni instalar herramientas globales sin autorización. Priorizar tests deterministas y reproducibles ejecutables con `cargo test --locked` y `cargo xtask check`.

---

## 4. Verificación del valor, trazabilidad y ejecución

1. **Prueba del saboteador (mutación manual dirigida)**:
   - Para confirmar que una prueba adversarial no es un falso positivo, verificar qué cambio concreto en el código de producción (invertir una condición, omitir la comprobación de revisión OAuth, descartar `RetainedFiles` o saltarse el rollback) la hace fallar inmediatamente.
2. **Conservación de garantías**:
   - Se pueden combinar o reescribir pruebas si la garantía observable se conserva y el cambio queda justificado.
3. **Separación estricta por plataforma y evidencia honesta**:
   - Ejecutar las suites afectadas durante el desarrollo (`cargo test --locked -p <crate>`) y validar el cierre con `cargo xtask check`.
   - Distinguir siempre ejecuciones nuevas de resultados en caché.
   - **Una prueba aprobada en Linux o mediante simulación no valida Windows nativo** (nombres cortos 8.3, dispositivos reservados `CON`/`PRN`/`NUL`, bloqueos de archivos de Windows, PowerShell/WiX). Registrar explícitamente cualquier pendiente de plataforma en la sección «Pendientes Windows» de `AGENTS.md`.

---

## 5. Referencias

- [Playbook de pruebas adversariales para EmuSync](references/adversarial-testing-playbook.md): anatomía de ataques, patrones por crate, servidores loopback de caos, TOCTOU y checklist de auditoría.
- `rust-testing`: mecánicas generales de `#[test]`, pruebas de integración e infraestructura de `cargo test`.
- `rust-concurrency`: patrones de cancelación cooperativa en Tokio, canales, exclusión mutua y prevención de deadlocks.
- `AGENTS.md` en la raíz del repositorio prevalece siempre sobre cualquier guía genérica.
