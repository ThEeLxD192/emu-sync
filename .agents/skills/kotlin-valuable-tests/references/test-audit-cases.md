# Escenarios auditados de H1

Fecha: 2026-10-30. Fuentes revisadas sobre `21e8e56`; entrega H1 registrada en `8c0f90c`.

Esta es la matriz por escenario de [la auditoría](test-audit.md). Cada fila identifica
el método exacto y su contrato observable en inglés. Riesgo, fallo plausible, punto
de entrada, nivel y dobles se indican en la cabecera de su archivo y se heredan por
las filas. «Conservar» significa que las aserciones de resultado, efecto o protocolo
observan el fallo indicado; no se consideran redundantes por compartir preparación.
Los niveles mixtos distinguen reglas de los casos con disco, HTTP o procesos descritos
en el nombre. Evidencia común: [E2](test-audit.md#ejecuciones); los cinco escenarios
reescritos también se ejecutaron en E1. Protección equivalente: no aplica en las filas
conservadas o reescritas; las cuatro retiradas tienen su tabla en la auditoría.

H1 dejó 426 métodos en 56 archivos de pruebas y 427 casos JUnit. Tras H3 hay
428 métodos en 61 archivos de pruebas, más `SaveMappingDrive.kt`: 62 archivos Kotlin
en total. Se conservan los 427 casos originales y se añaden dos comprobaciones de
los límites cambiados (429 casos). Las fuentes enlazadas reflejan su ubicación actual;
los resultados de H1 anteriores siguen siendo evidencia histórica. Tras H4 hay
430 métodos, 431 casos JUnit y 65 suites en 73 archivos Kotlin (ocho son fixtures).
H4 añade solo la compatibilidad literal del historial y la revisión del almacén de
credenciales inyectado; no retira escenarios. Las integraciones
que observan `AppStatus` usan ahora el adaptador de presentación; el caso nuevo de
respuestas a conflictos consume `SaveSynchronizer` y eventos neutrales directamente.
Tras H5 se conservan esos 430 métodos y 431 casos en 67 suites y 76 archivos Kotlin
(nueve fixtures): dos escenarios de reglas pasan a application; las integraciones
con adaptadores siguen en la raíz hasta desktopApp.
Tras H6 hay 431 métodos, 432 casos y 68 suites en 77 archivos Kotlin (nueve fixtures).
Se traslada plataforma con sus pruebas, se divide la suite mixta de actualizaciones
y se añade únicamente la regresión de preparación Steam inyectada.
Tras H7 hay 433 métodos, 434 casos y 70 suites en 81 archivos Kotlin (11 fixtures).
Tras la revisión posterior de H8 hay 440 métodos, 441 casos y 72 suites en 83 archivos Kotlin (11 fixtures).
Se conservan todos los escenarios anteriores y se añaden siete regresiones; las siete de
búsqueda de portadas pasan a plataforma.
Las pruebas visuales e integraciones viven en presentation y el ciclo de vida del
propietario de recursos se comprueba en desktopApp; no se retiran escenarios.

## data/local/config/ConfigLocationTest.kt

Fuente: [ConfigLocationTest.kt](../../../../data/local/src/test/kotlin/com/emusync/config/ConfigLocationTest.kt). Punto de entrada: `resolveDefaultConfigFile / getDefaultPlatformConfigFile`.
Nivel: integración local. Dobles/límites: archivos temporales y proveedores de entorno/propiedades.
Riesgo: media: elegir otra configuración o consultar proveedores de menor prioridad.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `explicit overrides preserve priority without consulting lower priority providers` | conservar: garantía observable específica de este escenario |
| `AppImage configuration precedes working directory and missing portable config falls back` | conservar: garantía observable específica de este escenario |
| `absent blank or parentless AppImage path preserves working directory fallback` | conservar: garantía observable específica de este escenario |
| `missing portable and working configurations use the platform directory` | conservar: garantía observable específica de este escenario |

## data/local/config/ConfigManagerTest.kt

Fuente: [ConfigManagerTest.kt](../../../../data/local/src/test/kotlin/com/emusync/config/ConfigManagerTest.kt). Punto de entrada: `ConfigManager.load/save / getDefaultPlatformConfigFile`.
Nivel: integración local. Dobles/límites: JSON literal, archivos temporales y fallo de escritura inyectado.
Riesgo: crítica: perder configuración/credenciales, romper JSON legacy o inferir incorrectamente destinos.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `empty configuration with automatic creation disabled preserves the file` | conservar: garantía observable específica de este escenario |
| `save reports failed parent creation without modifying existing files` | conservar: garantía observable específica de este escenario |
| `should serialize and deserialize polymorphic entries correctly` | conservar: garantía observable específica de este escenario |
| `should automatically create default config when file does not exist` | conservar: garantía observable específica de este escenario |
| `should throw IllegalArgumentException when autoCreate is false and file does not exist` | conservar: garantía observable específica de este escenario |
| `should create parent directories on save when they do not exist` | conservar: garantía observable específica de este escenario |
| `should resolve Windows platform default config path` | reescribir: exigir la ruta completa APPDATA/emusync/config.json, en vez de fragmentos que admitían directorios incorrectos |
| `legacy paths are migrated once to explicit save target types` | conservar: garantía observable específica de este escenario |
| `explicit save target type is never inferred again` | conservar: garantía observable específica de este escenario |
| `missing dotted legacy path stays unknown until created as a directory` | conservar: garantía observable específica de este escenario |
| `missing legacy per ROM paths migrate and resolve after creation` | conservar: garantía observable específica de este escenario |
| `interrupted save preserves last valid configuration` | conservar: garantía observable específica de este escenario |
| `saved configuration is private on POSIX systems` | conservar: garantía observable específica de este escenario |
| `existing directory target misconfigured as FILE resolves to DIRECTORY on load` | conservar: garantía observable específica de este escenario |

## data/local/config/ConfigUpdatesTest.kt

Fuente: [ConfigUpdatesTest.kt](../../../../data/local/src/test/kotlin/com/emusync/config/ConfigUpdatesTest.kt). Punto de entrada: `LibraryStore.updateEntries / loadEntries; CredentialStore.snapshotCredentials / compareAndSetCredentials`.
Nivel: integración local/concurrente. Dobles/límites: dos instancias, executor y barrera CountDownLatch.
Riesgo: crítica: perder ediciones concurrentes o restaurar credenciales desconectadas.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `updates from separate manager instances preserve concurrent additions` | conservar: garantía observable específica de este escenario |
| `explicit unlink invalidates an OAuth snapshot across manager instances` | conservar: garantía observable específica de este escenario |
| `credential compare and set retains unrelated updated configuration` | conservar: garantía observable específica de este escenario |

## drive/CloudFolderMigrationTest.kt

Fuente: [CloudFolderMigrationTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/CloudFolderMigrationTest.kt). Punto de entrada: `SyncOrchestrator.renameEntryCloudData / renameGameCloudData`.
Nivel: integración de flujo. Dobles/límites: MigrationDrive con MockEngine y fallos/mutaciones controlados.
Riesgo: crítica: perder partidas en un merge, mover datos ajenos o no recuperar tras cancelación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `files added by another device during merge remain in the source` | conservar: garantía observable específica de este escenario |
| `successful merge moves files and nested folders while keeping the source` | conservar: garantía observable específica de este escenario |
| `game rename refuses duplicate names before moving unique saves` | conservar: garantía observable específica de este escenario |
| `matching subfolders are preserved instead of creating ambiguous duplicate folders` | conservar: garantía observable específica de este escenario |
| `duplicate names already in source also block a merge` | conservar: garantía observable específica de este escenario |
| `failed move restores previous moves and reports failure` | conservar: garantía observable específica de este escenario |
| `failed rollback retains files on both sides and identifies recovery location` | conservar: garantía observable específica de este escenario |
| `cancellation restores acknowledged moves and propagates` | conservar: garantía observable específica de este escenario |
| `native group change moves only its game folder` | conservar: garantía observable específica de este escenario |
| `paths resolving to the same folder do not move or delete its contents` | conservar: garantía observable específica de este escenario |

## drive/ConditionalCloudWriteTest.kt

Fuente: [ConditionalCloudWriteTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/ConditionalCloudWriteTest.kt). Punto de entrada: `DriveConditionalUpdate.update / canonicalCloudFiles / sincronización rastreada`.
Nivel: protocolo e integración. Dobles/límites: MockEngine, SaveMappingDrive; CIO y servidor loopback en el caso de dos escritores.
Riesgo: crítica: sobrescribir una versión remota concurrente o seleccionar duplicados divergentes.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `concurrent remote change keeps local cloud and previous synchronization history` | conservar: garantía observable específica de este escenario |
| `simultaneous initial creates retain both copies and divergent duplicates block selection` | conservar: garantía observable específica de este escenario |
| `identical duplicate content remains safe to compare but unknown content does not` | conservar: garantía observable específica de este escenario |

## drive/DirectorySyncTest.kt

Fuente: [DirectorySyncTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/DirectorySyncTest.kt). Punto de entrada: `SyncOrchestrator.playWithSync / syncEntry / checkEntrySyncStatus`.
Nivel: integración de flujo. Dobles/límites: archivos/configuración reales; MockEngine y callbacks de conflicto/lanzamiento.
Riesgo: crítica: mezclar destinos, perder estructura/timestamps o sobrescribir el hermano de un directorio.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `play pre-sync separates unknown directory from same-name file sibling` | conservar: garantía observable específica de este escenario |
| `manual sync separates unknown directory from same-name file sibling` | conservar: garantía observable específica de este escenario |
| `migrated per ROM unknown directory restores before play and uploads after play` | conservar: garantía observable específica de este escenario |
| `post-sync does not update file sibling with colliding directory save` | conservar: garantía observable específica de este escenario |
| `migrated missing per ROM extensionless file restores exact Drive match before play` | conservar: garantía observable específica de este escenario |
| `checkEntrySyncStatus should detect OUT_OF_SYNC when local folder has newer files` | conservar: garantía observable específica de este escenario |
| `checkEntrySyncStatus should return IN_SYNC when content hashes match` | conservar: garantía observable específica de este escenario |
| `checkEntrySyncStatus should return NOT_CONFIGURED when no refreshToken` | conservar: garantía observable específica de este escenario |
| `syncEntry manual download should preserve nested directory structure` | conservar: garantía observable específica de este escenario |
| `syncEntry should prompt Conflict dialog on directory conflict and resolve to DOWNLOAD_CLOUD` | conservar: garantía observable específica de este escenario |
| `syncEntry manual download should preserve cloud modifiedTime on local files and subsequent check returns IN_SYNC` | conservar: garantía observable específica de este escenario |
| `syncEntry manual upload should update local file timestamp to Drive response modifiedTime and subsequent check returns IN_SYNC` | conservar: garantía observable específica de este escenario |
| `manual upload updates the cloud duplicate selected by the planner` | conservar: garantía observable específica de este escenario |
| `checkEntrySyncStatus should return IN_SYNC when directory has no local and no cloud files` | conservar: garantía observable específica de este escenario |
| `save target configured as FILE uploads directory contents if physical target is a directory` | conservar: garantía observable específica de este escenario |
| `multiple directory targets isolate files and maintain in sync status` | conservar: garantía observable específica de este escenario |

## data/drive/DriveFoldersTest.kt

Fuente: [DriveFoldersTest.kt](../../../../data/drive/src/test/kotlin/com/emusync/drive/DriveFoldersTest.kt). Punto de entrada: `DriveFolders.ensurePath / findPath / renameFolder / deleteFolder`.
Nivel: protocolo. Dobles/límites: HttpClient real con MockEngine y respuestas JSON controladas.
Riesgo: alta: consultar/mutar la carpeta equivocada, escapar mal nombres o repetir creación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `ensurePath creates nested folder hierarchy EmuSync to Switch to Game` | conservar: garantía observable específica de este escenario |
| `findPath returns leaf folder ID when path exists, and null when a folder is missing` | conservar: garantía observable específica de este escenario |
| `searchFolder properly escapes single quotes in game titles` | conservar: garantía observable específica de este escenario |
| `renameFolder sends PATCH with new name and parent parameters` | conservar: garantía observable específica de este escenario |
| `deleteFolder sends DELETE request to folder endpoint` | conservar: garantía observable específica de este escenario |

## domain/operation/OperationRecoveryTest.kt

Fuente: [OperationRecoveryTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/operation/OperationRecoveryTest.kt). Punto de entrada: `recoverOperationFailure`.
Nivel: unitaria. Dobles/límites: excepciones y runTest, sin red.
Riesgo: alta: consumir cancelación o recuperar perdiendo la causa/contexto.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `recovery receives the original failure` | conservar: garantía observable específica de este escenario |
| `cancellation escapes without invoking recovery` | conservar: garantía observable específica de este escenario |
| `suspending actions retain the caller coroutine` | conservar: garantía observable específica de este escenario |

## data/drive/DriveSearchTest.kt

Fuente: [DriveSearchTest.kt](../../../../data/drive/src/test/kotlin/com/emusync/drive/DriveSearchTest.kt). Punto de entrada: `DriveSearch.listFilesInFolder / listChildrenInFolder`.
Nivel: protocolo. Dobles/límites: MigrationDrive o HttpClient con MockEngine.
Riesgo: alta: omitir páginas o tratar carpetas como archivos de partida.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `save listings exclude folders while migration listings include them` | conservar: garantía observable específica de este escenario |
| `listFilesInFolder follows next page tokens and combines results` | conservar: garantía observable específica de este escenario |

## data/drive/DriveTransferCioTest.kt

Fuente: [DriveTransferCioTest.kt](../../../../data/drive/src/test/kotlin/com/emusync/drive/DriveTransferCioTest.kt). Punto de entrada: `DriveTransfer.download / upload`.
Nivel: integración de transporte. Dobles/límites: CIO real, servidor HTTP local y señales de hilos.
Riesgo: crítica: cortar transferencias lentas, bloquear cancelación o corromper reanudación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `CIO download completes when response streams slowly beyond configured request timeout` | conservar: bytes completos tras 300 ms frente a defaults de 150 ms, con reloj real |
| `CIO stalled download remains cancellable and cleans temporary file` | conservar: respuesta retenida hasta el cierre de la prueba, cancelación acotada con reloj real, partida intacta y limpieza de temporales |
| `CIO resumable upload completes when response is slow beyond configured request timeout` | conservar: transferencia lenta con CIO real; las peticiones de control tienen margen independiente de los defaults de datos |
| `CIO caller cancellation interrupts stalled resumable upload` | conservar: creación de sesión retrasada 300 ms, confirmación del PUT retenido y cancelación acotada con reloj real, sin modificar la partida |
| `CIO resumable upload recovers when server accepts a chunk but drops its response` | conservar: garantía observable específica de este escenario |
| `CIO resumable upload sends multiple chunks and follows range acknowledgements` | conservar: garantía observable específica de este escenario |
| `CIO resumable upload restarts once after session expiry` | conservar: garantía observable específica de este escenario |

## data/drive/DriveTransferTest.kt

Fuente: [DriveTransferTest.kt](../../../../data/drive/src/test/kotlin/com/emusync/drive/DriveTransferTest.kt). Punto de entrada: `DriveTransfer.download / upload / update`.
Nivel: protocolo con archivos. Dobles/límites: MockEngine, ByteChannel, disco temporal y progreso observable.
Riesgo: crítica: perder el archivo anterior, enviar bytes/rangos erróneos o reintentar un fallo permanente.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `download replaces destination and applies remote timestamp after success` | conservar: garantía observable específica de este escenario |
| `failed download preserves existing save and removes temporary files` | conservar: garantía observable específica de este escenario |
| `download fails cleanly when destination parent cannot be created` | conservar: garantía observable específica de este escenario |
| `download removes temporary file and preserves existing save when stream write fails` | conservar: garantía observable específica de este escenario |
| `cancelled download removes temporary file and preserves existing save` | conservar: garantía observable específica de este escenario |
| `download streams a slow large response and reports progress` | conservar: garantía observable específica de este escenario |
| `upload creates resumable session and sends file content` | reescribir: verificar bytes 1,2,3,4 en PUT además de sesión/rangos/progreso; TempDir y cierre del cliente aun si falla una aserción |
| `update starts resumable session with PATCH and uploads media` | conservar: garantía observable específica de este escenario |
| `large upload sends aligned chunks and follows Drive range acknowledgements` | conservar: garantía observable específica de este escenario |
| `upload recovers from lost chunk response using server acknowledged range` | conservar: garantía observable específica de este escenario |
| `upload retries a chunk when server reports no bytes received` | conservar: garantía observable específica de este escenario |
| `upload restarts once when resumable session expired` | conservar: garantía observable específica de este escenario |
| `upload handles empty save file` | conservar: garantía observable específica de este escenario |
| `upload propagates permanent server errors instead of reporting success` | conservar: garantía observable específica de este escenario |

## data/drive/OAuthFlowTest.kt

Fuente: [OAuthFlowTest.kt](../../../../data/drive/src/test/kotlin/com/emusync/drive/OAuthFlowTest.kt). Punto de entrada: `OAuthFlow.buildAuthUrl / validateCallback / callbackHandler / loopbackAddress`.
Nivel: unitaria y loopback. Dobles/límites: MockEngine que rechaza red; HttpServer/Socket locales para callbacks HTTP.
Riesgo: crítica: aceptar un callback sin estado válido o exponer el listener fuera de loopback.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `authorization URL includes the login state` | conservar: garantía observable específica de este escenario |
| `callback requires matching state before accepting code` | conservar: garantía observable específica de este escenario |
| `callback reports provider errors and malformed requests` | conservar: garantía observable específica de este escenario |
| `callback server address is restricted to loopback` | conservar: garantía observable específica de este escenario |
| `invalid and malformed HTTP callbacks do not prevent a valid callback` | conservar: garantía observable específica de este escenario |

## drive/OAuthPersistenceTest.kt

Fuente: [OAuthPersistenceTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/OAuthPersistenceTest.kt). Punto de entrada: `OAuthFlow.authorize con ConfigManager`.
Nivel: integración de autenticación. Dobles/límites: configuración real, MockEngine y autorización/barreras controladas.
Riesgo: crítica: aplicar un token obsoleto, borrar uno nuevo o perder ediciones durante OAuth.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `revoked token can reauthorize while preserving edits made during the new login` | conservar: garantía observable específica de este escenario |
| `interactive authorization merges token into current configuration` | conservar: garantía observable específica de este escenario |
| `invalid grant clears only the rejected token and preserves current entries` | conservar: garantía observable específica de este escenario |
| `late invalid grant cannot clear a replacement refresh token` | conservar: garantía observable específica de este escenario |
| `unlink cancels pending first login even when token was already null` | conservar: garantía observable específica de este escenario |
| `unlink during code exchange rejects the returned token` | conservar: garantía observable específica de este escenario |
| `valid refresh response is rejected when connection changed while waiting` | conservar: garantía observable específica de este escenario |
| `authorization with stale credentials makes no token request` | conservar: garantía observable específica de este escenario |
| `provider error text alone does not invalidate refresh token` | conservar: garantía observable específica de este escenario |
| `cancelled refresh propagates cancellation without changing credentials` | conservar: garantía observable específica de este escenario |

## drive/OfflineSyncTest.kt

Fuente: [OfflineSyncTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/OfflineSyncTest.kt). Punto de entrada: `OAuthFlow.authorize / SyncOrchestrator.playWithSync`.
Nivel: protocolo y flujo. Dobles/límites: MockEngine con fallo de red, configuración y callback de juego.
Riesgo: alta: abrir login interactivo offline, borrar un token válido o ocultar partidas pendientes.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `authorize with allowInteractive false should fail fast when no refresh token` | conservar: garantía observable específica de este escenario |
| `authorize with allowInteractive false should fail fast on network error without clearing token` | conservar: garantía observable específica de este escenario |
| `SyncOrchestrator should launch game and report unsynced save when offline` | conservar: garantía observable específica de este escenario |

## drive/PerGameSyncTest.kt

Fuente: [PerGameSyncTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/PerGameSyncTest.kt). Punto de entrada: `SyncOrchestrator.syncEntry / playWithSync / renameGameCloudData`.
Nivel: reglas e integración de flujo. Dobles/límites: MockEngine, archivos temporales y callbacks de conflicto/configuración/lanzamiento.
Riesgo: crítica: subir datos de otro juego, aplanar rutas o lanzar tras cancelar la restauración.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `syncEntry uploads individual game saves into game subfolder on Google Drive` | conservar: garantía observable específica de este escenario |
| `playWithSync postSync uploads into game-scoped subfolder` | conservar: garantía observable específica de este escenario |
| `renameGameCloudData renames cloud folder when destination does not exist yet` | conservar: garantía observable específica de este escenario |
| `renameGameCloudData prompts Conflict when destination already exists and resolves to DOWNLOAD_CLOUD` | conservar: garantía observable específica de este escenario |
| `rename conflict date ignores newer in-sync file sibling excluded from directory` | conservar: garantía observable específica de este escenario |
| `rename uploads nested directory saves with their relative cloud paths` | conservar: garantía observable específica de este escenario |
| `rename downloads nested directory saves to their relative local paths` | conservar: garantía observable específica de este escenario |
| `playWithSync prompts for save path before launch when cloud saves exist and downloads them to selected path` | conservar: garantía observable específica de este escenario |
| `playWithSync aborts launch when cloud saves exist and user cancels save setup` | conservar: garantía observable específica de este escenario |
| `playWithSync launches without pre-prompt when no cloud saves exist and prompts after game exits` | conservar: garantía observable específica de este escenario |

## drive/PostPlayAuthorizationTest.kt

Fuente: [PostPlayAuthorizationTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/PostPlayAuthorizationTest.kt). Punto de entrada: `SyncOrchestrator.playWithSync`.
Nivel: integración de flujo. Dobles/límites: MockEngine con tokens renovados/rechazados y desconexión durante el juego.
Riesgo: crítica: subir partidas usando una conexión desconectada o un token caducado.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `post-play requests use a fresh token when the pre-play token is no longer accepted` | conservar: garantía observable específica de este escenario |
| `unlink during play prevents uploading with the old connection` | conservar: garantía observable específica de este escenario |

## drive/PostSyncFailureTest.kt

Fuente: [PostSyncFailureTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/PostSyncFailureTest.kt). Punto de entrada: `SyncOrchestrator.playWithSync y callback de reintento`.
Nivel: integración de flujo. Dobles/límites: MockEngine con fallos parciales y elección de conflicto; dos valores EnumSource.
Riesgo: crítica: repetir partidas ya subidas, relanzar el juego o forzar la copia equivocada.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `partial post game upload reports successes and retries only failed saves` | conservar: garantía observable específica de este escenario |
| `retry retries transient upload while keeping name collision blocked` | conservar: garantía observable específica de este escenario |
| `directory retry resolves a newer cloud conflict using the selected copy [UPLOAD_LOCAL, DOWNLOAD_CLOUD]` | conservar: garantía observable específica de este escenario |

## drive/SaveRestorePathsTest.kt

Fuente: [SaveRestorePathsTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/SaveRestorePathsTest.kt). Punto de entrada: `resolveCloudRestoreTargets / resolveCloudRestorePaths / resolveRemoteFile`.
Nivel: reglas con filesystem. Dobles/límites: TempDir y symlink real para escape de ruta.
Riesgo: crítica: escribir fuera del destino o colapsar dos claves remotas en un archivo.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `nested restore targets retain their exact remote keys` | conservar: garantía observable específica de este escenario |
| `different remote names resolving to one local file are rejected before restoration` | conservar: garantía observable específica de este escenario |
| `remote file cannot also become a parent directory` | conservar: garantía observable específica de este escenario |
| `restore paths are empty until a destination is selected` | conservar: garantía observable específica de este escenario |
| `restore folder resolves to one explicit path per cloud file` | conservar: garantía observable específica de este escenario |
| `restore folder ignores names that escape the selected folder` | conservar: garantía observable específica de este escenario |
| `remote paths reject traversal absolute paths and empty segments` | conservar: garantía observable específica de este escenario |
| `remote paths accept nested names with either separator` | conservar: garantía observable específica de este escenario |
| `restore paths reject a nested symlink that points outside the selected folder` | conservar: garantía observable específica de este escenario |

## drive/SaveTargetMappingTest.kt

Fuente: [SaveTargetMappingTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/SaveTargetMappingTest.kt). Punto de entrada: `SyncOrchestrator.syncEntry / playWithSync / scopeNewSaveTargets / ConfigManager.load`.
Nivel: integración de flujo y reglas. Dobles/límites: SaveMappingDrive, configuración/disco reales y callbacks.
Riesgo: crítica: confundir prefijos portables, reclamar archivos no asignados o lanzar con rutas inválidas.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `new directory scopes upload and restore without depending on local folder names` | conservar: garantía observable específica de este escenario |
| `directory restore rejects a remote file that is also a parent path` | conservar: garantía observable específica de este escenario |
| `legacy target migration preserves cloud paths globally and per ROM` | conservar: garantía observable específica de este escenario |
| `loading legacy directories keeps their original flat cloud layout` | ampliar: carga rutas legacy y sincroniza carpetas con archivos distintos sin mezclar destinos ni cambiar nombres remotos; una actualización modifica solo su archivo |
| `individual nested restore keeps exact keys before play and when uploading` | conservar: garantía observable específica de este escenario |
| `two missing directory roots restore identical relative names to their own destinations` | conservar: garantía observable específica de este escenario |
| `saved mapping survives changing local paths and reversing directory order` | conservar: garantía observable específica de este escenario |
| `manual upload updates only the mapped directory even when relative names match` | conservar: garantía observable específica de este escenario |
| `pre-play and post-play use the same directory mappings` | conservar: garantía observable específica de este escenario |
| `partial upload retry retains the full remote path and does not repeat completed saves` | conservar: garantía observable específica de este escenario |
| `ambiguous legacy restore fails before writing files or launching` | ampliar: conserva bloqueo previo a efectos e identifica las rutas configuradas para diagnosticar el dispositivo afectado |
| `existing empty legacy directories report unmatched cloud files without restoring them` | añadir: aunque existan las carpetas, el aviso identifica destinos y archivos remotos sin correspondencia, manteniéndolos intactos |
| `legacy roots with colliding local names are not selected by timestamps` | ampliar: conserva ambas partidas e identifica el nombre relativo que colisiona |
| `legacy roots do not claim newly discovered unassigned remote files` | conservar: garantía observable específica de este escenario |
| `file mapping survives a different local filename` | conservar: garantía observable específica de este escenario |
| `overlapping directory mappings fail before any transfer` | conservar: garantía observable específica de este escenario |
| `invalid cloud path does not silently fall back to playing offline` | conservar: garantía observable específica de este escenario |
| `new directory scopes are distinct even with the same basename and explicit prefixes` | conservar: garantía observable específica de este escenario |

## domain/sync/SyncConflictTest.kt

Fuente: [SyncConflictTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/sync/SyncConflictTest.kt). Punto de entrada: `resolveConflict / planSyncDecision`.
Nivel: unitaria. Dobles/límites: fechas constantes y esperados explícitos.
Riesgo: crítica: invertir dirección, truncar fracciones o equivocarse en el límite de tolerancia.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `local file significantly newer should resolve to CONFLICT` | conservar: garantía observable específica de este escenario |
| `cloud file significantly newer should resolve to DOWNLOAD_CLOUD` | conservar: garantía observable específica de este escenario |
| `identical timestamps should resolve to IN_SYNC` | conservar: garantía observable específica de este escenario |
| `timestamps within tolerance threshold (3s) should resolve to IN_SYNC` | conservar: garantía observable específica de este escenario |
| `timestamps exactly at boundary should resolve to IN_SYNC` | conservar: garantía observable específica de este escenario |
| `local timestamps beyond threshold (6s) should resolve to CONFLICT` | conservar: garantía observable específica de este escenario |
| `fractional timestamps beyond threshold are not truncated` | conservar: garantía observable específica de este escenario |
| `convenience overload with epoch millis and ISO string works correctly` | conservar: garantía observable específica de este escenario |
| `plan chooses no action when neither copy exists` | conservar: garantía observable específica de este escenario |
| `plan downloads when only cloud copy exists` | conservar: garantía observable específica de este escenario |
| `plan uploads when only local copy exists` | conservar: garantía observable específica de este escenario |
| `plan delegates to timestamp conflict policy when both copies exist` | conservar: garantía observable específica de este escenario |
| `post-play policy uploads even when local timestamp is inside conflict window` | conservar: garantía observable específica de este escenario |

## domain/sync/SyncPlannerTest.kt

Fuente: [SyncPlannerTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/sync/SyncPlannerTest.kt). Punto de entrada: `planDirectorySync`.
Nivel: unitaria. Dobles/límites: estados de archivos inmutables y fechas constantes.
Riesgo: crítica: acciones no deterministas, duplicado equivocado o no subir progreso posterior al juego.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `planner creates deterministic actions for local cloud and shared files` | conservar: garantía observable específica de este escenario |
| `post-play planning prefers newer local progress` | conservar: garantía observable específica de este escenario |
| `duplicate cloud paths choose the newest file independent of input order` | conservar: garantía observable específica de este escenario |

## drive/TrackedFileSyncTest.kt

Fuente: [TrackedFileSyncTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/TrackedFileSyncTest.kt). Punto de entrada: `TrackedFileSync / SyncHistoryStore / SyncOrchestrator`.
Nivel: reglas e integración local/flujo. Dobles/límites: SaveMappingDrive, disco real, snapshots y fallos de backup/historial.
Riesgo: crítica: ocultar cambios offline, adelantar historial tras un fallo o reemplazar progreso sin recuperación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `download preserves a deduplicated recovery copy and advances history` | conservar: garantía observable específica de este escenario |
| `backup failure prevents replacement` | conservar: garantía observable específica de este escenario |
| `checksum mismatch and cancelled downloads keep local data and history` | conservar: garantía observable específica de este escenario |
| `local progress created during download is retained` | conservar: garantía observable específica de este escenario |
| `upload uses a stable snapshot and newer local progress remains pending` | conservar: garantía observable específica de este escenario |
| `failed upload leaves previous history and local progress intact` | conservar: garantía observable específica de este escenario |
| `two devices changing offline conflict after restart even with identical dates` | conservar: garantía observable específica de este escenario |
| `status inspection does not establish synchronization history` | conservar: garantía observable específica de este escenario |
| `pre-play local choice does not force unrelated post-play conflicts` | conservar: garantía observable específica de este escenario |
| `corrupt history aborts launch instead of falling back to offline play` | conservar: garantía observable específica de este escenario |
| `failed explicit local resolution aborts launch and leaves both copies intact` | conservar: garantía observable específica de este escenario |

## data/local/config/ConfigGroupsTest.kt

Fuente: [ConfigGroupsTest.kt](../../../../data/local/src/test/kotlin/com/emusync/config/ConfigGroupsTest.kt). Punto de entrada: `ConfigManager.load/save con DTO y conversiones explícitas`.
Nivel: reglas y persistencia. Dobles/límites: entradas reales; JSON legacy literal y TempDir en persistencia.
Riesgo: alta: cambiar la carpeta compartida o romper group/cloudFolder al persistir.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `ConfigManager serializes and deserializes group and cloudFolder` | conservar: garantía observable específica de este escenario |
| `ConfigManager deserializes legacy JSON without group or cloudFolder` | conservar: garantía observable específica de este escenario |

## data/local/saves/GameEntrySaveTargetsTest.kt

Fuente: [GameEntrySaveTargetsTest.kt](../../../../data/local/src/test/kotlin/com/emusync/local/saves/GameEntrySaveTargetsTest.kt). Punto de entrada: `effectiveSaveTargets / effectiveSaveTargetsForRom`.
Nivel: reglas con filesystem. Dobles/límites: archivos/directorios reales y rutas ausentes en TempDir.
Riesgo: crítica: perder prioridad de overrides o inferir un tipo incorrecto para rutas ausentes.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `existing save paths resolve their actual type and normalize blank cloud paths` | conservar: garantía observable específica de este escenario |
| `missing save paths preserve declared types and explicit cloud names` | conservar: garantía observable específica de este escenario |
| `ROM target lookup preserves typed exact stem and legacy fallback priority` | reescribir: esperar SaveTarget UNKNOWN explícito para ambas rutas legacy ausentes; no llamar al helper de producción para construir el esperado |

## domain/library/GameTitleUtilsTest.kt

Fuente: [GameTitleUtilsTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/library/GameTitleUtilsTest.kt). Punto de entrada: `cleanGameTitle / normalizeGameKey / EmulatorSystem.getEffectiveTitle`.
Nivel: unitaria. Dobles/límites: nombres/overrides literales, sin I/O.
Riesgo: media: borrar indicadores de disco o elegir otro título al cambiar de dispositivo.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `cleanGameTitle replaces underscores with spaces` | conservar: garantía observable específica de este escenario |
| `cleanGameTitle removes dump tags in brackets` | conservar: garantía observable específica de este escenario |
| `cleanGameTitle removes region and language tags in parentheses` | conservar: garantía observable específica de este escenario |
| `cleanGameTitle preserves multi-disc and multi-part indicators in parentheses` | conservar: garantía observable específica de este escenario |
| `cleanGameTitle strips file extension if present` | conservar: garantía observable específica de este escenario |
| `cleanGameTitle collapses extra whitespace and trims` | conservar: garantía observable específica de este escenario |
| `cleanGameTitle falls back to original trimmed if everything was stripped` | conservar: garantía observable específica de este escenario |
| `normalizeGameKey matches games across different naming conventions` | conservar: garantía observable específica de este escenario |
| `EmulatorSystem getEffectiveTitle resolves exact match first` | conservar: garantía observable específica de este escenario |
| `EmulatorSystem getEffectiveTitle resolves match without extension` | conservar: garantía observable específica de este escenario |
| `EmulatorSystem getEffectiveTitle resolves flexible normalized cross-device match` | conservar: garantía observable específica de este escenario |
| `EmulatorSystem getEffectiveTitle falls back to cleanGameTitle when no override set` | conservar: garantía observable específica de este escenario |
| `title lookup preserves exact then stem then normalized priority and ignores blank values` | conservar: garantía observable específica de este escenario |
| `normalized title lookup preserves first nonblank matching entry` | conservar: garantía observable específica de este escenario |

## platform/desktop/runner/ExternalProcessTrackerTest.kt

Fuente: [ExternalProcessTrackerTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/runner/ExternalProcessTrackerTest.kt). Punto de entrada: `ExternalProcessTracker.waitForProcess`.
Nivel: unitaria con tiempo virtual. Dobles/límites: runTest y sondeos/tiempo/procesos falsos.
Riesgo: alta: esperar un proceso ajeno, terminar antes del juego o ignorar cancelación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `appearance timeout ends without tracking unrelated existing processes` | conservar: garantía observable específica de este escenario |
| `shader compilation renews the timeout until the game appears and exits` | conservar: garantía observable específica de este escenario |
| `timeout resumes after shader compilation stops without a game appearing` | conservar: garantía observable específica de este escenario |
| `desktop handoff confirms reuse and waits for the existing game to exit` | conservar: garantía observable específica de este escenario |
| `tracking follows replacement processes and excludes a preexisting decoy` | conservar: garantía observable específica de este escenario |
| `caller cancellation interrupts waiting for a running game` | conservar: garantía observable específica de este escenario |

## platform/desktop/runner/GameRunnerTest.kt

Fuente: [GameRunnerTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/runner/GameRunnerTest.kt). Punto de entrada: `GameRunner.launch / openEmulator / isProcessRunning / isSteamShaderCompiling`.
Nivel: integración de procesos. Dobles/límites: procesos reales aislados, scripts y archivos de argumentos.
Riesgo: crítica: lanzar con argumentos alterados o sincronizar antes de que termine el juego correcto.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `Steam launch uses the injected preparation and stops on its failure` | añadir en H6: usa GameRunner.launch con preparación inyectada; verifica entrada y ruta exactas, conserva ROM y propaga el fallo antes del handoff a Steam |
| `openEmulator starts without rom arguments` | conservar: garantía observable específica de este escenario |
| `should fail when EmulatorSystem is launched without romFile` | conservar: garantía observable específica de este escenario |
| `should successfully launch and wait for a direct command` | conservar: garantía observable específica de este escenario |
| `should successfully launch native game direct command` | conservar: garantía observable específica de este escenario |
| `isProcessRunning should return false for blank or non-existent process` | conservar: garantía observable específica de este escenario |
| `isProcessRunning should detect active process and handle lifecycle` | conservar: garantía observable específica de este escenario |
| `should wait for external process when launcher exits early` | conservar: garantía observable específica de este escenario |
| `existing process with the same name does not delay newly launched game` | conservar: garantía observable específica de este escenario |
| `existing single instance process is tracked when launcher reuses it` | conservar: garantía observable específica de este escenario |
| `successful desktop handoff can confirm a reused process` | conservar: garantía observable específica de este escenario |
| `failed launch request does not treat an existing process as a launched game` | conservar: garantía observable específica de este escenario |
| `isSteamShaderCompiling should detect fossilize_replay active state` | conservar: garantía observable específica de este escenario |
| `direct launch preserves quoted fullscreen arguments and expands inline ROM placeholder` | conservar: garantía observable específica de este escenario |

## platform/desktop/scanner/RomScannerTest.kt

Fuente: [RomScannerTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/scanner/RomScannerTest.kt). Punto de entrada: `scanRoms`.
Nivel: integración local. Dobles/límites: árboles de archivos, firmas y symlinks temporales.
Riesgo: alta: recursión infinita, juegos duplicados u ocultar ROMs por una firma excluida.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `should scan ROM files recursively and ignore non-ROM files` | conservar: garantía observable específica de este escenario |
| `should return empty list when scanning nonexistent directory` | conservar: garantía observable específica de este escenario |
| `should handle empty extensions list gracefully` | conservar: garantía observable específica de este escenario |
| `symlink cycles and aliases do not duplicate games or recurse indefinitely` | conservar: garantía observable específica de este escenario |
| `extensions accept whitespace dots and mixed case` | conservar: garantía observable específica de este escenario |
| `signature files identify complete games and exclude their individual ROMs` | conservar: garantía observable específica de este escenario |
| `signature with an excluded extension does not hide matching ROMs or subfolders` | conservar: garantía observable específica de este escenario |
| `a regular file cannot be scanned as a directory` | conservar: garantía observable específica de este escenario |
| `extensions that normalize to empty do not match files without an extension` | conservar: garantía observable específica de este escenario |

## platform/desktop/steam/BinaryVdfFieldTest.kt

Fuente: [BinaryVdfFieldTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/steam/BinaryVdfFieldTest.kt). Punto de entrada: `BinaryVdf.read`.
Nivel: compatibilidad de formato. Dobles/límites: bytes VDF escritos por fixture independiente del escritor de producción.
Riesgo: alta: perder aliases legacy o dar prioridad incorrecta a campos canónicos vacíos.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `legacy aliases remain fallbacks and empty canonical fields retain precedence` | conservar: garantía observable específica de este escenario |

## platform/desktop/steam/BinaryVdfTest.kt

Fuente: [BinaryVdfTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/steam/BinaryVdfTest.kt). Punto de entrada: `BinaryVdf.write/read`.
Nivel: integración local de formato. Dobles/límites: shortcuts conocidos y archivos temporales.
Riesgo: alta: perder campos/entradas al reescribir shortcuts.vdf.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `should round-trip encode and decode Steam shortcuts binary VDF` | conservar: garantía observable específica de este escenario |
| `should append new shortcut and preserve existing shortcuts` | conservar: garantía observable específica de este escenario |
| `should handle empty shortcuts list` | conservar: garantía observable específica de este escenario |

## platform/desktop/steam/LauncherIsolationTest.kt

Fuente: [LauncherIsolationTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/steam/LauncherIsolationTest.kt). Punto de entrada: `SteamShortcutManager.launcherKey / migrateLegacyLaunchers / createLinuxWrapperScript`.
Nivel: integración de lanzadores. Dobles/límites: scripts/procesos reales, VDF y señales en TempDir.
Riesgo: crítica: colisión de nombres, lanzar otro juego o romper accesos ya cargados por Steam.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `normalized names Unicode and case variations have different stable paths` | conservar: garantía observable específica de este escenario |
| `migration retains Steam ids settings and a working legacy shortcut` | conservar: garantía observable específica de este escenario |
| `ambiguous old alias refuses to launch but new wrappers keep separate ROM signals` | conservar: garantía observable específica de este escenario |
| `renaming a hashed launcher keeps the shortcut already loaded by Steam working` | conservar: garantía observable específica de este escenario |

## platform/desktop/steam/SteamSaveSafetyTest.kt

Fuente: [SteamSaveSafetyTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/steam/SteamSaveSafetyTest.kt). Punto de entrada: `wrapper Linux generado / registerEntry / BinaryVdf.read/write`.
Nivel: integración de plataforma. Dobles/límites: bash real, argumentos literales, disco y fallo de escritura inyectado.
Riesgo: crítica: ejecutar metacaracteres o destruir el VDF ante truncado/fallo.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `Linux wrapper preserves literal arguments quoted flags and embedded ROM paths` | conservar: garantía observable específica de este escenario |
| `native re-registration updates launch paths while preserving Steam id and preferences` | conservar: garantía observable específica de este escenario |
| `interrupted VDF replacement leaves the complete previous file` | conservar: garantía observable específica de este escenario |
| `truncated VDF values are rejected instead of being rewritten as valid entries` | conservar: garantía observable específica de este escenario |
| `NUL in VDF fields is rejected before touching the original file` | conservar: garantía observable específica de este escenario |

## platform/desktop/steam/SteamShortcutManagerTest.kt

Fuente: [SteamShortcutManagerTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/steam/SteamShortcutManagerTest.kt). Punto de entrada: `SteamShortcutManager.registerEntry / unregisterEntry / prepareLaunch / búsquedas de Steam`.
Nivel: integración local y reglas. Dobles/límites: VDF/scripts reales, consulta de registro y plataforma controladas.
Riesgo: alta: duplicar shortcuts, perder appId/opciones, señalizar otra ROM o elegir otra cuenta.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `should register emulator entry and create shortcut on Linux` | conservar: garantía observable específica de este escenario |
| `should register emulator entry and create batch script on Windows` | conservar: garantía observable específica de este escenario |
| `should preserve appId when re-registering entry with modified path` | conservar: garantía observable específica de este escenario |
| `should unregister entry cleanly` | conservar: garantía observable específica de este escenario |
| `should register native PC game directly with launch options` | conservar: garantía observable específica de este escenario |
| `should parse Windows registry output accurately` | conservar: garantía observable específica de este escenario |
| `should locate Windows Steam userdata directory via registry query` | conservar: garantía observable específica de este escenario |
| `should detect fullscreen flags for various emulators including RetroArch` | conservar: garantía observable específica de este escenario |
| `prepareLaunch writes ROM path to signal file correctly` | conservar: garantía observable específica de este escenario |
| `findShortcutsFile should pick most recently active user account skipping 0` | conservar: garantía observable específica de este escenario |

## platform/desktop/steam/WindowsLauncherTest.kt

Fuente: [WindowsLauncherTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/steam/WindowsLauncherTest.kt). Punto de entrada: `wrapper/configuración generados y lanzador PowerShell`.
Nivel: artefacto e integración de proceso. Dobles/límites: JSON/batch reales; PowerShell y grabador de argumentos cuando están disponibles.
Riesgo: crítica: interpolar metacaracteres, perder argumentos vacíos o alterar el código de salida.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `wrapper keeps paths and arguments outside batch source` | conservar: garantía observable específica de este escenario |
| `PowerShell launcher preserves empty quotes slashes metacharacters and exit code` | conservar: garantía observable específica de este escenario |
| `missing ROM and batch emulator fail without launching` | conservar: garantía observable específica de este escenario |

## platform/desktop/steam/WindowsSteamPathsTest.kt

Fuente: [WindowsSteamPathsTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/steam/WindowsSteamPathsTest.kt). Punto de entrada: `WindowsSteamPaths.find`.
Nivel: unitaria con filesystem. Dobles/límites: consultas de registro falsas y directorios userdata reales.
Riesgo: alta: elegir una instalación vacía o ignorar registro nativo tras el redirigido.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `user registry location avoids consulting machine registry` | conservar: garantía observable específica de este escenario |
| `existing machine userdata precedes an uninitialized user install` | conservar: garantía observable específica de este escenario |
| `native machine registry is checked after missing redirected registry` | conservar: garantía observable específica de este escenario |

## ui/AppViewModelTest.kt

Fuente: [AppViewModelTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/AppViewModelTest.kt). Punto de entrada: `AppViewModel y uiState`.
Nivel: integración de presentación. Dobles/límites: configuración/scanner reales; MockEngine y contratos de actualizaciones controlados.
Riesgo: alta: UI y configuración divergentes, perder overrides o no informar fallo de reinicio.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `should load config and auto-select first entry in uiState` | conservar: garantía observable específica de este escenario |
| `should select native game entry and update uiState` | conservar: garantía observable específica de este escenario |
| `should update status reactively in uiState` | conservar: garantía observable específica de este escenario |
| `reorderEntries should change entry positions and persist to config` | conservar: garantía observable específica de este escenario |
| `should auto-create default config and reach Idle state when config file is missing` | conservar: garantía observable específica de este escenario |
| `setupGoogleDrive should save credentials and unlinkGoogleDrive should clear refreshToken` | conservar: garantía observable específica de este escenario |
| `restartApp sets UpdateUiState Error when updateManager fails` | conservar: garantía observable específica de este escenario |
| `restartApp returns true when updateManager succeeds` | conservar: garantía observable específica de este escenario |
| `editGameOverride updates coverPathByRom and savePathsByRom for EmulatorSystem` | conservar: garantía observable específica de este escenario |
| `clearing game save paths removes override and preserves global fallback` | conservar: garantía observable específica de este escenario |
| `editGameOverride updates custom title in titleByRom for EmulatorSystem` | conservar: garantía observable específica de este escenario |
| `editGameOverride updates coverPath and savePaths for NativePCGame` | conservar: garantía observable específica de este escenario |
| `addEntry and editEntry preserve and update group and cloudFolder` | conservar: garantía observable específica de este escenario |

## ui/CloudFolderEditTest.kt

Fuente: [CloudFolderEditTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/CloudFolderEditTest.kt). Punto de entrada: `AppViewModel.editEntry / editGameOverride`.
Nivel: integración de presentación/flujo. Dobles/límites: MigrationDrive, disco y respuestas de migración controlados.
Riesgo: crítica: persistir una edición bloqueada o mover la carpeta compartida de otros juegos.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `entry migration preserves unrelated configuration changes while waiting for Drive` | conservar: garantía observable específica de este escenario |
| `regrouping a shared emulator keeps its cloud location and other entries` | conservar: garantía observable específica de este escenario |
| `explicit migration of a shared root is rejected before saving` | conservar: garantía observable específica de este escenario |
| `blocked entry merge keeps old configuration and both remote copies` | conservar: garantía observable específica de este escenario |
| `blocked game title merge keeps its previous title and remote saves` | conservar: garantía observable específica de este escenario |
| `renaming grouped native game updates only its own folder once` | conservar: garantía observable específica de este escenario |
| `clearing a custom title migrates saves back to the default title` | conservar: garantía observable específica de este escenario |

## platform/desktop/CoverLookupTest.kt

Fuente: [CoverLookupTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/desktop/CoverLookupTest.kt). Punto de entrada: `DesktopCoverFiles.find`.
Nivel: integración local. Dobles/límites: archivos temporales y prioridades de ubicación explícitas.
Riesgo: media: mostrar portada de otro origen ignorando override o fallback.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `resolves cover when image is in rom directory with exact name` | conservar: garantía observable específica de este escenario |
| `resolves cover from custom coversDirectory` | conservar: garantía observable específica de este escenario |
| `resolves cover from covers subfolder with region tag stripped` | conservar: garantía observable específica de este escenario |
| `resolves cover for native game with custom coverPath` | conservar: garantía observable específica de este escenario |
| `resolves custom coverPathByRom with highest priority over same-folder and fallback` | conservar: garantía observable específica de este escenario |
| `resolves same-folder image over fallback subfolder when individual cover is not set` | conservar: garantía observable específica de este escenario |
| `resolves fallback subfolder when no individual cover and no same-folder image exists` | conservar: garantía observable específica de este escenario |

## ui/DriveConnectionTest.kt

Fuente: [DriveConnectionTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/DriveConnectionTest.kt). Punto de entrada: `AppViewModel.loginGoogleDrive / setupGoogleDrive / unlinkGoogleDrive`.
Nivel: integración de presentación/autenticación. Dobles/límites: MockEngine, ConfigManager y autorización suspendida controlada.
Riesgo: crítica: reconectar tras unlink, perder entradas o mostrar credenciales obsoletas.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `browser timeout returns an actionable login error and preserves configuration` | conservar: garantía observable específica de este escenario |
| `adding an entry while browser authorization waits keeps the entry and new token` | conservar: garantía observable específica de este escenario |
| `library edits preserve credentials updated outside the current UI snapshot` | conservar: garantía observable específica de este escenario |
| `changing OAuth client clears a token issued for the previous client` | conservar: garantía observable específica de este escenario |
| `saving unchanged credentials retains the existing refresh token` | conservar: garantía observable específica de este escenario |
| `unlink resets entry sync indicators` | conservar: garantía observable específica de este escenario |
| `pending login cannot reconnect Drive after unlink` | conservar: garantía observable específica de este escenario |
| `background invalid grant updates the UI connection status` | conservar: garantía observable específica de este escenario |
| `cancelled login propagates cancellation` | conservar: garantía observable específica de este escenario |

## platform/desktop/picker/FilePickerTest.kt

Fuente: [FilePickerTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/desktop/picker/FilePickerTest.kt). Punto de entrada: `FilePicker.pickPath / pickFile / pickDirectory`.
Nivel: unitaria de frontera. Dobles/límites: proveedores de plataforma y selectores falsos, restaurados en AfterEach.
Riesgo: media: reabrir otro diálogo tras cancelar o perder directorio/archivo inicial.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `pickPath returns null for PickerType ANY` | conservar: garantía observable específica de este escenario |
| `pickPath delegates to fileChooser for PickerType FILE on Windows` | conservar: garantía observable específica de este escenario |
| `pickFile uses Linux external tool when available` | conservar: garantía observable específica de este escenario |
| `pickFile returns null and does not fall back to AWT when Linux external tool is cancelled` | conservar: garantía observable específica de este escenario |
| `pickFile falls back to fileChooser on Linux when external tools are not available` | conservar: garantía observable específica de este escenario |
| `pickDirectory delegates directly to Swing chooser on Windows` | conservar: garantía observable específica de este escenario |
| `pickDirectory uses Linux external tool when available` | conservar: garantía observable específica de este escenario |
| `pickDirectory returns null and does not fall back to Swing when Linux external tool is cancelled` | conservar: garantía observable específica de este escenario |
| `pickDirectory falls back to Swing chooser on Linux when external tools are not available` | conservar: garantía observable específica de este escenario |
| `parentDir defaults to user home when initialPath is blank` | conservar: garantía observable específica de este escenario |

## ui/GameCoverLoaderTest.kt

Fuente: [GameCoverLoaderTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/GameCoverLoaderTest.kt). Punto de entrada: `loadGameCover / loadGameCoverAsync`.
Nivel: integración local de imágenes. Dobles/límites: PNG reales, cache y dispatcher de runTest.
Riesgo: media: portada obsoleta tras reemplazo o excepción al abrir imagen corrupta.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `grid and preview requests resolve and decode covers on the IO dispatcher` | añadir tras H8: el localizador exige un worker distinto del llamador; PNG real, caché compartida y override ausente sin fallback |
| `async cover loading uses the same cache with an injected dispatcher` | conservar: garantía observable específica de este escenario |
| `replacing cover content at the same path invalidates the cached bitmap` | conservar: garantía observable específica de este escenario |
| `missing directory and corrupt covers fall back without crashing` | conservar: garantía observable específica de este escenario |

## ui/LibraryConcurrencyTest.kt

Fuente: [LibraryConcurrencyTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/LibraryConcurrencyTest.kt). Punto de entrada: `AppViewModel / LibraryController / LibraryOperationCoordinator`.
Nivel: integración de coordinación. Dobles/límites: CompletableDeferred, StateFlow, MockEngine/SaveMappingDrive y disco reales.
Riesgo: crítica: operación duplicada, diálogo obsoleto o respuesta tardía que pisa el estado vigente.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `latest selection wins when scans complete in reverse order` | conservar: garantía observable específica de este escenario |
| `cancelled old scan cannot clear newer loading state` | conservar: garantía observable específica de este escenario |
| `deleting a category invalidates its unfinished scan` | conservar: garantía observable específica de este escenario |
| `duplicate launches edits and syncs cannot replace a pending save setup` | conservar: garantía observable específica de este escenario |
| `cancelled setup releases the operation and old dialog cannot complete a new one` | conservar: garantía observable específica de este escenario |
| `retry is exclusive and a completed callback cannot be replayed` | conservar: garantía observable específica de este escenario |
| `a failed action releases the coordinator and cancellation clears transient state` | conservar: garantía observable específica de este escenario |
| `old status check cannot overwrite a newer status response` | conservar: garantía observable específica de este escenario |
| `status check started before a save operation cannot overwrite its result` | conservar: garantía observable específica de este escenario |
| `stale reorder events keep current entry details and newly added entries` | conservar: garantía observable específica de este escenario |
| `editing a ROM from an old item keeps other ROM overrides` | conservar: garantía observable específica de este escenario |
| `duplicate category names are rejected before changing configuration` | conservar: garantía observable específica de este escenario |
| `renaming during initial restore uses the saved cloud folder for the active session` | conservar: garantía observable específica de este escenario |
| `failed initial setup does not launch with an unpersisted mapping` | conservar: garantía observable específica de este escenario |

## ui/UiRecoveryTest.kt

Fuente: [UiRecoveryTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/UiRecoveryTest.kt). Punto de entrada: `recoverUiFailure / restoreUiOnCancellation`.
Nivel: unitaria. Dobles/límites: excepciones/suspensión controladas con runTest.
Riesgo: alta: consumir cancelación, perder la causa o dejar estado transitorio visible al llamador.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `an error boundary receives the original failure and returns its recovery result` | conservar: garantía observable específica de este escenario |
| `cancellation bypasses normal error reporting` | conservar: garantía observable específica de este escenario |
| `cancelled suspending actions restore transient state before the caller resumes` | conservar: garantía observable específica de este escenario |
| `successful actions retain their result and do not restore cancellation state` | conservar: garantía observable específica de este escenario |
| `ordinary failures are propagated without applying cancellation cleanup` | conservar: garantía observable específica de este escenario |

## ui/UpdateConcurrencyTest.kt

Fuente: [UpdateConcurrencyTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/UpdateConcurrencyTest.kt). Punto de entrada: `UpdateController.checkForUpdates / download / restart`.
Nivel: integración de coordinación. Dobles/límites: MockEngine, CompletableDeferred y MutableStateFlow.
Riesgo: crítica: descargar dos veces, aceptar bytes incompletos o reiniciar durante una partida.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `late update check cannot replace a completed download` | conservar: garantía observable específica de este escenario |
| `duplicate download is ignored and a cancelled download can be retried` | conservar: garantía observable específica de este escenario |
| `release size mismatch cannot become a ready update` | conservar: garantía observable específica de este escenario |
| `cancelled update check propagates cancellation and restores its previous state` | conservar: garantía observable específica de este escenario |
| `restart does not interrupt an active game session` | conservar: garantía observable específica de este escenario |

## platform/desktop/picker/LinuxFileChooserTest.kt

Fuente: [LinuxFileChooserTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/desktop/picker/LinuxFileChooserTest.kt). Punto de entrada: `LinuxFileChooser.pickFile / pickDirectory`.
Nivel: unitaria de protocolo de proceso. Dobles/límites: ejecutor falso que registra argv/resultados, sin abrir diálogos.
Riesgo: media: dividir rutas con espacios o ignorar cancelación y abrir otro selector.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `file selection falls back to zenity and passes paths with spaces as single arguments` | conservar: garantía observable específica de este escenario |
| `cancelling the first directory chooser stops without opening another tool` | conservar: garantía observable específica de este escenario |
| `directory fallback requests directory mode with the initial directory` | conservar: garantía observable específica de este escenario |
| `unavailable tools return control to the native fallback` | conservar: garantía observable específica de este escenario |

## ui/components/SidebarInteractionStateTest.kt

Fuente: [SidebarInteractionStateTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/components/SidebarInteractionStateTest.kt). Punto de entrada: `SidebarInteractionState.activateFolder / dragFolder / dragEntry / moveChildEntry`.
Nivel: unitaria. Dobles/límites: listas/filas reales y callbacks que registran selección/reordenación.
Riesgo: media: partir un grupo, reordenar una entrada eliminada o cambiar orden ante índices inválidos.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `activating a folder selects its first child then toggles collapse around that selection` | conservar: garantía observable específica de este escenario |
| `opening a collapsed unselected folder expands it and selects its first child` | conservar: garantía observable específica de este escenario |
| `expanding a folder removes collapsed names regardless of case` | conservar: garantía observable específica de este escenario |
| `folder dragging accumulates partial movement and moves the whole group` | conservar: garantía observable específica de este escenario |
| `child dragging stays within its group even beyond the last sibling` | conservar: garantía observable específica de este escenario |
| `standalone dragging crosses a whole group without splitting its children` | conservar: garantía observable específica de este escenario |
| `an entry removed during dragging cannot reorder the remaining library` | conservar: garantía observable específica de este escenario |
| `invalid sibling moves preserve every entry in its original order` | reescribir: verificar contenido/orden explícitos; permitir otra instancia de lista con el mismo resultado observable |

## ui/components/SidebarTest.kt

Fuente: [SidebarTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/components/SidebarTest.kt). Punto de entrada: `computeSidebarRows / partitionIntoUnits / flattenUnits / moveTopLevelUnit / moveChildEntry`.
Nivel: unitaria. Dobles/límites: entradas conocidas y listas de salida explícitas.
Riesgo: media: ocultar cabeceras, perder selección o mezclar hijos al reordenar grupos.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `computeSidebarRows includes FolderHeader and child items when expanded` | conservar: garantía observable específica de este escenario |
| `computeSidebarRows keeps FolderHeader and excludes children when collapsed` | conservar: garantía observable específica de este escenario |
| `FolderHeader is never removed when toggling collapsedGroups back and forth` | conservar: garantía observable específica de este escenario |
| `computeSidebarRows groups case-insensitively and respects case-insensitive collapse` | conservar: garantía observable específica de este escenario |
| `computeSidebarRows sets hasSelectedChild when child is selected` | conservar: garantía observable específica de este escenario |
| `computeSidebarRows handles mixed groups and standalone entries correctly` | conservar: garantía observable específica de este escenario |
| `partitionIntoUnits and flattenUnits preserves contiguous groups and entries` | conservar: garantía observable específica de este escenario |
| `moveTopLevelUnit allows dragging folder past standalone entries and other folders` | conservar: garantía observable específica de este escenario |
| `moveChildEntry allows dragging children within their folder` | conservar: garantía observable específica de este escenario |

## ui/dialogs/EntryFormStateTest.kt

Fuente: [EntryFormStateTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/dialogs/EntryFormStateTest.kt). Punto de entrada: `EntryFormState.buildEntry / applyTypeDefaults`.
Nivel: reglas de presentación. Dobles/límites: borradores reales y archivos temporales, sin Compose renderizado.
Riesgo: alta: perder identificadores/overrides al editar o aceptar rutas/argumentos inválidos.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `new emulator trims fields and preserves quoted arguments and extension order` | conservar: garantía observable específica de este escenario |
| `native games require a save target and trim optional process and cover fields` | conservar: garantía observable específica de este escenario |
| `invalid required fields and unmatched quotes reject the draft` | conservar: garantía observable específica de este escenario |
| `switching entry types adjusts only the default ROM arguments` | conservar: garantía observable específica de este escenario |
| `editing an emulator preserves launch identifiers and per ROM overrides` | conservar: garantía observable específica de este escenario |
| `new directory targets get cloud prefixes while existing legacy targets retain their layout` | conservar: garantía observable específica de este escenario |
| `unsafe cloud mappings are rejected before required field validation` | conservar: garantía observable específica de este escenario |

## ui/dialogs/GameEditFormStateTest.kt

Fuente: [GameEditFormStateTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/dialogs/GameEditFormStateTest.kt). Punto de entrada: `GameEditFormState.result / canSave / restorePaths`.
Nivel: reglas de presentación. Dobles/límites: GameItem y rutas temporales, sin red ni escritura de restauración.
Riesgo: crítica: importar fallback global al juego, restaurar fuera del destino o perder claves remotas.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `game drafts do not import emulator wide fallback saves` | conservar: garantía observable específica de este escenario |
| `blank title and cover overrides become null while existing game targets keep cloud names` | conservar: garantía observable específica de este escenario |
| `individual cloud restore keeps exact remote names and avoids whole directory syncing` | conservar: garantía observable específica de este escenario |
| `whole folder restore produces one directory target` | conservar: garantía observable específica de este escenario |
| `unsafe restore names cannot enable saving outside the destination` | conservar: garantía observable específica de este escenario |
| `invalid edited cloud names disable saving even without cloud restore` | conservar: garantía observable específica de este escenario |

## ui/dialogs/SaveTargetFormStateTest.kt

Fuente: [SaveTargetFormStateTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/dialogs/SaveTargetFormStateTest.kt). Punto de entrada: `SaveTargetFormState.buildTargets / actualización de filas / cloudPathsAreValid`.
Nivel: reglas con filesystem. Dobles/límites: filas conocidas y archivos/directorios temporales.
Riesgo: alta: desacoplar ruta/tipo/prefijo al borrar filas o aceptar traversal.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `late type discovery cannot change the type of a newer path` | añadir tras H8: un resultado de tipo pendiente no altera la ruta ni el tipo de la edición posterior |
| `saving resolves a coherent draft even when fields change during IO dispatch` | añadir tras H8: snapshot coherente de ruta/título y publicación del error de validación conservando los valores nuevos |
| `removing a row keeps the remaining path type and cloud name together` | conservar: garantía observable específica de este escenario |
| `physical paths determine their type while missing paths retain the selected type` | conservar y ampliar tras H8: la observación asíncrona respeta rutas ausentes y vacías; archivo/directorio existentes conservan su tipo físico |
| `blank rows are excluded and target values are trimmed` | conservar: garantía observable específica de este escenario |
| `cloud path validation accepts relative names and rejects traversal or absolute paths` | conservar: garantía observable específica de este escenario |

## platform/desktop/update/AppImageInstallerTest.kt

Fuente: [AppImageInstallerTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/update/AppImageInstallerTest.kt). Punto de entrada: `AppImageInstaller.replace`.
Nivel: integración local con fallos. Dobles/límites: archivos reales e inyección de copia/promoción/rollback/lanzamiento.
Riesgo: crítica: perder el ejecutable anterior o su backup tras fallo/cancelación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `cancelled installation restores the original before rethrowing cancellation` | conservar: garantía observable específica de este escenario |
| `keeps original and download until launch succeeds` | conservar: garantía observable específica de este escenario |
| `partial staging copy failure preserves original and download` | conservar: garantía observable específica de este escenario |
| `failed promotion restores original and retains download` | conservar: garantía observable específica de este escenario |
| `failed launcher restores original and retains download` | conservar: garantía observable específica de este escenario |
| `failed rollback keeps backup available for recovery` | conservar: garantía observable específica de este escenario |
| `rejects replacing a file with itself` | conservar: garantía observable específica de este escenario |

## data/updates/UpdateDownloadSafetyTest.kt

Fuente: [UpdateDownloadSafetyTest.kt](../../../../data/updates/src/test/kotlin/com/emusync/update/UpdateDownloadSafetyTest.kt). Punto de entrada: `GitHubUpdateSource.download`.
Nivel: protocolo con filesystem. Dobles/límites: MockEngine/ByteChannel y archivo previo conocido.
Riesgo: crítica: reemplazar una descarga válida con bytes truncados o dejar temporales al cancelar.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `download without content length preserves all bytes and reports completion` | conservar: garantía observable específica de este escenario |
| `progress callback failure retains the previous download and removes staging files` | conservar: garantía observable específica de este escenario |
| `cancellation during progress reporting escapes and preserves the previous download` | conservar: garantía observable específica de este escenario |
| `truncated update retains an existing download and removes staging files` | conservar: garantía observable específica de este escenario |
| `empty and rejected downloads never replace a previous file` | conservar: garantía observable específica de este escenario |
| `cancelled download propagates cancellation and preserves destination` | conservar: garantía observable específica de este escenario |

## data/updates/GitHubUpdateSourceTest.kt

Fuente: [GitHubUpdateSourceTest.kt](../../../../data/updates/src/test/kotlin/com/emusync/update/GitHubUpdateSourceTest.kt). Punto de entrada: `GitHubUpdateSource.check / download / reglas de versiones y artefactos`.
Nivel: reglas y protocolo con disco. Dobles/límites: JSON literal con MockEngine, plataforma inyectada y archivos temporales.
Riesgo: alta: seleccionar otra versión/artefacto o alterar la descarga.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `correctly compares semantic versions` | conservar: garantía observable específica de este escenario |
| `missing trailing version components are equivalent to zero` | conservar: garantía observable específica de este escenario |
| `checkForUpdates detects newer release and AppImage asset on Linux` | conservar: garantía observable específica de este escenario |
| `checkForUpdates detects newer release and MSI asset on Windows` | conservar: garantía observable específica de este escenario |
| `findBestAssetForPlatform prioritizes MSI then EXE then ZIP on Windows` | conservar: garantía observable específica de este escenario |
| `findBestAssetForPlatform prioritizes AppImage then tar gz then deb on Linux` | conservar: garantía observable específica de este escenario |
| `downloadUpdate streams bytes and reports progress` | conservar: garantía observable específica de este escenario |

## platform/desktop/update/DesktopUpdateInstallerTest.kt

Fuente: [DesktopUpdateInstallerTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/update/DesktopUpdateInstallerTest.kt). Punto de entrada: `DesktopUpdateInstaller.restart / instalación por plataforma`.
Nivel: integración local. Dobles/límites: disco real y lanzadores/plataforma controlados.
Riesgo: crítica: sustituir el ejecutable incorrecto o informar éxito tras una instalación fallida.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `applyWindowsUpdateAndRestart launches msiexec for MSI files` | conservar: garantía observable específica de este escenario |
| `applyWindowsUpdateAndRestart launches EXE installer directly` | conservar: garantía observable específica de este escenario |
| `applyWindowsUpdateAndRestart returns false for non-installer files` | conservar: garantía observable específica de este escenario |
| `applyLinuxUpdateAndRestart replaces AppImage file and calls launcher` | conservar: garantía observable específica de este escenario |
| `applyLinuxUpdateAndRestart returns false when APPIMAGE path is missing` | conservar: garantía observable específica de este escenario |
| `applyLinuxUpdateAndRestart returns false when downloaded file does not exist` | conservar: garantía observable específica de este escenario |
| `applyUpdateAndRestart delegates to Linux update on Linux platform` | conservar: garantía observable específica de este escenario |
| `applyLinuxUpdateAndRestart in Steam Game Mode calls launcher with updated path` | conservar: garantía observable específica de este escenario |
| `isSteamGameMode returns false by default in non-gaming test environment` | reescribir en H1: declarar precondición ambiental con assumeFalse; una sesión gamescope real produce omisión, no falso fallo. En H6 el instalador ya no posee HTTP ni requiere cierre |

## platform/desktop/update/UpdateRestartTest.kt

Fuente: [UpdateRestartTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/update/UpdateRestartTest.kt). Punto de entrada: `DesktopUpdateInstaller.restart`.
Nivel: integración local/de procesos. Dobles/límites: ejecutable Linux real en un caso; lanzadores y salida de aplicación falsos en los demás.
Riesgo: crítica: salir antes de instalar/lanzar o no restaurar el ejecutable si falla el reinicio.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `real Linux launcher starts the updated file before exiting` | conservar: garantía observable específica de este escenario |
| `launch failure restores current AppImage and keeps application open` | conservar: garantía observable específica de este escenario |
| `Steam Game Mode finishes installation before exit without starting a process` | conservar: garantía observable específica de este escenario |
| `failed Windows installer launch keeps application open` | conservar: garantía observable específica de este escenario |
| `successful Windows installer launch exits afterwards` | conservar: garantía observable específica de este escenario |

## platform/desktop/util/AppImageEnvironmentTest.kt

Fuente: [AppImageEnvironmentTest.kt](../../../../platform/desktop/src/test/kotlin/com/emusync/util/AppImageEnvironmentTest.kt). Punto de entrada: `AppImageEnvironment.clean`.
Nivel: unitaria. Dobles/límites: mapas de entorno literales.
Riesgo: alta: heredar bibliotecas del AppImage e impedir el arranque del juego.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `removes appimage paths and variables while preserving other library paths` | conservar: garantía observable específica de este escenario |
| `removes library path when it contains only appimage runtime paths` | conservar: garantía observable específica de este escenario |

## domain/execution/ArgumentsCodecTest.kt

Fuente: [ArgumentsCodecTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/execution/ArgumentsCodecTest.kt). Punto de entrada: `ArgumentsCodec.parse / format`.
Nivel: unitaria. Dobles/límites: argv/textos literales y una propiedad de ida/vuelta complementaria.
Riesgo: alta: perder argumentos vacíos, separadores Windows, escapes o aceptar comillas incompletas.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `format and parse preserve arguments containing spaces` | conservar: garantía observable específica de este escenario |
| `parse supports quoted arguments and whitespace separators` | conservar: garantía observable específica de este escenario |
| `parse keeps Windows path separators inside quotes` | conservar: garantía observable específica de este escenario |
| `parse rejects an unmatched quote` | conservar: garantía observable específica de este escenario |
| `parse decodes escaped quotes and backslashes but preserves other escapes` | conservar: garantía observable específica de este escenario |
| `parse rejects a trailing escape inside a quoted argument` | conservar: garantía observable específica de este escenario |
| `parse preserves empty arguments and joins adjacent quoted and unquoted fragments` | conservar: garantía observable específica de este escenario |
| `format quotes only arguments that require it` | conservar: garantía observable específica de este escenario |

## domain/sync/CloudPathsTest.kt

Fuente: [CloudPathsTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/sync/CloudPathsTest.kt). Punto de entrada: `cloudPathSegments`.
Nivel: unitaria, sin HTTP ni filesystem; destinos observados suministrados como datos.
Riesgo: alta: cambiar carpetas remotas legacy o confundir saves globales con overrides.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `getCloudPathSegments respects game title for EmulatorSystem with savePathsByRom` | conservar: garantía observable específica de este escenario |
| `global save paths use emulator cloud folder despite empty or copied rom overrides` | conservar: garantía observable específica de este escenario |
| `getCloudPathSegments handles custom title in titleByRom` | conservar: garantía observable específica de este escenario |
| `getCloudPathSegments for NativePCGame in a group uses group and game name` | conservar: garantía observable específica de este escenario |


## domain/sync/TrackedSyncPlannerTest.kt

Fuente: [TrackedSyncPlannerTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/sync/TrackedSyncPlannerTest.kt). Punto de entrada: `planTrackedSync`.
Nivel: unitaria, huellas y metadatos literales, sin DTO Drive ni cálculo de hashes.
Riesgo: crítica: sobrescribir cambios divergentes, confiar en relojes o propagar borrados.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `bootstrap requires evidence of equal content even with equal timestamps` | conservar: garantía observable específica de este escenario |
| `clock ordering does not conceal local changes` | conservar: garantía observable específica de este escenario |
| `remote content change is detected even when timestamps and versions match` | conservar: garantía observable específica de este escenario |
| `independent changes to the same content converge without a conflict` | conservar: garantía observable específica de este escenario |
| `metadata version is used when checksum is absent` | conservar: garantía observable específica de este escenario |
| `legacy remote metadata compares exact timestamps without tolerance` | conservar: garantía observable específica de este escenario |
| `remote replacement with a different id is a change` | conservar: garantía observable específica de este escenario |
| `one missing copy is restored without propagating deletions` | conservar: garantía observable específica de este escenario |


## domain/library/GameEntryGroupTest.kt

Fuente: [GameEntryGroupTest.kt](../../../../domain/src/test/kotlin/com/emusync/domain/library/GameEntryGroupTest.kt). Punto de entrada: `GameEntry.effectiveCloudFolder`.
Nivel: unitaria, entradas literales sin I/O.
Riesgo: alta: cambiar la prioridad de carpetas o la ubicación compartida.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `effectiveCloudFolder defaults to name when group and cloudFolder are null` | conservar: garantía observable específica de este escenario |
| `effectiveCloudFolder uses group when group is set and cloudFolder is null` | conservar: garantía observable específica de este escenario |
| `effectiveCloudFolder uses cloudFolder over group when both are set` | conservar: garantía observable específica de este escenario |
| `effectiveCloudFolder trims whitespace and falls back appropriately` | conservar: garantía observable específica de este escenario |
| `multiple emulators with same group share the exact same effectiveCloudFolder` | conservar: garantía observable específica de este escenario |


## data/local/config/ConfigFormatCompatibilityTest.kt

Fuente: [ConfigFormatCompatibilityTest.kt](../../../../data/local/src/test/kotlin/com/emusync/config/ConfigFormatCompatibilityTest.kt). Punto de entrada: `ConfigManager.load/save`.
Nivel: integración local; documento JSON literal, esperado neutral independiente y TempDir.
Riesgo: crítica: perder campos o cambiar claves, discriminadores y enums al separar DTO.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `legacy document preserves every entry field and serialized save type across domain conversion` | añadir en H3: compara ambos modelos completos y el JSON escrito contra el documento literal, incluidos overrides, identificadores y file/directory/unknown |

## data/local/config/RustPersistenceCompatibilityTest.kt

Fuente: [RustPersistenceCompatibilityTest.kt](../../../../data/local/src/test/kotlin/com/emusync/config/RustPersistenceCompatibilityTest.kt). Puntos de entrada: `ConfigManager.load/save`, `SyncHistoryStore.get/record`.
Nivel: integración local; con `EMUSYNC_R3_EXCHANGE` intercambia archivos sintéticos producidos por el ejemplo Rust, y sin esa variable usa un fixture literal aislado.
Riesgo: crítica: formatos incompatibles que pierden configuraciones o checkpoints durante la migración.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `Kotlin reads and writes Rust persistence when exchange directory is supplied` | añadir en R3: Kotlin lee una configuración e historial Rust, conserva argumentos vacíos y registra cambios que después lee Rust. |

## drive/SyncResponsesTest.kt

Fuente: [SyncResponsesTest.kt](../../../../presentation/src/test/kotlin/com/emusync/drive/SyncResponsesTest.kt). Punto de entrada: `SaveSynchronizer.syncEntry / resolveConflict`.
Nivel: integración del contrato neutral; SaveMappingDrive con MockEngine, canal de conflictos, cancelación y archivos reales en TempDir.
Riesgo: crítica: una respuesta de una operación cancelada sobrescribe la partida de otra operación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `cancelled conflict reply cannot resolve an identical later request or write saves` | añadir en H3: respuesta obsoleta UPLOAD_LOCAL sin efectos; solo la solicitud vigente autoriza DOWNLOAD_CLOUD y conserva el contenido remoto |

## data/local/saves/SyncHistoryStoreTest.kt

Fuente: [SyncHistoryStoreTest.kt](../../../../data/local/src/test/kotlin/com/emusync/local/saves/SyncHistoryStoreTest.kt). Punto de entrada: `SyncHistoryStore.key / record / get`.
Nivel: integración local, TempDir, fallo beforeReplace y metadatos neutrales.
Riesgo: crítica: perder checkpoints, resets silenciosos o índices parciales.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `history survives restart and merges records from independent instances` | conservar: garantía observable específica de este escenario |
| `failed history replacement retains the previous complete index` | conservar: garantía observable específica de este escenario |
| `corrupt or unsupported history fails safely and is never reset silently` | conservar: garantía observable específica de este escenario |


## data/drive/ConditionalCloudUpdateTest.kt

Fuente: [ConditionalCloudUpdateTest.kt](../../../../data/drive/src/test/kotlin/com/emusync/drive/ConditionalCloudUpdateTest.kt). Punto de entrada: `DriveConditionalUpdate.update`.
Nivel: protocolo MockEngine y transporte CIO real con servidor loopback en el escenario de dos escritores.
Riesgo: crítica: sobrescritura remota concurrente sin precondición fuerte.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `update streams all snapshot bytes and sends the exact strong precondition` | conservar: garantía observable específica de este escenario |
| `missing weak or wildcard version tokens never authorize an update` | conservar: garantía observable específica de este escenario |
| `changes before metadata check are rejected before sending save bytes` | conservar: garantía observable específica de este escenario |
| `unverifiable observed metadata does not fall back to timestamps` | conservar: garantía observable específica de este escenario |
| `cloud precondition rejection is not retried unconditionally` | conservar: garantía observable específica de este escenario |
| `two actual CIO writers with the same observed version have only one winner` | conservar: garantía observable específica de este escenario |

## data/local/saves/SyncHistoryCompatibilityTest.kt

Fuente: [SyncHistoryCompatibilityTest.kt](../../../../data/local/src/test/kotlin/com/emusync/local/saves/SyncHistoryCompatibilityTest.kt). Punto de entrada: `SyncHistoryStore.get / record`.
Nivel: integración local; documento legacy literal y JSON esperado independiente.
Riesgo: crítica: romper el historial existente al separar DTO y observaciones neutrales.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `version one history retains complete and legacy checkpoints when adding a neutral observation` | añadir en H4: conserva claves, versión, hashes, tiempos y metadatos ausentes; leer no reescribe y añadir una observación no altera registros previos |

## data/drive/OAuthCredentialStoreTest.kt

Fuente: [OAuthCredentialStoreTest.kt](../../../../data/drive/src/test/kotlin/com/emusync/drive/OAuthCredentialStoreTest.kt). Punto de entrada: `OAuthFlow.authorize con CredentialStore inyectado`.
Nivel: contrato del adaptador, MockEngine y almacén neutral en memoria, sin data:local.
Riesgo: crítica: aceptar un token de una conexión reemplazada aunque sus credenciales vuelvan a ser iguales.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `refresh rejects a changed store revision even when credential values are identical` | añadir en H4: sustituye la conexión durante HTTP y exige rechazo, una sola petición y credenciales conservadas |

## application/sync/SaveTargetClassificationTest.kt

Fuente: [SaveTargetClassificationTest.kt](../../../../application/src/test/kotlin/com/emusync/application/sync/SaveTargetClassificationTest.kt). Punto de entrada: `SaveTargetMapping.isDirectorySaveTarget`.
Nivel: regla de clasificación. Dobles/límites: MemorySaveAccess declara una ruta ausente sin I/O.
Riesgo: crítica: interpretar como archivo un directorio cuyo nombre remoto pertenece a otro destino.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `unknown missing target ignores exact cloud name owned by sibling target` | trasladar de PerGameSyncTest en H5: mismas cuatro expectativas de propiedad y tipo; sin necesidad de disco |

## application/sync/SaveTargetResolverTest.kt

Fuente: [SaveTargetResolverTest.kt](../../../../application/src/test/kotlin/com/emusync/application/sync/SaveTargetResolverTest.kt). Punto de entrada: `SaveTargetResolver.getCloudPathSegments / getSaveTargets`.
Nivel: regla de destinos. Dobles/límites: MemorySaveAccess con rutas ausentes y tipos explícitos.
Riesgo: crítica: perder el destino de una ROM al compartir ruta local con un destino global.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `ROM mapping with a different cloud prefix is distinct from the global fallback` | trasladar de SaveTargetMappingTest en H5: conserva ruta remota y ambos prefijos independientes |

Fixture H5: [MemorySaveAccess.kt](../../../../application/src/test/kotlin/com/emusync/application/sync/MemorySaveAccess.kt)
representa exclusivamente observaciones de rutas ausentes. Sus dos consumidores anteriores
no prueban disco, symlinks ni inferencia real; esas garantías siguen en las integraciones
de SaveRestorePathsTest, SaveTargetMappingTest y TrackedFileSyncTest con DesktopSaveAccess.

## desktopApp/composition/DesktopApplicationTest.kt

Fuente: [DesktopApplicationTest.kt](../../../../desktopApp/src/test/kotlin/com/emusync/composition/DesktopApplicationTest.kt).
Punto de entrada: `DesktopApplication.viewModel / close`.
Nivel: integración local. Dobles/límites: clientes HTTP MockEngine, directorio temporal;
composición y cliente reales. Riesgo: alto: dejar clientes activos tras cerrar la ventana
o cerrarlos antes de que termine su consumidor.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `desktop owner closes both HTTP clients after presentation has borrowed them` | añadir H7: ambos clientes responden durante el uso y sus jobs terminan al cerrar el propietario |

Fixtures H7 de integración: [DesktopTestComposition.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/DesktopTestComposition.kt)
y [TestSaveSynchronization.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/TestSaveSynchronization.kt).
Montan contratos con adaptadores reales solo para los tests; no hay dependencia de
presentation hacia desktopApp ni accesos especiales del compilador.

## presentation/library/GameItemTest.kt

Fuente: [GameItemTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/library/GameItemTest.kt).
Punto de entrada: identidad de GameItem y copy(entry).
Nivel: unidad del descriptor visual y resolución de destinos mediante el contrato explícito.
Riesgo: medio: perder identidad visual o mantener destinos antiguos tras copiar una entrada.
En H7 se excluyó la capacidad de la igualdad; en la corrección posterior de H8 esa capacidad
se elimina de GameItem y los destinos se consultan en el límite de la operación.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `equivalent displayed games retain their identity across copies` | conservar y adaptar tras H8: pertenencia al conjunto de selección, distinción por ROM/entrada y destinos nuevos al copiar sin capacidades en el descriptor |

## application/library/SteamRegistrationTest.kt

Fuente: [SteamRegistrationTest.kt](../../../../application/src/test/kotlin/com/emusync/application/library/SteamRegistrationTest.kt).
Punto de entrada: `SteamRegistration.register / unregister`.
Nivel: caso de uso con almacén neutral en memoria y Steam controlado.
Riesgo: alto: sobrescribir ediciones/credenciales recientes al persistir el ID o crear una entrada eliminada.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `registration uses the latest entry and retains edits made during the Steam operation` | añadir tras H8: entrada reciente enviada a Steam; solo se añade el ID sobre los datos y credenciales vigentes |
| `unregister clears only the requested Steam ID after the shortcut is removed` | añadir tras H8: se conserva el resto de la entrada y de la biblioteca |
| `failed Steam registration does not persist an ID or recreate a removed entry` | añadir tras H8: fallo sin escritura; entrada ausente rechazada antes de otra llamada Steam |

## presentation/LibraryThreadingTest.kt

Fuente: [LibraryThreadingTest.kt](../../../../presentation/src/test/kotlin/com/emusync/ui/LibraryThreadingTest.kt).
Punto de entrada: AppViewModel, carga/edición/reordenación/registro/desconexión y lecturas visuales Steam.
Nivel: coordinación con dispatcher real de un solo hilo, contratos controlados y cliente MockEngine cerrado.
Riesgo: alto: bloquear el hilo visual o repetir I/O al componer, y conservar indicadores obsoletos tras renombrar.

| Escenario / contrato observable (método exacto) | Decisión y motivo |
| --- | --- |
| `library IO runs on its worker while Compose reads cached Steam state` | añadir tras H8: almacén y Steam exigen el worker; las lecturas repetidas de estado no llaman adaptadores; registro, renombrado y baja actualizan el indicador |
