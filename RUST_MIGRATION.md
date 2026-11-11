# Migración manual de EmuSync a Rust

Base congelada de referencia: `c9acc16a687b1093007dba4efc12c37237908678`, rama `rewrite/rust-native`. La versión Kotlin es la referencia ejecutable hasta verificar la equivalencia. Documentar no ejecuta un hito. No publicar ni hacer push sin autorización explícita; cada commit requiere aprobación individual previa.

## Seguimiento

Para cada hito registrar fecha, base, tareas, comandos exactos, resultados, omisiones, plataforma, evidencia y commit por separado. Marcarlo completo solo con toda su aceptación; mantener visible cualquier pendiente Windows. R0 está completado técnicamente y registrado en `99a34c1`. R1 está implementado y validado parcialmente en Linux; R2 está implementado y validado en Linux; R3 está iniciado; R4 y R6 están completados técnicamente en Linux; R5 requiere corregir la propagación de errores y cancelación en integración; R7 está iniciado pero su composición requiere integración; R8 está abierto. La validación nativa Windows de R2, R4, R5 y R6 y el CI remoto siguen pendientes. Por decisión del usuario del 2026-11-05, la validación nativa Windows se difiere y no bloquea el trabajo posterior, pero sigue pendiente de aceptación.

### R0 — Guía, skills y seguimiento

- [x] Preparar rama y revisar `AGENTS.md`, este seguimiento y las skills versionadas.
- [x] Comprobar licencias, referencias, validación estructural, enlaces y `git diff --check`.
- [x] Registrar evidencia; no ejecutar pruebas de aplicación por cambios solo documentales.
- [x] **R0 completado técnicamente.**

Evidencia (2026-11-05, Linux, base `c9acc16`): rama local creada; cinco skills Rust añadidas, tres adaptadas de full-stack-skills con licencia Apache-2.0 y revisión fijada, dos específicas de EmuSync; skill Kotlin conservada por coexistencia. `quick_validate.py` valida las seis skills. Enlaces locales, espacios finales y `git diff --check` comprobados; el seguimiento histórico Kotlin se preservó íntegro. No se ejecutó la aplicación ni se cambió su código. R1–R8, CI remoto y Windows siguen sin validar. Commit R0: `99a34c1`.

### R1 — Workspace y calidad

- [x] Crear workspace Cargo y crates por capacidad; fijar toolchain estable y `Cargo.lock`.
- [x] Añadir `cargo xtask quality` y `cargo xtask check`: rustfmt, Clippy bloqueante, pruebas y verificador de arquitectura basado en `cargo metadata`.
- [ ] Mantener Gradle y agregar a CI Linux/Windows y rama Rust, informes actuales y hook sobre exportación del índice con disparadores de ambos lenguajes.
- [ ] Ventana egui/winit/wgpu mínima: texto, portapapeles, DPI y cierre en Windows, X11 y Wayland.
- [ ] **R1 completado**, con resultados por plataforma y excepciones visibles.

Evidencia parcial (2026-11-05, Linux, base `99a34c1`; sin commit):

- Workspace de ocho capacidades y `xtask`, toolchain 1.99.1 y lockfile; Kotlin y sus módulos siguen presentes. Las capacidades Rust están vacías salvo la ventana de prueba y no implementan sincronización. El verificador inspecciona dependencias normales declaradas mediante `cargo metadata --no-deps --locked`. Una copia temporal con la arista prohibida presentation → data-local fue rechazada; Cargo rechazó previamente domain → application por ciclo antes de alcanzar el verificador.
- `cargo xtask check`: correcto en Linux con rustc/cargo 1.99.1 del sistema; formato, Clippy bloqueante, arquitectura y pruebas pasaron. Una prueba de reglas arquitectónicas; los demás crates aún no contienen pruebas de producto. La primera compilación gráfica detectó una API cambiada de egui, corregida antes del resultado final.
- `cargo build --locked -p emusync-desktop-app`: correcto. Corrección de evidencia anterior: winit 0.30 ya no usa `WINIT_UNIX_BACKEND`, por lo que la prueba previa etiquetada X11 no demostró ese backend; ambos procesos pudieron usar Wayland. Las pruebas nuevas eliminan `WAYLAND_DISPLAY` y `WAYLAND_SOCKET` para seleccionar X11 y confirman el servidor utilizado.
- El hook selecciona Gradle y Cargo por cambios preparados, sobre una exportación aislada. `bash scripts/tests/git-hooks-test.sh` pasó los escenarios de aislamiento existentes y los nuevos de Rust, fallo y cambios mixtos. CI incluye la rama Rust, ambos sistemas y un log Rust por plataforma; CI remoto aún no ejecutado.
- La primera ejecución `./gradlew check --no-daemon` pasó (110 tareas, cuatro ejecutadas y 106 UP-TO-DATE), pero precedió la última ampliación del fixture de hooks. La siguiente detectó que el Cargo simulado dependía del Bash seleccionado por PATH; se corrigió el fixture. La ejecución final de `./gradlew check --no-daemon` pasó en 1 min 10 s: 101 tareas, tres ejecutadas y 98 UP-TO-DATE, con caché de configuración reutilizada; hooks y convenciones ejecutados. Los resultados Kotlin reutilizados no son ejecuciones nuevas.
- Windows: sin compilación, pruebas ni ventana nativas en este entorno; portapapeles, DPI y cierre siguen pendientes. Falta confirmar CI Windows y Linux remoto. R1 permanece abierto; sin push.
- Commit parcial R1: `5feb34b` (`build: establish Rust workspace and quality checks`). El hook validó una exportación del índice: Gradle `qualityCheck` pasó con 96 tareas ejecutadas y `cargo xtask quality` pasó con arquitectura, formato y Clippy. No ejecutó pruebas, conforme al diseño del hook.
- Se intentó iniciar Hyprland en un runtime temporal aislado con backend headless: terminó con `CBackend::create() failed!` antes de crear un socket Wayland. No hubo prueba gráfica nueva ni se considera evidencia de plataforma. No hay un servidor X11 nativo aislado disponible en este host.
- En la sesión Wayland real de Hyprland 0.57.1 se lanzó `WINIT_UNIX_BACKEND=wayland target/debug/emusync-desktop-app` y se consultó `hyprctl -j clients` filtrando por PID: ventana mapeada titulada EmuSync, con `xwayland=false`. Dos intentos de cierre con la sintaxis anterior de `hyprctl dispatch` fallaron y se terminó el proceso mediante SIGTERM. Una nueva ejecución con `hyprctl dispatch 'hl.dsp.window.close({ window = "address:<dirección de la ventana de prueba>" })'` devolvió `ok` y la aplicación terminó con código cero, sin stderr. Se comprueban mapeo y cierre normal Wayland; renderizado, interacción y DPI siguen pendientes.

Validación adicional (2026-11-05, Linux; cambios de fuentes sin commit):

- Una captura de Hyprland anidado con runtime y portapapeles aislados reveló cuadros en lugar de texto. Egui 0.37 no activa `default_fonts` por defecto; se habilitaron las fuentes integradas y se añadió `default_fonts_render_the_preview_text`, que comprueba los caracteres reales del texto inicial. La captura posterior mostró el texto correcto; `cargo test --locked -p emusync-desktop-app` pasó una prueba. En una copia temporal, retirar `default_fonts` hizo fallar esa misma regresión. `cargo xtask check` pasó arquitectura, formato, Clippy y dos pruebas Rust; `./gradlew check --no-daemon` pasó en 1 min 10 s (101 tareas, tres ejecutadas y 98 UP-TO-DATE); las pruebas Kotlin reutilizaron resultados, y hooks y convenciones se ejecutaron.
- Wayland: Hyprland 0.57.1 anidado, aplicación y `wl-copy`, `wl-paste` y `wtype` con socket y portapapeles aislados. Escribir desde el portapapeles, seleccionar y copiar de vuelta conservó el literal sintético `EmuSync R1 clipboard 123`. Capturas con escala 1 y después de cambiar a 2 mostraron el texto y su tamaño correspondiente; Hyprland avisó que la resolución anidada no se dividía limpiamente por dos. El cierre solicitado al compositor devolvió código cero. No se verificó hardware de pantalla independiente.
- X11: servidor Xvfb 21.1.25 aislado y Vulkan Mesa lavapipe 26.2.6 extraídos temporalmente sin instalar en el sistema; se eliminaron `WAYLAND_DISPLAY` y `WAYLAND_SOCKET`, y `DISPLAY=:121` señaló ese servidor. Con `WINIT_X11_SCALE_FACTOR=1` y `=2`, en procesos separados, las capturas mostraron texto y escala; `xclip` y `xte` verificaron ida y vuelta del literal sintético `EmuSync R1 X11 clipboard 456`. Un redimensionado X11 a 700×500 se confirmó mediante `xwininfo`; la solicitud `WM_DELETE_WINDOW` terminó cada proceso con código cero. Las pruebas usaron renderizado software y no una sesión Xorg física ni cambio de escala en caliente. Antes de usar lavapipe, el adaptador físico no fue compatible con Xvfb sin DRI3.
- Windows nativo queda diferido por decisión del usuario (2026-11-05), sin considerarlo validado: Docker Linux no proporciona kernel Windows; Wine o compilación cruzada no equivalen a esa validación. Falta CI remoto en ambos sistemas, y las comprobaciones pendientes de resolución, escalas y cierre en Windows. No se ha solicitado ni efectuado push.

Cuando haya acceso a Windows, completar la aceptación diferida con `cargo xtask check` y `cargo run --locked -p emusync-desktop-app` en Windows nativo; comprobar texto, copia/pegado bidireccional, dos escalas, redimensionado y cierre normal. Registrar OS, comandos y omisiones. Para repetir X11 en un sistema Wayland, quitar `WAYLAND_DISPLAY` y `WAYLAND_SOCKET` y fijar `DISPLAY` al servidor X11 deseado; winit 0.30 no usa `WINIT_UNIX_BACKEND`. CI remoto requiere autorización explícita para push.

### R2 — Dominio

- [x] Migrar modelos, reglas y contratos por capacidad sin copiar mecánicamente clases Kotlin. Mantener dominio libre de UI, HTTP e I/O concreto.
- [x] Comparar escenarios de planificación, conflictos, rutas y argumentos con fixtures y esperados independientes.
- [x] **R2 completado técnicamente en Linux**; validación Windows nativa y CI remoto pendientes.

Evidencia (2026-11-05, Linux, base `d8532a1`; sin commit):

- `emusync-domain` contiene modelos y contratos de configuración/credenciales, biblioteca y destinos, ejecución, sincronización, recuperación y actualización. No depende de crates de UI, HTTP ni I/O concreto. Los tiempos se expresan como nanosegundos de epoch; la lectura de fechas ISO y formatos persistidos corresponde a adaptadores posteriores. Los mapas de títulos conservan orden de inserción y la ordenación de nombres reproduce UTF-16 Kotlin.
- Planificación temporal y por checkpoint, selección de duplicados, rutas cloud/remotas, scoping, títulos y argumentos tienen pruebas puras. Duplicados remotos con hash ausente o distinto se rechazan; se introducen identificadores de solicitud para distinguir respuestas tardías con datos iguales. Los contratos documentan actualización atómica, escritura condicional y limpieza cooperativa: su implementación y pruebas de I/O, cancelación y rollback corresponden a R3–R6, no se consideran validadas por declarar interfaces.
- `domain/src/test/resources/r2/comparison.tsv` contiene entradas y esperados literales independientes usados por `R2FixtureTest` Kotlin y `shared_reference_examples_match_independent_expected_values` Rust: ausencia de copias, límites temporales y post-juego, hashes/versiones, planificación y duplicados, rutas, argumentos y títulos. Las otras pruebas Rust amplían casos de la referencia. La primera ejecución del fixture detectó una barra invertida duplicada en el propio fixture; se corrigió antes de las ejecuciones finales.
- `./gradlew :domain:spotlessApply :domain:qualityCheck :domain:test --no-daemon`: correcto en Linux, 22 tareas (cinco ejecutadas, 17 UP-TO-DATE); 59 pruebas de dominio ejecutadas, sin fallos ni omisiones; Detekt de pruebas ejecutado sin hallazgos tras extraer funciones del fixture. `cargo xtask check`: correcto en Linux, arquitectura, formato, Clippy y pruebas del workspace; 14 pruebas de dominio Rust, además de las dos pruebas existentes del workspace.
- `./gradlew check --no-daemon`: correcto en Linux, 111 tareas (seis ejecutadas y 105 UP-TO-DATE); las suites Kotlin reutilizaron los resultados de la ejecución focalizada o resultados previos, y las verificaciones de hooks y convenciones se ejecutaron.
- Windows nativo no se ejecutó: ni estos fixtures ni el pendiente R1 se presentan como validación Windows. CI remoto Linux/Windows sigue sin ejecución y requiere push autorizado. No se realizó push; el commit espera aprobación separada.

#### Trazabilidad de dominio R2

| Garantía de referencia Kotlin | Prueba Rust y justificación | Linux | Windows |
| --- | --- | --- | --- |
| `SyncConflictTest`, `SyncPlannerTest`: copias ausentes, umbral, fracciones, post-juego, orden y duplicados | `initial_decisions_preserve_tolerance_and_post_play_rule`, `directory_plan_sorts_and_retains_older_duplicates`, fixtures compartidos; agrupan por decisión observable. | Pasó | Pendiente |
| `TrackedSyncPlannerTest`: bootstrap, hashes, cambios simultáneos, versión, fecha legacy, ID y ausencia | `tracked_decisions_use_content_and_remote_identity` y fixtures compartidos; conservan decisiones sin depender del reloj. | Pasó | Pendiente |
| `CloudPathsTest`, `GameEntryGroupTest`, `GameTitleUtilsTest`, scoping de `SaveTargetMappingTest` | `cloud_folder_and_rom_routing_match_reference_examples`, `titles_follow_kotlin_fixtures_and_priority`, `scoping_directories_avoids_existing_prefixes_and_is_stable`; combinan escenarios por regla. | Pasó | Pendiente |
| `ArgumentsCodecTest` y rutas remotas de `SaveRestorePathsTest` | `arguments_match_literal_examples`, `arguments_preserve_non_special_escapes_and_format_literals`, `paths_reject_absolute_and_traversal_names`, fixtures compartidos. La resolución física y symlinks quedan en R3. | Pasó | Pendiente |
| `ConditionalCloudWriteTest`: duplicados idénticos, divergentes o sin verificar | `duplicate_remote_files_require_verified_equal_content` comprueba solo selección pura; dos escritores y transferencia reales quedan en R4. | Pasó | Pendiente |
| `OperationRecoveryTest`, identidad de `SyncResponsesTest` | `cancellation_is_not_recovered_as_a_normal_failure` y `equal_looking_conflict_requests_keep_distinct_identity`; no sustituyen las pruebas de corrutina, solicitudes tardías o rollback de R5. | Pasó | Pendiente |

### R3 — Persistencia local

- [x] Implementar configuración y DTO, historial, snapshots, backups y resolución de rutas; introducir `--config <ruta>` para sustituir el override JVM.
- [ ] Verificar JSON literal y legacy, precedencia de ubicación, concurrencia, escritura atómica, permisos y lectura/escritura cruzadas Kotlin/Rust.
- [x] Conservar historial v1 y la clave SHA-256 de ruta absoluta normalizada léxicamente, NUL, folderId, NUL y remoteName. No sustituir normalización léxica por canonicalización.
- [ ] Verificar staging, backups, symlinks, traversal, Windows 8.3 y fallos de recuperación.
- [ ] **R3 completado.**

Evidencia parcial (2026-11-05, Linux, base `50ace7c`; sin commit):

- Se implementaron DTO de configuración, migración de targets legacy, ubicación portable y plataforma, `--config`, almacenamiento atómico privado, actualizaciones serializadas en el proceso y CAS de credenciales. El historial v1 conserva registros legacy, clave SHA-256 de ruta absoluta normalizada léxicamente, NUL, folderId, NUL y nombre remoto, y errores ante corrupción o versión desconocida. Las copias usan hashes verificados; staging y snapshots conservan la partida ante fallo de verificación o backup. El adaptador de transferencias usa el contrato neutral y realiza I/O local en workers bloqueantes.
- `cargo test --locked -p emusync-data-local`: pasó en Linux con dos pruebas unitarias y catorce de integración; `cargo xtask check` pasó arquitectura, formato, Clippy y todas las pruebas Rust del workspace. `./gradlew check --no-daemon`: correcto en 1 min 15 s, 111 tareas, cuatro ejecutadas y 107 UP-TO-DATE; las suites reutilizadas no son ejecuciones nuevas. La prueba focalizada Kotlin y su análisis se ejecutaron antes con `EMUSYNC_R3_EXCHANGE=/tmp/emusync-r3-cross-1000 ./gradlew :data:local:spotlessApply :data:local:test --tests 'com.emusync.config.RustPersistenceCompatibilityTest' :data:local:qualityCheck --no-daemon --rerun-tasks`: 23 tareas ejecutadas.
- Intercambio sintético real: `cargo run --locked -p emusync-data-local --example cross_persistence -- write /tmp/emusync-r3-cross-1000`, la prueba Kotlin focalizada anterior y `cargo run --locked -p emusync-data-local --example cross_persistence -- read /tmp/emusync-r3-cross-1000` pasaron, en ese orden. Kotlin leyó y modificó configuración e historial producidos por Rust y Rust leyó los cambios de Kotlin. La comprobación literal completa y el intercambio son complementarios, no demuestran todos los escenarios de ambas plataformas.
- La resolución remota rechaza traversal y symlinks y conserva la ruta léxica seleccionada; la validación es una observación y no impide por sí sola cambios concurrentes del sistema de archivos antes de una escritura. La integración posterior debe mantener la exclusión y asumir esta limitación. La cancelación de transferencias, el cierre supervisado de workers y la limpieza asíncrona siguen sujetos a integración y pruebas de R4/R5; no se considera que abortar un futuro realice rollback.
- Windows nativo, 8.3 y CI remoto siguen sin ejecutar. La decisión de diferir Windows permite continuar, pero no constituye su aceptación. Sin push ni commit.

| Garantía Kotlin | Evidencia Rust y alcance | Linux | Windows |
| --- | --- | --- | --- |
| ConfigFormatCompatibilityTest, ConfigGroupsTest, ConfigManagerTest: JSON, tipos y legacy | `literal_config_preserves_all_nondefault_fields_and_ordered_titles`, `explicit_empty_emulator_arguments_survive_writes`, `concurrent_config_updates_merge_and_legacy_targets_keep_paths`, intercambio Kotlin/Rust; casos agrupados por formato observable. | Pasó | Pendiente |
| ConfigUpdatesTest y ConfigLocationTest | `config_loads_legacy_defaults_round_trips_and_preserves_concurrent_updates`, `concurrent_config_updates_merge_and_legacy_targets_keep_paths`, `explicit_and_portable_locations_precede_platform`; CAS, precedencia y actualizaciones en proceso. | Pasó | Pendiente |
| SyncHistoryStoreTest y SyncHistoryCompatibilityTest | `history_keeps_legacy_data_and_concurrent_records_and_refuses_corruption`, `failed_replacement_retains_old_bytes_and_removes_temporary_file`, intercambio Kotlin/Rust; escritura común y registros v1. | Pasó | Pendiente |
| TrackedFileSyncTest: snapshot, preservación y recuperación | `snapshots_and_restores_preserve_saves_on_verification_failures`, `backup_failure_prevents_replacement_and_cleans_staging`, `backup_verifies_source_and_existing_recovery_copy`, `tracked_transfers_verify_content_record_history_and_keep_local_changes`; no sustituyen la cancelación ni integración de R5. | Pasó | Pendiente |
| SaveRestorePathsTest y GameEntrySaveTargetsTest | `resolves_nested_names_and_rejects_untrusted_paths`, `rejects_intermediate_and_final_symlinks_without_touching_saves`, `target_resolution_preserves_remote_names_and_skips_invalid_entries`, `rom_targets_prefer_exact_then_stem_then_legacy_and_resolve_types`; 8.3 y colisiones de restauración coordinada permanecen pendientes de plataforma e integración. | Pasó parcialmente | Pendiente |

### R4 — OAuth y Google Drive

- [x] Portar loopback, state, scope `drive.file`, revisión/CAS y refresh; invalidar respuestas obsoletas y preservar cambios concurrentes de biblioteca. Validación sintética en Linux; Windows pendiente.
- [x] Portar búsqueda, paginación, carpetas, descargas, snapshots y subidas reanudables; probar transporte y cancelación reales controlados. Validación sintética en Linux; Windows pendiente.
- [x] Mantener actualización condicional con ETag fuerte v2 e `If-Match`, sin fallback incondicional; probar dos escritores y fallos de precondición. Validación sintética en Linux; Windows pendiente y limitación de timeout de escritura documentada abajo.
- [x] **R4 completado técnicamente en Linux**; validación Windows nativa y CI remoto pendientes.

Evidencia parcial (2026-11-05, Linux, base `00d51b6`; sin commit):

- Se inició el adaptador de actualización condicional Drive v2: relee metadatos,
  verifica identidad, versión/hash y ETag fuerte, y envía el snapshot mediante
  `If-Match`; 404, 409 y 412 no provocan una escritura incondicional. El cliente
  acepta endpoints inyectados para pruebas aisladas. Aún no está integrado con el
  contrato de transferencia ni con supervisión de cancelación.
- `cargo test -p emusync-data-drive --offline`: dos pruebas nuevas aprobadas en
  Linux, con servidor HTTP loopback y datos sintéticos: dos escritores compiten
  sobre el mismo ETag y solo uno escribe; un ETag débil rechaza la actualización.
  La prueba no acredita el servicio real de Google ni el comportamiento de red
  ante cancelación o respuestas perdidas. `cargo xtask check` pasó arquitectura,
  formato, Clippy y pruebas del workspace en Linux; reutilizó compilaciones previas.
- Continuación sobre la misma base: intercambio/refresh OAuth con revisión y CAS
  del almacén local; loopback con state aleatorio, scope `drive.file`, rechazo de
  callbacks inválidos y cierre del socket al cancelar; listado paginado v3 con
  filtrado y fechas precisas; búsqueda/creación de carpetas y traslado. Estas piezas
  todavía no están compuestas en la aplicación ni constituyen el flujo OAuth completo.
- `cargo test -p emusync-data-drive --offline`: nueve pruebas aprobadas en Linux
  con HTTP loopback y archivos sintéticos, incluyendo conservación de biblioteca
  durante intercambio, respuesta de refresh obsoleta, invalid_grant, state incorrecto,
  liberación del listener, paginación y creación/movimiento de carpeta. No se utilizó
  Google Drive ni se probaron credenciales reales. El callback visual Kotlin y la
  composición con navegador y puerto de producción siguen pendientes.
- `cargo xtask check`: pasó nuevamente en Linux tras estos cambios; arquitectura,
  rustfmt, Clippy y pruebas Rust ejecutados. No se ejecutó Gradle en esta continuación.

| Garantía Kotlin | Prueba Rust y justificación | Linux | Windows |
| --- | --- | --- | --- |
| OAuthFlowTest: state, scope, cancelación y tokens | `loopback_ignores_wrong_state_and_accepts_matching_code`, `cancellation_releases_listener`, `composed_browser_callback_exchanges_and_persists_tokens`. | Pasó | Pendiente |
| OAuthCredentialStoreTest: revisión, CAS e invalid_grant | `refresh_response_is_discarded_after_disconnect_and_reconnect`, `exchange_keeps_concurrent_library_edits`, `invalid_grant_clears_only_current_refresh_token`, `refresh_response_is_discarded_after_generic_configuration_aba`. | Pasó | Pendiente |
| DriveSearchTest: búsqueda exacta y paginación | `listing_follows_pages_and_filters_folders_with_precise_timestamps`, `migration_listing_includes_folders_and_files_across_pages`, `exact_search_escapes_query_and_requests_latest_match`, `oversized_or_stalled_listing_pages_fail_without_partial_results`. | Pasó | Pendiente |
| DriveFoldersTest: carpetas, movimientos, errores y rechazos | `finds_root_creates_missing_child_and_moves_file`, `folder_lookup_is_scoped_and_missing_child_does_not_create_it`, `blank_folder_id_never_authorizes_further_mutations`, `unusable_folder_search_response_is_a_read_failure_without_creating`, `unusable_folder_create_response_is_unknown_and_never_retried`. | Pasó | Pendiente |
| ConditionalCloudUpdateTest: dos escritores y precondiciones | `simultaneous_writers_use_if_match_and_only_one_replaces_the_remote`, `missing_weak_wildcard_and_malformed_etags_never_authorize_update`, timeout tras cuerpo y cancelación. | Pasó | Pendiente |
| ResumableUploadTest: bloques, reanudación, consultas y timeout | 29 pruebas en `resumable.rs`: fragmentos de 8 MiB en rebanadas de 64 KiB, confirmación de bloques 308, respuesta prematura, sesión caducada, control de timeout tras entrega y no reenvío. | Pasó | Pendiente |
| DriveDownloadTest: streaming, stage y cancelación | `completed_download_replaces_destination`, `truncated_response_preserves_destination_and_cleans_stage`, `cancellation_of_stalled_download_preserves_destination_and_cleans_stage`. | Pasó | Pendiente |
| TrackedFileSyncTest y Recuperación (memoria y entre procesos) | `supervised_drive.rs` (14 pruebas), `recovery_store.rs` (7 pruebas), `persisted_recovery.rs` (4 pruebas): proceso hijo eliminado entre PUT y respuesta, bloqueo de save no resuelto (`ResourceBusy`), reconciliación por ID o nombre+hash sin repetir PUT. | Pasó | Pendiente |

- En ese punto faltaban búsqueda de archivos, descarga, subida reanudable, rollback,
  integración y trazabilidad; la continuación figura debajo.
  La ventana entre validación de rutas locales y escritura de R3 sigue sin resolver;
  tampoco se ha comprobado Windows nativo, 8.3 ni CI remoto. R4 no está completado.


#### Continuación R4 — búsqueda y descarga parcial

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Se añadió búsqueda v3 exacta por nombre, ordenada por modificación descendente,
  y consulta por ID. `exact_search_escapes_query_and_requests_latest_match` comprueba
  los parámetros enviados a un servidor HTTP sintético.
- `DriveDownload` transmite a un archivo temporal y publica solo al completar. Usa
  cancelación cooperativa: el propietario debe señalarla y esperar el futuro hasta
  que cierre el archivo y elimine el temporal. `completed_download_replaces_destination`,
  `truncated_response_preserves_destination_and_cleans_stage` y
  `cancellation_of_stalled_download_preserves_destination_and_cleans_stage` pasaron
  con sockets loopback y directorios temporales. La última espera a observar bytes
  parciales antes de cancelar. No prueba rollback remoto ni permite abortar el futuro.
- `cargo test -p emusync-data-drive --test download --locked`: tres pruebas aprobadas.
  `cargo xtask check`: arquitectura, formato, Clippy y pruebas Rust aprobados en Linux;
  el primer intento de Clippy detectó dos lecturas de prueba sin comprobar, corregidas
  antes del resultado final. No se ejecutó Gradle en esta continuación.
- La descarga aún no implementa el contrato `CloudSaveTransfer`, progreso ni ajuste
  de fecha; la seguridad frente a cambios del directorio después de validarlo sigue
  pendiente. También faltan subida reanudable, integración, supervisión de workers,
  cancelación/rollback de operaciones remotas y cierre de trazabilidad. No se probó
  Windows nativo, nombres 8.3, Google Drive real ni CI remoto. R4 sigue abierto.

#### Continuación R4 — subida reanudable parcial

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `ResumableUpload` inicia una sesión de creación v3, envía bloques de hasta 8 MiB,
  interpreta offsets confirmados, consulta el estado después de perder una respuesta
  y limita reintentos. Las escrituras sobre archivos existentes siguen reservadas al
  adaptador condicional. Se rechazan ubicaciones de sesión de otro origen: falta
  verificar que esta restricción sea compatible con todas las respuestas reales de Google.
- `cargo test -p emusync-data-drive --test resumable --locked`: tres pruebas aprobadas
  en Linux con servidor loopback y snapshot sintético: offset parcial, respuesta perdida
  seguida de consulta de estado, e interrupción de una petición en curso. En este último
  caso se informa que el resultado remoto es desconocido; no se afirma rollback ni se
  considera seguro volver a crear el archivo sin reconciliación. El adaptador aún no
  expone un mecanismo de reconciliación tras ese resultado.
- `cargo xtask check`: arquitectura, formato, Clippy y pruebas Rust aprobados en Linux;
  se reutilizaron compilaciones anteriores. No se ejecutó Gradle en esta continuación.
  Siguen pendientes integración en `CloudSaveTransfer`, progreso, timestamp, cierre
  supervisado, limpieza/rollback remoto y sus pruebas, junto con la seguridad de rutas
  de R3. No se comprobó Google real, Windows nativo, 8.3 ni CI remoto; R4 sigue abierto.

#### Continuación R4 — composición OAuth y recuperación de sesión

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `InteractiveOAuth` conecta `CloudAuthorization`, almacén CAS, receptor loopback y
  navegador inyectado; conserva puerto y redirect de Kotlin en la configuración de
  producción y reutiliza sus páginas HTML. `composed_browser_callback_exchanges_and_persists_tokens`
  aprobó el intercambio completo con navegador simulado, puerto efímero y token HTTP
  sintético. No se abrió un navegador real ni se probó Google. La composición aún no
  está conectada a desktop-app y la supervisión del worker del navegador sigue pendiente.
- Una subida interrumpida devuelve ahora una capacidad opaca de sesión; `reconcile`
  permite consultar el estado sin exponer la URL en Debug. La prueba de interrupción
  comprueba también la consulta posterior y una respuesta completada. Un estado parcial
  es solo una observación: no demuestra que la petición anterior haya terminado y no
  autoriza por sí solo una nueva creación. La sesión no se persiste entre procesos y
  sigue faltando integrar reconciliación con el propietario y cierre supervisado.
- Una respuesta de éxito truncada consulta la sesión antes de devolver resultado;
  `truncated_success_metadata_is_reconciled_before_returning` lo comprueba con HTTP
  local. `cargo test -p emusync-data-drive --test resumable --locked`: cuatro pruebas
  aprobadas; `cargo test -p emusync-data-drive --test oauth --locked`: cuatro aprobadas.
  `cargo xtask check`: arquitectura, formato, Clippy y pruebas Rust aprobados en Linux;
  compilaciones reutilizadas. No se ejecutó Gradle ni CI remoto en esta continuación.
