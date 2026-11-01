# Auditoría de pruebas de EmuSync

Estado: **H1 completado el 2026-10-30; commit `8c0f90c`**.
Auditoría sobre `21e8e56` (H0), sin cambios de producción, dependencias ni módulos.
La matriz por escenario está en [escenarios auditados](test-audit-cases.md).

## Línea base

Inspección del 2026-10-30 sobre `484012f`:

- 57 archivos Kotlin de pruebas y fixtures; no equivalen a 57 clases de prueba.
- Los 56 reportes XML existentes contienen 431 casos: 429 aprobados, cero fallos,
  cero errores y dos omisiones de integración PowerShell en Linux.
- El tiempo sumado de esos reportes es 11,801 segundos. Excluye inicio de Gradle,
  compilación, comprobaciones adicionales y empaquetado; no es un tiempo de CI.
- Los tiempos y cantidades posteriores se registran como mediciones nuevas sin
  reinterpretar esta fotografía como una ejecución realizada por la auditoría.

| Carpeta bajo `src/test/kotlin/com/emusync` | Archivos Kotlin | Revisión |
| --- | ---: | --- |
| `config` | 3 | 21 métodos vigentes; cuatro retiradas equivalentes trazadas |
| `drive` | 20 | Auditado: 169 métodos |
| `model` | 3 | Auditado: 24 métodos |
| `runner` | 2 | Auditado: 19 métodos |
| `scanner` | 1 | Auditado: 9 métodos |
| `steam` | 7 | Auditado: 29 métodos |
| `ui` (sin subcarpetas) | 9 | Auditado: 73 métodos |
| `ui/components` | 3 | Auditado: 21 métodos |
| `ui/dialogs` | 3 | Auditado: 17 métodos |
| `update` | 4 | Auditado: 34 métodos |
| `util` | 2 | Auditado: 10 métodos |

Las pruebas de hooks en `scripts/tests/git-hooks-test.sh` también deben revisarse por
sus garantías de índice aislado, instalación y limpieza. No forman parte de los 431
casos JUnit ni del tiempo anterior.

## Matriz de decisiones

Se revisaron condiciones, aserciones, límites controlados y preparación de los 56
archivos con tests, el fixture adicional y las pruebas de hooks. El
[catálogo por escenario](test-audit-cases.md) enumera los 426 métodos vigentes, con
contrato, riesgo, punto de entrada, nivel, dobles y decisión. Conserva las fechas,
regresiones y variantes por separado; el método parametrizado de reintento conserva
sus dos elecciones. No se clasificó una clase entera como redundante.

Balance sobre los 430 métodos originales: **421 conservar, cinco reescribir y cuatro
retirar con protección equivalente**. No se combinaron escenarios independientes.
Las cuatro retiradas reducen los casos de 431 a 427; no son una meta de reducción.

### Retiradas y protección equivalente

Todos los métodos retirados estaban en `config/ConfigManagerTest.kt`. Nivel:
reglas de localización con proveedores controlados y archivos temporales. Contrato:
prioridad de configuración. Riesgo medio: abrir otra configuración al arrancar.
Evidencia: E1 ejecutó los reemplazos y E2 toda la suite. Fecha: 2026-10-30;
commit H1 `8c0f90c`.

| Método retirado | Protección equivalente en `ConfigLocationTest` | Motivo y fallo que sigue detectándose |
| --- | --- | --- |
| `should resolve explicit custom path over environment or defaults` | `explicit overrides preserve priority without consulting lower priority providers` | Mismo customPath prioritario; además falla si se consulta un proveedor inferior y cubre propiedad/entorno. |
| `should resolve co-located config next to AppImage when it exists` | `AppImage configuration precedes working directory and missing portable config falls back` | Mantiene configuración junto al AppImage incluso con config en cwd; detecta invertir la prioridad. |
| `should resolve co-located config in working directory when it exists` | `AppImage configuration precedes working directory and missing portable config falls back` y `absent blank or parentless AppImage path preserves working directory fallback` | Cubre cwd cuando no hay config portable y APPIMAGE nulo, vacío o sin padre. |
| `should resolve Linux platform default config path` | `missing portable and working configurations use the platform directory` | Mantiene la ruta XDG exacta a través del resolver y su función de plataforma; detecta cambiar directorio/nombre y cubre también el fallback sin home. |

### Reescrituras aplicadas

| Escenario | Cambio y garantía conservada | Evidencia |
| --- | --- | --- |
| `ConfigManagerTest::should resolve Windows platform default config path` | Compara el `File` completo bajo APPDATA, en vez de admitir cualquier ruta con fragmentos AppData/emusync. Es una simulación de plataforma, no ejecución nativa Windows. | E1, E2 |
| `GameEntrySaveTargetsTest::ROM target lookup preserves typed exact stem and legacy fallback priority` | Los esperados legacy son destinos UNKNOWN explícitos en rutas ausentes. Ya no se calculan con `inferLegacySaveTarget`, que podría compartir el fallo. | E1, E2 |
| `DriveTransferTest::upload creates resumable session and sends file content` | Comprueba los bytes literales 1,2,3,4 del PUT, además de metadatos, rango y progreso. Antes una subida con bytes erróneos de igual longitud podía pasar. Usa TempDir y cierra el cliente incluso ante fallo. | E1, E2 |
| `SidebarInteractionStateTest::invalid sibling moves preserve every entry in its original order` | Verifica todos los elementos y su orden para grupo/índices inválidos y movimiento nulo; permite devolver una lista nueva equivalente. Sustituye el antiguo nombre terminado en `and list instance`. | E1, E2 |
| `UpdateManagerTest::isSteamGameMode returns false by default in non-gaming test environment` | Declara la precondición de sesión de escritorio con una assumption y cierra el manager. En gamescope/SteamGamepadUI=1 se omite explícitamente; en esta ejecución sí pasó. Conserva el contrato del escritorio sin depender de que todo desarrollador use esa sesión. | E1, E2 |

### Fixtures y límites compartidos

