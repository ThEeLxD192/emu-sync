# Auditoría de pruebas de EmuSync

Estado: **H1 completado técnicamente el 2026-10-30; commit pendiente de confirmación**.
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
commit H1 pendiente.

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
| Actualizaciones, temporales y orden de salida | UpdateDownloadSafetyTest: `truncated update retains an existing download and removes staging files`; AppImageInstallerTest: `failed rollback keeps backup available for recovery`; UpdateRestartTest: `launch failure restores current AppImage and keeps application open`, `successful Windows installer launch exits afterwards`; UpdateConcurrencyTest: `late update check cannot replace a completed download`, `duplicate download is ignored and a cancelled download can be retried`. |
| Formularios y reglas comunes | SaveTargetFormStateTest: `removing a row keeps the remaining path type and cloud name together`, `cloud path validation accepts relative names and rejects traversal or absolute paths`; EntryFormStateTest: `editing an emulator preserves launch identifiers and per ROM overrides`, `unsafe cloud mappings are rejected before required field validation`; GameEditFormStateTest: `game drafts do not import emulator wide fallback saves`, `unsafe restore names cannot enable saving outside the destination`. |
| Índice aislado y limpieza | Los ocho escenarios de hooks de la tabla anterior; no usan stash/reset ni alteran el índice del usuario. |

## Ejecuciones

Fecha local: **2026-10-30**, Linux, Java 25. Sin cuentas ni credenciales reales.
Commit base `21e8e56`; cambios H1 pendientes de autorización de commit.

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

Commit H1: pendiente de confirmación explícita del usuario.