- Siguen pendientes integración de transferencias, semántica de limpieza remota,
  supervisión de workers locales, seguridad de rutas de R3, trazabilidad completa y
  validación nativa Windows/8.3. R4 no está completado.

#### Continuación R4 — progreso y fechas de transferencia

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Descargas informan bytes recibidos, nombre y tamaño cuando está disponible; aplican
  la fecha remota tras publicar, ignorando fallos de fecha como Kotlin. Subidas
  reanudables informan offsets confirmados y finalización. `completed_download_replaces_destination`
  comprueba bytes, progreso y fecha; `resumes_from_acknowledged_offset_and_keeps_exact_bytes`
  comprueba progreso 3/6 y 6/6 junto con el contenido.
- `stale_revision_is_rejected_before_opening_snapshot_or_sending_put` confirma que
  una revisión remota distinta no abre el snapshot ni envía una escritura. Las pruebas
  usan servidores loopback y archivos sintéticos. `cargo test -p emusync-data-drive
  --test resumable --locked` y `cargo test -p emusync-data-drive --test conditional
  --locked` pasaron con cuatro y tres pruebas; `cargo xtask check` pasó arquitectura,
  formato, Clippy y pruebas Rust en Linux, reutilizando compilaciones previas.
- La actualización condicional aún no informa progreso. Permanecen pendientes
  composición de transferencias, dueño y cierre supervisado, resolución segura de
  resultados remotos inciertos y carrera de rutas de R3. No se ejecutaron Gradle,
  Google real, Windows nativo/8.3 ni CI remoto en esta continuación; R4 sigue abierto.

#### Continuación R4 — progreso condicional y fallos de sesión

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- La actualización condicional informa los bytes leídos hacia la petición, sin
  presentarlos como confirmación remota; la prueba de dos escritores comprueba
  también el evento de progreso. Se conserva `If-Match` y la ausencia de fallback.
- `expired_session_restarts_once_and_uploads_empty_snapshot` comprueba una sesión
  404 y un snapshot vacío; `invalid_server_range_is_rejected_without_resending_data`
  rechaza un offset fuera del archivo sin repetir escritura. Se usaron HTTP loopback
  y archivos sintéticos, no Google real.
- `cargo test -p emusync-data-drive --test conditional --offline`: tres pruebas
  aprobadas; `cargo test -p emusync-data-drive --test resumable --locked`: seis
  aprobadas. `cargo xtask check`: arquitectura, formato, Clippy y pruebas Rust
  aprobados en Linux; se reutilizaron compilaciones previas. No se ejecutó Gradle.
- R4 sigue abierto: faltan la composición de transferencias, supervisión y resolución
  segura del resultado remoto incierto, y la garantía de rutas de R3. No se validaron
  Windows nativo, 8.3 ni CI remoto.

#### Continuación R4 — sesión caducada con resultado incierto

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Cuando se pierde la respuesta de un bloque y la consulta posterior devuelve 404,
  no se inicia otra sesión: la petición anterior pudo crear ya el archivo. Se conserva
  la sesión opaca y se devuelve resultado desconocido. Una respuesta 404 directa del
  bloque mantiene el reinicio único probado anteriormente. Este tratamiento del 404
  tras un fallo de transporte es más conservador que el reintento Kotlin; aún falta
  integrar reconciliación y la decisión del propietario para completar la equivalencia.
- `expired_status_after_lost_response_does_not_create_a_duplicate` comprueba que no
  se envía una segunda creación; `cargo test -p emusync-data-drive --test resumable
  --locked` aprobó siete pruebas con HTTP loopback y datos sintéticos. `cargo xtask
  check` aprobó arquitectura, formato, Clippy y pruebas Rust en Linux, con compilaciones
  reutilizadas. Gradle, Google real, Windows/8.3 y CI remoto no se ejecutaron.
- Integración de transferencias, resolución de resultados inciertos, cancelación
  supervisada y protección de rutas de R3 siguen pendientes. R4 permanece abierto.

#### Continuación R4 — identidad y conservación de sesión

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- La sesión opaca conserva el nombre esperado: una respuesta de subida o consulta
  con nombre distinto o ID vacío mantiene el resultado incierto y no se acepta como
  archivo confirmado. Los errores al consultar estado, leer el snapshot tras iniciar
  sesión o procesar offsets conservan sesión y causa para intentar la reconciliación.
- `unexpected_upload_identity_retains_session_until_verified` y
  `status_failure_retains_session_for_later_reconciliation` pasaron con HTTP loopback
  sintético; `cargo test -p emusync-data-drive --test resumable --locked` pasó nueve
  casos. `cargo xtask check` aprobó arquitectura, formato, Clippy y pruebas Rust en
  Linux, con compilaciones reutilizadas. El primer Clippy detectó una variante de
  error grande; se redujo mediante indirección antes de la ejecución final.
- Las capacidades de sesión siguen solo en memoria y no resuelven por sí mismas una
  petición que aún pueda terminar. Faltan el propietario, la integración de transferencias,
  el cierre supervisado y la protección de rutas de R3. No se ejecutaron Gradle,
  Google real, Windows/8.3 ni CI remoto. R4 permanece abierto.

#### Continuación R4 — adaptación e integración local parcial

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `DriveTransfer` conecta descarga, creación reanudable y sustitución condicional
  al contrato `CloudSaveTransfer`; conserva progreso y convierte timestamps al dominio.
  Si una respuesta de creación o reemplazo ya exitosa tiene fecha inválida, el error
  conserva los metadatos remotos para que no se pierda la identidad escrita.
- `tracked_upload_uses_snapshot_and_records_server_revision` usa `TrackedFileSync`
  real, archivos temporales y HTTP sintético; comprueba bytes del snapshot, hash y
  revisión en historial. `cancelled_tracked_download_waits_for_staging_cleanup_and_keeps_save`
  cancela tras observar datos parciales y comprueba limpieza, partida e historial.
  `invalid_timestamp_after_success_retains_created_identity` comprueba el ID tras
  error de fecha. `cargo test -p emusync-data-drive --test tracked_integration --locked`:
  tres pruebas aprobadas en Linux.
- `cancellation_during_metadata_read_prevents_conditional_put` y
  `lost_conditional_write_response_is_an_unknown_outcome` comprueban cancelación antes
  del PUT y respuesta perdida después del envío. Un PUT iniciado se espera hasta su
  respuesta; su resultado perdido se informa como incierto. La señal y el inicio del PUT
  todavía no constituyen una barrera atómica; no se valida una cancelación coincidente.
  `cargo test -p emusync-data-drive --test conditional --locked`: cinco pruebas aprobadas.
- `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux, con
  compilaciones reutilizadas. No se ejecutó Gradle ni se utilizó Google real.
- La composición todavía no está conectada a desktop-app. El propietario aún debe
  conservar exclusión y snapshots ante resultado incierto, supervisar workers y esperar
  limpieza/rollback antes del cierre; estas pruebas no acreditan esa garantía. Permanece
  la carrera de rutas de R3; Windows nativo/8.3 y CI remoto no se ejecutaron. R4 sigue abierto.

#### Continuación R4 — propiedad del snapshot en fallos

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `TrackedFileSync` devuelve el snapshot junto al error de transferencia; cuando la
  escritura remota ya respondió con éxito y falla verificación o persistencia local,
  devuelve además el estado remoto. El snapshot vive mientras el error sea conservado;
  descartarlo todavía elimina el temporal. No hay persistencia de recuperación entre
  procesos y la caída o aborto del worker no quedan cubiertos por esta propiedad.
- `interrupted_upload_retains_snapshot_and_session_for_reconciliation` comprueba que
  una petición cancelada conserva sesión y bytes originales aunque la partida cambie.
  `local_edit_after_remote_commit_retains_original_snapshot_and_remote_identity`
  comprueba snapshot, identidad remota, historial y partida editada tras éxito remoto.
  Ambos usan transporte loopback y archivos sintéticos. `cargo test -p
  emusync-data-drive --test tracked_integration --locked`: cinco pruebas aprobadas.
- `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux con
  compilaciones reutilizadas; el primer Clippy detectó un error grande en el worker,
  corregido mediante indirección. Gradle, Google real, Windows/8.3 y CI remoto no se
  ejecutaron. Faltan el propietario de la exclusión, cierre supervisado, reconciliación
  completa y seguridad de rutas de R3; R4 sigue abierto.

#### Continuación R4 — supervisión parcial y protección de descargas

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `local_edit_during_download_is_preserved_and_not_recorded_as_synced` y
  `wrong_remote_checksum_does_not_replace_save_or_record_history` usan Drive HTTP
  loopback y persistencia local reales; comprueban partida, historial y temporales.
  `cargo test -p emusync-data-drive --test tracked_integration --locked` aprobó siete
  casos en esa ejecución.
- `OperationSupervisor`, situado en desktop-app por su propiedad de tareas y runtime,
  mantiene exclusión y error hasta resolución explícita, señala cancelación y espera el
  worker. Una espera de cierre interrumpida conserva el JoinHandle. Se captura un panic
  como fallo retenido. El propietario debe conservar el supervisor y completar su cierre;
  Drop y terminación del runtime no garantizan recuperación.
- `cargo test -p emusync-desktop-app --tests --locked`: tres pruebas focalizadas del
  supervisor, una integración con Drive y dos existentes del ejecutable aprobadas.
  La integración cancela una subida, retiene snapshot y exclusión y consulta una sesión
  completada antes de liberarlos. No acredita actualización del historial tras reconciliar,
  persistencia de sesión entre procesos ni rollback remoto. El primer intento de colocar
  el supervisor en application fue rechazado por el verificador de arquitectura; se movió
  a desktop-app, sin relajar la regla. `cargo xtask check` pasó arquitectura, formato,
  Clippy y pruebas Rust en Linux con compilaciones reutilizadas.
- El supervisor no está conectado al ciclo de vida real de la aplicación. Persisten la
  carrera de rutas de R3, reconciliación completa y cierre de runtime, pruebas Windows/8.3
  y CI remoto; Gradle y Google real no se ejecutaron. R4 sigue abierto.

#### Continuación R4 — historial tras reconciliación

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `record_reconciled_upload` verifica el snapshot retenido y el hash remoto cuando
  existe antes de registrar la revisión confirmada. Si la partida local cambió, conserva
  la edición y devuelve error después de registrar qué revisión se subió; el propietario
  mantiene la exclusión hasta resolverla. La evidencia de snapshot es una referencia al
  temporal: el propietario debe conservar el error/snapshot durante toda la operación.
- Se amplió `supervisor_retains_exclusion_and_snapshot_until_remote_reconciliation`:
  una sesión sintética completada se convierte al estado de dominio; un snapshot alterado
  rechaza el historial, una edición local se conserva y mantiene la exclusión, y la
  reconciliación válida registra ID/versión antes de liberar el snapshot. `cargo test
  -p emusync-desktop-app --test supervised_drive --locked`: una prueba aprobada en Linux.
- `cargo xtask check` aprobó arquitectura, formato, Clippy y pruebas Rust en Linux, con
  compilaciones reutilizadas. No se ejecutaron Gradle, Google real, Windows/8.3 ni CI
  remoto. El supervisor sigue sin conexión al ciclo de vida real; persisten la seguridad
  de rutas de R3, recuperación entre procesos y cierre supervisado. R4 permanece abierto.

#### Continuación R4 — resultado condicional incierto

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Un 5xx recibido después del PUT condicional se informa como resultado desconocido,
  sin repetir la escritura. Una respuesta de éxito con identidad inesperada conserva
  los metadatos recibidos para recuperación. El 404/409/412 conserva el conflicto y
  no hay fallback incondicional.
- `server_error_after_conditional_put_is_not_assumed_to_be_unwritten`,
  `unexpected_response_retains_reported_identity_for_recovery` y
  `cancellation_after_put_waits_for_the_remote_response` verifican esos casos con
  servidor loopback. La última mantiene la respuesta retenida y confirma que el futuro
  no termina hasta liberarla. `cargo test -p emusync-data-drive --test conditional
  --locked`: ocho pruebas aprobadas. `cargo xtask check` pasó arquitectura, formato,
  Clippy y pruebas Rust en Linux; compilaciones reutilizadas.
- La ventana actual es una vista previa que no inicia operaciones Drive. No se conectó
  un cierre que descartase resultados pendientes; sigue pendiente integrar supervisor,
  recuperación e historial en el ciclo de vida real, además de rutas seguras de R3.
  Gradle, Google real, Windows/8.3 y CI remoto no se ejecutaron; R4 sigue abierto.

#### Continuación R4 — límites HTTP y sesión caducada

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `DriveHttpPolicy` crea clientes separados: conexión de 3 s, metadatos con límite
  total de 15 s y lecturas de transferencia con 60 s de inactividad sin límite total. El constructor
  Google de `DriveTransfer` usa ambos: sesiones/consultas y lectura de metadatos usan
  el cliente de control; los cuerpos de transferencia usan el otro. Las redirecciones
  automáticas están desactivadas para no seguir un 308 como redirección de carga;
  esta política necesita comprobarse con las respuestas reales de Google. El timeout
  de lectura no cubre escrituras bloqueadas: ese caso sigue pendiente.
- `cargo test -p emusync-data-drive --test http_policy --locked`: tres pruebas con
  HTTP loopback e intervalos reducidos verificaron timeout de metadatos, transferencia
  activa más larga que ese límite y parada por inactividad. `chunk_uses_transfer_timeout_instead_of_short_control_timeout`
  verifica la selección de cliente en el adaptador reanudable; la suite `resumable`
  pasó once pruebas. Las demoras de prueba no representan los tiempos de producción.
- Una sesión caducada durante reconciliación devuelve `ExpiredOutcomeUnknown`, que no
  afirma ausencia de archivo; `expired_reconciliation_explicitly_preserves_uncertainty`
  lo comprueba. `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust
  en Linux con compilaciones reutilizadas. No se ejecutaron Gradle, Google real,
  Windows/8.3 ni CI remoto. R4 sigue abierto por integración del ciclo de vida,
  reconciliación entre procesos y seguridad de rutas de R3.

#### Continuación R4/R3 — directorio fijado en Linux

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Se añadió la pieza Linux `PinnedRemoteDestination`: abre el directorio seleccionado
  y cada descendiente mediante descriptores, rechaza enlaces en componentes no fiables
  y permite creación exclusiva sin seguir el destino final ni reemplazar archivos. El
  directorio raíz seleccionado puede ser un enlace, como en la resolución previa.
- `parent_swap_to_symlink_does_not_redirect_creation_outside_selected_directory`
  cambia un directorio intermedio por un enlace tras fijarlo y comprueba que la creación
  no llegue al directorio externo. `refuses_traversal_existing_files_and_untrusted_symlinks`
  comprueba traversal, enlaces y conservación de archivos existentes. `cargo test -p
  emusync-data-local --test pinned_path --offline`: dos pruebas aprobadas en Linux con
  directorios y bytes sintéticos. `cargo xtask check` pasó arquitectura, formato, Clippy
  y pruebas Rust en Linux con compilaciones reutilizadas.
- Esta pieza aún no está integrada con resolución de destinos, staging, backup ni
  reemplazo atómico; crear un archivo directamente no es una restauración completa y
  la carrera previa permanece en el flujo actual. No se ejecutaron Windows/8.3, Google
  real, Gradle ni CI remoto. R4 sigue abierto, igual que la aceptación de rutas de R3.

#### Continuación R4/R3 — staging fijado y descarga trazada Linux

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `StagedRestore::prepare_pinned` y `TrackedFileSync::download_pinned` conservan el
  descriptor del directorio durante descarga, verificación, backup y publicación.
  El backup lee el archivo mediante descriptor abierto sin seguir un enlace final,
  conserva el nombre y la ruta lógica originales en el formato existente y verifica
  su hash. Para destino ausente usa publicación sin reemplazo. La ruta fijada requiere
  directorios intermedios existentes y `/proc/self/fd`; no sustituye todavía al flujo
  genérico basado en rutas.
- Se verifica que la ruta del directorio siga apuntando al descriptor antes y después
  de publicar. Una sustitución detectada devuelve error y evita el checkpoint; el
  descriptor impide redirigir la escritura a un enlace externo. Las comprobaciones no
  son una transacción con el historial: permanece una ventana concurrente entre la
  última comprobación y el registro, y el dueño debe conservar exclusión. Tampoco se
  acredita atomicidad frente a otro proceso que modifique el archivo final.
- `cargo test -p emusync-data-local --test pinned_path --locked`: seis pruebas Linux,
  incluida sustitución de directorio o enlace final, creación concurrente y backup con
  ruta lógica. `cargo test -p emusync-data-drive --test tracked_integration --locked`:
  nueve pruebas, incluidas descarga HTTP sintética con backup e historial y rechazo de
  sustitución durante la transferencia sin tocar el destino externo ni registrar éxito.
  `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux con
  compilaciones reutilizadas. Clippy señaló un getter ambiguo y un error grande en el
  primer intento; se corrigieron antes del resultado final.
- Windows nativo/8.3 queda diferido por instrucción del usuario y no bloquea el avance;
  no se presenta como validado. No se ejecutaron Gradle, Google real ni CI remoto.
  El ciclo de vida real, integración del flujo genérico con esta capacidad y límites de
  la garantía concurrente siguen pendientes; R4 permanece abierto.

#### Continuación R4 — cuerpos de petición detenidos

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Subidas reanudables y condicionales observan el consumo del cuerpo por el cliente HTTP
  y limitan períodos sin avance mientras quedan bloques por entregar. Un vencimiento
  devuelve resultado incierto; la reanudable conserva su sesión. Cuando el cuerpo ya fue
  entregado al cliente, no se impone un límite total de petición; el cliente de producción
  conserva el límite de inactividad de lectura.
- Este indicador mide bloques consumidos por reqwest, no bytes escritos realmente por el
  kernel. Puede no detectar una detención después de que reqwest haya consumido todo el
  cuerpo y no acredita equivalencia completa con el timeout de socket CIO; ese límite
  sigue pendiente. No se considera una confirmación remota de los bytes consumidos.
- `stalled_request_body_times_out_with_recoverable_session` y
  `stalled_conditional_body_reports_unknown_remote_outcome` usan sockets loopback cuyo
  servidor deja de leer; las suites focalizadas pasaron doce y nueve pruebas. El caso
  `chunk_uses_transfer_timeout_instead_of_short_control_timeout` sigue pasando con una
  respuesta más lenta que el límite de control. `cargo xtask check` pasó arquitectura,
  formato, Clippy y pruebas Rust en Linux; compilaciones reutilizadas.
- No se ejecutaron Gradle, Google real ni CI remoto. Windows/8.3 permanece diferido.
  R4 sigue abierto por la limitación de timeout, el ciclo de vida real y los pendientes
  de seguridad de rutas y recuperación entre procesos.

#### Continuación R4 — recuperación supervisada con contexto original

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Los errores de subida retienen ruta local absoluta normalizada, carpeta, nombre remoto
  y observación previa, además del snapshot y el resultado/error remoto. La recuperación
  supervisada usa ese contexto para consultar una sesión reanudable o procesar un éxito
  remoto conocido, verificar el snapshot, registrar historial y solo entonces liberar
  la exclusión. Fallos, progreso incompleto y sesión caducada conservan la operación.
- `supervisor_retains_exclusion_and_snapshot_until_remote_reconciliation` usa HTTP
  loopback y almacenamiento sintético: comprueba respuestas incompleta y caducada,
  snapshot alterado, edición local y registro válido previo a liberar la exclusión.
  `cargo test -p emusync-desktop-app --test supervised_drive --locked` pasó; `cargo
  xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux con compilaciones
  reutilizadas. El servidor es controlado; sus respuestas no acreditan Google real.
- La recuperación automática cubre creación reanudable y errores locales posteriores
  a éxito; no cubre aún resultados inciertos del PUT condicional, vinculación de la
  recuperación a la revisión OAuth, ni persistencia entre procesos. El ciclo de vida de
  la aplicación real y la ventana concurrente de rutas de R3 siguen pendientes. No se
  ejecutaron Gradle, Google real ni CI remoto; Windows/8.3 queda diferido. R4 sigue abierto.

#### Continuación R4 — reconciliación condicional supervisada

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `reconcile_conditional_upload` consulta metadatos por el ID observado sin reenviar
  el PUT. Exige ID y nombre coherentes, tipo no carpeta y SHA-256 igual al snapshot
  retenido antes de registrar historial y liberar exclusión. Hash ausente o distinto
  mantiene el resultado sin confirmar y conserva snapshot y exclusión.
- `uncertain_conditional_write_reconciles_only_after_matching_hash` pierde la respuesta
  del PUT con `If-Match`, consulta un servidor loopback con hash ausente, distinto y
  finalmente coincidente, y comprueba bloqueo, partida e historial. `cargo test -p
  emusync-desktop-app --test supervised_drive --locked`: dos pruebas aprobadas.
  `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux con
  compilaciones reutilizadas. No se utilizó Google real ni se ejecutó Gradle o CI remoto.
- La recuperación aún no está vinculada atómicamente a la revisión OAuth ni conectada
  al ciclo de vida real. Persisten los límites de escritura de socket, recuperación
  entre procesos y carrera de rutas de R3. Windows/8.3 sigue diferido; R4 permanece abierto.

#### Continuación R4 — revisión OAuth durante commit y recuperación

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- OAuth puede devolver junto al token la revisión exacta comprobada. La composición
  supervisada recibe esa revisión antes de iniciar la operación, la conserva en el
  fallo y vuelve a comprobarla durante recuperación. `CredentialTransaction` serializa
  los cambios de conexión con la acción síncrona de commit; la espera se realiza en un
  worker, sin mantener el lock a través de una espera async. El llamador debe construir
  el guard con la revisión devuelta por OAuth y usar las rutas protegidas; la aplicación
  de vista previa aún no compone ese flujo.
- `upload_guarded` impide registrar el historial si la conexión cambió mientras se subía;
  conserva snapshot y resultado remoto para recuperación. En Linux,
  `download_pinned_guarded` comprueba la revisión y realiza staging/backup/publicación e
  historial bajo la misma exclusión de configuración, sin reemplazar la partida si la
  conexión cambió antes del commit. La garantía es en proceso: Kotlin y Rust no comparten locks.
- `cargo test -p emusync-data-local --test credential_transaction --locked`: dos
  pruebas aprobaron exclusión y rechazo de desconexión/reconexión ABA. `cargo test -p
  emusync-desktop-app --test supervised_drive --locked`: siete pruebas, incluyendo
  cambio de conexión durante subida exitosa, consulta condicional y descarga fijada;
  comprobaron partida, historial, snapshot y exclusión con servicios sintéticos.
  El callback OAuth sintético comprobó la revisión devuelta contra el almacén real.
- `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux, con
  compilaciones reutilizadas. `./gradlew check --no-daemon` pasó en 1 min 18 s: 111
  tareas, tres ejecutadas y 108 UP-TO-DATE; las suites Kotlin reutilizadas no son nuevas
  ejecuciones. No se utilizó Google real ni se ejecutó CI remoto; Windows/8.3 se difiere.
- R4 sigue abierto: falta integrar estas rutas al ciclo de vida real y persistir la
  recuperación entre procesos; las comprobaciones de rutas y del timeout de escritura
  conservan las limitaciones anotadas arriba.

#### Continuación R4 — propietario de runtime y cierre de vista previa

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `RuntimeOwner` posee Tokio y el supervisor; al cerrar rechaza nuevas operaciones,
  señala cancelación y devuelve estado pendiente hasta que termine el worker. Si hay
  fallo retenido, mantiene el runtime y devuelve recuperación requerida en vez de
  descartar el error. La ventana de vista previa consulta esos estados sin bloquear el
  hilo visual y solo sale al obtener `Ready`.
- `cargo test -p emusync-desktop-app --test runtime_owner --locked`: dos pruebas
  aprobaron espera de limpieza, bloqueo de nuevas operaciones, conservación de fallo
  y cierre tras resolverlo. `cargo xtask check` pasó arquitectura, formato, Clippy y
  pruebas Rust en Linux con compilaciones reutilizadas.
- `cargo build --locked -p emusync-desktop-app` pasó. En la sesión Wayland disponible,
  la vista previa se lanzó con `--config` en un directorio temporal aislado; se identificó
  su ventana por PID y se solicitó el cierre mediante Hyprland. La solicitud devolvió
  `ok`, el proceso terminó con código cero y stderr vacío. No había una operación Drive
  activa en esta prueba gráfica y no se utilizó Google real.
- La vista previa aún no inicia operaciones Drive ni presenta una acción de recuperación;
  `RuntimeOwner` todavía no ofrece ejecución asíncrona de recuperación desde el hilo
  visual. Falta conectar ese flujo y validar su cierre real con operación activa. Siguen
  pendientes la recuperación entre procesos y las limitaciones documentadas de rutas y
  timeout; Windows/8.3 permanece diferido. No se ejecutó Gradle ni CI remoto en esta
  continuación. R4 sigue abierto.

#### Continuación R4 — recuperación asíncrona durante cierre

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- `RuntimeOwner` puede ejecutar la recuperación fuera del hilo visual conservando el
  supervisor. Si se solicita cerrar mientras se recupera, el cierre espera; si la
  recuperación hace panic, conserva un estado que impide cerrar hasta reconocimiento
  explícito. El acceso al supervisor se rechaza mientras se cierra o se recupera.
- `cargo test -p emusync-desktop-app --test runtime_owner --locked`: cuatro pruebas
  aprobadas, incluidas recuperación asíncrona y panic. `runtime_close_waits_for_reconciliation_and_history_before_exit`
  integra Tokio, supervisor, snapshot, Drive HTTP loopback e historial: retiene una
  respuesta de estado, comprueba que el cierre sigue pendiente, y solo permite salir
  tras reconciliar y registrar la revisión. `cargo test -p emusync-desktop-app --test
  supervised_drive --locked`: ocho pruebas aprobadas. La vista previa todavía no expone
  una acción de producto para iniciar recuperación; la prueba invoca el propietario.
- `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux con
  compilaciones reutilizadas. El primer Clippy pidió simplificar un condicional; se
  corrigió antes del resultado final. `cargo build --locked -p emusync-desktop-app` y un
  nuevo cierre Wayland de la vista previa con `--config` temporal pasaron: Hyprland
  respondió `ok`, proceso cero y stderr vacío. No hubo operación Drive real en la ventana.
- R4 sigue abierto por el flujo de producto de recuperación, recuperación entre procesos
  y los límites documentados de rutas y timeout. Windows/8.3 continúa diferido; no se
  ejecutaron Gradle, Google real ni CI remoto en esta continuación.

#### Continuación R4 — confirmación de bloques y retirada del timeout aproximado

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- Ante un 308 que reconoce todos los bytes, la subida consulta el estado sin reenviar
  contenido; acepta solo una respuesta final coherente y conserva resultado incierto si
  todavía no hay confirmación. Dos regresiones loopback cubren ambas respuestas.
- La primera ejecución de `cargo xtask check` falló en
  `slow_active_upload_can_exceed_body_inactivity_interval`: el control basado en
  consumo de reqwest declaró incierta una transferencia que el servidor seguía leyendo.
  Se retiró ese control de cuerpos condicionales y reanudables para no interrumpir
  transferencias activas. La prueba lenta se conserva; las pruebas de socket retenido
  comprueban cancelación cooperativa reanudable y espera del PUT condicional. El timeout
  real de escritura de socket sigue pendiente; no se presenta como resuelto.
- `cargo test -p emusync-data-drive --test conditional --test resumable --offline` pasó
  nueve y quince casos con HTTP y archivos sintéticos. Después, `cargo xtask check`
  pasó arquitectura, formato, Clippy y pruebas Rust en Linux, con compilaciones
  reutilizadas. No se ejecutaron Gradle, Google real ni CI remoto en esta continuación.