| Ubicación | Contrato, consumidores y riesgo | Nivel / decisión |
| --- | --- | --- |
| `drive/SaveMappingDrive.kt` | Servidor Drive en memoria con contenidos, versiones/ETag, sesiones, fallos y barreras. Lo usan SaveTargetMappingTest, TrackedFileSyncTest, ConditionalCloudWriteTest y LibraryConcurrencyTest; permite observar escrituras, conservación de copias y reintentos. | MockEngine con HttpClient/serialización reales; conservar. No demuestra el transporte CIO ni fidelidad completa de Google Drive. |
| `drive/CloudFolderMigrationTest.kt::MigrationDrive` | Árbol de nodos, movimientos, borrados y rollback; también lo usan CloudFolderEditTest y DriveSearchTest. Detecta mutaciones de datos ajenos y conserva la ubicación de recuperación. | MockEngine, AutoCloseable; conservar junto a su funcionalidad mientras no haya módulos extraídos. |
| `drive/DriveTransferCioTest.kt::LocalHttpServer` y fixture de dos escritores en ConditionalCloudWriteTest | Servidores loopback con puertos del sistema y espera acotada; representan cortes, stalls y concurrencia que MockEngine no prueba. | Transporte real, cierre en finally/use; conservar. Los sleeps introducen demora real del transporte, no ordenan lógica con tiempo virtual. |
| Helpers locales de procesos/argv en runner, steam y UpdateRestartTest | Crean procesos o grabadores de argumentos en temporales; comprueban bytes/argv, señales y exit codes del artefacto generado. | Integración de sistema; conservar. Leer scripts/batch generados verifica un artefacto externo, no el texto fuente Kotlin. |
| Resto de factories, JSON y helpers privados en sus archivos | Preparan entradas/fechas, respuestas, fallos o barreras para los escenarios enumerados; los esperados relevantes son literales, efectos persistidos o invariantes explícitas. | Conservar locales. No crear un módulo global de fixtures ni compartirlos con producción. |

### Hooks

Todos se ejecutan desde `scripts/tests/git-hooks-test.sh`, con repositorios desechables
(incluido un path con espacios), un gradlew falso que inspecciona la exportación y Git
real. Nivel: integración de scripts/Git; decisión: **conservar** en cada fila. No
pertenecen a los 427 casos JUnit. Evidencia E2: ocho mensajes PASS.

| Escenario | Contrato / fallo que detecta |
| --- | --- |
| Índice GOOD / trabajo BAD | Debe pasar por el índice, conservar árbol del índice y hash de trabajo y eliminar exportación; detecta validar el working tree. |
| Índice BAD / trabajo GOOD | Debe bloquear sin alterar índice/trabajo y limpiar; detecta aceptar contenido preparado inválido. |
| Solo documentación | No llama a Gradle; detecta ejecutar calidad por cambios sin código/configuración. |
| Configuración Detekt | Sí llama a qualityCheck; detecta omitir cambios de configuración de calidad. |
| Interrupción TERM | No acepta el hook y elimina exportación; detecta temporales abandonados. |
| Rename Kotlin | Comprueba el archivo renombrado sin perderlo y limpia la exportación. |
| Delete Kotlin | Dispara calidad también cuando se elimina un archivo Kotlin rastreado. |
| Instalación | Instala hooksPath, es idempotente y rechaza reemplazar hooksPath personalizado o saltarse un hook activo. |

## Garantías que deben quedar trazadas

Cada referencia identifica escenarios exactos del catálogo, no una afirmación de
cobertura total de la aplicación. Se conservan las capas de reglas y de integración
porque detectan fallos distintos.

| Garantía / fallo plausible | Pruebas concretas conservadas |
| --- | --- |
| Dirección, límites, fracciones y determinismo | SyncConflictTest: `timestamps exactly at boundary should resolve to IN_SYNC`, `fractional timestamps beyond threshold are not truncated`, `post-play policy uploads even when local timestamp is inside conflict window`; los tres escenarios de SyncPlannerTest. |
| Sobrescritura concurrente y duplicados | ConditionalCloudWriteTest: `missing weak or wildcard version tokens never authorize an update`, `cloud precondition rejection is not retried unconditionally`, `simultaneous initial creates retain both copies and divergent duplicates block selection`, `two actual CIO writers with the same observed version have only one winner`. |
| Integridad, cancelación y reanudación de transferencias | DriveTransferTest: `cancelled download removes temporary file and preserves existing save`, `upload recovers from lost chunk response using server acknowledged range`; DriveTransferCioTest: `CIO stalled download remains cancellable and cleans temporary file`, `CIO resumable upload recovers when server accepts a chunk but drops its response`. |
| JSON, escrituras atómicas, historial y backups | ConfigManagerTest: `legacy paths are migrated once to explicit save target types`, `interrupted save preserves last valid configuration`; ConfigUpdatesTest: `updates from separate manager instances preserve concurrent additions`; GameEntryGroupTest: `ConfigManager deserializes legacy JSON without group or cloudFolder`; TrackedFileSyncTest: `failed history replacement retains the previous complete index`, `download preserves a deduplicated recovery copy and advances history`, `local progress created during download is retained`. |
| Revisiones OAuth, autorización tardía y desconexión | OAuthFlowTest: `callback requires matching state before accepting code`; OAuthPersistenceTest: `late invalid grant cannot clear a replacement refresh token`, `unlink during code exchange rejects the returned token`, `valid refresh response is rejected when connection changed while waiting`; PostPlayAuthorizationTest: ambos escenarios. |
| Rutas seguras y aislamiento de destinos | SaveRestorePathsTest: `remote paths reject traversal absolute paths and empty segments`, `restore paths reject a nested symlink that points outside the selected folder`; SaveTargetMappingTest: `saved mapping survives changing local paths and reversing directory order`, `ambiguous legacy restore fails before writing files or launching`; DirectorySyncTest: `post-sync does not update file sibling with colliding directory save`. |
| Migración recuperable y merge bloqueado | CloudFolderMigrationTest: `failed move restores previous moves and reports failure`, `failed rollback retains files on both sides and identifies recovery location`, `cancellation restores acknowledged moves and propagates`; CloudFolderEditTest: `blocked entry merge keeps old configuration and both remote copies`. |
| Exclusión, edición segura, respuestas obsoletas y reintentos | LibraryConcurrencyTest: `latest selection wins when scans complete in reverse order`, `duplicate launches edits and syncs cannot replace a pending save setup`, `cancelled setup releases the operation and old dialog cannot complete a new one`, `old status check cannot overwrite a newer status response`; PostSyncFailureTest: `partial post game upload reports successes and retries only failed saves` y reintento parametrizado con ambas copias. |
| Argumentos, procesos, Steam y VDF | GameRunnerTest: `existing process with the same name does not delay newly launched game`; ExternalProcessTrackerTest: `caller cancellation interrupts waiting for a running game`; SteamSaveSafetyTest: `Linux wrapper preserves literal arguments quoted flags and embedded ROM paths`, `interrupted VDF replacement leaves the complete previous file`; LauncherIsolationTest: `ambiguous old alias refuses to launch but new wrappers keep separate ROM signals`; WindowsLauncherTest: escenarios PowerShell conservados, omitidos en Linux. |
| Actualizaciones, temporales y orden de salida | UpdateDownloadSafetyTest (data:updates): `truncated update retains an existing download and removes staging files`; AppImageInstallerTest (platform:desktop): `failed rollback keeps backup available for recovery`; UpdateRestartTest: `launch failure restores current AppImage and keeps application open`, `successful Windows installer launch exits afterwards`; UpdateConcurrencyTest: `late update check cannot replace a completed download`, `duplicate download is ignored and a cancelled download can be retried`. |
| Formularios y reglas comunes | SaveTargetFormStateTest: `removing a row keeps the remaining path type and cloud name together`, `cloud path validation accepts relative names and rejects traversal or absolute paths`; EntryFormStateTest: `editing an emulator preserves launch identifiers and per ROM overrides`, `unsafe cloud mappings are rejected before required field validation`; GameEditFormStateTest: `game drafts do not import emulator wide fallback saves`, `unsafe restore names cannot enable saving outside the destination`. |
| Índice aislado y limpieza | Los ocho escenarios de hooks de la tabla anterior; no usan stash/reset ni alteran el índice del usuario. |

## Ejecuciones

Fecha local: **2026-10-30**, Linux, Java 25. Sin cuentas ni credenciales reales.
Commit base `21e8e56`; entrega H1 registrada en `8c0f90c`.

**E1 — suites afectadas y formato:**

```bash
./gradlew spotlessApply test \
  --tests 'com.emusync.config.ConfigManagerTest' \
  --tests 'com.emusync.config.ConfigLocationTest' \
  --tests 'com.emusync.model.GameEntrySaveTargetsTest' \
  --tests 'com.emusync.drive.DriveTransferTest' \
  --tests 'com.emusync.ui.components.SidebarInteractionStateTest' \
  --tests 'com.emusync.update.UpdateManagerTest' --no-daemon
```

`BUILD SUCCESSFUL` en 12 s; 12 tareas accionables, cuatro ejecutadas y ocho
UP-TO-DATE. `test` ejecutado: seis XML, **59 casos aprobados**, cero fallos/errores y
cero omisiones; suma de tiempos JUnit **1,182 s**. Spotless aplicó el formato Kotlin.

**E2 — cierre de integración:**

```bash
./gradlew check --no-daemon
```

`BUILD SUCCESSFUL` en 23 s; 15 tareas accionables, seis ejecutadas y nueve
UP-TO-DATE. `test` ejecutado nuevamente para toda la suite (filtro distinto de E1):
56 XML, **427 casos: 425 aprobados, cero fallos/errores y dos omitidos**; suma de
tiempos JUnit **11,358 s**. Ambas omisiones están en WindowsLauncherTest:
`PowerShell launcher preserves empty quotes slashes metacharacters and exit code`
y `missing ROM and batch emulator fail without launching`; requieren PowerShell
no disponible en este entorno Linux. No son validación nativa de Windows.

Se ejecutaron verifyGitHooks (ocho PASS), spotlessKotlinCheck y detektTest.
spotlessKotlinGradleCheck y detektMain reutilizaron resultados UP-TO-DATE. Los
SARIF main/test contienen **cero hallazgos**; el principal es evidencia reutilizada,
el de pruebas se generó en E2. No se cambiaron reglas, supresiones ni ignoreFailures.
La caché de configuración guardó una entrada; esto no demuestra su reutilización.

**E3 — revisión documental:** contraste de los 426 métodos contra catálogo y XML,
57 archivos Kotlin inventariados, cuatro retiradas y cinco reescrituras registradas;
enlaces relativos, saltos de línea y `git diff --check` correctos. Los dos casos
EnumSource se verificaron por separado. No se volvieron a ejecutar suites tras
editar únicamente este seguimiento.

### Límites de la evidencia

- Una ejecución no mide inestabilidad estadística ni demuestra ausencia de todos
  los fallos; la auditoría es por contratos y aserciones, sin campaña de mutaciones
  ni objetivo porcentual de cobertura.
- La detección positiva del entorno Steam Game Mode no se demuestra al simular
  `isSteamGameMode`; la prueba de escritorio declara su precondición. Se conservan
  las integraciones de instalación/reinicio en ambos modos.
- Los round trips de JSON/VDF se complementan con JSON legacy literal y el fixture
  binario independiente de BinaryVdfFieldTest; no se presentan como prueba de
  compatibilidad de todas las versiones externas.
- Algunos tests antiguos crean clientes MockEngine sin cierre explícito; se conserva
  su garantía y se revisará la propiedad de esos recursos al extraer sus adaptadores
  en H4/H6. En los casos reales de transporte auditados sí hay cierre explícito.
- Sigue el aviso Gradle de funcionalidades incompatibles con Gradle 10. Su diagnóstico
  pertenece a H2; no se actualizaron plugins ni dependencias durante H1.
- No se ejecutaron empaquetado, UI renderizada, cuentas cloud ni Windows nativo;
  H6/H8 deben verificar sus plataformas. Este cierre es el de auditoría/consolidación.

## Cierre de H1

- [x] Todos los archivos del inventario y las pruebas de hooks tienen clasificación.
- [x] Toda consolidación o retirada tiene motivo y protección equivalente cuando aplica.
- [x] Las garantías críticas están trazadas a pruebas concretas.
- [x] Las suites afectadas pasan y las omisiones tienen causa y plataforma identificadas.
- [x] Se registran cantidad/tiempos posteriores sin tratarlos como objetivo de reducción.