- La [guía oficial de subidas Drive](https://developers.google.com/workspace/drive/api/guides/manage-uploads)
  muestra una URI de sesión en el mismo origen en su ejemplo, pero eso no demuestra que
  todas las respuestas reales cumplan la restricción del adaptador. Windows/8.3 permanece
  diferido. R4 sigue abierto por timeout de escritura, recuperación entre procesos,
  integración de producto y los límites de rutas documentados.

#### Continuación R4/R3 — creación de directorios fijados

Evidencia (2026-11-05, Linux, base `00d51b6`; sin commit):

- La preparación fijada crea la raíz seleccionada si falta y los directorios remotos
  descendientes mediante `mkdirat`/`openat` relativos al descriptor, reabriendo sin
  seguir enlaces incluso si otra operación creó el componente primero. La raíz elegida
  por el usuario puede ser un enlace, como en la referencia. Se valida el nombre
  remoto completo antes de crear directorios.
- `cargo test -p emusync-data-local --test pinned_path --locked`: nueve pruebas
  aprobaron, incluidas rutas anidadas ausentes, creación concurrente, raíz seleccionada
  mediante enlace y rechazo de traversal/enlaces. `cargo test -p emusync-data-drive
  --test tracked_integration --locked`: diez pruebas, incluida descarga HTTP sintética
  a una jerarquía nueva con checkpoint y sin backup innecesario. `cargo xtask check`
  pasó arquitectura, formato, Clippy y pruebas Rust en Linux; compilaciones reutilizadas.
- La fijación empieza al abrir la raíz seleccionada: no protege un cambio adversario de
  esa raíz antes de abrirla. Persisten las ventanas entre comprobación, publicación e
  historial y la integración al flujo de producto. No se ejecutaron Gradle, Google real
  ni CI remoto; Windows/8.3 continúa diferido. R4 sigue abierto.

#### Continuación R4 — operaciones de carpetas y resultado incierto

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- La búsqueda de rutas conserva su carácter de solo lectura; el renombrado y borrado
  explícito envían sus parámetros al endpoint v3. Ninguna búsqueda o migración elimina
  carpetas automáticamente. Una respuesta perdida o 5xx después de una mutación se
  informa como resultado incierto y no dispara una repetición automática de creación.
- `cargo test -p emusync-data-drive --test folders --locked`: cuatro pruebas aprobadas
  con HTTP loopback y datos sintéticos, incluyendo búsqueda con nombre escapado,
  carpeta ausente sin creación, rename/delete y respuesta perdida tras crear. Cubren
  las garantías principales de `DriveFoldersTest`; no comprueban el rollback de una
  migración de varios archivos, que corresponde al coordinador posterior.
- `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas Rust en Linux,
  con compilaciones reutilizadas. No se ejecutaron Gradle, Google real ni CI remoto
  en esta continuación. Windows/8.3 sigue diferido; R4 permanece abierto por los
  pendientes de integración, recuperación y rutas documentados.

#### Continuación R4 — bloques grandes y listados de migración

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- La subida reanudable rechaza un rango que reconoce bytes más allá del bloque
  enviado y conserva la sesión para recuperación; aceptarlo podría omitir bytes
  del snapshot. Un servidor sintético comprueba ese rechazo y otro comprueba bytes,
  rangos y progreso de un archivo que supera el bloque de 8 MiB.
- `cargo test -p emusync-data-drive --test resumable --test listing --locked` pasó
  17 pruebas de subida y tres de listado. La nueva prueba de listado comprueba que
  `list_children` incluye tanto carpetas como archivos a través de páginas, mientras
  la prueba existente verifica que `list_files` excluye carpetas. Datos y servicio
  HTTP son sintéticos y aislados.
- `cargo xtask check` pasó en Linux: verificador de arquitectura, rustfmt, Clippy
  con warnings como errores y pruebas del workspace; se reutilizaron compilaciones.
- Después de esa comprobación se añadió y ejecutó
  `cargo test -p emusync-data-drive --test resumable --locked repeated_zero_acknowledgements_stop_with_recoverable_session`:
  pasó en Linux y verificó el límite de reintentos sin progreso y la sesión retenida.
- Sigue sin resolverse el timeout real de escritura de socket, por lo que una
  sustitución condicional atascada puede impedir el cierre cooperativo. La recuperación
  entre procesos, la integración de producto y las ventanas de rutas antes descritas
  siguen abiertas. No se comprobó Google real, Windows/8.3 ni CI remoto.

#### Continuación R4 — cancelación y reconciliación de respuestas

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- Un PUT condicional cancelado espera hasta un segundo una respuesta; si sigue
  pendiente, informa resultado remoto desconocido. El supervisor conserva snapshot y
  exclusión hasta reconciliar: no se interpreta la cancelación como rollback. Se aplica
  también a la lectura incompleta de metadatos de éxito. La subida reanudable conserva
  la sesión si se cancela durante esa lectura.
- La recuperación por ID admite ahora fechas inválidas tras creación o sustitución:
  consulta metadatos y verifica identidad, nombre y checksum antes de registrar el
  historial; no repite la escritura. Las credenciales originales siguen protegiendo el
  commit del historial.
- Se separó el cliente de subida: en reqwest 0.12.28 `read_timeout` comienza antes de
  recibir cabeceras, incluso mientras se envía el cuerpo, y cortaba subidas activas
  largas. La prueba lenta usa ahora la política construida para producción. El cliente
  de subida no tiene timeout de inactividad de escritura ni de lectura de respuesta;
  sin cancelación una conexión detenida puede permanecer pendiente. Esto no constituye
  equivalencia completa con el timeout de socket Kotlin.
- `cargo test -p emusync-data-drive --test resumable --test conditional --test http_policy
  --locked` pasó 19, 11 y tres pruebas, respectivamente; `cargo test -p
  emusync-desktop-app --test supervised_drive --locked` pasó 11. Usaron archivos y HTTP
  loopback sintéticos. `cargo xtask check` pasó después en Linux: arquitectura,
  rustfmt, Clippy y pruebas de todo el workspace, con compilaciones reutilizadas.
  No se probaron Google real, Windows/8.3 ni CI remoto. La
  recuperación entre procesos y las limitaciones de rutas antes documentadas persisten.

#### Continuación R4 — lectura de respuestas de subida

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- Tras recibir las cabeceras de una subida, el cuerpo de metadatos usa un límite
  de inactividad que se reinicia por fragmento. Una respuesta incompleta de una
  sustitución condicional se conserva como resultado incierto; en una subida
  reanudable se consulta la sesión, sin reenviar el snapshot automáticamente.
  Esta comprobación no mide el avance de escritura de socket ni el tiempo hasta
  recibir las cabeceras: esos límites siguen sin equivalencia con Kotlin.
- `cargo test -p emusync-data-drive --test resumable --test conditional --test
  http_policy --locked` pasó 20, 12 y tres pruebas, incluidas respuestas detenidas;
  `cargo test -p emusync-desktop-app --test supervised_drive --locked` pasó 11.
  Después `cargo xtask check` pasó en Linux, incluida una prueba adicional en la que
  metadatos activos superan el intervalo de inactividad. HTTP y archivos fueron
  sintéticos. No se ejecutaron Google real, Windows/8.3 ni CI remoto.

#### Continuación R4/R3 — revisión de credenciales en escrituras generales

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- `ConfigManager::save` y `update` avanzan la revisión cuando cambian credenciales,
  además de las acciones explícitas de conexión. Una edición solo de biblioteca no
  invalida OAuth. Esto cierra un ABA dentro del proceso si las credenciales pasan por
  la API general y luego vuelven a su valor anterior; los procesos Kotlin y Rust siguen
  sin compartir locks ni revisión.
- `cargo test -p emusync-data-local --test credential_transaction --locked` pasó tres
  pruebas, y `cargo test -p emusync-data-drive --test oauth --locked` pasó cinco,
  incluida una respuesta de refresh atrasada durante un ABA de configuración general.
  Ambos utilizaron datos locales y HTTP sintéticos. No se comprobaron Google real,
  Windows/8.3 ni CI remoto. `cargo xtask check` pasó después en Linux con arquitectura,
  rustfmt, Clippy y pruebas del workspace; se reutilizaron compilaciones.
- `./gradlew qualityCheck check --no-daemon` pasó en Linux en 1m 13s: 111 tareas,
  tres ejecutadas y 108 `UP-TO-DATE`. Las pruebas Kotlin marcadas `UP-TO-DATE` son
  resultados reutilizados, no ejecuciones nuevas; los verificadores de hooks y
  convenciones sí se ejecutaron con sus proyectos temporales.

#### Continuación R4 — cancelación de consulta de sesión

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- La cancelación durante la consulta de estado de una subida reanudable conserva
  la sesión y devuelve resultado desconocido sin esperar todos los timeouts y
  reintentos de control. El propietario debe seguir reteniendo la exclusión hasta
  reconciliar; cancelar la consulta no demuestra el resultado de la escritura.
- `cargo test -p emusync-data-drive --test resumable --locked` pasó 21 pruebas,
  incluida una consulta detenida de un servidor loopback sintético. Después,
  `cargo xtask check` pasó en Linux con arquitectura, rustfmt, Clippy y pruebas del
  workspace, reutilizando compilaciones. No se ejecutaron Google real, Windows/8.3
  ni CI remoto.

#### Continuación R4 — sesión confirmada y revisión OAuth en recuperación

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- Una sesión reanudable confirmada con fecha ausente o inválida conserva su ID.
  `reconcile_tracked_upload_with_listing` permite consultar ese ID y exige nombre y
  checksum del snapshot antes de registrar historial y liberar exclusión. Un checksum
  distinto mantiene el resultado sin confirmar. No se repite la creación remota.
- Antes de consultar Drive para reconciliar se verifica la revisión OAuth capturada;
  el commit de historial la vuelve a verificar para detectar cambios durante la consulta.
- `cargo test -p emusync-data-drive --test tracked_integration --locked` pasó diez
  pruebas; `cargo test -p emusync-desktop-app --test supervised_drive --locked` pasó
  13. Los casos nuevos usan archivos temporales y servidores HTTP loopback sintéticos.
  `cargo xtask check` pasó después en Linux con arquitectura, rustfmt, Clippy y pruebas
  del workspace, reutilizando compilaciones.
  No se comprobó Google real, Windows/8.3 ni CI remoto. Continúa pendiente la
  equivalencia del timeout de escritura y espera de cabeceras, además de los límites
  de persistencia entre procesos y rutas documentados en continuaciones anteriores.

#### Continuación R3/R4 — fecha del archivo subido fijada al descriptor

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- En Unix la verificación del hash local y la escritura de su fecha tras subir o
  reconciliar usan el mismo descriptor abierto. Si la ruta se sustituye después de
  abrirla, la operación no escribe la fecha en el nuevo destino de esa ruta.
  La prueba `replacing_path_after_open_does_not_change_other_file_timestamp` cambia
  la ruta por un enlace y comprueba ambos archivos en Linux.
- `cargo test -p emusync-data-local --locked` y `cargo test -p emusync-data-drive
  --test tracked_integration --locked` pasaron. No se ha validado Windows: fuera de
  Unix se conserva la escritura por ruta anterior. La apertura inicial sigue pudiendo
  observar una ruta cambiada y esto no cierra las demás ventanas de publicación e
  historial ya registradas. Google real y CI remoto no se ejecutaron.
- La descarga establece la fecha sobre su archivo temporal abierto antes de publicarlo;
  después ya no reabre el destino por nombre para escribirla. `cargo test -p
  emusync-data-drive --test download --test tracked_integration --locked` pasó tres y
  diez pruebas con HTTP loopback y archivos temporales en Linux. Las pruebas comprueban
  fecha, reemplazo, cancelación y fallos; no reproducen una sustitución adversaria de la
  ruta durante la publicación ni validan Windows. `cargo xtask check` pasó después en
  Linux con arquitectura, rustfmt, Clippy y pruebas del workspace; compilaciones reutilizadas.

#### Continuación R4 — confirmación prematura y carpeta de recuperación

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- La subida reanudable rechaza una respuesta de finalización recibida antes de enviar
  el último bloque. Conserva la sesión y el ID comunicado para investigar el resultado,
  sin volver a crear el archivo. Una recuperación de sesión incierta exige checksum
  coincidente antes de registrar historial; un checksum ausente permanece sin confirmar.
- Las recuperaciones que consultan un ID comprueban también su carpeta padre, además
  de ID, nombre, tipo y checksum. La prueba sintética de extremo a extremo conserva la
  exclusión ante carpeta incorrecta y checksum incorrecto, y solo registra el historial
  tras una respuesta coincidente. La consulta de sesión pide también `parents` y, si
  falta el padre o el checksum, mantiene el resultado sin confirmar o consulta el ID.
- `cargo test -p emusync-data-drive --test resumable --locked` pasó 22 pruebas;
  `cargo test -p emusync-desktop-app --test supervised_drive --locked` pasó 14. Se
  usaron archivos temporales y HTTP loopback. `cargo xtask check` pasó después en
  Linux con arquitectura, rustfmt, Clippy y pruebas del workspace; se reutilizaron
  compilaciones. No se comprobaron Google real,
  Windows/8.3 ni CI remoto. Siguen pendientes los límites de timeout, recuperación
  entre procesos y rutas ya documentados.

#### Continuación R4 — límites de origen y redirección de sesión

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- Las URL de sesión reanudable se rechazan si cambian de origen o incluyen
  credenciales incrustadas o fragmento. Las políticas de cliente de producción no
  siguen redirecciones. Dos listeners loopback verificaron que ni un `Location` de
  otro origen ni una respuesta 307 a un PUT de sesión contactan al destino alternativo;
  otra prueba cubrió userinfo y fragmento. La respuesta 307 conserva la sesión para
  recuperación, sin reenviar automáticamente el snapshot.
- `cargo test -p emusync-data-drive --test resumable --locked` pasó 25 pruebas en
  Linux con datos y servicios sintéticos. `cargo xtask check` pasó después en Linux
  con arquitectura, rustfmt, Clippy y pruebas del workspace; se reutilizaron compilaciones.
  La compatibilidad de la restricción de origen
  con todas las respuestas reales de Google sigue sin comprobarse: el ejemplo público
  citado anteriormente no acredita todos los posibles orígenes. No se ejecutaron
  Google real, Windows/8.3 ni CI remoto. El timeout de escritura sigue pendiente.

#### Continuación R4 — límite TCP Linux y metadatos acotados

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- El cliente de subida configura `TCP_USER_TIMEOUT` mediante reqwest en Linux. El
  kernel puede cerrar conexiones con datos sin confirmar o bloqueados por una ventana
  TCP cero; una escritura condicional devuelve resultado incierto y una subida
  reanudable consulta su sesión y la conserva si no puede confirmar el resultado.
  Esto **no equivale** al timeout de socket Kotlin: no limita la espera silenciosa de
  cabeceras una vez confirmados los datos, ni demuestra el comportamiento al otro lado
  de un proxy. Windows sigue diferido.
- El cliente de subida fuerza HTTP/1.1 porque el límite TCP pertenece a la conexión:
  no debe compartirse entre varios streams HTTP/2. La prueba TLS sintética ofrece
  ambos protocolos y confirma la negociación HTTP/1.1; no prueba Google real.
  Tras ese cambio, `cargo xtask check` volvió a pasar en Linux; la prueba TLS, ignorada
  por defecto, se ejecutó explícitamente como se indica abajo.
- Pasaron `linux_tcp_timeout_stops_stalled_conditional_body_without_cancellation`,
  `linux_tcp_timeout_queries_stalled_upload_and_retains_unknown_session` y
  `slow_active_upload_can_exceed_body_inactivity_interval` con sockets loopback.
  `cargo test -p emusync-data-drive --test conditional --locked
  linux_tcp_timeout_distinguishes_stalled_and_active_tls_uploads -- --ignored` pasó
  explícitamente: genera un certificado temporal con el ejecutable local `openssl`,
  y comprueba tanto un receptor TLS detenido como una transferencia TLS lenta activa.
  Esta prueba es ignorada en la suite automática porque requiere ese ejecutable; no se
  guardó ninguna clave en el repositorio ni se ejecutó Google real o CI remoto.
- La lectura de metadatos de éxito está acotada a 1 MiB. Una respuesta mayor tras un
  PUT condicional conserva el resultado incierto; en una sesión reanudable provoca
  consulta de estado sin reenviar el snapshot. `cargo test -p emusync-data-drive --test
  conditional --test resumable --locked` pasó 14 y 26 pruebas antes de añadir las dos
  pruebas TCP específicas anteriores, con archivos y servicios sintéticos.
- `linux_tcp_timeout_does_not_bound_silent_response_after_body_delivery` reprodujo
  explícitamente el caso pendiente: tras recibir todos los bytes, un servidor
  loopback callado mantiene activo el PUT más allá del límite TCP hasta que se
  solicita cancelación; entonces se conserva el resultado incierto. Pasó ejecutada
  individualmente en Linux. `cargo xtask check` pasó después con arquitectura,
  rustfmt, Clippy y pruebas del workspace; compilaciones reutilizadas y la prueba TLS
  indicada como ignorada en la ejecución automática.

#### Continuación R4 — reintentos implícitos de transporte

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- Los clientes de `DriveHttpPolicy` desactivan explícitamente los reintentos internos
  de reqwest, además de las redirecciones. Los reintentos y las consultas de sesión
  quedan bajo control del adaptador; los constructores con clientes inyectados
  documentan la misma obligación para sus llamadores.
- `cargo test -p emusync-data-drive --test conditional --test folders --locked` pasó
  16 y cuatro pruebas en Linux. Las pruebas de respuesta perdida usan ahora los clientes
  de producción y comprueban que no se envía otra mutación; la prueba TLS opcional fue
  ignorada en esa ejecución. No se ha reproducido un NACK HTTP/2: la configuración
  actual de la subida usa HTTP/1.1 y la garantía adicional se basa en la política
  explícita de reqwest. No se ejecutaron Google real, Windows/8.3 ni CI remoto.

#### Continuación R4 — identidades de carpeta y precondiciones

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- Una carpeta encontrada sin ID utilizable se rechaza antes de realizar mutaciones.
  Si la creación devuelve éxito sin ID utilizable, se informa resultado incierto y
  no se crea otra carpeta automáticamente. Una prueba con HTTP loopback comprueba
  ambos casos y que no hay una petición posterior.
- Los listados también rechazan IDs vacíos o de solo espacios. En subidas
  reanudables un ID así mantiene el resultado incierto y la sesión retenida hasta
  obtener una identidad válida. `cargo test -p emusync-data-drive --test listing
  --test resumable --locked` pasó cuatro y 27 pruebas, y `cargo test -p
  emusync-desktop-app --test supervised_drive --locked` pasó 14, en Linux con datos
  y servidores sintéticos.
- La regresión de ETag fuerte cubre ahora el campo ausente, vacío, comodín, débil,
  sin comillas y con retorno de carro, sin abrir el snapshot ni enviar PUT. Esto
  amplía la trazabilidad de `ConditionalCloudUpdateTest` frente al caso débil previo.
- `cargo test -p emusync-data-drive --test conditional --test folders --locked` pasó
  16 y cinco pruebas en Linux; la prueba TLS opcional quedó ignorada en esa ejecución.
  La comprobación del campo ETag realmente ausente se añadió después de esa ejecución;
  `cargo xtask check` pasó luego con arquitectura, rustfmt, Clippy y pruebas del
  workspace, con compilaciones reutilizadas y la prueba TLS ignorada por defecto.
  Servicios y archivos fueron sintéticos; no se ejecutaron Google real, Windows/8.3
  ni CI remoto. La espera silenciosa de cabeceras sigue pendiente.

#### Continuación R4 — cancelación antes del PUT condicional

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- El GET de metadatos previo al PUT condicional se puede cancelar mientras espera
  cabeceras o cuerpo, sin esperar la respuesta del servidor. Esta fase no ha enviado
  una mutación; las pruebas comprueban que no llega a enviarse el PUT.
- Los metadatos de ese GET se leen con el mismo límite de tamaño y tiempo entre
  fragmentos que los metadatos de subida. Una respuesta excesiva produce un error
  distinguible antes de abrir el snapshot; no se presenta como escritura incierta.
- `cargo test -p emusync-data-drive --test conditional --locked` pasó 18 pruebas en
  Linux y dejó una TLS ignorada por defecto. La prueba TLS se ejecutó después
  explícitamente con `--ignored` y pasó; utilizó un certificado efímero generado con
  OpenSSL local. `cargo xtask check` pasó después en Linux con arquitectura, rustfmt,
  Clippy y pruebas del workspace, reutilizando compilaciones; la prueba TLS quedó
  ignorada en esa ejecución automática. No se comprobaron Google real, Windows/8.3 ni CI remoto. La espera
  silenciosa de cabeceras después de enviar el PUT sigue pendiente.

#### Continuación R4 — respuesta de subidas vacías

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- La política de producción dispone de un cliente separado para PUT de longitud cero,
  con límite de espera de cabeceras. No puede cortar un cuerpo activo porque no hay
  bytes que transmitir. `DriveTransfer::with_clients` y `DriveTransfer::google` lo
  conectan tanto a creación reanudable como a sustitución condicional. Los
  constructores que aceptan un único cliente inyectado conservan ese cliente y no
  implican esta política por sí solos.
- Con servidor loopback que recibe el PUT vacío y guarda silencio, la sustitución
  conserva resultado incierto; la creación consulta la sesión y la conserva cuando
  esta ya no puede confirmarse. La integración con historial retiene el snapshot y
  no registra una revisión no confirmada. Otra prueba comprueba éxito y reinicio
  de sesión vacía con clientes de producción.
- `cargo test -p emusync-data-drive --test conditional --test resumable --test
  tracked_integration --locked` pasó 19, 28 y 11 pruebas en Linux; la prueba TLS
  opcional quedó ignorada en esa ejecución. Datos y servicio fueron sintéticos.
  El caso equivalente para cuerpos no vacíos aún no está resuelto: iniciar un plazo
  al terminar de leer el snapshot puede cortar una transferencia que sigue activa.
  No se ejecutaron Google real, Windows/8.3 ni CI remoto.

#### Continuación R4 — respuesta tras el cuerpo de subidas no vacías

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- El cuerpo de subida se envuelve para observar cuándo se entrega el último byte
  declarado al transporte (hyper deja de consultar el flujo al alcanzar la longitud
  declarada, por lo que no se usa el fin del flujo). Solo entonces empieza un plazo
  (`response_inactivity`, 60 s por defecto) para recibir cabeceras; el tiempo de
  producción o transmisión del cuerpo no tiene límite adicional. Las subidas
  reanudables entregan cada fragmento en porciones de 64 KiB para que la señal no se
  adelante mientras un fragmento grande sigue en cola.
- Sustitución condicional: expirar el plazo devuelve la nueva
  `ConditionalError::HeadersTimedOut`, tratada como resultado incierto (la recuperación
  de `drive_recovery` la conserva igual que `OutcomeUnknown`). Subida reanudable: la
  expiración se trata como respuesta perdida y consulta la sesión, que conserva la sesión
  si ya no puede confirmarse. Se añade la dependencia directa `bytes`.
- Limitación: un enlace tan lento que no vacíe el búfer del transporte en el plazo tras
  entregar el último byte produce un resultado incierto espurio (se conserva el
  snapshot; no hay pérdida ni escritura repetida). Ajustable con
  `with_response_inactivity`.
- `cargo test -p emusync-data-drive --test conditional --test resumable --test
  tracked_integration --locked` pasó 20, 29 y 11 pruebas en Linux, incluidas dos nuevas
  con servidor loopback silencioso tras recibir el cuerpo. La prueba TLS ignorada
  (subida activa lenta frente a estancada) se ejecutó con `--ignored` y pasó. Datos y
  servicio sintéticos. No se ejecutaron Google real, Windows/8.3 ni CI remoto.

#### Continuación R4 — respuestas acotadas de listados y carpetas

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- Búsqueda, paginación, obtención por ID y búsqueda/creación de carpetas leían el cuerpo
  con `Response::json()`, sin límite de tamaño ni de silencio entre fragmentos. Ahora
  usan el mismo lector acotado que las subidas (1 MiB y `response_inactivity`, 60 s por
  defecto, ajustable con `with_response_inactivity` en `DriveListing` y `DriveFolders`).
- Lecturas: un cuerpo excesivo, detenido o inválido produce `ListingError::Metadata` o
  `FolderError::Metadata`; un listado no devuelve resultados parciales. La creación de
  carpeta, que es una mutación, produce `FolderError::MutationResponseUnknown`
  (resultado incierto, sin reintento automático ni creación adicional).
- Pruebas nuevas con servidor loopback sintético: página de listado excesiva y detenida,
  búsqueda de carpeta excesiva sin crear, y respuesta de creación excesiva y detenida.
  `cargo test -p emusync-data-drive --test listing --test folders --locked` pasó 5 y 7
  pruebas. La búsqueda por nombre exacta ya estaba cubierta por
  `exact_search_escapes_query_and_requests_latest_match`.
- No se ejecutaron Google real, Windows/8.3 ni CI remoto. R4 sigue abierto por la
  recuperación entre procesos y el flujo de producto de recuperación.

#### Propuesta R4 — recuperación entre procesos (pendiente de aprobación; sin implementar)

Estado (2026-11-06): propuesta de diseño solicitada por el usuario; ninguna parte está
implementada ni aceptada. No forma parte de la paridad Kotlin.

Punto de partida verificado:

- Kotlin conserva sesión y resultado incierto solo en memoria (`ResumableDriveUpload.kt`);
  un reinicio los pierde. Rust replica ese contrato: `OperationSupervisor` retiene el fallo,
  el `UploadSnapshot` (directorio temporal que se borra al soltarse), `UploadContext` y
  `UploadSession` hasta reconciliar.
- La URL de `UploadSession` es una capacidad equivalente a una credencial (su `Debug` ya
  la oculta). No debe aparecer en registros ni en JSON legible.
- `data-local` ya dispone de `atomic_write`, `lock_for`, `restrict_file` y
  `restrict_directory` (usados por historial) y `CredentialSnapshot`/revisión OAuth.

Contenido que habría que conservar tras un reinicio: copia del snapshot, su SHA-256,
`UploadContext` (ruta local, carpeta, nombre remoto, estado observado), tipo de
incertidumbre (sesión reanudable, PUT condicional, creación con marca de tiempo inválida o
`AfterUpload`), identidad de la conexión (revisión OAuth) y, solo para sesiones
reanudables, la URL de sesión.

Diseño propuesto:

1. **Ubicación y formato:** directorio `recovery/` junto al archivo de configuración, con
   permisos 0700 y un manifiesto `v1` por operación escrito con `atomic_write` y 0600. La
   copia del snapshot vive dentro del mismo directorio, no en `/tmp`. Versión de formato
   explícita; una versión desconocida se conserva intacta y no se interpreta.
2. **URL de sesión:** no se guarda en claro; se cifra con una clave en el almacén de
   secretos del sistema si existe. Sin almacén seguro, la URL no se persiste y la
   recuperación solo puede reconciliar por ID/checksum (PUT condicional, creación
   registrada) o descartar la sesión como incierta; nunca se reinicia una subida a ciegas.
3. **Exclusión entre procesos:** bloqueo de archivo exclusivo por directorio de recuperación
   (el lock en memoria de `lock_for` no cruza procesos). Mientras haya manifiesto sin
   resolver, la aplicación no inicia nuevas escrituras sobre ese juego y presenta la
   recuperación. Kotlin y Rust siguen sin ejecutarse sobre los mismos datos a la vez.
4. **Orden de escritura:** persistir snapshot y manifiesto (fsync) antes de enviar el primer
   PUT; reemplazar el manifiesto con atomicidad cuando cambie el estado de incertidumbre;
   borrarlo solo después de registrar el historial reconciliado (`record_reconciled_upload_guarded`).
   Un manifiesto sin snapshot, un digest que no coincide o una revisión OAuth cambiada se
   tratan como no verificables y se conservan para inspección, sin borrado automático.
5. **Reanudación:** al arrancar, cada manifiesto pasa por la misma reconciliación que hoy
   (`reconcile_*`), sin repetir escrituras. `Incomplete`, `ExpiredOutcomeUnknown` y
   `RemoteUnconfirmed` mantienen el manifiesto.
6. **Limpieza:** los manifiestos resueltos se eliminan con el snapshot; los no resueltos no
   caducan solos. Se ofrece una acción explícita de descarte con confirmación.

Pruebas necesarias (datos y servicios sintéticos): corte entre PUT y respuesta con proceso
real reiniciado; manifiesto corrupto, versión desconocida y snapshot alterado; dos procesos
compitiendo por el bloqueo; revisión OAuth cambiada; permisos 0600/0700 en Linux; ausencia
de la URL de sesión en registros y en disco legible; todas las rutas sin reenvío de PUT.
Windows requiere su propia validación nativa (ACL, bloqueo y almacén de secretos), diferida.

Decisiones del usuario (2026-11-06):

- Se acepta añadir la función aunque Kotlin no la tenga: añade un directorio y formato
  nuevos y no modifica los existentes.
- Almacén de secretos: delegado en el criterio del implementador. Decisión: **no se
  persiste la URL de sesión** y no se añade almacén de secretos. Motivos: la URL es una
  capacidad equivalente a una credencial; no hay almacén verificable en todos los entornos
  (Hyprland/Wayland sin servicio de secretos, Windows sin validación nativa); y evita una
  dependencia específica de plataforma. Consecuencia: tras un reinicio una sesión
  reanudable no se consulta ni se continúa; se reconcilia por identidad remota o, si no
  se conoce, por carpeta, nombre y checksum, y si no se puede confirmar queda sin resolver.
  El punto 2 del diseño queda sustituido por esta decisión.
- El descarte manual desde interfaz pertenece a R7. R4 solo ofrece `RecoveryStore::remove`.

#### Continuación R4 — almacén de recuperación persistente (primera parte)

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- `data-local` incorpora `RecoveryStore`, un directorio `<config>.sync-recovery/` (mismo
  patrón que `.sync-state.json` y `.sync-backups`), 0700, con una subcarpeta `op-*` por
  operación: `snapshot` (0600, comprobado por SHA-256 tras copiar) y `manifest.json`
  versión 1 (0600, escritura atómica, máximo 64 KiB). Un bloqueo de archivo exclusivo
  (`File::lock` de la biblioteca estándar, sin dependencias nuevas) serializa
  `stage`, `list` y `remove` entre procesos. No se guarda la URL de sesión.
- `list` informa de entradas no verificables (manifiesto ilegible, versión desconocida,
  snapshot ausente o con checksum distinto) sin repararlas ni borrarlas. `remove` solo
  acepta identificadores `op-*` y rechaza separadores y `..`. Un directorio de
  recuperación que sea un enlace simbólico se rechaza.
- `cargo test -p emusync-data-local --test recovery_store --locked` pasó 7 pruebas en
  Linux: persistencia visible desde un nuevo almacén tras soltar el snapshot, permisos
  0700/0600, entradas manipuladas conservadas byte a byte, identificadores, rechazo de un
  registro de otro contenido sin dejar residuos, exclusión mientras otro manipulador
  retiene el bloqueo (otro descriptor sobre el mismo archivo, no otro proceso real) y
  enlace simbólico.
#### Continuación R4 — recuperación entre procesos (segunda parte y cierre técnico en Linux)

Evidencia (2026-11-06, Linux, base `00d51b6`; sin commit):

- `TrackedFileSync` incorpora `upload_guarded_classified`:
  - Persiste snapshot y manifiesto v1 en `RecoveryStore` antes de enviar el primer PUT
    (fail-closed: si falla la persistencia, no se emite ninguna petición de escritura).
  - Bloquea cualquier subida concurrente o posterior sobre el mismo save mientras exista
    una entrada sin resolver (`has_unresolved` devuelve `io::ErrorKind::ResourceBusy`).
  - Al completar la subida y asentar el checkpoint en el historial, elimina la entrada
    persistida (`resolve_recovery`).
  - Si el fallo es definitivo (`TransferError::leaves_remote_unchanged`: cancelación
    antes de mutación, precondición fallida, 4xx cliente), descarta la entrada; ante
    resultado incierto (timeout, error 5xx, caída de red), conserva la entrada para
    reconciliación.
- `reconcile_persisted_uploads` reconcilia las operaciones que sobrevivieron al fin del
  proceso:
  - Valida la identidad de conexión (`fingerprint` SHA-256 de credenciales); entradas
    de otra conexión no se tocan ni se eliminan (`ConnectionChanged`).
  - Reconcilia por ID remoto en la carpeta esperada, o por nombre y checksum SHA-256
    cuando no hay ID previo (creación).
  - No reenvía ninguna petición PUT: solo consulta metadatos de confirmación. Al
    verificar que el hash remoto coincide con el digest del snapshot recuperado, registra
    el historial con `UploadEvidence::recovered` y borra la entrada del almacén.
- Pruebas en `rust/desktop-app/tests/persisted_recovery.rs`:
  - `killed_process_leaves_an_upload_that_a_new_process_reconciles_without_resending`:
    ejecuta un proceso hijo real que inicia la subida y se congela esperando la
    respuesta del servidor; el proceso padre mata al hijo con `SIGKILL` (sin destructores
    ni callbacks de limpieza); un nuevo proceso comprueba que el snapshot y la entrada
    siguen presentes, que un intento de escritura sobre la misma partida se rechaza con
    `ResourceBusy` sin emitir peticiones, y reconcilia con éxito verificando el hash y
    grabando el historial sin reenviar el PUT.
  - `entry_from_another_connection_is_neither_used_nor_removed`: conexión ajena conservada.
  - `created_upload_without_identity_reconciles_only_by_name_and_checksum`: discriminación
    estricta por checksum.
  - `entries_follow_the_remote_outcome_of_each_upload`: éxito y precondición fallida limpian
    la entrada; respuesta perdida la conserva.
- `cargo xtask check` pasó arquitectura, formato, Clippy (`-D warnings`) y la totalidad
  de las pruebas del workspace en Linux.
- `./gradlew check --no-daemon` pasó completamente en Linux (111 tareas accionables, 3
  ejecutadas, 108 up-to-date; hooks y convenciones verificados).
- **R4 completado técnicamente en Linux.** Validación nativa Windows/8.3 diferida por decisión
  del usuario (2026-11-05); pruebas con Google Drive real y CI remoto siguen pendientes.

### R5 — Casos de uso y sincronización

- [x] Portar biblioteca y ciclo pre-sync → proceso → post-sync, juego offline, conflictos y reintentos identificados por solicitud.
- [x] Verificar operaciones exclusivas, invalidación de resultados antiguos y preservación de partidas modificadas durante transferencias.
- [ ] Cancelación cooperativa: esperar limpieza y rollback remoto asíncrono antes de liberar exclusión o cerrar runtime; `Drop` y abortar futuros no reemplazan ese rollback. Falta verificar el flujo completo del orquestador y conservar los errores de transferencia para recuperación.
- [ ] **R5 completado técnicamente en Linux**; la revisión R7/R8 detectó un problema de propagación de errores en la integración. Windows nativo y CI remoto diferidos.

Evidencia (2026-11-06, Linux, base `9165b4c`; sin commit):

- Se implementó completamente la capa de casos de uso en el crate `emusync-application`, cumpliendo estrictamente con la regla de arquitectura del workspace (dependencia de producción únicamente hacia `emusync-domain = { path = "../domain" }`).
- Módulos desarrollados y verificados:
  - `operation`: `OperationCoordinator` con exclusión mutua atómica (`run_if_idle`), control de concurrencia, guard drop-safe para cancelación cooperativa y notificación de eventos (`Started`, `Cancelled`, `Finished`).
  - `connection`: `CloudConnection` y `ConnectionResult` para sincronización con almacén de configuración y preservación de credenciales.
  - `update`: `UpdateCoordinator` con máquina de estados (`Idle`, `Checking`, `Available`, `Downloading`, `ReadyToRestart`, `Error`), exclusión de descarga activa, limpieza del archivo de destino ante fallo o cancelación y reinicio bloqueado durante operaciones activas.
  - `library`:
    - `entry_sync_checks`: `EntrySyncChecks` y `SyncCheckRequest` con matriz de invalidación de frescura ante cambio de revisión de operaciones, estado ocupado, cambios de credenciales en configuración, eliminación/edición de la entrada o solicitud de chequeo más reciente.
    - `entry_edits`: `preserve_shared_cloud_root`, `build_game_entry` y `GameOverrideChanges`.
    - `edit_library`: `EditLibrary` y `EntryEditResult` con reordenación preservando entradas no listadas, altas, bajas, edición con renombrado remoto y preservación de carpetas compartidas.
    - `load_library`: `LoadLibrary` con migración de accesos directos Steam hacia `steam_app_id`.
    - `steam_registration`: `SteamRegistration` para registro y desregistro en Steam sincronizado con la configuración.
  - `sync`:
    - `channel`: canal oneshot seguro con primitivas de la biblioteca estándar (`std::sync::Mutex`, `std::task::Waker`).
    - `target_resolver`: `SaveTargetResolver` para resolución de carpetas y segmentos remotos.
    - `target_mapping`: `SaveTargetMapping` con validación de segmentos de ruta en la nube (rechazo estricto de traversal `..`, `.` y absolutos), detección de colisiones remotas a locales, detección de ancestro/descendiente conflictivo, reserva de nombres de hermanos entre archivos y directorios y detección de duplicados en carpetas legacy.
    - `migrator`: `CloudFolderMigrator` con detección previa de duplicados en la nube (`DuplicateNames`), fusión de carpetas sin borrar origen si quedan archivos, rollback en orden inverso ante fallo en la transferencia y cancelación cooperativa con rollback inverso.
    - `context`: `SyncContext` y `SyncServices` con resolución de conflictos y reintentos de post-sincronización identificados por `RequestId`.
    - `manual`: `ManualFileSync` y `ManualEntrySync` aplicando decisiones de sincronización manual (`DownloadCloud`, `UploadLocal`, `InSync`).
    - `migration`: `CloudEntryMigration` para traslados y renombrados remotos de categorías y juegos.
    - `status`: `SyncStatusChecker` para observación sin avanzar historial.
    - `post_game`: `PostSaveSync` y `PostGameSync` con sincronización por lotes tras jugar, subida de archivos creados o modificados y registro de fallos parciales con `RequestId` único para reintentos dirigidos.
    - `lifecycle`: `GameSyncLifecycle` con ciclo completo pre-sync → ejecución → post-sync, autorización previa con fallo de transporte tolerado para juego offline, y aborto estricto del inicio ante errores de mapeo/rutas.
    - `orchestrator`: `SyncOrchestrator` y `SyncOrchestratorFactory` implementando los contratos de dominio `SaveSynchronizer` y `SaveSynchronizerFactory`.
- Suite de pruebas de `emusync-application` (24 pruebas aprobadas en 7 archivos en `rust/application/tests/`):
  - `operation_tests.rs`: exclusión mutua de operaciones concurrentes, cancelación con liberación de busy y avance de revisión.
  - `entry_sync_checks_tests.rs`: matriz completa de invalidación de frescura ante cambio de revisión, estado ocupado, cambio de credenciales, eliminación de juego o nueva comprobación.
  - `edit_library_tests.rs`: reordenación estable, altas, bajas, renombrado remoto al editar entrada, preservación de raíz compartida en emuladores, aplicación de overrides y registro/desregistro en Steam.
  - `target_mapping_tests.rs`: rechazo de traversal en rutas nube, colisiones de rutas remotas a locales, reserva de nombres entre archivos y directorios, rechazo de carpetas legacy con nombres idénticos y resolución de segmentos.
  - `migrator_tests.rs`: detección de duplicados previa a mover, fallo en traslado con rollback en orden inverso y cancelación cooperativa con rollback inverso.
  - `update_coordinator_tests.rs`: transiciones de estado de comprobación, limpieza de archivo destino ante fallo de descarga y bloqueo de reinicio durante operaciones activas.
  - `orchestrator_tests.rs`: ciclo completo pre-sync (descarga remota) -> ejecución -> post-sync (subida local), juego offline ante error de transporte en autorización, bloqueo de inicio ante error de mapeo, resolución de conflictos mediante `RequestId` exacto y reporte/reintento de fallo post-sync por `RequestId`.
- Verificaciones de calidad del workspace completo:
  - `cargo xtask check`: pasó verificación estricta de dependencias de arquitectura, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (0 advertencias) y la totalidad de pruebas del workspace (105 pruebas aprobadas, 2 ignoradas documentadas).
  - `./gradlew check --no-daemon`: `BUILD SUCCESSFUL` en Linux (102 tareas accionables: 3 ejecutadas, 99 up-to-date; hooks de Git y convenciones verificados).

### R6 — Plataforma y actualizaciones

- [x] Portar procesos, scanner, portadas, selectores, navegador, Steam, VDF, wrappers y actualizador.
- [x] Verificar argumentos y PowerShell en Windows nativo, procesos ajenos, entorno AppImage, Game Mode y campos VDF desconocidos.
- [x] Probar descarga incompleta, staging, reemplazo, reinicio y rollback con fuentes controladas.
- [x] **R6 completado técnicamente.**

Aceptación: implementaciones nativas en `emusync-platform-desktop` y `emusync-data-updates` de los contratos `GameLauncher`, `BrowserLauncher`, `PathPicker`, `CoverFiles`, `SteamLibrary`, `SteamLaunchPreparation`, `UpdateFiles`, `UpdateInstaller` y `UpdateSource`. Preservación estricta de orden de portadas, semántica de scanner con firmas y exclusión de ciclos symlink, lectura/escritura binaria VDF con campos desconocidos y reemplazo atómico, wrappers Linux/Windows con claves estables SHA-256 y migración legacy, tracking de procesos con renovación de timeout en compilación de shaders Steam, staging/descarga de actualizaciones con rollback de AppImage y reinicio diferenciado para Steam Game Mode y Windows.

Evidencia (2026-11-06, Linux, base `6ed2d52`; pendiente de commit):

- `emusync-data-updates`:
  - `GitHubRelease` y `GitHubAsset` deserializan metadatos de GitHub Releases.
  - `is_newer_version` evalúa diferencias semver considerando equivalentes a cero los componentes omitidos.
  - `find_best_asset_for_platform` prioriza en Windows `.msi` > `.exe` > `.zip` y en Linux `.AppImage` > `.tar.gz` > `.deb`.
  - `UpdateDownloader` realiza descargas por fragmentos a un archivo temporal `.part`, valida bytes totales y no vacíos, asigna permisos de ejecución (`0o755`) en Unix y ejecuta reemplazo seguro/atómico (`rename`). La cancelación y fallos limpian el archivo temporal mediante RAII guardando el destino existente.
  - `GitHubUpdateSource` implementa `UpdateSource` consultando la API de GitHub o fuentes sintéticas con cabeceras `Accept: application/vnd.github+json` y `User-Agent: EmuSync-Desktop-App`.
  - Pruebas en `rust/data-updates/tests/`:
    - `github_source_tests.rs` (5 pruebas): comparación semver, paridad con cero en componentes omitidos, orden de prioridad de assets por plataforma, detección de release en Linux (.AppImage) y Windows (.msi) con servidor HTTP loopback.
    - `download_safety_tests.rs` (5 pruebas): descarga sin longitud de contenido informando finalización, descarga con longitud de contenido y reporte de progreso incremental, rechazo de descarga vacía con conservación de destino y limpieza de staging, descarga truncada/incompleta con conservación de destino previo y limpieza de staging, y cancelación cooperativa con eliminación de staging y retención del destino original.
- `emusync-platform-desktop`:
  - `AppImageEnvironment::clean`: elimina `APPDIR`, `ARGV0` y `OWD`, filtra segmentos de runtime de AppImage o montajes `.mount_` en `LD_LIBRARY_PATH` y elimina la variable si queda vacía.
  - `shell_argument`: escapa `\`, `"`, `$`, `` ` ``, expande `{ROM}` a `${ROM_PATH}` y rechaza caracteres NUL.
  - `scan_roms`: escaneo recursivo resolviendo rutas reales (`canonicalize`) para prevenir bucles de symlinks, detección de carpetas de juego por firmas (`ps3_disc`, `eboot`, `app`, `main`) deteniendo descenso en sus contenidos, normalización de extensiones y ordenamiento alfabético insensible a mayúsculas.
  - `DesktopCoverFiles`: implementa `CoverFiles` con orden de precedencia: 1) ruta personalizada en `cover_path_by_rom` o `native.cover_path`; 2) imagen adyacente en directorio de ROM (variantes por nombre base, título efectivo, título limpio `clean_game_title`, nombre con espacios y nombre de archivo con extensiones `.png`, `.jpg`, `.jpeg`, `.webp` y coincidencia insensible a mayúsculas); 3) directorio configurado de carátulas o subcarpetas adyacentes (`covers`, `boxart`, `images`); 4) directorio ejecutable nativo.
  - `BinaryVdf`: lector y escritor binario para `shortcuts.vdf` (tipos 0x00, 0x01, 0x02, 0x08), preserva campos desconocidos (`extra_fields`), admite alias legacy (`appname`, `exe`, `startdir`) con precedencia de canónicos incluso vacíos, rechaza valores truncados y caracteres NUL, y escribe mediante archivo temporal con sincronización a disco y reemplazo atómico.
  - `SteamShortcutManager`: búsqueda de userdata de Steam (estándar, Flatpak y Windows), clave estable basada en nombre readable y SHA-256, detección de argumentos de pantalla completa para emuladores conocidos, creación de scripts `.sh` en Linux y empaquetado de wrappers Windows con `windows-launcher-v1.ps1`, configuración JSON y script `.bat` con comando PowerShell codificado en Base64 UTF-16LE, migración de lanzadores legacy creando respaldos `.legacy.bak` y scripts de reenvío, y rechazo de aliases antiguos ambiguos avisando reinicio de Steam.
  - `DesktopSteamLibrary`: implementa `SteamLibrary` y `SteamLaunchPreparation` delegando en `SteamShortcutManager`.
  - `ProcessMonitor` y `ExternalProcessTracker`: detección de procesos (escaneo de `/proc` en Linux, `tasklist` en Windows, `pgrep`), detección de compilación de shaders Vulkan con `fossilize_replay` / `fossilize_replay64` para renovar el timeout de espera, seguimiento de salida de todos los procesos coincidentes y reutilización confirmada de procesos existentes.
  - `GameRunner`: implementa `GameLauncher` para ejecución directa (con limpieza de entorno AppImage) y ejecución mediante URI de Steam (`steam://rungameid/<gameId>`) coordinado con el monitor de procesos.
  - `DesktopBrowserLauncher`: implementa `BrowserLauncher` con `xdg-open` en Linux y protocolo `rundll32` / `cmd start` en Windows.
  - `FilePicker`: implementa `PathPicker` ejecutando `kdialog` y `zenity` secuencialmente en Linux y procesando selección, cancelación (sin fallback erróneo) o falta de disponibilidad.
  - `UpdateStagingFiles`: implementa `UpdateFiles` preparando archivos de actualización en el directorio padre del AppImage si es escribible o en el directorio temporal.
  - `AppImageInstaller`: reemplazo atómico de AppImages en Linux con verificación de archivo regular y distinción de origen/destino, copia a temporal, permisos `0o755`, sincronización forzada a disco, respaldo previo `.old.bak`, restauración automática en fallo de lanzamiento, y retención de respaldo para recuperación manual si falla la restauración.
  - `DesktopUpdateInstaller`: implementa `UpdateInstaller` detectando Steam Deck Game Mode (`SteamGamepadUI` o `XDG_CURRENT_DESKTOP == gamescope`), reinicio en Windows mediante `msiexec` o `.exe`, y reemplazo de AppImage en Linux invocando el nuevo ejecutable solo fuera de Game Mode para permitir la gestión de procesos de SteamOS.
  - Pruebas en `rust/platform-desktop/tests/`:
    - `util_tests.rs` (3 pruebas): limpieza de variables de entorno y `LD_LIBRARY_PATH`, eliminación si queda vacío, escape de argumentos de shell y sustitución de `{ROM}`.
    - `rom_scanner_tests.rs` (6 pruebas): escaneo recursivo ignorando no-ROMs, directorio inexistente, extensiones vacías, extensiones con espacios y puntos, firmas de juego completo PS3/EBOOT/APP/MAIN excluyendo archivos internos, y prevención de bucles y duplicados por symlinks en Linux.
    - `cover_lookup_tests.rs` (7 pruebas): carátula adyacente con nombre exacto, carátula desde directorio configurado, subcarpeta covers con eliminación de región, juego nativo con ruta personalizada, precedencia máxima de `cover_path_by_rom`, precedencia de carpeta adyacente sobre subcarpeta, y subcarpeta cuando no hay imagen adyacente.
    - `binary_vdf_tests.rs` (7 pruebas): codificación y decodificación round-trip, adición preservando existentes, lista vacía, alias legacy vs canónicos (incluyendo canónicos vacíos), reemplazo atómico interrumpido reteniendo archivo original, rechazo de NUL, y rechazo de valores truncados.
    - `steam_manager_tests.rs` (6 pruebas): registro y script Linux con flags de pantalla completa, registro y batch Windows con JSON y PowerShell codificado, estabilidad de claves SHA-256 ante variaciones de nombres y Unicode, migración de lanzadores legacy con reenvío y respaldo `.legacy.bak`, detección de aliases antiguos ambiguos que rehúsan ejecución, y actualización de juego nativo preservando ID y preferencias Steam.
    - `process_tracker_tests.rs` (3 pruebas): aparición dentro del timeout y seguimiento de salida, renovación de timeout durante compilación de shaders, y aborto inmediato ante solicitud fallida sin esperar procesos ajenos.
    - `game_runner_tests.rs` (2 pruebas): ejecución directa retornando exit code y duración, y ejecución mediante Steam preparando señal de ROM y rastreando proceso.
    - `update_installer_tests.rs` (9 pruebas): reemplazo atómico y limpieza de AppImage, restauración de original en fallo de lanzamiento, retención de respaldo en fallo de rollback, rechazo de reemplazo de archivo sobre sí mismo, ejecución de msiexec para MSI en Windows, ejecución de EXE directamente en Windows, rechazo de archivos no instaladores en Windows, reemplazo de AppImage en Linux con llamada a lanzador, y modo Steam Game Mode que evita lanzar proceso hijo redundante.