Commit H1: `8c0f90c`, confirmado por el usuario.

## H2 — Garantías de infraestructura añadidas

La suite de aplicación conserva sus 427 casos. La nueva base Gradle añade pruebas
externas a JUnit de aplicación; no se cuentan como cobertura de lógica de negocio.

| Ubicación / escenario | Contrato y riesgo | Nivel / decisión / evidencia |
| --- | --- | --- |
| `scripts/tests/git-hooks-test.sh`: catálogo, propiedades/recursos de build-logic y scripts de módulo | Los cambios preparados activan calidad y se valida su contenido GOOD pese al BAD sin preparar; índice y trabajo conservan sus hashes. Detecta que nuevas convenciones salten el hook. | Git real con gradlew falso; conservar. Antes de ampliar el hook falló con `build-logic/gradle.properties did not run Gradle`; después pasó. Es el noveno grupo PASS, además de los ocho de H1. |
| `scripts/tests/gradle-conventions-test.sh`: agregados y source sets | `:test` ejecuta una prueba de módulo con fixture; `:qualityCheck` produce SARIF de main, test y fixtures. El resultado GlobalCoroutineUsage deliberado demuestra la regla de seguridad en fixtures y no aparece FunctionNaming; bytecode major 69 confirma Java 25. | Integración Gradle en copia desechable; conservar. No basta un dry-run de módulos vacíos. |
| Mismo script: segunda ejecución | Las mismas tareas reutilizan la caché de configuración. Detecta capturas incompatibles en convenciones o agregados. | Gradle real; conservar. Se comprueba `Configuration cache entry reused`. |
| Mismo script: aserción de módulo inválida | Una aserción fallida hace fallar `:test` raíz y el XML del módulo registra un fallo. Detecta pérdida de agregación o descubrimiento JUnit. | Gradle/JUnit reales; conservar. El fallo se inyecta solo en la copia. |
| Mismo script: fixture sin formato | Un archivo Kotlin sin formato en testFixtures hace fallar `:qualityCheck` raíz mediante `:domain:spotlessKotlinCheck`. Detecta dejar fixtures fuera del formato o del agregado. | Gradle/Spotless reales; conservar. La copia se elimina al salir y no modifica fuentes ni índice del usuario. |

Las cuatro comprobaciones de convenciones se ejecutan desde `check`, sin invocar
`check` de nuevo en la copia. La evidencia y limitaciones de H2 se registran en
[AGENTS.md](../../../../AGENTS.md); los resultados de H1 anteriores son históricos.

## H3 — Dominio y límites neutrales

Fecha: 2026-10-30, Linux; base `5e3d6f7` (H2). No se retira ningún escenario de H1.
El [catálogo](test-audit-cases.md) enlaza ahora cada escenario con su ubicación actual.
Las 47 reglas de seis suites pasan a `domain`: SyncConflictTest (13), SyncPlannerTest
(3), GameTitleUtilsTest (14), GameEntryGroupTest (5), CloudPathsTest (4) y
TrackedSyncPlannerTest (8). Las cuatro reglas de carpetas dejan de construir clientes
HTTP; las ocho reglas por contenido reciben huellas y metadatos neutrales literales.
Las comprobaciones de filesystem, protocolo, transporte y procesos permanecen en raíz.
Los dos casos JSON de agrupación se conservan en `config/ConfigGroupsTest.kt`.

| Límite cambiado | Garantía y evidencia |
| --- | --- |
| DTO de configuración ↔ biblioteca neutral | ConfigFormatCompatibilityTest añade un documento literal con todos los campos no predeterminados de ambas entradas. Compara modelos esperados independientes y JSON escrito, incluidos claves, enums, argumentos, mapas e identificadores. Las pruebas legacy y de escritura atómica siguen en ConfigManagerTest. |
| Persistencia por capacidad | ConfigUpdatesTest mantiene dos instancias y adiciones concurrentes, ahora mediante LibraryStore. Exige 20 entradas distintas y credenciales intactas. No se añade una prueba de delegación. |
| Eventos de conflicto y respuestas tardías | SyncResponsesTest consume SaveSynchronizer directamente: cancela una operación, recibe otra solicitud igual por valor y contesta a la antigua con UPLOAD_LOCAL. No cambia ninguna copia; solo la respuesta vigente DOWNLOAD_CLOUD restaura la nube. Detecta confundir igualdad por valor con la solicitud pendiente. |
| Traducción UI y reintentos | Las integraciones existentes pasan por createSaveSynchronizer/playWithSync de presentación; siguen comprobando textos/fechas mostradas, bloqueo de setup, configuración modificada, reintentos parciales y autorización posterior. Esa traducción ya no está en Drive. |
| Plataforma | GameRunnerTest conserva sus procesos/argv reales y usa GameLaunchRequest. No se sustituye el límite real por un mock al implementar GameLauncher. |

Se añaden solo los dos escenarios de conversión JSON y respuesta tardía: **428 métodos,
429 casos, 61 suites y un fixture**, distribuidos en 62 archivos Kotlin de pruebas.
Los resultados de H1/H2 registrados antes de esta sección son históricos.

La ejecución focalizada de DTO, respuestas, sincronización rastreada y runner ejecutó
29 casos de raíz (7,803 s JUnit) y los 47 de dominio (0,111 s), todos aprobados.
El cierre `./gradlew spotlessApply :domain:spotlessApply check --no-daemon` pasó en
57 s: raíz ejecutada con 382 casos, 380 aprobados y dos omitidos por PowerShell
(14,069 s); dominio UP-TO-DATE de la ejecución focalizada. Ambos SARIF main/test de
raíz y dominio contienen cero hallazgos. No hay supresiones nuevas ni reglas relajadas.
Se ejecutaron los nueve grupos de hooks y los cuatro de convenciones. Comandos,
empaquetado y limitaciones completos en [AGENTS.md](../../../../AGENTS.md).

No hay evidencia nativa Windows ni cuentas reales, y las integraciones visuales no
son pruebas de renderizado. La composición/adaptadores aún presentes en raíz se
extraerán en sus hitos; las pruebas de integración seguirán a su propietario.

## H4 — Persistencia local, OAuth y transferencias

Fecha: 2026-10-30–31, Linux; base `0def750`. Se conservan todos los escenarios de
H1/H3. Las fuentes actuales están enlazadas en el [catálogo](test-audit-cases.md).
Configuración, tipos de destinos e historial pasan a local; protocolo y transporte
pasan a Drive. La recuperación local recibe CloudSaveTransfer y expone LocalSaveFiles;
las integraciones de ambos adaptadores conservan componentes reales en la raíz.
OperationRecoveryTest pertenece a dominio: conserva la propagación de cancelación.

| Cambio de límite | Contrato y decisión |
| --- | --- |
| CredentialStore/BrowserLauncher por constructor | OAuth ya no conoce ConfigManager ni abre AWT directamente. OAuthPersistenceTest y DriveConnectionTest conservan respuestas tardías, revisiones, desconexión, cancelación y edición simultánea con persistencia real. OAuthCredentialStoreTest añade rechazo de una revisión cambiada con valores idénticos, usando almacenamiento neutral dentro del módulo Drive. |
| DTO de credenciales y nombres neutrales | ConfigFormatCompatibilityTest conserva documento literal completo y esperado independiente. Exige googleDrive/clientId/clientSecret/refreshToken en JSON mientras los consumidores usan cloudCredentials/OAuthCredentials. |
| Checkpoints de historial neutrales | SyncHistoryCompatibilityTest añade JSON versión uno literal, registros completos y sin campos opcionales. Leer no reescribe; añadir conserva claves, hashes, IDs, versiones y tiempos previos. Las tres pruebas originales de historial pasan a SyncHistoryStoreTest sin perder fallos de reemplazo ni corrupción. |
| Recuperación/transferencia entre módulos | TrackedFileSyncTest y ConditionalCloudWriteTest de raíz siguen usando DriveTransfer y SyncHistoryStore reales, con SaveMappingDrive. Comprueban snapshots estables, conservación de progreso/copia remota/historial, backups, reintentos, conflictos y cancelación. Solo se traducen metadatos y rutas al cruzar el límite neutral. |
| Condicionales y transporte | Seis escenarios de ConditionalCloudUpdateTest pasan a Drive con el acceso interno normal de su propio módulo. DriveTransferCioTest conserva siete escenarios de sockets reales, y dos escritores CIO aún exigen un único ganador con el mismo ETag. |
| Fixtures y recursos | MigrationDrive/MigrationNode/MigrationMove viven en testFixtures de Drive y los usan DriveSearchTest y las integraciones de migración/UI de raíz. El esquema ConditionalFileResponse de raíz y sus fingerprints son independientes de DTO/helpers privados de producción. MemoryCredentialStore representa revisiones neutrales. Los clientes propiedad de suites trasladadas se cierran en AfterEach; los de transporte/fixtures conservan use/finally. |

Se añaden solo dos casos: **430 métodos, 431 casos JUnit, 65 suites y ocho archivos
fixture**, en 73 archivos Kotlin. Sin retiradas, supresiones nuevas ni accesos especiales
del compilador para cruzar módulos.

La ejecución final combinó check, grafos runtime de local/Drive y packageReleaseAppImage:
correcta en 1 min 4 s, 109 tareas (23 ejecutadas y 86 UP-TO-DATE). Raíz: 310 casos,
308 aprobados y dos omitidos por PowerShell, 13,210 s JUnit. Drive: 40 aprobados,
4,167 s. Local (31, 0,509 s) y dominio (50, 0,289 s) UP-TO-DATE de ejecuciones
afectadas anteriores; total vigente 429 aprobados y dos omitidos, sin fallos/errores.
La comprobación afectada posterior de calidad Drive y paquete pasó en 16 s,
tras retirar únicamente un cuerpo vacío al separar el enum de progreso. Se ejecutó
Detekt main de Drive; los otros análisis y el paquete estaban UP-TO-DATE. El ajuste
no cambió bytecode ni se repitieron suites. Nueve reportes Detekt limpios, incluido
testFixtures, y sin avisos de errores de resolución de tipos. Hooks: nueve PASS; convenciones: cuatro PASS.

Los comandos y límites completos están en [AGENTS.md](../../../../AGENTS.md). El staging
Linux no equivale a un AppImage independiente ni a Windows nativo; no hay cuentas reales
ni CI remoto. Los resultados de H1–H3 anteriores siguen siendo evidencia histórica.

## Extracción H5 — casos de uso y límites de coordinación

Fecha: 2026-10-31; base H4 `27d8eb1`. H5 mueve los flujos a application, con dominio
como única dependencia de proyecto. Los servicios reciben capacidades prestadas;
la composición y los propietarios HTTP siguen en la raíz hasta H7. No hay nuevas
pruebas por el movimiento ni escenarios retirados.

| Cambio de límite | Contrato, riesgo y protección conservada |
| --- | --- |
| CloudFile, listado y rutas locales neutrales | SaveTargetMappingTest, DirectorySyncTest, PerGameSyncTest y TrackedFileSyncTest conservan adaptadores reales, rutas anidadas, ownership, bytes, historial y recuperación. DriveFileListing traduce los metadatos y DesktopSaveAccess conserva observaciones, recorrido y resolución segura; application no ejecuta I/O concreto. |
| Validación interna de destinos | Los dos casos de alias/archivo padre en SaveRestorePathsTest entran ahora por SaveSynchronizer.syncEntry, comprueban rechazo y ausencia de transferencias/escrituras. No requieren publicar el validador ni accesos especiales entre módulos. |
| Reglas junto a application | Dos escenarios existentes pasan a SaveTargetClassificationTest y SaveTargetResolverTest con MemorySaveAccess: ruta ausente, reserva de nombres por hermanos y distinción de prefijos ROM/global. Se conservan sus expectativas y los escenarios de disco permanecen en raíz. |
| Exclusión, revisión y cancelación | LibraryConcurrencyTest y UiCancellationTest siguen entrando por AppViewModel y verifican rechazo de operaciones simultáneas, propagación de cancelación, liberación del bloqueo y descarte de consultas viejas. OperationCoordinator y EntrySyncChecks poseen ahora esas garantías neutrales. |
| Conflictos y reintentos | SyncResponsesTest conserva la solicitud exacta con eventos neutrales; PostSyncFailureTest conserva fallos parciales y reintento solo de destinos pendientes. Las integraciones de UI conservan el rechazo de callbacks antiguos y la respuesta de setup vinculada a su diálogo. |
| Edición, migración y conexión | CloudFolderEditTest, CloudFolderMigrationTest, AppViewModelGameOverrideTest y DriveConnectionTest conservan migración antes de persistir, rollback, carpetas compartidas, overrides, revisión OAuth y resultados tardíos. Las excepciones de migración son neutrales SaveSyncException; las pruebas conservan razón y efectos observables. |
| Actualizaciones | UpdateConcurrencyTest conserva descarga única, consulta tardía, cancelación/reintento, limpieza, tamaño esperado y bloqueo de reinicio durante juego. La restauración compara el valor visual anterior al traducir desde UpdateState; la comprobación de consulta tardía mantiene la identidad visual para detectar reemplazos. Transporte/instalación siguen cubiertos con implementaciones reales en la raíz hasta H6. |