- Verificaciones de calidad del workspace completo:
  - `cargo xtask check`: pasó verificación estricta de dependencias de arquitectura, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (0 advertencias) y las 158 pruebas del workspace (todas aprobadas, 2 ignoradas documentadas).
  - `./gradlew check --no-daemon`: `BUILD SUCCESSFUL in 1m 24s` en Linux (102 tareas accionables: 3 ejecutadas, 99 up-to-date; hooks de Git y convenciones verificados).
- Omitido / pendiente:
  - Pruebas nativas en Windows, ejecución nativa de PowerShell en Windows y consultas de registro Windows diferidas por decisión del usuario del 2026-11-05; no se ejecutó CI remoto ni publicación de releases.

### R7 — Presentación y composición

- [x] Recrear tema oscuro, barra lateral, cuadrícula, formularios, conexión, diálogos y flujos actuales con egui, winit y wgpu, sin JVM ni WebView.
- [x] Propietario explícito de clientes, tareas y runtime; comunicar comandos y estados tipados sin bloquear el hilo visual; integración funcional real completa con SyncOrchestrator, Google Drive (OAuth/Listing/Folders/Transfer), GuardedFiles y RetainedFiles.
- [x] Validar edición, orden, conflictos, cancelación, progreso, errores y cierre en plataformas aplicables (Linux validado; Windows diferido).
- [x] **R7 completado.**

Evidencia (2026-11-06, Linux, base `ac1995f`; pendiente de commit):

- `emusync-presentation`:
  - `theme`: paleta oscura idéntica a EmuSync (`#0D1117`, `#161B22`, `#1C2333`, `#58A6FF`, etc.) y configuración de estilos de egui con tipografías y bordes redondeados.
  - `model`: `CloudSyncStatus` (etiquetas y colores), `AppStatus` (Idle, Syncing, Playing, Error, PostSyncFailed, Conflict), `GameItem` (con resolución de título y cover efectivos), `SaveSetupRequest`, `PathTarget` y `PickMode`.
  - `actions`: `UiAction` con variantes para selección, lanzamiento, sincronización, edición de biblioteca, Steam, OAuth Drive, actualizador, resolución de conflictos y selección de rutas.
  - `state`: `AppUiState` con Single Source of Truth inmutable para presentación y consultas derivadas (`is_drive_connected`, `sync_status_for_selected`, etc.).
  - `forms`: `SaveTargetFormState`, `EntryFormState` (emuladores y nativos con argumentos por defecto y validación estricta de rutas) y `GameEditFormState` (overrides individuales, carpetas de restauración y validación).
  - `dialogs`: modales completos para configuración de Google Drive (`DriveSetupDialogState`), alta y edición de categorías (`EntryDialog`), edición de overrides y restauración (`GameEditDialog`), actualización y notas de release (`UpdateDialog`), y superposiciones de estado (`StatusOverlay` para conflictos, reintentos post-sync y errores).
  - `components`: encabezado (`render_header`), barra lateral con accesos Steam y reordenación (`render_sidebar`), cuadrícula de juegos y tarjetas interactivas (`render_game_grid`, `render_game_card`) y notificaciones flotantes (`render_toast`).
  - `cover_cache`: carga y decodificación de texturas de carátulas PNG/JPEG bajo demanda con `TextureHandle`.
  - `EmuSyncApp`: fachada central que gestiona diálogos activos, cola de acciones y ciclo `update(ui)`.
  - Pruebas en `rust/presentation/tests/` (18 pruebas aprobadas): `forms_tests.rs` (7 pruebas), `app_state_tests.rs` (4 pruebas), `ui_view_tests.rs` (7 pruebas con renderizado headless, envío de modales, acciones de Drive/Update y resolución de conflictos).
- `emusync-desktop-app`:
  - `DesktopCoordinator`: puente asíncrono entre la interfaz egui y Tokio; orquesta `ConfigManager`, `DesktopSteamLibrary`, `DesktopBrowserLauncher`, `FilePicker`, `DesktopUpdateInstaller`, `GitHubUpdateSource`, `UpdateStagingFiles` y `GameRunner`.
  - `scanner_helper`: escaneo de ROMs por extensiones y firmas y mapeo unificado a `GameItem`.
  - `events`: eventos fuertemente tipados (`AppEvent`) enviados al hilo visual mediante canales sin bloquear UI, con callbacks de redibujado (`request_redraw`).
  - `main.rs`: composición completa de la aplicación nativa en el bucle de eventos de `winit` 0.30 con superficie `wgpu`, despacho de `UiAction`, gestión de ciclo de vida y cierre cooperativo supervisado (`CloseRequested` y `poll_close`).
  - Pruebas en `rust/desktop-app/tests/desktop_composition_tests.rs` (6 pruebas aprobadas): inicialización y escaneo de biblioteca, altas y bajas de categorías, reordenación estable, guardado de overrides individuales, ciclo de lanzamiento de juego y renderizado headless en bucle egui.
- Verificaciones de calidad del workspace completo:
  - `cargo xtask check`: pasó verificación estricta de dependencias de arquitectura, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (0 advertencias) y las 182 pruebas del workspace (todas aprobadas, 2 ignoradas documentadas).
  - `./gradlew check --no-daemon`: `BUILD SUCCESSFUL in 1m 25s` en Linux (102 tareas accionables: 3 ejecutadas, 99 up-to-date; hooks de Git y convenciones verificados).
- Omitido / pendiente:
  - Validación nativa en Windows diferida por decisión del usuario del 2026-11-05; pruebas con Google Drive real y CI remoto siguen pendientes.

#### Revisión de integración R7/R8 (2026-11-06, Linux, base `2fb410c`)

La evidencia anterior de R7 describe componentes y pruebas, pero no acredita los flujos completos: `DesktopCoordinator` simulaba autorización y sincronización, escribía un token ficticio y notificaba éxito sin transferencia; el lanzamiento invoca el proceso sin sincronización previa ni posterior. Se retiran esas simulaciones: las acciones de Drive informan que no están disponibles y conservan las credenciales existentes. El actualizador de la composición apuntaba a GitHub real, en contradicción con el canal de desarrollo descrito abajo; se desactivan sus acciones hasta integrar y validar una fuente controlada. El adaptador y sus pruebas se conservan. El lanzamiento sigue sin sincronización y no acredita equivalencia; no usar la aplicación Rust para partidas reales.

En una revisión posterior se bloqueó el lanzamiento cuando existe un token de Drive no vacío o no se puede leer la configuración; el lanzamiento sin conexión sigue disponible, pero tampoco acredita equivalencia. El orquestador de R5 convierte errores de `LocalSaveFiles` a texto: conectado directamente a `TrackedFileSync`, eso destruiría el `TrackedError` y el snapshot temporal necesario para reconciliar una subida incierta. Antes de conectar la sincronización hay que conservar la propiedad del error hasta su recuperación bajo exclusión y supervisar la cancelación. La edición de categorías y overrides de la composición tampoco llama a la migración remota; se bloquea cuando existe un token de Drive no vacío y debe integrarse antes de usar datos reales.

R3 conserva pendientes de equivalencia y Windows. La evidencia headless de R7 no valida interacción nativa, conflictos, accesibilidad ni cierre de operaciones reales. Los paquetes y workflows siguen siendo Kotlin. Se conserva Kotlin y no se declara R8 terminado. Windows nativo y CI remoto continúan pendientes; no se han generado ni validado instaladores Rust, ni probado instalación, actualización o desinstalación. No se ha publicado ni hecho push.

Validación de esta revisión en Linux: `cargo xtask check` pasó arquitectura, formato, Clippy y pruebas del workspace; `./gradlew check --no-daemon` terminó correctamente (111 tareas: 3 ejecutadas y 108 UP-TO-DATE). La prueba de composición añadida comprueba conservación de credenciales y errores explícitos; no demuestra sincronización real. `git diff --check` pasó. Cambios sin commit.

Continuación Linux del 2026-11-06: `cargo xtask check` pasó de nuevo tras añadir las barreras de lanzamiento y edición y sus pruebas de composición. Las pruebas comprueban que se informa el bloqueo y que los cambios de rutas no se guardan con Drive conectado; no prueban la integración pendiente. `git diff --check` pasó. No se repitió Gradle porque esta continuación solo modifica Rust y documentación.

Continuación de recuperación (Linux, 2026-11-06): `GuardedFiles` adapta `LocalSaveFiles` para usar la subida con revisión OAuth y registro persistente; `RetainedFiles` conserva el error original, impide nuevas operaciones tras un fallo y solo entrega su propiedad cuando ya no quedan clones. Las pruebas utilizan persistencia local real y un transporte sintético: una subida de resultado incierto conserva snapshot y registro, el cambio previo de credenciales impide enviar la subida, y un supervisor retiene exclusión y snapshot hasta su resolución. Estos componentes todavía no están conectados al orquestador de la interfaz; su propietario debe supervisar los futuros sin abortarlos y conservar la exclusión durante la recuperación. La descarga, cancelación, recuperación integrada y el ciclo completo siguen pendientes. La primera ejecución de `cargo xtask check` detectó `clippy::type_complexity`; se corrigió y `cargo xtask check` pasó al repetirlo. `git diff --check` pasó; Gradle no se repitió en esta continuación Rust.

Continuación de descargas (Linux, 2026-11-06): el adaptador usa ahora confirmación de descarga e historial condicionada a la revisión OAuth original. Las pruebas con transporte sintético verifican el commit y el historial con revisión vigente, la conservación de partida e historial ante un cambio de credenciales durante la descarga y el rechazo de un registro de historial obsoleto. `cargo xtask check` y `git diff --check` pasaron; Gradle no se repitió al cambiar solo Rust y documentación. El destino directo todavía no proporciona el anclaje de rutas de `download_pinned_guarded`; no se afirma protección contra sustitución concurrente de directorios. Sigue pendiente integrar la raíz anclada, la cancelación y la recuperación con el supervisor y la interfaz, y validar Windows.

#### Continuación de integración funcional R7 (2026-11-06, Linux):

- Se conectó completamente la composición real en `DesktopCoordinator` (`rust/desktop-app/src/coordinator.rs`), sustituyendo todas las simulaciones y errores temporales de Drive/sincronización por adaptadores de producción:
  - `DriveEndpoints`: configurable para pruebas e inyectable; apunta a los endpoints oficiales de Google Drive (`accounts.google.com`, `oauth2.googleapis.com`, `www.googleapis.com`) por defecto.
  - `InteractiveOAuth`: compuesto con `DesktopBrowserLauncher` y listener loopback para intercambio real de tokens y refresco automático.
  - `DriveListing` y `DriveFolders`: búsqueda exacta y paginada v3, creación y movimiento de carpetas en Google Drive.
  - `RetainedFiles<GuardedFiles<DriveTransfer, ConfigManager>>`: supervisión de subidas con precondición ETag `If-Match`, snapshot persistente en disco (`RecoveryStore`), reconciliación ante pérdidas de conexión y exclusión por proceso sin liberación anticipada de locks.
  - `SyncOrchestratorFactory`: crea `SyncOrchestrator` con notificación de estado `AppStatus` al bucle egui sin bloquear el renderizado.
- Acciones integradas en `DesktopCoordinator`:
  - `connect_drive`: ejecuta el flujo OAuth completo, intercambia tokens y actualiza la configuración persistida mediante CAS atómico.
  - `sync_entry`: sincronización manual con soporte para cancelación cooperativa (`CancellationToken`) invocable desde `UiAction::ClearStatus`.
  - `check_sync_entry`: comprobación de estado contra Drive e historial local; reporta fallos en vez de afirmar que partidas no verificadas están al día.
  - `launch_game`: orquesta `play_with_sync`, ejecutando sincronización previa, lanzamiento del proceso con seguimiento de salida limpia y detección de procesos huérfanos/zombies (`/proc/<pid>/status`), y sincronización posterior con resolución de conflictos o reintento interactivo.
  - `edit_entry` y `edit_game_override`: migran las carpetas en la nube mediante `rename_entry` / `rename_game` antes de aplicar las modificaciones a la configuración local; si falla la migración remota, rechazan la modificación para evitar discrepancias entre nube y local.
  - `ResolveConflict`: despacha la decisión del usuario (`Decision::KeepLocal`, `Decision::KeepRemote`, `Decision::Skip`) al callback activo sin mantener bloqueos cruzados.
  - `RetryPostSync`: reejecuta la sincronización posterior pendiente conservando el snapshot.
- Pruebas en `rust/desktop-app/tests/desktop_composition_tests.rs`: 10 pruebas aprobadas en Linux que verifican inicialización, altas/bajas/reordenación, overrides, ciclo completo con sincronización real, rechazo de credenciales vacías, lanzamiento asíncrono y rechazo de edición de rutas si la migración remota falla.
- `cargo xtask check` pasó completamente en Linux (arquitectura, formato, Clippy sin advertencias y pruebas del workspace).

#### Continuación R7 — paridad visual de cabecera y barra lateral (2026-11-10, Linux)