Balance H5: **430 métodos, 431 casos JUnit, 67 suites y nueve fixtures en 76 archivos
Kotlin**. No se añade ni retira ningún escenario. El catálogo mueve únicamente las
dos filas de reglas a sus fuentes nuevas; las integraciones permanecen localizables.

La comprobación afectada de Drive/UI y application pasó en 28 s (88 tareas: 36
ejecutadas, 52 UP-TO-DATE). El cierre `./gradlew spotlessApply check
:application:dependencies --configuration runtimeClasspath packageReleaseAppImage
--no-daemon` pasó en 58 s (113 tareas: 12 ejecutadas, 101 UP-TO-DATE). Ejecutó raíz:
308 casos, 306 aprobados y dos omitidos PowerShell, 11,548 s JUnit. Application (dos,
0,077 s), dominio (50, 0,376 s), local (31, 0,546 s) y Drive (40, 3,841 s)
reutilizaron resultados correctos de las comprobaciones afectadas de esta integración.
Evidencia vigente combinada: 429 aprobados, cero fallos/errores y dos omitidos;
no son 431 ejecuciones nuevas en el cierre.

Once SARIF limpios, incluido testFixtures de Drive. Main/test de raíz ejecutados;
los módulos reutilizaron sus análisis recientes limpios. Hooks: nueve PASS;
convenciones: cuatro PASS. Sin supresiones nuevas ni reglas relajadas. Los errores
de compilación por imports y los hallazgos estructurales intermedios quedaron
corregidos antes de ese cierre. El grafo application carece de Compose, Ktor,
serialización, adaptadores y dependencia de raíz. El staging incluye su JAR y
classpath, sin fixtures ni clases de sync duplicadas en raíz.

Comandos, propietarios y limitaciones completos en [AGENTS.md](../../../../AGENTS.md).
No se ejecutaron Windows nativo, UI gráfica, cuentas reales, Docker o CI remoto,
ni se generó AppImage independiente. H6–H8 siguen pendientes.


## Extracción H6 — plataforma y origen de actualizaciones

Fecha: 2026-10-31; base H5 `7f5c549`. Implementación verificada en Linux; H6 sigue
abierto por falta de validación nativa Windows. Commit autorizado; su hash se
registrará en la próxima actualización del seguimiento.

Se conservan los 431 casos anteriores: procesos, Steam/VDF/lanzadores, scanner,
selectores e instalación pasan a platform:desktop; protocolo/descarga a data:updates;
el codec de argumentos puro a domain. La suite mixta UpdateManagerTest se divide
según esos propietarios. Las integraciones de presentación permanecen en la raíz
y consumen los contratos con adaptadores HTTP/disco reales. Cada enlace del catálogo
identifica su fuente actual, sin perder el escenario parametrizado de reintento.

Se añade solo `GameRunnerTest::Steam launch uses the injected preparation and stops
on its failure`. Contrato: usar la preparación Steam inyectada y detenerse ante fallo
antes del handoff; riesgo alto de preparar otras señales o lanzar sin preparación.
Entra por GameRunner.launch, con un colaborador que registra entrada/ruta y falla,
y una ROM real aislada que debe conservarse. Comprueba valores del fallo propagado;
la reconstrucción de trazas de corrutinas puede crear otra instancia de excepción.

Balance H6: **431 métodos, 432 casos, 68 suites y nueve fixtures en 77 archivos Kotlin**.
No se retiran escenarios. GitHubUpdateSource recibe un cliente prestado; las pruebas
que lo crean lo cierran también ante aserciones fallidas. Los helpers siguen internos,
sin publicar DTO ni añadir accesos especiales para pruebas.

La comprobación focalizada final de plataforma pasó en 18 s, 95 casos: 93 aprobados
y dos omitidos PowerShell. El cierre con spotlessApply/check/dependencias/paquete
pasó en 1 min (124 tareas: 26 ejecutadas y 98 UP-TO-DATE). Ejecutó raíz (193,
2,272 s), plataforma (95, 8,434 s) y updates (13, 0,397 s). Dominio (58, 0,221 s),
application (dos, 0,076 s), local (31, 0,310 s) y Drive (40, 2,599 s) reutilizaron
resultados correctos de esta integración. Evidencia combinada: **430 aprobados,
cero fallos/errores y dos omitidos**; no son 432 ejecuciones nuevas en el cierre.

Quince SARIF limpios. El cierre ejecutó main/test de raíz y plataforma y test de
updates; el resto reutilizó análisis limpios. Hooks: nueve PASS; convenciones:
cuatro PASS, incluidos fixtures, caché y bloqueos deliberados. Sin nuevas supresiones
ni reglas relajadas. Los grafos de los nuevos adaptadores solo dependen de domain
y bibliotecas; plataforma no incorpora Ktor ni Compose y updates no conoce UI,
Steam o instalación. Staging con ambos JAR y classpath correctos, sin UpdateManager
ni fixtures. Catálogo, enlaces y escenarios contrastados con fuentes/reportes.