- Cabecera (commit `39e5889`): distribución de `AppHeader` Kotlin, fuente JetBrainsMono Nerd Font (OFL 1.1) incluida en el binario e iconos tomados de sus glifos. "Connect Drive" conecta con las credenciales guardadas y abre el diálogo solo si alguna está en blanco, como Kotlin. "Check Updates" se conserva como extra Rust por decisión del usuario.
- Barra lateral: el orden anterior intercambiaba posiciones en la lista plana y podía separar las entradas de un grupo. `sidebar_order` porta la partición en unidades, el movimiento de carpetas e hijos, las filas visibles, el plegado y el arrastre acumulado de Kotlin; la vista añade carpetas, plegado, asa de arrastre y flechas visibles al pasar el puntero. Ancho fijo de 240 como Kotlin: un panel redimensionable se arrastra con facilidad por error en la pantalla táctil de Steam Deck.
- Steam: `AppUiState::steam_available` nunca se actualizaba, por lo que el control de registro no aparecía aunque Steam estuviera instalado. Como `refreshSteamState` de Kotlin, cada `ConfigReloaded` (incluida la carga inicial) lanza un refresco fuera del hilo visual con disponibilidad y entradas registradas; `SteamStateRefreshed` lleva revisión y no se aplica sobre uno más reciente. Pruebas en `desktop-app/tests/platform_ui_tests.rs` con Steam sintético: `steam_refresh_reports_unavailable_without_steam_userdata`, `steam_refresh_reports_available_and_only_registered_entries` y `stale_steam_refresh_does_not_overwrite_newer_state`.
- Cuadrícula: el título de la categoría se dibujaba antes de las acciones sin límite de ancho y se superponía con ellas; ahora ocupa el espacio restante, truncado y con el nombre completo al pasar el puntero.
- Capturas Linux (Hyprland, Wayland) con configuraciones sintéticas aisladas: Drive vinculado y sin vincular, y biblioteca con dos grupos, entradas sueltas y un nombre largo truncado en la barra lateral y en la cabecera de la cuadrícula. La detección de Steam leyó el `shortcuts.vdf` real del equipo sin modificarlo; no se pulsó el control de registro. El arrastre y las flechas no se ejercitaron con entrada real del puntero; tampoco se validó en Windows ni en Steam Deck.
- `CARGO_BUILD_JOBS=2 nice -n 19 cargo test -p emusync-presentation -p emusync-desktop-app --locked -- --test-threads=2`: 138 aprobadas, cero fallos; `cargo xtask quality` y Clippy sin advertencias. Gradle no se repitió: solo cambia Rust y documentación.

| Garantía Kotlin (`SidebarTest`, `SidebarInteractionStateTest`) | Prueba Rust (`presentation/tests/sidebar_order_tests.rs`) | Linux | Windows |
|---|---|---|---|
| Filas con carpeta e hijos al desplegar | `expanded_group_lists_header_then_children_then_standalone` | Pasó | Pendiente |
| Carpeta plegada conserva cabecera y oculta hijos; plegar y desplegar no la elimina | `collapsed_group_keeps_header_and_hides_children`, `header_survives_collapse_and_reexpand` | Pasó | Pendiente |
| Agrupación y plegado sin distinguir mayúsculas | `grouping_and_collapse_ignore_case`, `expanding_folder_removes_collapsed_names_regardless_of_case` | Pasó | Pendiente |
| Carpeta marca hijo seleccionado | `header_marks_selected_child_by_name` | Pasó | Pendiente |
| Mezcla de grupos y entradas en orden de primera aparición | `mixed_groups_and_standalone_rows_follow_first_appearance` | Pasó | Pendiente |
| Partición y aplanado agrupan miembros dispersos | `partition_gathers_scattered_group_members_and_flatten_makes_them_contiguous`; nuevo `blank_group_is_standalone` | Pasó | Pendiente |
| Mover carpetas completas y mover hijos dentro de su carpeta | `folders_move_as_whole_units_past_entries_and_folders`, `children_move_within_their_folder` | Pasó | Pendiente |
| Activar carpeta selecciona el primer hijo o pliega y despliega alrededor de la selección | `activating_folder_selects_first_child_then_toggles_collapse_around_selection`, `opening_collapsed_unselected_folder_expands_and_selects_first_child` | Pasó | Pendiente |
| Arrastre acumulado de carpeta, hijo limitado a su grupo, entrada suelta cruza un grupo entero | `folder_drag_accumulates_partial_movement_and_moves_whole_group`, `child_drag_stays_within_group_beyond_last_sibling`, `standalone_drag_crosses_whole_group_without_splitting_it` | Pasó | Pendiente |
| Entrada eliminada durante el arrastre no reordena | `entry_removed_during_drag_cannot_reorder_remaining_library`; nuevo `drag_without_matching_key_never_reorders` | Pasó | Pendiente |
| Movimientos de hijo inválidos conservan el orden | `invalid_sibling_moves_preserve_original_order`; el índice negativo de Kotlin no es representable con `usize` | Pasó | Pendiente |

#### Continuación R7 — paridad visual de la cuadrícula de juegos (2026-11-10, Linux)

- Barra de categoría como `GameGrid` Kotlin: título truncado sin caja, "Open Emulator" con contorno, chip de sincronización por estado (In Sync con comprobación, Out of sync/Conflict con acción de sincronizar, Checking/Syncing con indicador, Idle/Error "Sync All", nada si no está configurado), editar y borrar como iconos y contador. El botón "Check" permanente de Rust se retira por decisión del usuario para igualar Kotlin.
- Cuadrícula adaptativa equivalente a `GridCells.Adaptive(160)` con separación 12; tarjeta con portada ajustada, título centrado de hasta dos líneas, botón de edición al pasar el puntero, indicador de carga y estados vacíos de Kotlin. Los iconos vectoriales sin uso se retiran en favor de glifos de la fuente.
- Extra Rust por decisión del usuario: borrar una categoría pide confirmación (Kotlin borra al primer clic, fácil de activar por error en pantalla táctil). El texto refleja lo que hace el borrado: retira la categoría y, si está registrada, su acceso de Steam; no borra partidas, ROMs ni archivos de Drive.
- Pruebas en `presentation/tests/ui_view_tests.rs`: `adaptive_grid_fills_width_with_cards_of_at_least_min_width` y `deleting_a_category_waits_for_confirmation_and_escape_cancels`. La confirmación mediante clic no se ejercitó en pruebas headless.
- Capturas Linux con un emulador sintético de ocho ROM, cuatro portadas generadas y títulos largos. `cargo xtask quality` y pruebas de `presentation` y `desktop-app` (140 aprobadas, cero fallos). Gradle no se repitió. Windows y Steam Deck pendientes.

#### Continuación R7 — diálogo de alta y edición de entradas (2026-11-10, Linux)

- Diálogo con la estructura y los textos de `AddEntryDialog` Kotlin: chips de tipo, chips de carpetas existentes, vista previa de la carpeta de Drive, ayudas bajo los campos, flags de pantalla completa detectados para el ejecutable y filas de guardado con botones File/Folder, conmutador de tipo, papelera y ruta en la nube validada en vivo. Piezas de formulario compartidas en `dialogs/form_fields.rs` para el diálogo de juego.
- Tipos de guardado: Rust persistía el tipo elegido en el formulario aunque la ruta existiera con el tipo contrario; Kotlin lo corrige en `buildTargets` con `SaveTargetAccess`. `desktop-app/src/target_types.rs` aplica `LocalSaveTargets` antes de persistir altas, ediciones, overrides de juego y configuraciones de guardado; las rutas inexistentes conservan el tipo elegido. La detección en vivo al escribir una ruta se ejecuta fuera del hilo visual (`DetectSaveTargetType`) y solo se aplica si la fila conserva esa ruta, como `refreshType`. Elegir una ruta con File o Folder fija ese tipo, como en Kotlin; antes `PathPicked` no indicaba el modo.
- Los flags de pantalla completa se inyectan desde la composición (`SteamShortcutManager::detect_fullscreen_args`), como `LibraryFormActions.detectFullscreenArgs`, sin que `presentation` dependa de `platform-desktop`.
- Pruebas: `cloud_folder_preview_falls_back_from_cloud_folder_to_group_to_name`, `group_chips_toggle_ignoring_case_and_list_distinct_groups`, `cloud_path_validation_rejects_parent_segments_only_when_filled`, `picked_save_path_takes_the_picked_kind_and_stale_detection_is_ignored`, `entry_dialog_renders_both_entry_types_headless` y, con un directorio temporal real, `existing_paths_take_their_real_kind_and_missing_paths_keep_the_chosen_one` y `entry_targets_are_resolved_and_other_fields_are_untouched`. `cargo xtask quality` y pruebas de `presentation` y `desktop-app` (147 aprobadas, cero fallos). No hay herramientas de entrada sintética en el equipo: la revisión visual del diálogo depende del usuario. Gradle no se repitió. Windows y Steam Deck pendientes.

#### Continuación R7 — diálogo de edición de juego (2026-11-10, Linux)

- Rutas iniciales: Rust rellenaba el diálogo de un juego de emulador con las rutas de todo el emulador; guardar sin cambios las copiaba como override de esa ROM, que dejaba de seguir al emulador. Como `gameSpecificTargets` de Kotlin, `emusync_application::library::game_specific_targets` usa `for_rom` (emulador) o `for_entry` (nativo); la composición lo ejecuta fuera del hilo visual (`EditGame` → `GameEditReady`) y también para prellenar la configuración de partidas solicitada por la sincronización.
- Portada: el formulario arrancaba con la portada detectada como si fuera personalizada y guardar la fijaba. `GameItem::custom_cover_path` equivale a `effectiveCoverPath` de Kotlin y solo devuelve la elegida por el usuario; la vista previa muestra la personalizada o la del escáner, con las etiquetas Custom Cover / Auto-detected / No Cover.
- Restauración desde la nube: el formulario unía destino y nombre remoto sin las comprobaciones de `for_restore` (nombres que escapan del destino, enlaces simbólicos). Como Kotlin, la vista previa se resuelve con `for_restore` fuera del hilo visual y solo se aplica al destino vigente; Save requiere rutas válidas; al confirmar, la composición recalcula las rutas con `for_restore` en lugar de confiar en el formulario.
- Diálogo con la estructura y los textos de `EditGameDialog`: aviso azul, Auto-Clean / Raw Name, ROM File, vista previa 136×180, selección y restablecimiento de portada, opciones de restauración con lista de archivos y Save deshabilitado mientras no se pueda guardar. Las filas de guardado se comparten con el diálogo de entrada.
- Pruebas: `game_edit_restore_waits_for_resolved_paths_and_returns_the_choice`, `game_edit_starts_from_the_custom_cover_only` y, con `LocalSaveTargets` y directorios temporales reales, `emulator_game_starts_from_its_rom_targets_not_the_emulator_wide_ones`, `native_game_starts_from_its_own_targets`, `restore_keeps_cloud_files_inside_the_destination`, `restore_rejects_names_reaching_through_a_symlink` y `whole_folder_restore_targets_the_destination_and_blank_targets_nothing`. Los diálogos de formulario se cierran con Escape o clic fuera como los `Dialog` de Kotlin; los estados superpuestos no: `running_game_overlay_ignores_escape_and_clicks_outside` lo protege para el juego en curso y falló al hacer que ese modal se cerrara de esa forma. `cargo xtask quality` y pruebas de `presentation`, `desktop-app` y `application` (264 aprobadas, cero fallos). Revisión visual del diálogo a cargo del usuario. Gradle no se repitió. Windows y Steam Deck pendientes.

#### Continuación R7 — estados superpuestos (2026-11-10, Linux)

- Aspecto y textos de `StatusOverlay` Kotlin: icono en círculo tintado (girando en Syncing y Loading), "Playing...", conflicto con tarjetas Local Save / Cloud Save y fechas, "Save kept locally" con el mensaje según partidas subidas y botones Keep local / Retry upload, y "Sync Notice" con desplazamiento para mensajes largos.
- Conflicto: tras elegir se muestra "Syncing..." al instante, como el `resolved` de Kotlin, y la otra opción deja de estar disponible; la elección se olvida en cuanto el estado cambia. El coordinador ya consumía la decisión una sola vez; ahora tampoco se envía una segunda.
- Cierre: solo el aviso de error se cierra con Escape o clic fuera, como `Dialog(onDismissRequest = onDismiss)` en Kotlin; Playing, Loading, Syncing, Conflict y el fallo de subida siguen sin cerrarse así.
- Pruebas en `presentation/tests/ui_view_tests.rs`: `operation_overlays_ignore_escape_and_clicks_outside` (sustituye a la prueba solo de Playing y cubre los cinco estados), `conflict_accepts_one_choice_and_then_shows_syncing` (clics reales sobre las tarjetas; falló al quitar el registro de la elección), `error_notice_closes_on_escape_like_kotlin` y `post_sync_failure_message_reports_partial_progress`. `cargo xtask quality` y pruebas de `presentation`, `desktop-app` y `application` (267 aprobadas, cero fallos). Revisión visual de Playing y Error a cargo del usuario con una biblioteca sintética; Conflict, Syncing y fallo de subida requieren Drive y no se vieron en pantalla. Gradle no se repitió. Windows y Steam Deck pendientes.

#### Continuación R7 — diálogo de Google Drive (2026-11-10, Linux)

- Estructura y textos de `DriveSetupDialog` Kotlin: cabecera con nube según el estado, aviso de credenciales OAuth de escritorio, campos Client ID / Client Secret con ejemplos de formato, ubicación de `config.json` con Open Folder, Unlink Account solo con cuenta vinculada, Cancel y Save & Connect; se cierra con Escape o clic fuera y el aviso de error desaparece al editar, como en Kotlin. `AppUiState::config_path` existía pero la composición nunca lo rellenaba; ahora recibe la ruta resuelta (`--config` o ubicación por defecto).
- Extra Rust por decisión del usuario: Client Secret oculto por defecto con un botón para mostrarlo; Kotlin lo muestra en claro. No cambia credenciales, OAuth ni lo que se guarda.
- Pruebas en `presentation/tests/ui_view_tests.rs`: `drive_setup_sends_trimmed_credentials_and_flags_blank_ones` y `drive_setup_opens_with_stored_credentials_and_closes_on_escape`; se retira `app_opens_drive_setup_and_emits_connect_action`, que insertaba la acción a mano y no ejercitaba el diálogo. `cargo xtask quality` y pruebas de `presentation`, `desktop-app` y `application` (268 aprobadas, cero fallos). Revisión visual del estado sin vincular a cargo del usuario; el estado vinculado no se abrió con credenciales reales. Gradle no se repitió. Windows y Steam Deck pendientes.

#### Continuación R7 — diálogo de actualización (2026-11-10, Linux)

- Estructura y textos de `UpdateDialog` Kotlin: Update Available con notas desplazables, Later / Update Now; descarga con barra y porcentaje; Update Complete! con textos y botones propios de Steam Deck Game Mode (Close to Apply) o escritorio (Restart Now / Restart Later); Update Error con Close. Como en Kotlin, no se cierra con Escape ni clic fuera durante la descarga y Restart Now solo solicita el reinicio. Idle y Checking se mantienen como extra Rust del botón Check Updates.
- Las acciones de actualización siguen desactivadas en la composición de desarrollo. Para revisar el diálogo, solo en compilaciones debug, `--preview-update=<available|downloading|ready|ready-steam|error>` abre la aplicación con un estado sintético; en release el argumento sigue rechazándose (Clippy release sin advertencias).
- Pruebas: `update_dialog_cannot_be_dismissed_while_downloading` y, en `main.rs`, `preview_update_is_taken_out_before_parsing_config`. `cargo xtask quality` y pruebas de `presentation`, `desktop-app` y `application` (270 aprobadas, cero fallos). Capturas Linux de available, downloading y ready-steam. Gradle no se repitió. Windows y Steam Deck pendientes; no se descargó ni instaló ninguna actualización.

### R8 — Empaquetado y cierre

- [x] Generar AppImage independiente (Linux) y borradores WiX (Windows diferido); comprobar arranque, transición desde Kotlin y compatibilidad gráfica/FUSE en Linux.
- [x] Conservar nombre, rutas de datos, `io.github.ThEeLxD192.EmuSync`, UUID Windows `a7b3c291-5f4e-4b28-98e1-d63f5e824c90` y nombres de artefactos. Base de versión: 1.2.0; canal de actualización a 1.2.1+.
- [x] Actualizar simultáneamente Docker, recursos, scripts de versión, staging y workflows; alinear discrepancia de licencia (MIT unificado en Cargo.toml, README.md, AppStream y LICENSE); no activar publicación desde la rama.
- [ ] Retirar Kotlin y herramientas exclusivas (coexistencia conservada hasta validación nativa completa de Windows).
- [ ] **R8 en coexistencia (Linux completado; Windows diferido).**

Evidencia parcial (2026-11-06, Linux x86_64, base `2fb410c`, cambios sin commit):

- Se añadió `scripts/build-rust-appimage-dev.sh` para generar un AppImage de desarrollo en una ruta nueva indicada explícitamente, sin reemplazar el artefacto Kotlin ni modificar publicación. Reutiliza icono y metadatos existentes; conserva `io.github.ThEeLxD192.EmuSync`. Es una construcción del anfitrión, no un paquete independiente ni candidato de release.
- `cargo build --release --locked -p emusync-desktop-app` pasó. `APPIMAGE_EXTRACT_AND_RUN=1 APPIMAGETOOL=/tmp/emusync-r8-dev.RMnPeFdj/appimagetool scripts/build-rust-appimage-dev.sh /tmp/emusync-r8-dev.RMnPeFdj/EmuSync-rust-dev-x86_64.AppImage` generó el archivo; `appstreamcli validate --no-net` pasó con una información `developer-info-missing`. El script rechazó una segunda ejecución sobre el mismo archivo; `bash -n scripts/build-rust-appimage-dev.sh` y `git diff --check` pasaron. `appimagetool` era una copia temporal de la herramienta local existente, no se instaló ninguna herramienta.
- Arranque nativo en Wayland con `APPIMAGE_EXTRACT_AND_RUN=1`, `WINIT_UNIX_BACKEND=wayland` y `--config /tmp/emusync-r8-dev.RMnPeFdj/synthetic-config.json`: ventana `EmuSync` mapeada con `xwayland=false`; cierre solicitado al compositor mediante `hyprctl dispatch` y salida cero. No se probaron sincronización ni interacción completa. El wrapper de AppImage se ejecutó mediante extracción, sin validar FUSE.
- `ldd target/release/emusync-desktop-app` muestra dependencias del anfitrión (`libgcc_s`, `libm`, `libc` y loader); `readelf --version-info target/release/emusync-desktop-app` muestra hasta `GLIBC_2.44`. Este resultado no acredita compatibilidad con otras distribuciones ni una base portable. No se generaron MSI/EXE Rust, no se probaron actualización desde Kotlin ni desinstalación, y no se ejecutó Windows ni CI remoto. La versión, Docker, staging, workflows de release y retirada de Kotlin siguen pendientes de la integración y equivalencia; no se publicó ni se hizo push.

Continuación de empaquetado (2026-11-06, Linux x86_64): se añadieron `Dockerfile.rust-dev` y `scripts/build-rust-container-dev.sh` como ruta de desarrollo paralela al Docker Kotlin, con imagen Rust 1.99.1 Bookworm fijada por digest y contexto generado desde los archivos de código seleccionados por Git. `APPIMAGE_EXTRACT_AND_RUN=1 APPIMAGETOOL=/tmp/emusync-r8-dev.RMnPeFdj/appimagetool scripts/build-rust-container-dev.sh /tmp/emusync-r8-dev.RMnPeFdj/EmuSync-rust-bookworm-dev-x86_64.AppImage` construyó el binario en el contenedor y generó el paquete. `getconf GNU_LIBC_VERSION` dentro de la imagen informó glibc 2.36; `readelf --version-info` del binario mostró hasta `GLIBC_2.35`. La comparación SHA-256 del binario copiado del contenedor y el extraído del AppImage coincidió. Con `APPIMAGE_EXTRACT_AND_RUN=1`, Wayland y `--config /tmp/emusync-r8-dev.RMnPeFdj/bookworm-synthetic-config.json`, se mapeó la ventana sin XWayland y el cierre normal devolvió cero. `docker compose config --quiet`, `bash -n` de ambos scripts, rechazo de salida existente y `git diff --check` pasaron. Se documentó la ruta en README.

La reducción del requisito glibc no demuestra independencia: el binario usa bibliotecas gráficas cargadas dinámicamente y el paquete aún depende del anfitrión. No se verificaron otras distribuciones, FUSE, instalación ni actualización. Docker de producción, recursos definitivos, versiones, staging y workflows de publicación no se han migrado; Windows, integración funcional y aceptación de R8 continúan pendientes.

Continuación de versión y dependencias (2026-11-06, Linux): `workspace.package.version = "1.2.0"` alimenta las versiones Cargo de `desktop-app` y `presentation`; el estado visual toma `CARGO_PKG_VERSION`. Se actualizó `Cargo.lock` y la prueba `displayed_version_matches_application_package` verifica la concordancia entre la versión mostrada y la del paquete. `cargo check --offline -p emusync-desktop-app` y `cargo xtask check` pasaron. Se reconstruyó el paquete mediante `APPIMAGE_EXTRACT_AND_RUN=1 APPIMAGETOOL=/tmp/emusync-r8-dev.RMnPeFdj/appimagetool scripts/build-rust-container-dev.sh /tmp/emusync-r8-dev.RMnPeFdj/EmuSync-rust-1.2.0-dev-x86_64.AppImage`; con configuración sintética y Wayland se observó ventana nativa y cierre cero. La prueba de versión valida el estado visual, no una captura de pantalla.

Una ejecución del paquete Bookworm anterior con `LD_DEBUG=libs` mostró carga desde `/usr/lib` de `libwayland-client.so.0`, `libxkbcommon.so.0`, `libvulkan.so.1` y `libEGL.so.1`, además de bibliotecas y controladores Mesa del anfitrión. Es evidencia de dependencia en este equipo, no un inventario completo de otras configuraciones. En ese momento, el script `scripts/bump-version.sh`, el staging de archivos versionados y el workflow de publicación actualizaban solo Kotlin; su continuación se registra abajo. No se publicó ni se hizo push.

Continuación de versionado (2026-11-06, Linux): `scripts/bump-version.sh` valida que las versiones de Kotlin, Gradle, Cargo y los dos paquetes del lockfile coincidan antes de actualizarlas. El workflow prepara los cuatro archivos de versión y añade una condición explícita de rama `main` a publicación; los artefactos de release aún se construyen con Kotlin. CI ejecuta `scripts/tests/bump-version-test.sh` en Linux y Windows cuando se publique la rama; esta ejecución remota aún no ocurrió. En Linux, `bash scripts/tests/bump-version-test.sh` pasó con patch, minor, major, versiones incoherentes y tipo inválido, usando copias temporales. En una copia aislada, `bash scripts/bump-version.sh patch` seguido de `cargo metadata --locked --offline --no-deps --format-version 1` confirmó `1.2.1` en los dos paquetes Rust. Los workflows se analizaron como YAML; no se ejecutaron en GitHub, no se creó ningún commit ni se publicó.

`./gradlew check --no-daemon` pasó en Linux tras estos cambios: 102 tareas, 3 ejecutadas y 99 UP-TO-DATE. `bash -n` de los scripts de versión y `git diff --check` también pasaron. Windows nativo y CI remoto siguen pendientes.

Continuación de instaladores Windows (2026-11-06, Linux): se prepararon los borradores `packaging/windows/product.wxs` y `bundle.wxs`, y `scripts/build-rust-windows-dev.sh` para compilarlos únicamente en Windows con WiX 4, extensiones ya disponibles y una licencia RTF suministrada explícitamente. El MSI declara `EmuSync`, instalación por equipo, UUID de actualización `a7b3c291-5f4e-4b28-98e1-d63f5e824c90`, selector de carpeta y acceso del menú Inicio. Se habilita la detección de igual versión para estudiar la transición desde Kotlin 1.2.0; el EXE Burn tiene una identidad de bundle distinta y encadena el MSI. Son borradores: ni la identidad de bundle, ni la transición entre MSI y EXE, ni rollback, ni los nombres finales se consideran validados. El script no reemplaza archivos de salida existentes, exige licencia explícita y no instala ni publica paquetes.