No se ejecutaron Windows nativo, MSI/EXE reales, CI remoto, UI gráfica, Docker,
cuentas reales ni appimagetool. Se mantienen las dos omisiones por PowerShell.
Las simulaciones de Windows no cierran su validación ni el hito. Comandos, tiempos
y límites completos en [AGENTS.md](../../../../AGENTS.md). No se repite la suite
por estos cambios documentales.


## Extracción H7 — presentación y aplicación de escritorio

Fecha: 2026-10-31; base H6 `d54561a`. Implementación verificada en Linux; validación
nativa Windows pendiente. Se actualiza también el hash autorizado de H6, sin cambiar
su pendiente de Windows. H8 no se ejecuta.

Se conservan los 432 escenarios existentes y sus aserciones. Las fuentes de UI y sus
integraciones pasan a presentation; únicamente los source sets de prueba dependen de
adaptadores de datos/plataforma. Los fixtures montan colaboradores reales mediante los
contratos, sin depender de desktopApp, duplicar lógica de reglas o publicar helpers
internos. Las consultas legacy y la restauración siguen entrando por LocalSaveTargets;
no se reemplazan los controles de disco/symlinks por validaciones meramente léxicas.

Se añade DesktopApplicationTest: riesgo alto de fuga o cierre prematuro de los clientes
HTTP al separar la UI de su propietario. Entra por el viewModel compuesto y por close;
ambos clientes reales con MockEngine responden antes y sus jobs terminan después.
AppViewModelTest cierra también sus clientes en AfterEach. Se añade GameItemTest:
la capacidad inyectada no participa en la identidad visual, y copy(entry) resuelve los
nuevos destinos. La prueba de pertenencia a un conjunto falló antes de conservar
la igualdad/hash de nombre, entrada y ROM y pasó después. No se añaden pruebas por
movimientos o tipos separados ni se retiran escenarios. Balance: **433 métodos,
434 casos, 70 suites, 11 fixtures y 81 archivos Kotlin**.

La comprobación focalizada previa a la corrección de identidad de presentation/desktopApp
pasó en 26 s: 194 casos aprobados. Tras reproducir y corregir ese fallo, el cierre
check/paquete pasó en 57 s (113 tareas: 14 ejecutadas y 99 UP-TO-DATE): ejecutó los 194
casos actuales de presentación y el de desktopApp, reutilizando las otras suites
correctas de esta integración. Evidencia combinada: **432 aprobados, cero fallos/errores
y dos omitidos PowerShell**. Los informes
históricos de la raíz sin fuentes ya no se cuentan como una suite actual. Diecisiete
SARIF de módulos sin hallazgos; sin reglas relajadas ni supresiones nuevas. Hooks:
nueve grupos PASS, ampliados a presentation/desktopApp; convenciones: cuatro PASS.
La tarea raíz qualityCheck también reutilizó la caché de configuración en cuatro segundos.

Las pruebas de versión en copias aisladas verifican ambos consumidores y outputs para
patch/minor/major. Staging local final con classpath de los ocho módulos y recursos de
un único propietario, sin clases duplicadas ni fixtures. Docker generó además un
AppImage independiente con las fuentes finales; no se probó su UI gráfica. El dry-run
MSI/EXE solo verifica las delegaciones y no cierra Windows. Comandos, tiempos, diferencias
entre ejecuciones y limitaciones completas en [AGENTS.md](../../../../AGENTS.md).

El precommit sobre la exportación final del índice pasó en un minuto: 95 tareas
ejecutadas, sin modificar el trabajo ni el índice para pasar sus comprobaciones.

## Validación H8 — revisión final de contratos y evidencia

Fecha: 2026-10-31; base H7 `46e1c1b` (commit ya registrado). H8 verifica Linux y
conserva abiertos H6/H7/H8 por la validación nativa Windows pendiente.

Se contrastaron las 70 fuentes enlazadas y los 433 métodos del catálogo con el
código y los XML actuales. Las dos elecciones EnumSource del reintento explican
los 434 casos. Se conservan las garantías de las tablas anteriores: no se añade,
retira ni reescribe ningún escenario en H8. Los 81 archivos de pruebas incluyen
11 fixtures; producción tiene 210 archivos Kotlin en los ocho módulos.

`./gradlew check packageReleaseAppImage --rerun-tasks --no-daemon` pasó en 1 min 6 s:
104 tareas ejecutadas. Son **434 casos ejecutados de nuevo: 432 aprobados, cero fallos/errores y
dos omitidos PowerShell**, 19,996 s JUnit. Se reutilizó la caché de configuración,
sin reutilizar las tareas de pruebas/análisis. Se excluyen los reportes históricos
de la raíz sin fuentes. Los 17 SARIF nuevos, incluido testFixtures Drive, no tienen
hallazgos. Hooks: nueve grupos PASS; convenciones: cuatro PASS. No se relajan reglas
ni se añaden supresiones o accesos especiales para las pruebas.

La compatibilidad de JSON/configuración e historial usa documentos literales en
ConfigFormatCompatibilityTest y SyncHistoryCompatibilityTest; los escenarios de
persistencia, backups, OAuth, conflictos, cancelación, rutas y escrituras condicionales
siguen ejecutándose en sus módulos propietarios. Las integraciones CIO reales y las
pruebas Linux de procesos/instalación conservan su límite de sistema. No se usaron
datos del usuario ni cuentas reales de Drive/Steam.

Los grafos resueltos de producción y pruebas coinciden con la arquitectura, sin
ciclos ni dependencias hacia la raíz. Presentation solo incorpora adaptadores y
fixtures en pruebas y no depende de desktopApp. No quedan dependencias o fachadas
temporales de la migración que retirar; las APIs estables y compatibilidad legacy
conservan consumidores reales. No se eliminan regresiones por compartir límites.

Docker generó el AppImage independiente y validó AppStream. Su AppRun extraído abrió
una ventana EmuSync de 1280×800 bajo Xvfb con el runtime incluido y configuración
temporal, dentro de un contenedor desechable. Esto comprueba el arranque básico;
no demuestra interacción manual, montaje FUSE ni funcionamiento en Windows.
Los dos escenarios PowerShell siguen omitidos en este host Linux sin PowerShell;
las pruebas Windows controladas no son instalación nativa. MSI/EXE y sus pruebas
nativas siguen pendientes, sin CI remoto, push ni publicación.

Comandos, tiempos por módulo, grafo, artefacto/SHA-256 y pasos pendientes están en
[la evidencia de H8](../../../../AGENTS.md#h8--validación-final-y-cierre). No se repite
la suite por la actualización documental.

El usuario autorizó después una limpieza previa al commit. Se retira únicamente el
callback legacy de Sidebar por índices y su envoltorio: el único consumidor siempre
proporciona la lista completa y esa rama tenía prioridad. Se mantienen los escenarios
de SidebarInteractionStateTest, SidebarTest y LibraryConcurrencyTest, así como las APIs
de edición todavía probadas. No se añaden ni eliminan escenarios por esta simplificación.
El cierre posterior `check packageReleaseAppImage --no-daemon` pasó en 49 s: 113 tareas,
18 ejecutadas y 95 UP-TO-DATE. Ejecutó presentación (194 casos, 4,121 s) y desktopApp
(uno, 0,411 s), todos aprobados; las otras suites reutilizaron la ejecución completa
anterior de H8. Detekt de presentación, hooks y convenciones pasaron; los 17 SARIF
vigentes están limpios. Los reportes históricos se conservan al retirar builds obsoletos.
El AppImage se regeneró con las fuentes finales y volvió a abrir la ventana bajo Xvfb
con el runtime incluido y configuración temporal, dentro de un contenedor desechable.
El commit de validación y limpieza está autorizado; Windows nativo sigue pendiente.


## Revisión posterior a H8 — efectos visuales y dependencias verificadas

Fecha: 2026-10-31; base `f7c7248`, commit previo de H8 ya registrado. Windows nativo
continúa pendiente. Se conservan los 434 casos por escenario anteriores y se
registran siete regresiones nuevas, sin retiradas: **440 métodos, 441 casos, 72 suites
y 83 archivos Kotlin de pruebas/fixtures (11 fixtures)**.

| Garantía | Evidencia que observa el fallo |
| --- | --- |
| Buscar y decodificar portadas fuera de Compose | GameCoverLoaderTest envía el descriptor al localizador en un worker real distinto del llamador, decodifica un PNG y comprueba caché/override. Los siete escenarios de CoverLookupTest pasan a plataforma con los mismos esperados independientes. |
| Persistencia/Steam fuera del hilo visual y lecturas sin I/O | LibraryThreadingTest exige el worker en los contratos de almacén/Steam, prueba carga, reordenación, alta, renombrado, baja y desconexión; las lecturas repetidas de los indicadores no llaman adaptadores. |
| Registro neutral con configuración vigente | SteamRegistrationTest conserva edición y credenciales que cambian durante el registro; la baja afecta solo al ID solicitado y los fallos/entradas ausentes no escriben configuración. |
| Tipos tardíos y borrador coherente | SaveTargetFormStateTest cambia la ruta antes de completar la observación y mantiene el tipo nuevo; readFormDraft retiene ruta/título del instante de guardar y aplica el error de validación sin sobrescribir campos editados después. |
| Dependencias de producción permitidas | La regresión Gradle desechable admite una dependencia de prueba; rechaza domain → application, Ktor en domain y fuentes raíz. También demuestra reutilización de caché. No cuenta como prueba JUnit de negocio. |

GameItem ya no contiene capacidades ni búsqueda de archivos. Su regresión de identidad
se adapta a descriptores/copias y resolución mediante el contrato explícito, sin perder
la distinción por entrada/ROM ni mantener destinos antiguos. Los formularios usan ese
contrato en sus límites de I/O. Las integraciones conservan disco/HTTP/procesos reales.

Las pruebas de coordinación reciben el dispatcher de la corrutina llamadora: el intento
inicial con Unconfined perdió el scheduler virtual del timeout OAuth y falló. Se corrigió
la inyección, manteniendo el timeout y la regresión. Las pruebas nuevas de aislamiento
usan workers reales que se cierran explícitamente; el resto conserva su scheduler.

El cierre `spotlessApply check packageReleaseAppImage --no-daemon` pasó en 1 min 29 s,
132 tareas (41 ejecutadas, 91 UP-TO-DATE). Ejecutó presentación (191 casos), plataforma
(102) y desktopApp (uno); reutilizó dominio (58), application (cinco), local (31), Drive
(40) y updates (13), correctos de esta integración. **439 aprobados, cero fallos/errores
y dos omitidos PowerShell**. Los 17 SARIF están limpios, sin nuevas supresiones ni reglas
relajadas. Hooks: nueve grupos PASS; convenciones: cinco grupos PASS, incluido el guard
arquitectónico. Tiempos por módulo, comandos y límites en [AGENTS.md](../../../../AGENTS.md).

El smoke visual encontró una ruta vacía clasificada como directorio por este JDK.
Se amplió el escenario físico existente: falló con FILE esperado y DIRECTORY recibido;
LocalSaveTargets.typeOf ahora devuelve null para rutas vacías/blancas. El cierre
posterior con el mismo comando pasó en 1 min 15 s (132 tareas: 24 ejecutadas y 108
UP-TO-DATE). Ejecutó local (31 casos), presentación (191) y desktopApp (uno); el resto
reutilizó resultados correctos. Continúan **439 aprobados, dos omitidos PowerShell y
cero fallos/errores**, con los 17 SARIF limpios. No se duplican escenarios para contar
este límite adicional. El AppImage también mostró la portada y abrió ambos formularios;
la validación vacía y el guardado de título conservaron la partida y la portada temporales.

El artefacto regenerado tras corregir la ruta vacía se probó de nuevo bajo Xvfb:
FILE inicial conservado, validación visible, preview y cuadrícula con la portada,
guardado de título con partida/destinos intactos y cierre Safe Close con código cero.
El builder y el runtime de prueba usaron red host porque bridge/veth no está disponible
en este entorno. Artefacto/SHA-256 y comandos en AGENTS.md; no hubo cuentas reales,
publicación ni validación nativa de Windows. El commit de la corrección está autorizado.