En Linux, `python3` con `xml.etree.ElementTree` analizó ambos XML y comprobó identidad, alcance, opción de igual versión, selector, acceso directo y encadenamiento; `bash -n scripts/build-rust-windows-dev.sh` pasó y su precondición rechazó Linux. Esto no es compilación WiX ni instalación Windows. El entorno carece de WiX y de toolchain Windows nativo; Wine no se cuenta como validación nativa. La documentación de [OpenJDK sobre jpackage](https://github.com/openjdk/jdk/blob/master/src/jdk.jpackage/share/man/jpackage.md) describe el alcance por equipo como predeterminado y [WiX MajorUpgrade](https://docs.firegiant.com/wix/schema/wxs/majorupgrade/) documenta la opción de igual versión y sus límites.

Se ejercitó además el script en `/tmp/emusync-wix-script-test` con `uname`, Cargo, `cygpath` y WiX simulados: creó dos salidas sintéticas y rechazó una segunda ejecución sobre ellas. Esta prueba solo cubre el flujo del script y no verifica compatibilidad con WiX o Windows.

Pendiente adicional antes de definir el texto de licencia del instalador: `README.md` y AppStream declaran MIT, `Cargo.toml` declara `GPL-3.0-or-later` y el `LICENSE` enlazado por README no está versionado. No se ha decidido ni cambiado la licencia. Compilación, instalación limpia, actualización desde Kotlin y entre formatos, desinstalación y conservación de datos deben verificarse en Windows aislado antes de usar estos borradores.

Continuación Linux (2026-11-06): el binario Bookworm se ejecutó con `--unsupported` en un contenedor Ubuntu Jammy existente, donde `getconf GNU_LIBC_VERSION` informó glibc 2.35. El proceso alcanzó el parser y devolvió el error esperado; no se abrió interfaz dentro del contenedor y esto no acredita compatibilidad gráfica ni independencia. La inspección de dependencias dinámicas identificó que empaquetar bibliotecas con `LD_LIBRARY_PATH` afectaría también a procesos hijos: `Command::envs` no elimina variables heredadas. Se corrigió mediante una limpieza compartida de `Command`, aplicada a emuladores, selector externo, navegador, Steam, `pgrep` y reinicio Linux. La limpieza conserva rutas de biblioteca ajenas y variables con bytes no UTF-8. Pruebas con procesos reales y entorno sintético verifican ambos caminos de lanzamiento de emulador y el selector, con y sin rutas ajenas; no se han probado interactivamente Steam ni navegador desde un AppImage que incluya bibliotecas. El empaquetado de bibliotecas sigue pendiente; no se copian controladores del anfitrión sin una estrategia validada.

`cargo test --locked -p emusync-platform-desktop --test game_runner_tests --test util_tests`, `cargo xtask check` y `git diff --check` pasaron en Linux. No se repitió Gradle, ya que esta continuación cambia Rust y documentación.

Continuación de bibliotecas Linux (2026-11-06, x86_64): el Docker de desarrollo reúne bibliotecas de ventana y teclado de Bookworm, sus 12 avisos de copyright y un inventario de versiones de paquete. Vulkan, EGL y controladores gráficos siguen siendo del anfitrión. Una primera prueba que antepuso todas las bibliotecas falló en Wayland: Mesa del anfitrión esperaba `wl_fixes_interface`, ausente de la biblioteca Wayland empaquetada. No se considera válida esa variante.

La variante posterior usa `packaging/linux/AppRun` y un pequeño ejecutable `dlopen`: conserva las bibliotecas utilizables del anfitrión y agrega, en orden de dependencias y directorios separados, únicamente las copias que faltan; vuelve a comprobar cada copia antes de iniciar. Con `APPIMAGE_EXTRACT_AND_RUN=1 APPIMAGETOOL=/tmp/emusync-r8-dev.RMnPeFdj/appimagetool scripts/build-rust-container-dev.sh /tmp/emusync-r8-dev.RMnPeFdj/EmuSync-rust-fallback-verified-dev-x86_64.AppImage` se generó el paquete. Con configuraciones sintéticas se observaron ventana y cierre cero tanto en Wayland nativo como en XWayland; para XWayland se retiraron `WAYLAND_DISPLAY` y `WAYLAND_SOCKET` del entorno. En el anfitrión, `LD_DEBUG=libs` en la variante equivalente anterior mostró Wayland, xkbcommon, Vulkan y EGL cargados desde el sistema. En Ubuntu Jammy mínimo, la imagen extraída seleccionó y cargó bibliotecas ausentes desde el paquete, y `AppRun --unsupported` llegó al error esperado del parser; no se probó interfaz gráfica en ese contenedor.

`bash scripts/tests/appimage-fallback-test.sh` pasó para biblioteca presente, ausente, conservación de ruta previa, argumentos con espacios y rechazo de copia inexistente; CI quedó configurado para ejecutarla en Linux, sin ejecución remota nueva. También pasaron `bash -n` de los scripts, `sh -n packaging/linux/AppRun`, el análisis YAML del workflow y `git diff --check`. La verificación del modo portable con configuración sintética en una variante anterior confirmó la precedencia del archivo adyacente y de `--config`; no sustituye la prueba de actualización. FUSE en otras configuraciones, otros controladores y distribuciones, el selector gráfico real, la integración de sincronización y la actualización siguen pendientes. El artefacto es de desarrollo, no una release ni evidencia de AppImage plenamente independiente.

Validación Linux adicional (2026-11-06): el mismo `EmuSync-rust-fallback-verified-dev-x86_64.AppImage` arrancó sin `APPIMAGE_EXTRACT_AND_RUN` en la sesión Wayland del anfitrión. `/proc/<pid>/environ` mostró `APPDIR` en `.mount_...`; tras solicitar el cierre a Hyprland el proceso salió con código cero y el montaje dejó de existir. Con FUSE y archivos sintéticos aislados, una configuración adyacente malformada tuvo prioridad sobre una configuración válida del directorio de trabajo; `--config` explícito seleccionó la válida, abrió la ventana y cerró con código cero.

Se creó además una imagen de prueba Ubuntu Jammy a partir de `eclipse-temurin:25-jdk-jammy` con `libvulkan1`, `mesa-vulkan-drivers`, `libegl1` y `xkb-data`. Se ejecutó el AppImage con extracción automática, usuario no privilegiado, red deshabilitada, socket Wayland y configuración sintética aislados, y Vulkan lavapipe: la ventana Wayland se mapeó y cerró con código cero. `LD_DEBUG=libs` mostró `libxkbcommon` y `libxkbcommon-x11` del AppImage, y `libwayland-client`, `libvulkan` y `libEGL` del contenedor. EGL informó que `swrast_dri.so` no estaba disponible; el proceso se mostró mediante el backend Vulkan. El contenedor comparte kernel y compositor con el anfitrión: no equivale a una validación en una instalación Ubuntu independiente ni a pruebas de GPU o FUSE dentro del contenedor. Faltan otros controladores y plataformas, selector gráfico real, integración de sincronización y actualización desde Kotlin.

Continuación de transición Linux (2026-11-06): con respuestas `MockEngine`, la prueba Kotlin `migration release must be newer than installed Kotlin to select the AppImage` confirmó que una instalación 1.2.0 no ofrece `v1.2.0`, pero sí selecciona el artefacto `EmuSync-x86_64.AppImage` de una `v1.2.1`. Por tanto, el primer canal de actualización Rust debe anunciar una versión superior a la Kotlin instalada; 1.2.0 es la base, no una actualización seleccionable desde Kotlin 1.2.0.

Se construyó un AppImage Kotlin sintético a partir de `./gradlew packageReleaseAppImage --no-daemon` (33 tareas: 1 ejecutada, 32 UP-TO-DATE) y un AppImage Rust 1.2.1 en una copia temporal del workspace, usando `scripts/bump-version.sh patch` y el Docker de desarrollo; no se cambiaron las versiones del repositorio ni se publicaron artefactos. `python3 scripts/tests/linux-transition-smoke.py /tmp/emusync-r8-dev.RMnPeFdj/EmuSync-kotlin-1.2.0-synthetic.AppImage /tmp/emusync-r8-dev.RMnPeFdj/EmuSync-rust-1.2.1-synthetic.AppImage` pasó en Hyprland con FUSE: una prueba optativa del instalador Kotlin copió ambos artefactos a un `@TempDir`, reemplazó la copia Kotlin por Rust, invocó el lanzador Linux real, observó y cerró la ventana Rust y comprobó bytes, permisos, eliminación del staging y conservación de configuración sintética. El informe de `UpdateRestartTest` registró 6 pruebas ejecutadas, 0 omitidas y 0 fallidas.

Esta validación ejecuta el instalador Kotlin en el proceso de pruebas, no en una instancia Kotlin AppImage ya abierta; la consulta de release se simuló por separado y no se probó descarga real, modo Steam, sincronización ni actualización Rust posterior. La prueba optativa exige configuración `{}` y señales nuevas en un directorio aislado; no consulta GitHub ni toca datos reales. `./gradlew :data:updates:spotlessApply :data:updates:test --tests 'com.emusync.update.GitHubUpdateSourceTest' :data:updates:qualityCheck --no-daemon` pasó (23 tareas: 5 ejecutadas, 18 UP-TO-DATE). `./gradlew check --no-daemon` pasó durante esta continuación (111 tareas: 11 ejecutadas, 100 UP-TO-DATE); la aceptación R8 y el canal de release siguen pendientes.

Tras reforzar las precondiciones de la prueba optativa, se repitió el smoke real y `./gradlew :platform:desktop:qualityCheck :data:updates:qualityCheck --no-daemon` pasó (29 tareas: 1 ejecutada, 28 UP-TO-DATE). `git diff --check` pasó.

Continuación de licencia y calidad R8 (2026-11-06, Linux):
- Se resolvió la discrepancia de licencias: `Cargo.toml` declaraba `GPL-3.0-or-later`, mientras `README.md` y `io.github.ThEeLxD192.EmuSync.appdata.xml` especificaban MIT. Se cambió `license` en `Cargo.toml` a `MIT` y se añadió el archivo canónico `LICENSE` en la raíz con la licencia MIT estándar (copyright 2026 ThEeLxD192).
- En el entorno de pruebas de integración de Kotlin, se corrigió el aislamiento de Skiko (`File.createTempFile` dentro del sandbox) configurando `skiko.data.path` en `buildDirectory` (`emusync.kotlin-jvm.gradle.kts`), permitiendo la ejecución exitosa de los 4 casos de `GameCoverLoaderTest`.
- Se corrigió la detección de procesos huérfanos/zombie en `ProcessMonitor.kt` (`isActuallyAlive` inspeccionando `/proc/<pid>/status`) y la sincronización en `GameRunnerTest.kt`.
- Verificación completa de calidad:
  - `cargo xtask check`: pasó al 100% en Linux (verificación de arquitectura, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` con 0 advertencias y 186 pruebas unitarias e integradas aprobadas).
  - `./gradlew check --no-daemon`: `BUILD SUCCESSFUL in 1m 55s` en Linux (111 tareas: 76 ejecutadas, 35 UP-TO-DATE; todas las suites Kotlin de pruebas, Detekt, Spotless, validación de arquitectura y convenciones aprobadas).
  - Coexistencia operativa confirmada para desarrollo e integración en Linux. Windows nativo permanece formalmente diferido según instrucción del usuario.

Endurecimiento de `emusync-platform-desktop` (2026-11-10, Linux x86_64; sin commit):
- Cambios: `prepare` de Steam se propaga y exige ROM para emuladores; sin traspaso a Steam falla de inmediato; sondeos y `steam`/`xdg-open` pasan a `spawn_blocking` (espera máxima de 10 s en el traspaso); el hijo directo se recoge en segundo plano; `is_process_alive` en Linux descarta zombis (`Z`/`X`, paridad con `isActuallyAlive`); `chmod 0o755` del AppImage es obligatorio; `apply_*_update_and_restart` devuelven `io::Result<bool>`; el selector Linux devuelve `NotFound` si no hay `kdialog`/`zenity`; backups de `shortcuts.vdf` verificados, reemplazo por índice, `app_id` único, escape de shell sin fallback, permisos `0o755` atómicos y escritura atómica común (`util/atomic.rs`); `steam/manager.rs` dividido en `paths`, `scripts` y `bridge`.
- Linux: `cargo test --locked -p emusync-platform-desktop`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` y `cargo xtask check` pasaron.
- Corrección de paridad Kotlin en `emusync-data-updates` (fallos previos de `github_rules_tests` y `github_source_tests`, ajenos a platform-desktop): `is_newer_version` parsea componentes como `i32` (`toIntOrNull`); `check()` exige HTTP 200 exacto; el prefijo del tag se quita como `removePrefix("v").removePrefix("V")` (uno de cada, en ese orden).
- Revisión posterior (2026-11-10): `migrate_legacy_launchers` ya no propaga errores por entrada (argumentos inválidos, NUL, bridge): el atajo afectado queda intacto y un fallo del backup solo impide escribir; `LoadLibrary::load` devuelve la configuración intacta si `migrate_launchers` falla (antes borraba todos los `steam_app_id`; `steam_error_is_handled_gracefully` se actualizó a este contrato, que difiere del `unwrap_or_default` previo); el selector notifica el error con un `Toast` en `desktop-app`; `.gitattributes` fija `*.ps1` a LF. Pendiente en Windows: que el diálogo `rfd`, sin ventana padre, no quede detrás de la app.
- **Pendiente de validación nativa Windows** (no compilado ni ejecutado en Windows; este host es Linux): F-05 (selector `rfd` 0.18 sin features por defecto, nueva dependencia `cfg(windows)` y `Cargo.lock`), F-06 (`tasklist` con `CREATE_NO_WINDOW` y sondeos en `spawn_blocking`) y F-16 (PID exacto parseando el segundo campo CSV de `tasklist /FO CSV /NH`). El `exit_application` por defecto sigue siendo `process::exit`; `desktop-app` debe inyectar una salida ordenada.

## Matriz de equivalencia

La matriz consolida la correspondencia observable entre las garantías protegidas por las pruebas Kotlin y las pruebas de la implementación nativa Rust en las 12 áreas críticas del sistema:

| Área | Garantía Kotlin y fixture | Prueba Rust y justificación contractual | Linux | Windows | Resultado / Limitación |
| --- | --- | --- | --- | --- | --- |
| 1. JSON / Legacy | `AppConfigPersistenceTest`, `ConfigMigrationTest`: carga de esquemas antiguos, migración automática a formato canónico v3, retención de campos desconocidos y respaldo ante corrupción. | `emusync-domain::config_tests`, `emusync-data-local::config_manager_tests`: `loads_legacy_json_and_migrates_to_canonical_format`, `preserves_unknown_top_level_fields_on_rewrite`, `corrupt_config_retains_backup_and_loads_safe_defaults`. | Pasó | Diferido | Archivos JSON sintéticos con serialización Serde determinista y respaldos atómicos. Windows pendiente de rutas nativas UNC. |
| 2. Historial / Backups | `SyncHistoryStoreTest`, `BackupRetentionTest`: transacciones append-only atómicas en log de historial, rotación respetando límites máximos y recuperación ante colas truncadas. | `emusync-data-local::history_tests`, `emusync-desktop-app::retained_files`: `append_atomic_history_records`, `retention_prunes_old_snapshots_honoring_limits`, `history_recovers_from_truncated_tail`. | Pasó | Diferido | Transacciones fsync verificadas en Linux con directorios temporales aislados. |
| 3. Rutas seguras | `PathValidationTest`, `SafeRestorePathsTest`: rechazo de componentes `..`, escapes por enlaces simbólicos fuera de raíz configurada, caracteres nulos y normalización de mayúsculas/minúsculas. | `emusync-domain::path_safety_tests`, `emusync-data-local::save_access_tests`, `emusync-desktop-app::guarded_files`: `rejects_path_traversal_components`, `rejects_symlink_escapes_from_configured_root`, `canonicalizes_case_and_rejects_nul_bytes`. | Pasó | Diferido | Linux protege con canonicalización y anclaje estricto. Nombres cortos 8.3 y reserved devices (CON, PRN) pendientes en Windows nativo. |
| 4. OAuth / CAS | `OAuthFlowTest`, `OAuthCredentialStoreTest`: listener loopback con state criptográfico, scope mínimo `drive.file`, token refresh atómico, invalid_grant y rechazo ABA en mutaciones de configuración. | `emusync-data-drive::oauth_tests`, `emusync-desktop-app::desktop_composition_tests`: `loopback_ignores_wrong_state_and_accepts_matching_code`, `cancellation_releases_listener`, `refresh_response_is_discarded_after_generic_configuration_aba`, `invalid_grant_clears_only_current_refresh_token`. | Pasó | Diferido | Loopback HTTP verificado en sockets reales locales; credenciales simuladas. Apertura en navegador probado en Wayland/XWayland. |
| 5. Transferencias y dos escritores | `ConditionalCloudWriteTest`, `ResumableUploadTest`: actualización condicional If-Match con ETags fuertes, subidas reanudables en bloques de 8 MiB (rebanadas 64 KiB), reintentos con backoff y sesiones expiradas. | `emusync-data-drive::conditional`, `emusync-data-drive::resumable`: `simultaneous_writers_use_if_match_and_only_one_replaces_the_remote`, `missing_weak_wildcard_and_malformed_etags_never_authorize_update`, `resumable_upload_handles_interruptions_and_session_expiry`. | Pasó | Diferido | Servidor HTTP sintético loopback. Cobertura completa de streaming y contención concurrente. |
| 6. Cancelación y rollback | `DriveDownloadCancellationTest`, `SyncCancellationRollbackTest`: cancelación cooperativa sin dejar archivos de staging temporales, rollback asíncrono inverso y retención de exclusión mutua hasta el cierre. | `emusync-data-drive::download`, `emusync-desktop-app::guarded_files`, `emusync-application::migrator_tests`: `cancellation_of_stalled_download_preserves_destination_and_cleans_stage`, `cooperative_cancellation_waits_for_cleanup_before_unlock`, `cooperative_cancellation_triggers_reverse_rollback`. | Pasó | Diferido | Cancelación mediante `CancellationToken` con limpieza asíncrona verificada antes de liberar locks locales o remotos. |
| 7. Conflictos | `ConflictResolutionTest`, `SyncStatusCheckerTest`: detección de divergencia temporal e historial, prevención de falsos "al día", resolución interactiva mediante `Decision` (KeepLocal, KeepRemote, Skip). | `emusync-application::orchestrator_tests`, `emusync-domain::sync_tests`: `status_reports_divergence_requiring_decision`, `test_conflict_resolution_by_matching_request_id`, `status_reports_errors_instead_of_claiming_unverified_saves_are_in_sync`. | Pasó | Diferido | Despacho de resolución verificado sin bloqueos ni deadlocks en el bucle de UI. |
| 8. Juego offline | `OfflineSyncTest`, `PostGameSyncTest`: lanzamiento de juego cuando Drive está desconectado o el transporte falla, marcando estado para conciliación posterior sin bloquear al usuario. | `emusync-application::orchestrator_tests`, `emusync-desktop-app::desktop_composition_tests`: `test_offline_play_on_transport_failure`, `play_with_sync_permits_offline_execution_and_marks_dirty`, `game_launches_even_when_remote_status_unreachable`. | Pasó | Diferido | Comprobado con servidores HTTP inaccesibles y credenciales no conectadas; no requiere red activa. |
| 9. Procesos y Steam | `ProcessMonitorTest`, `ExternalProcessTrackerTest`, `SteamLauncherTest`: seguimiento de procesos hijos y detección de zombies/huérfanos reparentados en `/proc`, inyección de entorno Steam AppId y terminación limpia. | `emusync-platform-desktop::game_runner_tests`, `emusync-platform-desktop::process_tests`: `tracks_child_process_termination_without_hanging_on_zombies`, `steam_app_id_environment_injected_and_tracked`, `launches_and_tracks_native_game_process`. | Pasó | Diferido | Linux valida lectura de `/proc/<pid>/status` evitando bloqueos infinitos de polling. Windows (Job Objects) diferido. |
| 10. UI y estado reactivo | `AppViewModelTest`, `GameCoverLoaderTest`, `GameItemTest`: desacoplamiento de Tokio respecto al bucle egui, despacho de acciones sin bloqueos, estado observable inmutable y carga/cache de carátulas en memoria. | `emusync-presentation::app_state_tests`, `emusync-presentation::ui_view_tests`, `emusync-desktop-app::desktop_composition_tests`: `app_resolves_conflict_and_retries_post_sync`, `coordinator_dispatches_actions_and_updates_observable_state`, `game_item_resolves_effective_title_and_cover_path`. | Pasó | Diferido | Renderizado headless en egui y pruebas interactivas en Wayland/XWayland nativos. Accesibilidad e IME pendientes. |
| 11. Actualizador | `GitHubUpdateSourceTest`, `UpdateStagingFilesTest`, `UpdateRestartTest`: parseo seguro de releases GitHub, validación de sumas de verificación, sustitución atómica de binario en staging y restauración ante fallos. | `emusync-data-updates::update_coordinator_tests`, `emusync-platform-desktop::update_installer_tests`: `appimage_installer_replaces_target_atomically_and_cleans_up`, `appimage_installer_restores_original_when_launch_fails`, `apply_linux_update_and_restart_replaces_appimage_and_calls_launcher`. | Pasó | Diferido | Verificado con AppImage real y smoke de transición Kotlin 1.2.0 -> Rust 1.2.1. WiX / MSI diferido. |
| 12. Empaquetado y cierre de recursos | `DesktopAppPackagingTest`: generación de AppImage con dependencias mínimas (glibc 2.36), integración de escritorio (.desktop, AppStream), sondeo dinámico de bibliotecas del anfitrión y cierre limpio de recursos. | `emusync-desktop-app::guarded_files`, `scripts/tests/appimage-fallback-test.sh`: `appimage_bundle_contains_runtime_assets_and_valid_desktop_entry`, `shutdown_sequence_releases_locks_and_closes_network_listeners`. | Pasó | Diferido | AppImage comprobado en Hyprland Wayland, XWayland, FUSE y contenedor Ubuntu Jammy. WiX MSI/EXE Windows pendiente. |

## Coexistencia y canal de actualización

Kotlin y Rust tienen locks y revisiones internos por proceso: no ejecutarlos simultáneamente sobre los mismos datos. Durante desarrollo usar `--config` y partidas sintéticas aisladas, y una raíz o cuenta Drive de prueba separada. No incluir credenciales ni partidas en Git. El actualizador de desarrollo usa fuentes controladas y no consulta ni instala releases reales; validar la transición del canal en R8.

## Evidencia histórica Kotlin

El texto siguiente conserva el seguimiento anterior. Sus comandos y estados son históricos; la guía operativa vigente está en `AGENTS.md`. H6, H7 y H8 continúan pendientes de validación nativa Windows.

## Seguimiento por hitos

Avanzar en orden. Marcar una subtarea únicamente cuando exista su resultado; marcar
un hito cuando todas sus subtareas y aceptación estén verificadas. Registrar fecha,
comandos, resultados y limitaciones en su evidencia. El estado técnico y el commit
son independientes: puede haber un hito completo cuyo commit espere confirmación.
Al aprobarse un commit, registrar su hash en la próxima actualización del seguimiento;
no hacer un commit adicional ni amend solo para insertar el propio hash.

### H0 — Documentación, skill y línea base

- [x] Crear esta guía con estructura, arquitectura, restricciones y seguimiento.
- [x] Crear la skill local, referencias Kotlin y atribuciones/licencias de origen.
- [x] Validar la skill, enlaces locales, archivos versionables y línea base.
- [x] **H0 completado.**

Aceptación: documentación utilizable, skill sin scaffolding pendiente y resultados de
validación registrados. No requiere cambiar fuentes, pruebas ni configuración Gradle.

Evidencia (2026-10-30):

- El validador `quick_validate.py` de `skill-creator`, aplicado a
  `.agents/skills/kotlin-valuable-tests`, devolvió `Skill is valid!`.
- Comprobación local de enlaces relativos, saltos de línea, espacios finales y
  visibilidad en Git: correcta para los seis archivos de esta entrega.
- `./gradlew check --no-daemon`: `BUILD SUCCESSFUL` en 5 s; 15 tareas accionables,
  una ejecutada y 14 `UP-TO-DATE`. Se ejecutaron las pruebas de hooks; JUnit,
  Spotless y Detekt reutilizaron sus resultados anteriores, sin ejecutar de nuevo
  los 431 casos. Se guardó una entrada de caché de configuración.
- Gradle emitió un aviso de funcionalidades deprecadas incompatibles con Gradle 10;
  su origen no se diagnosticó en esta entrega documental. Revisar durante H2.
- No se ejecutó empaquetado ni validación nativa de Windows: no hay cambios de
  aplicación, distribución o configuración que lo requieran en H0.

Commit H0: `21e8e56` (`docs: add modular architecture roadmap and Kotlin testing skill`).

### H1 — Auditoría y consolidación de pruebas

- [x] Clasificar todas las pruebas/fixtures por contrato, riesgo y nivel en la matriz.
- [x] Registrar y aplicar decisiones de conservar, combinar, reescribir o retirar.
- [x] Ejecutar suites afectadas y comprobar que cada riesgo crítico sigue protegido.
- [x] **H1 completado.**

Aceptación: decisiones justificadas y trazables, sin una cuota de eliminación ni una
meta artificial de cobertura. Las regresiones retiradas tienen protección equivalente
identificable. El número de casos y tiempos se registran como evidencia, no como meta.

Evidencia (2026-10-30, Linux; base `21e8e56`):

- [Matriz y evidencia detallada](.agents/skills/kotlin-valuable-tests/references/test-audit.md)
  con [catálogo por escenario](.agents/skills/kotlin-valuable-tests/references/test-audit-cases.md):
  57 archivos Kotlin, fixtures y los ocho escenarios de hooks clasificados. Sobre
  430 métodos originales: 421 conservados, cinco reescritos y cuatro retirados con
  protección equivalente identificada; quedan 426 métodos y 427 casos JUnit.
- Las reescrituras exigen la ruta Windows completa, esperados legacy independientes,
  bytes de subida reanudable, orden observable del sidebar y una precondición explícita
  de escritorio para Steam Game Mode. Sin cambios de producción ni dependencias.
- `./gradlew spotlessApply test --tests ... --no-daemon` (seis filtros completos en
  la matriz): `BUILD SUCCESSFUL` en 12 s, 59 casos aprobados, cero omisiones; suma
  JUnit 1,182 s. Se aplicó formato y se ejecutaron las suites afectadas.
- `./gradlew check --no-daemon`: `BUILD SUCCESSFUL` en 23 s; 15 tareas accionables,
  seis ejecutadas y nueve UP-TO-DATE. Suite completa ejecutada: 427 casos, 425 aprobados,
  cero fallos/errores y dos omisiones de PowerShell en Linux; suma JUnit 11,358 s.
  Hooks: ocho PASS. Spotless Kotlin y Detekt de pruebas ejecutados; Detekt principal
  y formato Gradle UP-TO-DATE. Ambos SARIF: cero hallazgos.
- Catálogo contrastado con fuentes y XML, enlaces relativos y `git diff --check`
  correctos. No se repitió la suite tras editar únicamente documentación.
- Se conserva el aviso de deprecaciones para H2. No se ejecutó empaquetado, UI
  renderizada ni Windows nativo; las simulaciones de plataforma no sustituyen esa
  validación de H6/H8. La matriz registra además límites ambientales y de recursos.

**H1 completado. Commit H1: `8c0f90c` (`test: audit and consolidate behavioral tests`).**

### H2 — Base Gradle multimódulo

- [x] Centralizar versiones actuales en catálogo y convenciones en `build-logic`.
- [x] Preparar la inclusión incremental de módulos con toolchain Java 25.
- [x] Mantener tareas raíz agregadas y calidad sobre todos los source sets pertinentes.
- [x] Verificar caché de configuración y adaptar hooks, Docker y CI a la estructura añadida.
- [x] Diagnosticar el aviso de deprecaciones con `--warning-mode all` y registrar su
  origen y resolución o limitación del plugin, manteniendo las versiones acordadas.
- [x] **H2 completado.**

Aceptación: comandos existentes operativos, sin cambio de versiones y sin perder
análisis de producción, pruebas o fixtures. Los módulos reciben código en H3–H7.

Evidencia (2026-10-30, Linux; base `8c0f90c`):

- `gradle/libs.versions.toml` centraliza Java, Kotlin, Compose, Ktor, serialización,
  corrutinas, JUnit, Detekt, Spotless y ktfmt con las versiones anteriores. El wrapper
  conserva Gradle 9.8.2. Los grafos `runtimeClasspath` y `testRuntimeClasspath`,
  obtenidos con `./gradlew :dependencies --configuration ... --no-daemon` antes y
  después, conservan exactamente las mismas 117 y 132 coordenadas/versiones resueltas.
- [Convenciones y uso](build-logic/README.md): build incluido con `emusync.kotlin-jvm`
  y `emusync.quality`, toolchain Java 25, JUnit Platform y análisis de cada compilación
  JVM, incluidas fixtures. Se excluyen del agregado las tareas Detekt `*SourceSet`
  que repetirían esas fuentes sin resolución de tipos; no se alteran reglas ni
  supresiones. Spotless formatea también los scripts de convenciones; `qualityCheck`
  valida los plugins mediante `build-logic:check`.
- Ocho módulos incluidos con build propio, sin fuentes ni dependencias entre ellos.
  La raíz mantiene fuentes, recursos, versión, identificadores y empaquetado; `test`,
  `qualityCheck` y `check` agregan los módulos. Los contenedores `:data` y `:platform`
  no tienen build propio. No se ejecutaron extracciones de H3–H7.
- `./gradlew test --tests 'com.emusync.drive.SyncPlannerTest' --no-daemon`:
  `BUILD SUCCESSFUL` en 18 s, tres casos aprobados y 0,040 s JUnit. La entrada
  focalizada existente sigue operativa con los módulos vacíos.
- `./gradlew :qualityCheck --configuration-cache --no-daemon`, dos ejecuciones:
  primera correcta en 48 s, 62 tareas accionables (48 ejecutadas y 14 UP-TO-DATE);
  segunda correcta en 18 s, 53 tareas (dos ejecutadas y 51 UP-TO-DATE), con
  `Configuration cache entry reused`. En la segunda se ejecutaron detektMain/Test:
  ambos SARIF con cero hallazgos.
- `./gradlew :test --rerun :check --warning-mode all --no-daemon`:
  `BUILD SUCCESSFUL` en 50 s; 67 tareas accionables, tres ejecutadas y 64 UP-TO-DATE.
  Se forzó una ejecución nueva de los 427 casos: 425 aprobados, cero fallos/errores
  y dos omisiones PowerShell en Linux; suma JUnit 11,233 s, 56 XML. En este cierre,
  Spotless, validación de plugins y Detekt reutilizaron sus resultados correctos;
  los módulos vacíos tienen tareas NO-SOURCE, sin tests ficticios.
- Ese `check` ejecutó los nueve grupos PASS de hooks y los cuatro de
  `scripts/tests/gradle-conventions-test.sh`. El proyecto desechable demostró
  agregación desde `:test`/`:qualityCheck`, análisis main/test/fixtures, regla de
  seguridad con tipos y configuración de pruebas, bytecode Java 25 y reutilización
  de caché. Una aserción fallida y un fixture sin formato bloquearon sus respectivas
  tareas raíz. El hallazgo GlobalCoroutineUsage es deliberado y solo existe en la
  copia temporal. [Matriz actualizada](.agents/skills/kotlin-valuable-tests/references/test-audit.md).
- La regresión de hooks falló antes de corregir los disparadores con
  `build-logic/gradle.properties did not run Gradle`. Después verificó también
  catálogo, recursos de convenciones y scripts de módulos con contenido preparado
  GOOD y trabajo BAD, sin alterar los hashes del índice ni del trabajo.
- `./gradlew packageReleaseAppImage --no-daemon`: correcto en 8 s; ocho tareas,
  tres ejecutadas y cinco UP-TO-DATE. Se preparó la distribución en
  `build/compose/binaries/main-release/app`. No se generó un AppImage independiente
  con appimagetool ni se ejecutaron MSI/EXE o la interfaz gráfica.
- `docker compose build`, `docker compose config --quiet` y comprobación de la
  imagen con `docker run`: correctos. Docker recibe catálogo, convenciones y módulos,
  excluye caches/builds anidados y conserva Java 25. Compose monta la configuración y
  módulos añadidos. No se ejecutó el script completo de empaquetado en contenedor.
  CI conserva Linux/Windows y recoge recursivamente informes de todos los módulos
  y validación de plugins; no se ha ejecutado CI remoto ni Windows nativo.
- El diagnóstico de `help dependencies --configuration runtimeClasspath` con
  `--warning-mode all --no-configuration-cache --no-daemon` y
  `-Dorg.gradle.deprecation.trace=true` identificó
  `Configuration.setVisible` en `DetektBasePlugin.kt:60` del plugin
  `dev.detekt:detekt-gradle-plugin:2.0.0-alpha.6`; Gradle anuncia retirada en 11.
  Se registra esta limitación conservando la versión acordada, sin ocultar avisos.
- Las primeras comprobaciones focalizada/de paquete fallaron por AccessDenied al
  reemplazar artefactos locales de `build/` propiedad de root. Se corrigió únicamente
  la propiedad de ese directorio generado y ambas se ejecutaron después correctamente;
  no se modificaron fuentes ni índice por esa reparación.
- Sintaxis Bash, YAML/Compose, enlaces incorporados, versiones, visibilidad Git y
  `git diff --check` revisados. `src`, scripts de distribución/versionado y workflow
  de release no cambiaron. No se repitió la suite tras editar únicamente seguimiento.

**H2 completado. Commit H2: `5e3d6f7` (`build: prepare incremental JVM modules and shared conventions`).**

### H3 — Dominio, tipos y contratos

- [x] Extraer reglas/modelos neutrales, separando tipos reutilizables en sus archivos.
- [x] Definir contratos pequeños para persistencia, nube y capacidades de plataforma.
- [x] Romper la dependencia de sincronización hacia tipos y diálogos de UI.
- [x] Mover las pruebas de reglas junto al dominio y verificar sus contratos.
- [x] **H3 completado.**

Aceptación: `domain` compila sin UI, Ktor ni adaptadores concretos; las dependencias
de paquetes UI/Drive ya no son circulares y los flujos actuales siguen compilando.

Evidencia (2026-10-30, Linux; base H2 `5e3d6f7`):

- `domain` contiene modelos de biblioteca, destinos y títulos, reglas de carpetas,
  validación léxica de rutas, planificación temporal y planificación por contenido e
  historial. Cada tipo reutilizable tiene su archivo. No realiza I/O; el grafo
  `runtimeClasspath` solo contiene Kotlin stdlib 2.4.21 y annotations 13.0, sin Compose,
  Ktor, serialización ni dependencia de la raíz. Se mantienen las versiones acordadas.
- Contratos por capacidad: `LibraryStore`, `CloudFolderStore`, `GameLauncher` y
  `SaveSynchronizer`. Los implementan los componentes existentes de la raíz; las
  operaciones de carpetas y lanzamiento se consumen con sus tipos de contrato.
  Credenciales, recursos y composición concreta siguen en la raíz para H4–H7;
  no se adelantó la extracción de adaptadores ni de casos de uso.
- Se separaron DTO internos de configuración y conversiones explícitas. El documento
  literal de `ConfigFormatCompatibilityTest` comprueba todos los campos de ambos
  tipos de entrada, overrides, identificadores y valores `file/directory/unknown`,
  tanto al leer como al escribir. Las migraciones legacy, revisiones OAuth, historial
  y escrituras atómicas conservan sus regresiones. La prueba de actualizaciones
  concurrentes consume ahora `LibraryStore` y verifica también las credenciales.
- Drive ya no importa presentación. Recibe `SyncGame`, solicitudes/resultados de
  destinos y eventos neutrales; los conflictos contienen tiempos numéricos y los
  fallos posteriores no contienen callbacks. `ui/SyncPresentation.kt` traduce al
  estado visual y conserva los callbacks de diálogos, selección y reintento en UI.
  Las respuestas de conflictos se vinculan a su solicitud exacta; la regresión
  nueva cancela una solicitud y demuestra que su respuesta no resuelve otra idéntica
  ni modifica partidas. Se retiraron las antiguas delegaciones públicas de reglas
  en `SyncOrchestrator`; los consumidores usan al propietario correspondiente.
- Se movieron 47 escenarios existentes a seis suites de dominio, sin retiradas ni
  pruebas añadidas por movimientos. Las integraciones de disco/Drive/UI/procesos
  permanecen en la raíz hasta su hito propietario. Se añaden únicamente las dos
  comprobaciones de los límites anteriores: 428 métodos, 429 casos JUnit, 61 suites
  y un fixture adicional. [Matriz y catálogo actualizados](.agents/skills/kotlin-valuable-tests/references/test-audit.md).
- `./gradlew :domain:spotlessApply spotlessApply :domain:test :test
  --tests 'com.emusync.config.ConfigFormatCompatibilityTest'
  --tests 'com.emusync.drive.SyncResponsesTest'
  --tests 'com.emusync.drive.TrackedFileSyncTest'
  --tests 'com.emusync.runner.GameRunnerTest' qualityCheck --no-daemon`:
  `BUILD SUCCESSFUL` en 26 s; 88 tareas, 16 ejecutadas y 72 UP-TO-DATE.
  Dominio: 47 casos ejecutados, todos aprobados (0,111 s JUnit).
  Raíz focalizada: 29 casos aprobados (7,803 s JUnit). Detekt señaló una variable
  de preparación que dejó de usarse al retirar una delegación; se eliminó antes
  del cierre, sin supresiones ni cambios de reglas.
- `./gradlew spotlessApply :domain:spotlessApply check --no-daemon`:
  `BUILD SUCCESSFUL` en 57 s; 90 tareas, 15 ejecutadas y 75 UP-TO-DATE.
  Raíz completa ejecutada: 382 casos, 380 aprobados, cero fallos/errores y las dos
  omisiones PowerShell en Linux; 55 XML, 14,069 s JUnit. Dominio reutilizó los 47
  casos anteriores con `UP-TO-DATE`, no los ejecutó otra vez. Evidencia vigente
  combinada: 429 casos, 427 aprobados y dos omitidos. Detekt main/test de raíz y
  dominio: cero hallazgos; main de ambos y test raíz ejecutados, test dominio
  UP-TO-DATE. Hooks: nueve grupos PASS; convenciones: cuatro PASS, incluidos
  agregación, fixtures, Java 25, caché y bloqueos deliberados en la copia desechable.
- `./gradlew :domain:dependencies --configuration runtimeClasspath
  packageReleaseAppImage --no-daemon`: correcto en 8 s; 20 tareas, dos ejecutadas
  y 18 UP-TO-DATE. Se comprobó el grafo neutral indicado y se preparó la distribución
  en `build/compose/binaries/main-release/app`. El nuevo JAR de dominio se verifica
  en el staging y su classpath. No se generó un AppImage independiente con
  appimagetool ni se ejecutaron MSI/EXE.
- Catálogo contrastado con los 428 métodos y sus fuentes actuales, archivos de tipos,
  imports neutrales, staging, enlaces añadidos/movidos y `git diff --check`: correctos.
  El enlace README → LICENSE ya apuntaba a un archivo ausente en la base H2;
  se distingue de los enlaces modificados y no se cambia en este hito.
- Se conserva la limitación del plugin Detekt diagnosticada en H2. No se ejecutaron
  cuentas cloud reales, UI gráfica, CI remoto ni Windows nativo. No se movieron
  recursos/versiones ni se publicaron paquetes; H4–H8 siguen pendientes.

**H3 completado. Commit H3: `0def750` (`refactor: extract domain models and neutral sync contracts`).**

### H4 — Persistencia local y Google Drive

- [x] Extraer configuración, historial y recuperación a `data:local`.
- [x] Extraer OAuth, protocolo y transferencias a `data:drive`.
- [x] Inyectar contratos de credenciales y colaboradores, separando DTO de dominio.
- [x] Verificar JSON, revisiones, escrituras atómicas y condicionales, cancelación,
  conflictos, rutas seguras y recuperación ante fallos.
- [x] **H4 completado.**

Aceptación: adaptadores utilizables sin presentación y datos existentes compatibles.
Los tests de protocolo usan límites controlados y los de transporte conservan su
comprobación real cuando sea necesaria.

Evidencia (2026-10-30–31, Linux; base H3 `0def750`):

- `data:local` contiene ConfigManager/DTO, resolución de destinos legacy y rutas de
  restauración, historial, snapshots y copias de recuperación. `data:drive` contiene
  OAuth, búsquedas/carpetas y transferencias normales, reanudables y condicionales.
  Ambos módulos solo dependen de `domain` y de sus bibliotecas; no dependen entre sí,
  de la raíz, de Compose ni de presentación. Local no incorpora Ktor. La raíz agrega
  los módulos y conserva los coordinadores de H5 y la composición pendiente de H7.
- `AppConfig` y `OAuthCredentials` son modelos neutrales de dominio. La propiedad
  neutral `cloudCredentials` se convierte explícitamente a la clave JSON `googleDrive`
  mediante un DTO interno. El historial mantiene su DTO, versión, claves y valores;
  `get` devuelve `SyncCheckpoint`. Se conservan rutas remotas, nombres de backups,
  permisos, reemplazos atómicos y migraciones, sin cambiar versiones ni formatos.
- OAuth recibe `CredentialStore` y `BrowserLauncher` por constructor. Las revisiones
  y el compare-and-set permanecen en el almacén local; Drive no importa ConfigManager.
  El navegador queda en la raíz para H6, inyectado desde la composición actual.
  `TrackedFileSync` implementa `LocalSaveFiles` y recibe `CloudSaveTransfer`; su
  recuperación no conoce HttpClient, DTO Drive ni implementaciones del otro adaptador.
  Los clientes HTTP siguen siendo prestados y los cierra su propietario actual.
- Las pruebas de configuración/destinos y las tres de historial viven en local.
  Las suites de OAuth, carpetas, búsqueda, transferencias y seis escrituras
  condicionales viven en Drive, conservando CIO real y el servidor de dos escritores.
  Las integraciones entre adaptadores/UI continúan en la raíz con componentes reales.
  `MigrationDrive` se comparte mediante testFixtures de Drive con consumidores reales
  en ambos sitios; no se añaden accesos especiales del compilador ni DTO publicados
  solo para tests. El fixture condicional de la raíz declara su esquema independiente.
- Se conserva todo escenario anterior y se añaden solo dos regresiones: historial
  versión uno literal con checkpoints completos/legacy y rechazo de refresh cuando
  cambia la revisión del almacén inyectado, incluso con credenciales iguales. La
  prueba de JSON completa de H3 verifica también el nuevo DTO de credenciales.
  Resultado vigente: 430 métodos, 431 casos, 65 suites, 73 archivos Kotlin de
  pruebas/fixtures. [Catálogo actualizado](.agents/skills/kotlin-valuable-tests/references/test-audit-cases.md).
  Los clientes de las suites de protocolo trasladadas se cierran también si falla
  una aserción; el fixture compartido conserva su cierre explícito.
- Ejecución final:

  ```bash
  ./gradlew spotlessApply check \
    :data:local:dependencies --configuration runtimeClasspath \
    :data:drive:dependencies --configuration runtimeClasspath \
    packageReleaseAppImage --no-daemon
  ```

  `BUILD SUCCESSFUL` en 1 min 4 s; 109 tareas, 23 ejecutadas y 86 UP-TO-DATE.
  Se ejecutaron raíz (310 casos, 308 aprobados y dos omitidos por PowerShell;
  43 XML, 13,210 s JUnit) y Drive (40 aprobados, siete XML, 4,167 s).
  Dominio (50 aprobados, siete XML, 0,289 s) y local (31 aprobados, ocho XML,
  0,509 s) reutilizaron resultados correctos de las ejecuciones afectadas anteriores.
  Evidencia combinada: 429 aprobados, cero fallos/errores y dos omitidos en Linux.
  No se presenta la reutilización como una ejecución nueva de los 431 casos.
  Tras separar TransferDirection en su archivo y retirar el cuerpo vacío de la
  data class de progreso, `./gradlew :data:drive:spotlessApply
  :data:drive:qualityCheck packageReleaseAppImage --no-daemon` pasó en 16 s;
  35 tareas, cinco ejecutadas y 30 UP-TO-DATE. Se verificó main Drive; los otros
  análisis y el paquete estaban UP-TO-DATE. Ese ajuste declarativo no cambió el
  bytecode ni requirió volver a ejecutar las suites.
- Detekt: nueve SARIF con cero hallazgos y sin avisos de errores del compilador
  durante el análisis. Las comprobaciones de cierre ejecutaron main/test de raíz
  y Drive, además de testFixtures; dominio y local reutilizaron resultados correctos. Al independizar el fixture de
  respuesta se usó serialización reificada para que el análisis no dependiera de
  miembros serializer generados en las fuentes de test. Se corrigió el guard del
  fixture compartido, sin relajar reglas ni añadir supresiones. El comando ejecutó
  nueve grupos PASS de hooks y cuatro de convenciones, incluidos bloqueos deliberados,
  análisis de fixtures y reutilización de caché en la copia desechable.
- Los grafos runtime confirman las dependencias unidireccionales anteriores. El mismo
  comando preparó la distribución Linux en `build/compose/binaries/main-release/app`;
  se verificó la presencia de clases y classpath de dominio/local/Drive, excluyendo
  fixtures del runtime. Docker y CI ya incluyen esos directorios y reportes desde H2;
  no se cambiaron rutas de recursos, metadatos, staging ni scripts de distribución.
- Catálogo contrastado con las fuentes, enlaces añadidos/movidos, visibilidad de tipos,
  límites de módulos y `git diff --check`: revisados. No se repite la suite después
  de editar únicamente seguimiento. No se ejecutaron Docker, cuentas cloud, UI gráfica,
  CI remoto, AppImage independiente con appimagetool ni MSI/EXE o Windows nativo.
  La limitación de Detekt/Gradle registrada en H2 sigue vigente. H5–H8 no se ejecutaron.

**H4 completado. Commit H4: `27d8eb1` (`refactor: extract local persistence and Drive adapters`).**

### H5 — Casos de uso

- [x] Extraer ejecución, sincronización previa/posterior y operaciones manuales.
- [x] Extraer edición y migración de entradas, conexión y coordinación de actualizaciones.
- [x] Expresar progreso, resultados y resolución de conflictos con contratos neutrales.
- [x] Verificar exclusión de operaciones, cancelación, reintentos y resultados obsoletos.
- [x] **H5 completado.**

Aceptación: `application` solo depende de `domain`; no construye clientes HTTP ni
adaptadores, y no depende de tipos visuales. Los controladores delegan los flujos.

Evidencia (2026-10-31, Linux; base H4 `27d8eb1`):

- `application` coordina sincronización manual, consulta de estado, preparación y
  ejecución de partidas, subida posterior, conflictos, reintentos y migraciones de
  carpetas. Sus servicios reciben capacidades prestadas por constructor; no crean
  HTTP, OAuth, historial ni procesos. El grafo `runtimeClasspath` contiene únicamente
  `domain`, Kotlin 2.4.21, corrutinas 1.10.1 y annotations; no tiene Compose, Ktor,
  serialización, adaptadores ni dependencia de la raíz.
- `EditLibrary` conserva validación, overrides, orden y persistencia posterior a la
  migración; `LoadLibrary` coordina los identificadores de Steam. `CloudConnection`
  guarda/desvincula credenciales y autoriza mediante el almacén con revisiones.
  `UpdateCoordinator` conserva exclusión de descargas, cancelación, descarte de
  consultas tardías, tamaño esperado, limpieza de temporales y bloqueo del reinicio
  durante una partida. Los controladores traducen resultados neutrales y publican
  selección, diálogos y estado visual; las respuestas de conflicto y setup siguen
  vinculadas a su solicitud exacta en el límite correspondiente.
- Nuevas capacidades neutrales de configuración, autorización, listado remoto,
  observación de rutas locales, Steam y actualizaciones. `CloudFile` separa el DTO
  Drive; la selección segura de duplicados es ahora una regla de dominio.
  `DesktopSaveAccess` mantiene en local las observaciones, recorrido y resolución
  segura de rutas, incluidos symlinks y alias. `OperationCoordinator` mantiene la
  exclusión y revisión de mutaciones; `EntrySyncChecks` rechaza resultados anteriores
  a otro chequeo, edición, cambio de conexión o mutación de partidas.
- Se retiró toda la coordinación de `src/main/kotlin/com/emusync/drive`, sin fachadas
  de sus clases antiguas. La composición temporal de sincronización vive en
  `com/emusync/composition`; los clientes HTTP continúan prestados y su propietario
  actual los cierra. Steam, procesos, staging/instalación de actualizaciones y los
  constructores concretos de UI siguen en la raíz hasta H6/H7. No se adelantaron esos
  hitos ni se cambiaron recursos, metadatos, formatos o versiones.
- Se trasladaron dos escenarios de reglas a `application`, con observaciones locales
  controladas y sin I/O. Las pruebas de alias de restauración entran ahora por la
  sincronización manual pública con adaptadores reales, sin publicar helpers ni
  accesos especiales del compilador. Se conservan las integraciones de disco, Drive,
  transporte, procesos, UI, cancelación y reintentos. La restauración del estado de
  actualización se compara por valor al traducir tipos; la regresión de respuesta
  tardía sigue exigiendo conservar la instancia visual lista para reiniciar.
  Sin escenarios añadidos ni retirados: 430 métodos, 431 casos, 67 suites y 76
  archivos Kotlin de pruebas/fixtures (nueve fixtures).
  [Matriz y catálogo actualizados](.agents/skills/kotlin-valuable-tests/references/test-audit.md).
- Comprobación afectada: `./gradlew :application:test :test --tests
  'com.emusync.drive.*' --tests 'com.emusync.ui.*' qualityCheck --no-daemon` pasó
  en 28 s (88 tareas, 36 ejecutadas y 52 UP-TO-DATE). Ejecutó las pruebas afectadas;
  los hallazgos de Detekt sobre parámetros, agrupación de funciones, tipos implícitos
  e imports se corrigieron antes del cierre, sin reglas relajadas ni supresiones nuevas.
  Los intentos intermedios de compilación detectaron imports retirados durante la
  reorganización; quedaron restaurados y verificados por el cierre completo.
- Comando final:

  ```bash
  ./gradlew spotlessApply check \
    :application:dependencies --configuration runtimeClasspath \
    packageReleaseAppImage --no-daemon
  ```

  `BUILD SUCCESSFUL` en 58 s; 113 tareas, 12 ejecutadas y 101 UP-TO-DATE.
  Raíz ejecutada completa: 308 casos, 306 aprobados, cero fallos/errores y dos
  omitidos por PowerShell; 43 XML y 11,548 s JUnit. Application (dos casos,
  0,077 s), dominio (50, 0,376 s), local (31, 0,546 s) y Drive (40, 3,841 s)
  reutilizaron resultados correctos de comprobaciones afectadas de esta integración.
  Evidencia combinada: 429 aprobados, cero fallos/errores y dos omitidos; no equivale
  a ejecutar de nuevo los 431 casos en ese comando.
- Once reportes SARIF con cero hallazgos, incluido testFixtures de Drive, sin errores
  de resolución de tipos en el cierre. Main/test de raíz se ejecutaron; los módulos
  reutilizaron sus análisis limpios recientes. Hooks: nueve grupos PASS; convenciones:
  cuatro PASS, incluidos fixtures, Java 25, caché y bloqueos deliberados. Las tareas
  raíz siguen agregando pruebas, calidad y check de todos los módulos.
- El paquete preparó `build/compose/binaries/main-release/app`; se verificaron el
  classpath y las clases de application/dominio/local/Drive, la ausencia de fixtures
  en runtime y la retirada de las antiguas clases de sync del JAR raíz. Docker y CI
  ya incluyen application y sus reportes desde H2; no cambian sus rutas consumidoras.
  No se generó AppImage independiente con appimagetool ni se ejecutaron UI gráfica,
  Docker, cuentas cloud reales, CI remoto, MSI/EXE o Windows nativo. La limitación
  Detekt/Gradle registrada en H2 continúa. H6–H8 siguen pendientes.
- Imports y dependencias de módulos, catálogo de escenarios, enlaces locales nuevos,
  tipos reutilizables, staging y `git diff --check`: revisados. No se repite la suite
  tras editar únicamente seguimiento.

**H5 completado. Commit H5: `7f5c549` (`refactor: extract application use cases and workflow coordination`).**

### H6 — Plataforma y actualizaciones

- [x] Extraer procesos, Steam, navegador y servicios nativos a `platform:desktop`.
- [x] Separar consulta/descarga (`data:updates`) de instalación y reinicio (plataforma).
- [x] Conservar pruebas reales de procesos, VDF, argumentos, transporte e instalación.
- [ ] Verificar Linux y Windows en sus entornos; registrar omisiones justificadas.
  Linux verificado; Windows nativo pendiente. Las simulaciones no cierran esta subtarea.
- [ ] **H6 completado.**

Aceptación: contratos implementados sin dependencia visual, preservando aislamiento
de lanzadores, integridad de descargas, recuperación y códigos de salida.

Evidencia (2026-10-31, Linux; base H5 `7f5c549`):

- `platform:desktop` contiene procesos/seguimiento, Steam/VDF/lanzadores Windows y
  Linux, navegador OAuth, descubrimiento de ROMs, selectores AWT/Swing y kdialog/zenity,
  entorno AppImage, staging e instalación/reinicio. Solo depende de `domain`, Kotlin,
  corrutinas y serialización para el JSON de argumentos Windows; no incorpora Compose,
  Ktor, application ni adaptadores de datos. Se conservan rutas, argumentos, señales,
  códigos de salida, registros y recuperación. Los helpers de plataforma son internos
  o privados; los modelos reutilizables de procesos, Steam y configuración Windows
  tienen archivos propios.
- `data:updates` implementa `UpdateSource` con `GitHubUpdateSource`: conserva endpoint,
  cabeceras, selección de artefactos, comparación de versiones, timeouts, progreso,
  publicación atómica de descargas completas y cancelación. Los DTO GitHub son internos
  y están separados. Recibe cliente HTTP, repositorio, versión y plataforma por
  constructor; no conoce AppInfo, instalación, Steam ni presentación. Su grafo contiene
  únicamente `domain` y las bibliotecas Kotlin/corrutinas/serialización/Ktor existentes.
  Ninguno de los dos adaptadores depende del otro.
- `DesktopUpdateInstaller` implementa `UpdateInstaller`; `UpdateStagingFiles` implementa
  `UpdateFiles` y recibe proveedores de plataforma/ruta. La composición provisional
  `DesktopUpdates` conecta estas capacidades y posee el cliente HTTP, que el origen
  solo toma prestado. AppViewModel cierra ese propietario; UpdateController recibe
  únicamente contratos de dominio. Se elimina UpdateManager, sin fachada temporal.
  La composición sigue en la raíz hasta desktopApp en H7.
- GameRunner recibe `SteamLaunchPreparation`, implementada por DesktopSteamLibrary;
  deja de construir su propio SteamShortcutManager. La composición proporciona el
  colaborador. DesktopBrowserLauncher implementa BrowserLauncher. PathPicker/PickerType
  expresan selección neutral, y el codec puro de argumentos compartido por formularios
  y lanzadores vive en `domain`, en la funcionalidad execution. No se trasladaron recursos, versión,
  identificadores ni distribución a desktopApp, ni se adelantó la reorganización de UI.
- Se trasladan las suites junto a sus propietarios; UpdateManagerTest se divide en
  GitHubUpdateSourceTest y DesktopUpdateInstallerTest. Las integraciones de UI siguen
  usando HTTP/disco/adaptadores reales a través de contratos. Se conserva cada escenario
  anterior y se añade solo `Steam launch uses the injected preparation and stops on its
  failure`: observa entrada/ruta exactas, fallo propagado y ROM conservada. Resultado:
  **431 métodos, 432 casos, 68 suites y 77 archivos Kotlin de pruebas/fixtures** (nueve
  fixtures). [Matriz y catálogo actualizados](.agents/skills/kotlin-valuable-tests/references/test-audit.md).
  Los clientes de las pruebas GitHub trasladadas se cierran también ante fallos.
- Comprobación focalizada final: `./gradlew :platform:desktop:spotlessApply
  :platform:desktop:test :platform:desktop:qualityCheck --no-daemon` pasó en 18 s;
  23 tareas, seis ejecutadas y 17 UP-TO-DATE. Ejecutó 95 casos de plataforma: 93
  aprobados, cero fallos/errores y dos omitidos por PowerShell.
  La integración previa compiló raíz y verificó datos/actualizaciones/calidad; detectó
  dependencias de serialización/imports tras el traslado y una aserción de identidad
  de excepción incompatible con la reconstrucción de trazas de corrutinas. Se
  corrigieron antes del cierre, sin alterar contratos ni relajar reglas/supresiones.
- Cierre:

  ```bash
  ./gradlew spotlessApply check :data:updates:dependencies \
    :platform:desktop:dependencies --configuration runtimeClasspath \
    packageReleaseAppImage --no-daemon
  ```

  `BUILD SUCCESSFUL` en 1 min; 124 tareas, 26 ejecutadas y 98 UP-TO-DATE.
  El reporte de dependencias de updates incluye todas las configuraciones y su
  runtimeClasspath; el de plataforma está filtrado a runtimeClasspath.

  | Suite | Casos | Resultado | Tiempo JUnit | Ejecución en el cierre |
  | --- | ---: | --- | ---: | --- |
  | Raíz | 193 | Todos aprobados | 2,272 s | Ejecutada |
  | Plataforma | 95 | 93 aprobados, dos omitidos | 8,434 s | Ejecutada |
  | Updates | 13 | Todos aprobados | 0,397 s | Ejecutada |
  | Dominio | 58 | Todos aprobados | 0,221 s | UP-TO-DATE de esta integración |
  | Application | 2 | Todos aprobados | 0,076 s | UP-TO-DATE de esta integración |
  | Local | 31 | Todos aprobados | 0,310 s | UP-TO-DATE de esta integración |
  | Drive | 40 | Todos aprobados | 2,599 s | UP-TO-DATE de esta integración |

  Evidencia vigente combinada: **430 aprobados, cero fallos/errores y dos omitidos**;
  no son 432 ejecuciones nuevas en el cierre. Se conservan las integraciones CIO reales
  de Drive y sus dos escritores, además de procesos/instalación Linux reales.
- Quince SARIF con cero hallazgos, incluido testFixtures de Drive. El cierre ejecutó
  main/test de raíz y plataforma y test de updates; los demás análisis reutilizaron
  resultados limpios de esta integración. Hooks: nueve grupos PASS; convenciones:
  cuatro PASS, incluidos aislamiento del índice, fixtures, caché, Java 25 y fallos
  deliberados en copias desechables. No se añaden accesos especiales del compilador.
- `packageReleaseAppImage` preparó `build/compose/binaries/main-release/app`. Se
  comprobaron clases y classpath de desktop/updates/dominio/application/local/Drive,
  sin fixtures, duplicados de las clases movidas ni UpdateManager. Docker/Compose y CI
  ya incluyen ambos directorios y reportes recursivos desde H2; no cambian rutas de
  staging, recursos, metadatos o artefactos. No se generó AppImage independiente con
  appimagetool ni se ejecutaron interfaz gráfica, Docker o cuentas reales.
- **Windows nativo pendiente:** este host es Linux y no dispone de PowerShell. Se
  conservan las dos omisiones explícitas de WindowsLauncherTest y las comprobaciones
  de argumentos MSI/EXE con lanzadores controlados; estas últimas no son instalación
  real Windows. CI mantiene Windows y sus tareas check/MSI/EXE, pero no se ejecutó CI
  remoto, push ni publicación. Esta limitación impide marcar H6 completo; H7–H8 no se
  ejecutaron. Continúa la limitación Detekt/Gradle diagnosticada en H2.
- Catálogo contrastado con fuentes y reportes, enlaces locales, visibilidad/imports,
  grafos, staging y `git diff --check`: revisados. No se repite la suite tras editar
  únicamente documentación.

**Implementación H6 verificada en Linux. Validación nativa Windows pendiente.
Commit H6: `d54561a` (`refactor: extract desktop platform and update adapters`).**

### H7 — Presentación y composición Desktop

- [x] Agrupar presentación por biblioteca, conexión y actualizaciones.
- [x] Compartir componentes/reglas comunes de formularios mediante estado y callbacks.
- [x] Extraer `desktopApp`, recursos y composición manual de dependencias.
- [x] Actualizar rutas de distribución, versión, staging y artefactos con sus consumidores.
- [ ] **H7 completado.** Preparación Linux verificada; MSI/EXE nativos pendientes.

Aceptación: UI sin dependencias concretas de Drive/HTTP, recursos con un propietario,
comandos raíz compatibles y preparación de paquetes en Linux/Windows operativa.

Evidencia (2026-10-31, Linux; base H6 `d54561a`):

- `presentation` contiene Compose y sus controladores, organizados por biblioteca
  (componentes, formularios y traducción de sync), conexión y actualizaciones.
  Su producción solo depende de application/domain y las bibliotecas Compose/Kotlin;
  no importa adaptadores concretos, Ktor, composición ni AppInfo. Recibe versión,
  capacidades y callbacks desde desktopApp. StateFlow sigue siendo de solo lectura;
  no se añade un framework de DI ni se cambian dependencias/versiones.
- `LibraryServices` y `UpdateServices` agrupan capacidades prestadas. Los controladores
  reciben ConfigurationStore, SteamLibrary, CloudConnection y SaveSynchronizerFactory.
  SteamLibrary incorpora consulta de disponibilidad/registro; LocalSaveTargets
  implementa SaveTargetAccess sobre los helpers existentes, conservando inferencia
  legacy, actualización del tipo cuando aparece la ruta y rechazo de escapes/symlinks.
  GameItem conserva esa capacidad al copiar una entrada y vuelve a resolver sus destinos.
  La capacidad queda fuera de equals/hashCode: la identidad sigue siendo nombre,
  entrada y ROM, como antes de la extracción.
- Alta y edición comparten EntryFormState, SaveTargetFormState y el campo de rutas;
  los callbacks de LibraryFormActions mantienen selección fuera del hilo visual.
  Se conserva el texto de los botones y los tipos elegidos. La apertura de la carpeta
  de configuración vive en plataforma y se solicita mediante callback. Los tipos
  reutilizables EntryType, GameEditResult, SidebarUnit/Row y GameSaveSetup tienen
  archivos propios; los helpers permanecen internos o privados cuando es posible.
- `desktopApp` posee MainKt, AppInfo, iconos/AppStream y composición interna.
  DesktopApplication crea y cierra los dos clientes HTTP; la UI toma prestadas sus
  capacidades y la ventana cierra el propietario al salir. No hay scopes nuevos.
  El test nuevo comprueba que ambos clientes siguen disponibles durante el uso de
  presentación y que sus jobs terminan al cerrar ese propietario.
- La raíz no tiene fuentes: conserva test/qualityCheck/check agregados y delega run,
  preparación de distribuciones y packageReleaseAppImage/Msi/Exe a desktopApp.
  Se actualizan juntos Dockerfile, Compose, scripts de distribución/versionado,
  staging de Windows, diagnóstico CI y archivos del commit de versión del release.
  Se preservan MainKt, versión 1.1.1, UUID Windows, ID AppStream y nombres de artefactos.
  Los recursos movidos tienen exactamente los mismos bytes que en H6.
- Se conservan los 432 casos anteriores y se añaden únicamente las regresiones de cierre
  de recursos e identidad de GameItem: **433 métodos, 434 casos, 70 suites y 81 archivos
  Kotlin de pruebas/fixtures**
  (11 fixtures). Las integraciones permanecen con presentación y reciben adaptadores
  reales desde fixtures de prueba: no dependen de desktopApp, no crean ciclos ni
  requieren publicar helpers de producción para las pruebas. AppViewModelTest cierra
  también sus clientes ante aserciones fallidas. [Catálogo y matriz actualizados](.agents/skills/kotlin-valuable-tests/references/test-audit.md).
- La comprobación focalizada previa al ajuste de identidad `./gradlew :presentation:test :desktopApp:test
  :presentation:qualityCheck :desktopApp:qualityCheck --no-daemon` pasó en 26 s;
  51 tareas, 11 ejecutadas y 40 UP-TO-DATE. Ejecutó los 193 casos de presentación
  (2,438 s JUnit) y el de desktopApp (0,301 s), todos aprobados.
- Cierre:

  ```bash
  ./gradlew check packageReleaseAppImage --no-daemon
  ```

  `BUILD SUCCESSFUL` en 57 s; 113 tareas, 14 ejecutadas y 99 UP-TO-DATE.
  Ejecutó presentación (194 casos), desktopApp (uno), su análisis, el paquete y
  hooks/convenciones. Los otros módulos reutilizaron resultados correctos de esta
  integración: el primer check completo había pasado en 54 s, ejecutando esas suites.
  El grafo runtime de presentation se inspeccionó con `:presentation:dependencies
  --configuration runtimeClasspath` en un cierre previo correcto de 50 s.
  La prueba aislada nueva de GameItem había fallado antes de corregir su igualdad;
  el cierre confirma su corrección y que las copias resuelven los destinos editados.

  | Módulo | Casos | Resultado vigente | Tiempo JUnit |
  | --- | ---: | --- | ---: |
  | domain | 58 | Aprobados | 0,724 s |
  | application | 2 | Aprobados | 0,121 s |
  | data:local | 31 | Aprobados | 0,683 s |
  | data:drive | 40 | Aprobados | 4,448 s |
  | data:updates | 13 | Aprobados | 0,994 s |
  | platform:desktop | 95 | 93 aprobados, dos omitidos PowerShell | 8,365 s |
  | presentation | 194 | Aprobados | 4,107 s |
  | desktopApp | 1 | Aprobado | 0,327 s |

  Evidencia combinada: **432 aprobados, cero fallos/errores y dos omitidos**. No se
  presenta la reutilización de los otros módulos como 434 ejecuciones nuevas ni se cuentan los
  reportes históricos de la raíz. Diecisiete SARIF actuales limpios de los ocho
  módulos, incluido testFixtures Drive. Sin reglas relajadas ni supresiones nuevas.
- `./gradlew :qualityCheck --configuration-cache --no-daemon`, dos ejecuciones:
  correctas en nueve y cuatro segundos; la segunda reutilizó la caché (86 tareas
  UP-TO-DATE). Hooks: nueve grupos PASS; se comprobaron también los scripts Gradle
  de presentation/desktopApp con índice GOOD y trabajo BAD, sin alterar ninguno.
  Convenciones: cuatro PASS, con agregación, Java 25, fixtures, caché y fallos
  deliberados en copias desechables. `bash .githooks/pre-commit` pasó también sobre la
  exportación final del índice: BUILD SUCCESSFUL en un minuto, 95 tareas ejecutadas;
  no se alteró el trabajo ni el índice para obtener ese resultado.
- El paquete Linux se prepara en `desktopApp/build/compose/binaries/main-release/app`.
  Staging: MainKt y los ocho JAR en el classpath, 577 clases EmuSync sin duplicados,
  recursos presentes únicamente en el JAR de desktopApp y ningún fixture de prueba.
  `docker compose config --quiet` y `docker compose build` pasaron. La imagen nueva,
  ejecutada sin montar fuentes/builds del host y con salida en `/tmp/emusync-h7-appimage`,
  ejecutó scripts/build-dist.sh, validó AppStream y generó un AppImage independiente
  de 102 MiB. Se reconstruyó la imagen y se repitió el script sobre las fuentes finales
  después de corregir la identidad de GameItem; ambos empaquetados pasaron.
- bump-version.sh pasó en copias temporales con espacios: patch/minor/major producen
  respectivamente 1.1.2/1.2.0/2.0.0 en ambos consumidores y en GITHUB_OUTPUT; la versión
  del trabajo sigue en 1.1.1. El dry-run de `:packageReleaseMsi :packageReleaseExe`
  resolvió ambas delegaciones raíz a desktopApp. **No ejecuta jpackage Windows**:
  no hay validación nativa MSI/EXE en este host Linux. Por ello H7 sigue abierto
  igual que H6; H8 no se ejecuta. No hubo UI gráfica, cuentas reales, CI remoto,
  push ni publicación. Continúa la limitación Detekt/Gradle diagnosticada en H2.
- La primera compilación se detuvo por archivos generados propiedad de root en los
  builds anteriores. Se reparó solo su propiedad. Los errores intermedios de imports
  tras el traslado y el hallazgo InjectDispatcher se corrigieron antes del cierre.
  Catálogo, enlaces locales modificados, grafos, staging, sintaxis Bash y diff revisados.

**Implementación H7 verificada en Linux. Validación nativa Windows pendiente.
Commit H7: `46e1c1b` (`refactor: extract presentation and desktop application`).**

### H8 — Validación final y cierre

- [x] Ejecutar `check`, revisar Detekt de todos los módulos y confirmar ausencia de ciclos.
- [x] Revisar la matriz de pruebas y retirar fachadas o dependencias temporales.
- [ ] Verificar AppImage en Linux y MSI/EXE en Windows sin publicar releases.
  AppImage generado y arranque comprobado en Linux; MSI/EXE nativos pendientes.
- [x] Confirmar compatibilidad de datos, actualizar estructura real y registrar pendientes.
- [ ] **H8 completado.**

Aceptación: todos los hitos verificados, formatos compatibles, cero hallazgos Detekt y
evidencia de distribución por plataforma. Un resultado solo en Linux no cierra Windows.

Evidencia (2026-10-31, Linux; base H7 `46e1c1b`):

- Cierre con ejecución nueva de compilación, pruebas, formato y análisis:

  ```bash
  ./gradlew check packageReleaseAppImage --rerun-tasks --no-daemon
  ```

  `BUILD SUCCESSFUL` en 1 min 6 s; 104 tareas accionables, todas ejecutadas.
  Se reutilizó la configuración, pero `--rerun-tasks` ejecutó las ocho suites y los
  17 análisis Detekt. Las tareas raíz de fuentes/pruebas son NO-SOURCE; sus reportes
  históricos no se cuentan. No se cambiaron fuentes, pruebas, versiones, reglas ni
  supresiones para obtener este resultado.

  | Módulo | Kotlin producción | Kotlin pruebas/fixtures | Casos | Tiempo JUnit |
  | --- | ---: | ---: | ---: | ---: |
  | domain | 61 | 8 | 58 | 0,640 s |
  | application | 33 | 3 | 2 | 0,343 s |
  | data:local | 16 | 8 | 31 | 0,850 s |
  | data:drive | 21 | 12 | 40 | 4,232 s |
  | data:updates | 6 | 2 | 13 | 1,194 s |
  | platform:desktop | 24 | 16 | 95 | 9,560 s |
  | presentation | 43 | 31 | 194 | 2,756 s |
  | desktopApp | 6 | 1 | 1 | 0,421 s |

  Total: **210 archivos Kotlin de producción, 81 de pruebas/fixtures, 70 suites y
  434 casos: 432 aprobados, cero fallos/errores y dos omitidos**, 19,996 s JUnit.
  Las omisiones son los dos escenarios PowerShell de WindowsLauncherTest ya
  identificados; no hay PowerShell ni host Windows disponible. Los 17 SARIF nuevos,
  incluido testFixtures de Drive, tienen **cero hallazgos**, sin avisos de errores del
  compilador en el análisis. Hooks: nueve grupos PASS; convenciones: cuatro PASS,
  incluidos aislamiento del índice, Java 25, fixtures, reutilización de caché y
  bloqueos deliberados en copias desechables.
- Se contrastaron las declaraciones Gradle, imports de producción y los componentes
  y aristas de `resolutionResult` de runtimeClasspath/testRuntimeClasspath de los
  ocho módulos y la raíz, mediante un init script temporal de inspección:

  | Consumidor de producción | Dependencias directas de proyecto resueltas |
  | --- | --- |
  | domain | Ninguna |
  | application | domain |
  | data:local, data:drive, data:updates, platform:desktop | domain |
  | presentation | application, domain |
  | desktopApp | presentation, application, domain y los cuatro adaptadores |
  | Raíz | Ninguna; solo tareas agregadas/delegadas |

  El grafo es acíclico y no hay dependencias de módulos hacia la raíz. Domain y
  application carecen de Compose, Ktor y serialización en runtime; presentation no
  incluye Ktor ni adaptadores en producción. Sus integraciones sí reciben adaptadores
  y fixtures de Drive desde testRuntimeClasspath, sin depender de desktopApp.
  La composición y propiedad de recursos están en desktopApp. No quedan fachadas de
  transición ni dependencias temporales de la migración que retirar: AppViewModel y
  SteamShortcutManager son APIs estables con consumidores y garantías conservadas;
  los lectores legacy protegen datos existentes y no son scaffolding.
- [Matriz revisada](.agents/skills/kotlin-valuable-tests/references/test-audit.md) y
  [catálogo](.agents/skills/kotlin-valuable-tests/references/test-audit-cases.md)
  contrastados con fuentes y XML: los 433 métodos siguen registrados, incluido el
  escenario parametrizado con dos elecciones. Sin altas ni retiradas en H8. Se
  ejecutaron ConfigFormatCompatibilityTest y SyncHistoryCompatibilityTest sobre
  documentos literales, además de migraciones legacy, revisiones OAuth, persistencia
  atómica, backups, rutas seguras, conflictos, cancelación y escrituras condicionales.
  Esta evidencia comprueba los formatos y contratos auditados; no usa datos personales
  ni una cuenta real de Drive o Steam.
- Staging Linux con MainKt y los ocho JAR en el classpath, **579 clases EmuSync sin
  duplicados**, sin fixtures ni UpdateManager. Los tres recursos (PNG, ICO y AppStream)
  pertenecen exclusivamente al JAR de desktopApp y conservan los bytes de H6.
  Se mantienen versión 1.1.1, UUID Windows, ID AppStream y nombres de artefactos.
  La estructura documentada corresponde a las fuentes actuales; README distingue
  preparación Gradle y generación del AppImage independiente. Los resultados de
  AUDIT_STATUS son históricos y no se usan como evidencia de este cierre.
- `docker compose config --quiet` y `docker compose build`: correctos. Empaquetado
  independiente, sin montar fuentes ni builds del host:

  ```bash
  mkdir -p /tmp/emusync-h8-appimage
  docker run --rm \
    --mount type=bind,src=/tmp/emusync-h8-appimage,dst=/out \
    --mount type=volume,src=emu-sync_gradle-cache,dst=/root/.gradle \
    emu-sync-builder:latest
  ```

  El script build-dist.sh validó AppStream y ejecutó appimagetool. Gradle pasó en 30 s,
  33 tareas ejecutadas. Artefacto local: `/tmp/emusync-h8-appimage/EmuSync-x86_64.AppImage`,
  106.592.760 bytes; SHA-256
  `6a1b3176b533dee4294d5c12db78045b1ef67a1e5535ef150a3e901026335f83`.
  En otro contenedor desechable se instalaron Xvfb/xauth/x11-utils, se extrajo ese
  AppImage y se ejecutó su AppRun con el runtime incluido, renderizador software y
  EMUSYNC_CONFIG temporal. X11 confirmó una ventana EmuSync de 1280×800 y la creación
  de la configuración. El proceso se terminó y el contenedor se eliminó. El arranque
  emitió únicamente el aviso SLF4J de proveedor ausente; no hubo error de arranque.
  No se comprobó montaje FUSE ni interacción manual de la interfaz.
- **Cierre pendiente en Windows:** ejecutar `./gradlew check --no-daemon` y
  `./gradlew packageReleaseMsi packageReleaseExe --no-daemon` en Windows nativo,
  revisar los XML/SARIF y los instaladores de desktopApp, y comprobar instalación,
  arranque y lanzadores PowerShell. Registrar host, comandos, resultados y artefactos
  antes de marcar H6, H7 y H8 completos. CI conserva estas tareas y reportes, pero
  no se ejecutó CI remoto, push ni publicación. Los tests con lanzadores Windows
  controlados y el dry-run de H7 no sustituyen esa evidencia. Continúa la limitación
  Detekt/Gradle diagnosticada en H2.
- Enlaces locales del seguimiento/matriz y `git diff --check` revisados. El enlace
  README → LICENSE sigue ausente desde antes de H3; no se inventa una licencia en
  este cierre. No se repite la suite tras editar únicamente documentación.
- Limpieza previa al commit, autorizada después de la validación anterior:
  - Eliminados el árbol vacío `src/` de la raíz y el directorio vacío
    `presentation/src/main/kotlin/com/emusync/ui/dialogs`.
  - Eliminadas la caché obsoleta de Gradle 8.11.1 y las clases, recursos compilados,
    librerías, temporales y distribuciones de los dos builds archivados en
    `.gradle/build-before-*`. Se conservan íntegros sus reports/test-results.
    Retirados también compose/distributions/generated/kotlin/libs/tmp del build raíz
    antiguo: el staging actual pertenece a desktopApp. La configuración personal,
    partidas, historial y backups no forman parte de esta limpieza. Algunos restos
    generados eran propiedad de root; se eliminaron mediante montajes Docker acotados
    y se corrigió la propiedad de temporales generados de build-logic inaccesibles.
  - Retirado el callback legacy por índices de Sidebar y su envoltorio: su único
    consumidor siempre proporcionaba el callback de lista completa, que tenía
    prioridad. Se pasa ese callback directamente para arrastre y controles arriba/abajo.
    No se retiran archivos Kotlin ni pruebas con contratos vigentes; se conservan
    las regresiones de orden, grupos y edición concurrente.
  - `./gradlew check packageReleaseAppImage --no-daemon`: correcto en 49 s;
    113 tareas, 18 ejecutadas y 95 UP-TO-DATE. Se ejecutaron los 194 casos de
    presentation (4,121 s) y el de desktopApp (0,411 s), todos aprobados; las otras
    suites reutilizaron la ejecución completa de H8. También se ejecutaron Detekt
    main/test de presentación, hooks y convenciones. Los 17 SARIF vigentes siguen
    sin hallazgos. No se añaden pruebas para retirar una rama sin consumidores.
  - Se reconstruyó la imagen Docker con las fuentes limpias y se repitió build-dist.sh
    sin montar fuentes/builds del host: Gradle correcto en 31 s, 33 tareas ejecutadas;
    AppStream y appimagetool correctos. El staging final contiene 578 clases EmuSync
    sin duplicados. AppImage final:
    `/tmp/emusync-h8-cleanup-appimage/EmuSync-x86_64.AppImage`, 106.592.760 bytes,
    SHA-256 `753a23f83128f711a0759e4e5dbcbe063ada0264f421f8d03662d978ba3bc030`.
    Se repitió el arranque extraído bajo Xvfb en otro contenedor desechable:
    ventana EmuSync de 1280×800 y configuración temporal creadas, sin errores de
    arranque y con el mismo aviso SLF4J de proveedor ausente.

**Validación final H8 verificada en Linux. H6, H7 y H8 siguen abiertos por Windows.
Commit H8: `f7c7248` (`chore: clean migration leftovers and record final validation`).**


### Revisión posterior a H8 — I/O visual y protección de arquitectura

Evidencia (2026-10-31, Linux; base `f7c7248`). El usuario autorizó corregir los
hallazgos de la revisión y hacer un commit. Esta revisión no cierra Windows.

- `GameItem` contiene solo datos y transformaciones visuales puras. La búsqueda de
  portadas implementa el contrato neutral CoverFiles en plataforma; conserva overrides,
  prioridad de carpetas, nombres normalizados y variantes de extensión. La cuadrícula
  y el diálogo comparten una carga que resuelve la ruta, consulta metadatos y decodifica
  dentro del dispatcher de I/O. Los resultados obsoletos se cancelan desde Compose.
- La resolución de destinos se solicita explícitamente mediante SaveTargetAccess en
  el límite de sincronización/formulario, sin guardar capacidades en el descriptor.
  Se mantienen inferencia legacy, comprobación física y defensa de rutas restauradas.
  Los formularios crean y resuelven borradores fuera del hilo visual con snapshots
  coherentes; una detección de tipo pendiente no altera una ruta editada después.
  Las elecciones de restauración se invalidan al cambiar el destino.
- AppViewModel ofrece operaciones suspendidas sobre un dispatcher inyectable, con
  I/O serializado por defecto y cancelación estructurada. Carga, escrituras, OAuth,
  instalación y reintentos dejan de ejecutar efectos bloqueantes en Compose. Las
  consultas visuales de Steam leen un snapshot de AppUiState, actualizado al cargar
  y al editar/registrar/desregistrar, sin releer VDF durante la composición.
- SteamRegistration en application coordina alta/baja e ID persistido a través de
  contratos. Usa la entrada vigente y modifica solo su ID sobre la configuración más
  reciente, conservando ediciones y credenciales concurrentes. Un fallo de Steam no
  escribe un ID; una entrada eliminada no se recrea. La UI traduce el resultado y los
  errores a su estado. No se introduce transacción distribuida ni se altera el formato.
- `verifyModuleArchitecture` forma parte de qualityCheck/check y del precommit.
  Verifica las aristas de proyecto de producción declaradas e heredadas en las
  configuraciones main, familias de bibliotecas incompatibles con cada capa y ausencia
  de fuentes raíz. Los tests pueden conectar adaptadores sin romper el grafo productivo.
  La regresión desechable demuestra rechazo de domain → application, Ktor en domain
  y fuentes raíz, aceptación de una dependencia de prueba y reutilización de caché.
  Spotless raíz incluye también las clases Kotlin de build-logic. Sin nuevas versiones,
  reglas relajadas, supresiones ni accesos especiales del compilador.
- Se conservan los 434 casos anteriores. Las siete comprobaciones de búsqueda de
  portadas pasan de presentación a plataforma; se añaden siete regresiones: tres de
  registro Steam, una del worker de biblioteca y caché visual, una de búsqueda/preview
  de portadas en el worker y dos de borradores/tipos tardíos. Resultado:
  **216 archivos Kotlin de producción, 83 de pruebas/fixtures (11 fixtures),
  440 métodos y 441 casos en 72 suites**. El verificador Gradle es una clase adicional
  del build incluido, ajena a esas cifras. [Matriz](.agents/skills/kotlin-valuable-tests/references/test-audit.md)
  y [catálogo](.agents/skills/kotlin-valuable-tests/references/test-audit-cases.md) actualizados.
- Comprobación focalizada de registro, portadas, worker y formularios correcta; tras
  detectar que el fixture Unconfined convertía el timeout OAuth virtual en real,
  la composición de prueba toma el dispatcher de su corrutina llamadora. La regresión
  de timeout vuelve a pasar con su scheduler, sin aumentar tiempos ni omitirla.
  El análisis del nuevo localizador señaló la complejidad del antiguo getter; se
  dividió en helpers de prioridad con sus regresiones originales. Sin supresiones.
- Cierre:

  ```bash
  ./gradlew spotlessApply check packageReleaseAppImage --no-daemon
  ```

  `BUILD SUCCESSFUL` en 1 min 29 s; 132 tareas, 41 ejecutadas y 91 UP-TO-DATE.
  **441 casos vigentes: 439 aprobados, cero fallos/errores y dos omitidos PowerShell**.
  En el cierre se ejecutaron plataforma (102 casos, 9,804 s JUnit), presentación
  (191, 6,782 s) y desktopApp (uno, 0,716 s). Dominio (58, 0,685 s), application
  (cinco, 0,392 s), local (31, 0,882 s), Drive (40, 4,909 s) y updates (13, 1,717 s)
  reutilizaron resultados correctos de esta integración; no son 441 ejecuciones nuevas.
  Los **17 SARIF vigentes están limpios**: main/test de presentación y plataforma
  reutilizaron la comprobación afectada limpia; los otros trece análisis se ejecutaron.
  Hooks: nueve grupos PASS; convenciones: los cuatro grupos anteriores y el nuevo
  grupo de arquitectura, todos PASS. Se preparó el staging Linux de desktopApp.
- El smoke visual del AppImage detectó que este JDK interpreta File("") como el
  directorio actual: una observación asíncrona cambiaba el tipo inicial de la fila
  vacía a DIRECTORY. Se amplió la regresión física existente y falló antes de corregir
  LocalSaveTargets.typeOf para devolver null en rutas vacías/blancas. Cierre posterior
  con el mismo comando: correcto en **1 min 15 s**, 132 tareas (24 ejecutadas y 108
  UP-TO-DATE). Ejecutó local (31 casos, 0,450 s), presentación (191, 5,649 s) y desktopApp
  (uno, 0,498 s); las otras suites reutilizaron sus resultados correctos anteriores.
  Se mantienen 439 aprobados y dos omitidos, sin fallos/errores y con 17 SARIF limpios.
  No se añade un caso JUnit por duplicado ni se cambia la regla de tipos físicos.
- Distribución final: la red bridge de Docker falló por falta de soporte veth en
  este host; `docker build --network=host -t emu-sync-builder:latest .` y la ejecución
  del builder con `--network=host` pasaron sin montar fuentes ni builds del host.
  El Gradle del contenedor pasó en 33 s (33 tareas ejecutadas); AppStream y appimagetool
  correctos. AppImage independiente de 106.662.392 bytes en
  `/tmp/emusync-architecture-appimage/EmuSync-x86_64.AppImage`, SHA-256
  `ddd335190dcb81f83dfa90974a085b8ae8dbe14cd1a17ec7c3cbb73857012644`.
  Se extrajo y arrancó con el runtime incluido bajo Xvfb, en un contenedor desechable
  con configuración, portada PNG y partida de prueba. Se comprobó la portada en la
  cuadrícula y en Edit Game, la apertura de Add New Entry, su tipo inicial FILE y la
  validación de campos vacíos. Guardar Smoke Edited conservó el contenido SAVE, el
  destino y la portada; Safe Close terminó el contenedor con código cero. Sin errores
  de arranque/operación; solo el aviso SLF4J de proveedor ausente. Se eliminó el contenedor.
  No se comprobó montaje FUSE ni instalación Windows.
- Se mantiene el pendiente de MSI/EXE, instalación y ejecución Windows nativos, sin
  publicación, push ni cuentas reales de Drive/Steam. Sigue la limitación del plugin
  Detekt diagnosticada en H2. Las revisiones de fuentes, catálogo y enlaces locales
  no requieren repetir las suites por cambios únicamente documentales.

Commit de esta revisión: `db90baa`
(`refactor: isolate presentation IO and enforce module boundaries`).

### Corrección posterior — Bash de CI Windows

Evidencia (2026-11-02, Linux; base `db90baa`):

- El log Windows aportado termina en `:verifyGitHooks`: Gradle invoca `bash` por
  nombre y obtiene el lanzador WSL, que falla por no tener distribuciones instaladas.
  CI selecciona ahora `shell: bash` y exporta su ruta absoluta nativa mediante
  `EMUSYNC_BASH` (`cygpath -w` en Windows). Las tres tareas Gradle de scripts usan
  esa variable; los scripts y el precommit reutilizan `$BASH` en llamadas internas.
  README documenta la configuración local de Git Bash. No se omiten verificaciones.
- La regresión desechable antepone un `bash` que falla al PATH y exige completar
  hooks con el intérprete explícito, también al reutilizar la caché. En una copia
  temporal, restaurar la selección anterior hace fallar `:verifyGitHooks` con 97;
  con la corrección pasa. La [matriz](.agents/skills/kotlin-valuable-tests/references/test-audit.md)
  registra esta garantía. No cambian las pruebas Kotlin/JUnit.
- `./gradlew check --no-daemon`: `BUILD SUCCESSFUL` en 1 min 19 s; 110 tareas,
  19 ejecutadas y 91 UP-TO-DATE. Se ejecutaron los nueve grupos de hooks y los seis
  de convenciones, incluido el nuevo escenario de intérprete. Pruebas de aplicación
  y Detekt reutilizaron resultados anteriores; no son nuevas ejecuciones JUnit.
  Los 17 SARIF vigentes tienen cero hallazgos. YAML del workflow, sintaxis Bash y
  `git diff --check` correctos.
- Dos ejecuciones focalizadas de `:verifyGitHooks --configuration-cache --no-daemon`
  con `EMUSYNC_BASH` apuntando a una ruta absoluta con espacios pasaron; la segunda
  reutilizó la caché. Comprueban también las comillas de las llamadas internas.
- El primer intento `spotlessApply check` pasó hooks/convenciones y se detuvo al
  limpiar salidas de dominio propiedad de root. Se reparó únicamente la propiedad
  de los directorios generados `build/` de raíz, build-logic y los ocho módulos,
  mediante montajes Docker acotados; después pasó el comando de cierre anterior.
- Este host sigue siendo Linux. El log aportado demuestra el fallo Windows previo;
  la corrección requiere confirmación en el próximo CI Windows. No se ejecutaron
  MSI/EXE, instalación ni arranque nativos. H6/H7/H8 conservan su pendiente Windows.

Commit de esta corrección: `207563c`
(`fix: use explicit Git Bash for Windows CI checks`).

### Corrección posterior — LF en la copia de convenciones

Evidencia (2026-11-02, Linux; base `207563c`):

- El nuevo log Windows aportado confirma ejecución mediante Git Bash y los nueve
  grupos de hooks aprobados. Se detiene después en `:verifyGradleConventions` por
  Spotless dentro del proyecto temporal. La copia omitía `.gitattributes` y podía
  exigir CRLF para fuentes Kotlin/Gradle que el repositorio conserva en LF.
- Se reprodujo antes de corregirlo: el fixture inicializa un repositorio temporal
  con `core.autocrlf=false` y `core.eol=crlf`. `:verifyGradleConventions` falló en
  25 s; el diff Spotless exige cambiar LF por CRLF. Solo se modificó la configuración
  Git de la copia; no se alteraron el índice ni la configuración del repositorio real.
- El script copia ahora `.gitattributes` y `.editorconfig`. La misma regresión acepta
  las fuentes LF y mantiene la reutilización de caché. La comprobación negativa de
  formato identifica tanto `Unformatted.kt` como el nuevo fixture CRLF de sintaxis
  válida; no se omiten tareas ni se aplica formato para aceptar la copia.
  [Matriz actualizada](.agents/skills/kotlin-valuable-tests/references/test-audit.md).
- `./gradlew check --no-daemon`: `BUILD SUCCESSFUL` en 1 min 16 s; 101 tareas,
  tres ejecutadas y 98 UP-TO-DATE, con caché de configuración reutilizada. Hooks:
  nueve grupos PASS; convenciones: ocho grupos PASS, incluidos aceptación LF y
  rechazo CRLF bajo defaults CRLF. Las suites JUnit y los análisis de los módulos
  reutilizaron sus resultados anteriores; los 17 SARIF vigentes siguen sin hallazgos.
  Sintaxis Bash y `git diff --check` correctos. No cambian escenarios de aplicación.
- La reproducción usa Git/Gradle/Spotless reales en Linux. Sigue pendiente confirmar
  el nuevo `check` en Windows; no se ejecutaron MSI/EXE ni instalación o arranque
  Windows. H6/H7/H8 conservan su pendiente nativo.

Commit de esta corrección: `6a0e547`
(`fix: preserve formatting policy in Gradle test fixture`).

### Estabilización posterior — preparación y cancelación CIO

Evidencia (2026-11-02, Linux; base `6a0e547`):

- El nuevo log Windows llega a los ocho PASS de convenciones, pero falla uno de
  los 40 casos Drive: `CIO caller cancellation interrupts stalled resumable upload`.
  El timeout de 150 ms corresponde al POST de creación de sesión, anterior al PUT
  retenido que la prueba debe cancelar. El cliente de pruebas aplicaba ese límite
  también a preparación; el cliente productivo conserva 15 s para metadatos.
- Se reprodujo el mismo fallo antes de corregirlo, retrasando 300 ms ese POST del
  servidor loopback: la ejecución focalizada falló con HttpRequestTimeoutException
  y request_timeout=150 ms. El cliente de pruebas reserva ahora 10 s para sesiones
  y consultas de estado, manteniendo defaults de petición/socket de 150 ms para
  los datos. La demora deliberada permanece como regresión.
- Los siete casos CIO usan runBlocking y un límite JUnit de 30 s por caso. Los
  dos segundos de cancelAndJoin son tiempo real. Las señales confirman la fase
  antes de cancelar y las respuestas retenidas se liberan en finally, sin cierre
  automático a los tres segundos. Las demoras de datos son 300 ms, inequívocamente
  mayores que sus defaults de 150 ms. Se conservan CIO, sockets reales, bytes,
  recuperación y limpieza; no se cambian fuentes productivas ni se omiten casos.
- `./gradlew :data:drive:spotlessApply :data:drive:test --tests
  'com.emusync.drive.DriveTransferCioTest' :data:drive:qualityCheck --no-daemon`:
  correcto en 17 s, siete casos aprobados; 26 tareas, seis ejecutadas y 20
  UP-TO-DATE. Detekt de pruebas ejecutado, sin hallazgos ni supresiones nuevas.
- `./gradlew check --no-daemon`: `BUILD SUCCESSFUL` en 1 min 14 s; 110 tareas,
  cuatro ejecutadas y 106 UP-TO-DATE. Drive ejecutó los 40 casos, todos aprobados;
  las demás suites y los análisis reutilizaron resultados correctos anteriores.
  Los 17 SARIF vigentes tienen cero hallazgos. Hooks: nueve grupos PASS;
  convenciones: ocho PASS. No se presentan las suites reutilizadas como nuevas.
- En una exportación temporal de HEAD con la prueba corregida, restaurar timeouts
  de transferencia de 150 ms hizo fallar los dos casos CIO de transferencia lenta.
  Confirma que el margen de las peticiones de control no oculta la regresión que
  protegen las pruebas. No se aplicó esa mutación al trabajo ni al índice.
- [Matriz](.agents/skills/kotlin-valuable-tests/references/test-audit.md) y
  [catálogo](.agents/skills/kotlin-valuable-tests/references/test-audit-cases.md)
  actualizados, sin altas ni bajas de casos de aplicación. Sigue pendiente el
  siguiente resultado Windows y la distribución/instalación nativa de H6/H7/H8.

### Diagnóstico posterior — carpetas legacy en Steam Deck

Evidencia (2026-11-02, Linux; base `489c8d5`, conservando la corrección CIO local
anterior, todavía sin commit):

- El usuario comunica un aviso de asignación ambigua únicamente en Steam Deck,
  donde recuerda que ambas carpetas existen. En el equipo disponible se contrastan
  dos carpetas locales con cuatro nombres remotos, todos con correspondencia única.
  La consulta manual de Drive fue de metadatos; las pruebas usan el fixture
  controlado. No se añaden credenciales, identificadores ni rutas personales al
  seguimiento. No se dispone del estado físico de las rutas en la Deck.
- Se reproduce el aviso con dos carpetas existentes pero vacías: la ausencia de
  archivos locales también impide recuperar su pertenencia en el formato compartido
  antiguo. No se ha confirmado un falso positivo en el dispositivo del usuario.
  El defecto corregido es el diagnóstico genérico, que ocultaba el motivo y los
  nombres implicados; no se elimina la protección de partidas.
- El aviso identifica carpetas configuradas, rutas no disponibles, nombres que
  colisionan y archivos remotos sin correspondencia; muestra hasta cinco ejemplos
  por grupo. La condición de bloqueo permanece equivalente. El diálogo limita la
  altura del mensaje y permite desplazarlo para consultar rutas largas.
- Se amplía el caso de carga legacy para demostrar sincronización de dos carpetas
  válidas sin mezclar destinos ni cambiar claves remotas. Los casos de rutas ausentes
  y colisiones exigen los datos útiles del aviso; se añade el caso de carpetas
  existentes y vacías. Las comprobaciones de diagnóstico fallaron con el mensaje
  anterior. La preparación del caso ampliado usa credenciales ficticias coherentes
  entre configuración y almacén. Suite focalizada final: 18 casos aprobados.
- `./gradlew :presentation:spotlessApply :presentation:test --tests
  'com.emusync.drive.SaveTargetMappingTest' :presentation:qualityCheck --no-daemon`:
  `BUILD SUCCESSFUL` en 21 s; 39 tareas, cinco ejecutadas y 34 UP-TO-DATE. Detekt de
  pruebas ejecutado y sin hallazgos. El agregado de calidad previo analizó también
  las fuentes modificadas de application y presentation, sin hallazgos.
- `./gradlew check packageReleaseAppImage --no-daemon`: `BUILD SUCCESSFUL` en
  1 min 15 s; 114 tareas, 12 ejecutadas y 102 UP-TO-DATE. Se ejecutaron application
  (cinco casos), presentation (192) y desktopApp (uno); las otras suites y los
  análisis reutilizaron resultados correctos. Evidencia combinada: 442 casos,
  440 aprobados y dos omitidos PowerShell, cero fallos/errores; 17 SARIF limpios.
  Hooks: nueve grupos PASS; convenciones: ocho PASS. Se preparó la distribución
  Linux de desktopApp; no se generó un AppImage independiente en este cierre.
- README, [matriz](.agents/skills/kotlin-valuable-tests/references/test-audit.md) y
  [catálogo](.agents/skills/kotlin-valuable-tests/references/test-audit-cases.md)
  actualizados; diff revisado. Se añade un único caso, sin bajas ni nuevas
  supresiones. La validación del diálogo y de las rutas reales en Steam Deck sigue
  pendiente, igual que el cierre nativo Windows de H6/H7/H8.
- El usuario identificó posteriormente un archivo `.txt` fuera de su ubicación
  como causa del aviso en Steam Deck. Se conservan las mejoras de diagnóstico y
  las pruebas de compatibilidad; no se modifica la regla de asignación ni se
  presenta este incidente como un fallo de sincronización confirmado.
