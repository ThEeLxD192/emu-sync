# Migración manual de EmuSync a Rust

Base congelada de referencia: `c9acc16a687b1093007dba4efc12c37237908678`, rama `rewrite/rust-native`. La versión Kotlin es la referencia ejecutable hasta verificar la equivalencia. Documentar no ejecuta un hito. No publicar ni hacer push sin autorización explícita; cada commit requiere aprobación individual previa.

## Seguimiento

Para cada hito registrar fecha, base, tareas, comandos exactos, resultados, omisiones, plataforma, evidencia y commit por separado. Marcarlo completo solo con toda su aceptación; mantener visible cualquier pendiente Windows. R0 está completado técnicamente; su commit está pendiente de confirmación. R1–R8 no se han iniciado.

### R0 — Guía, skills y seguimiento

- [x] Preparar rama y revisar `AGENTS.md`, este seguimiento y las skills versionadas.
- [x] Comprobar licencias, referencias, validación estructural, enlaces y `git diff --check`.
- [x] Registrar evidencia; no ejecutar pruebas de aplicación por cambios solo documentales.
- [x] **R0 completado técnicamente.**

Evidencia (2026-11-05, Linux, base `c9acc16`): rama local creada; cinco skills Rust añadidas, tres adaptadas de full-stack-skills con licencia Apache-2.0 y revisión fijada, dos específicas de EmuSync; skill Kotlin conservada por coexistencia. `quick_validate.py` valida las seis skills. Enlaces locales, espacios finales y `git diff --check` comprobados; el seguimiento histórico Kotlin se preservó íntegro. No se ejecutó la aplicación ni se cambió su código. R1–R8, CI remoto y Windows siguen sin validar. Commit pendiente de confirmación.

### R1 — Workspace y calidad

- [ ] Crear workspace Cargo y crates por capacidad; fijar toolchain estable y `Cargo.lock`.
- [ ] Añadir `cargo xtask quality` y `cargo xtask check`: rustfmt, Clippy bloqueante, pruebas y verificador de arquitectura basado en `cargo metadata`.
- [ ] Mantener Gradle y agregar a CI Linux/Windows y rama Rust, informes actuales y hook sobre exportación del índice con disparadores de ambos lenguajes.
- [ ] Ventana egui/winit/wgpu mínima: texto, portapapeles, DPI y cierre en Windows, X11 y Wayland.
- [ ] **R1 completado**, con resultados por plataforma y excepciones visibles.

### R2 — Dominio

- [ ] Migrar modelos, reglas y contratos por capacidad sin copiar mecánicamente clases Kotlin. Mantener dominio libre de UI, HTTP e I/O concreto.
- [ ] Comparar escenarios de planificación, conflictos, rutas y argumentos con fixtures y esperados independientes.
- [ ] **R2 completado.**

### R3 — Persistencia local

- [ ] Implementar configuración y DTO, historial, snapshots, backups y rutas seguras; introducir `--config <ruta>` para sustituir el override JVM.
- [ ] Verificar JSON literal y legacy, precedencia de ubicación, concurrencia, escritura atómica, permisos y lectura/escritura cruzadas Kotlin/Rust.
- [ ] Conservar historial v1 y la clave SHA-256 de ruta absoluta normalizada léxicamente, NUL, folderId, NUL y remoteName. No sustituir normalización léxica por canonicalización.
- [ ] Verificar staging, backups, symlinks, traversal, Windows 8.3 y fallos de recuperación.
- [ ] **R3 completado.**

### R4 — OAuth y Google Drive

- [ ] Portar loopback, state, scope `drive.file`, revisión/CAS y refresh; invalidar respuestas obsoletas y preservar cambios concurrentes de biblioteca.
- [ ] Portar búsqueda, paginación, carpetas, descargas, snapshots y subidas reanudables; probar transporte y cancelación reales controlados.
- [ ] Mantener actualización condicional con ETag fuerte v2 e `If-Match`, sin fallback incondicional; probar dos escritores y fallos de precondición.
- [ ] **R4 completado.**

### R5 — Casos de uso y sincronización

- [ ] Portar biblioteca y ciclo pre-sync → proceso → post-sync, juego offline, conflictos y reintentos identificados por solicitud.
- [ ] Verificar operaciones exclusivas, invalidación de resultados antiguos y preservación de partidas modificadas durante transferencias.
- [ ] Cancelación cooperativa: esperar limpieza y rollback remoto asíncrono antes de liberar exclusión o cerrar runtime; `Drop` y abortar futuros no reemplazan ese rollback.
- [ ] **R5 completado.**

### R6 — Plataforma y actualizaciones

- [ ] Portar procesos, scanner, portadas, selectores, navegador, Steam, VDF, wrappers y actualizador.
- [ ] Verificar argumentos y PowerShell en Windows nativo, procesos ajenos, entorno AppImage, Game Mode y campos VDF desconocidos.
- [ ] Probar descarga incompleta, staging, reemplazo, reinicio y rollback con fuentes controladas.
- [ ] **R6 completado.**

### R7 — Presentación y composición

- [ ] Recrear tema oscuro, barra lateral, cuadrícula, formularios, conexión, diálogos y flujos actuales con egui, winit y wgpu, sin JVM ni WebView.
- [ ] Propietario explícito de clientes, tareas y runtime; comunicar comandos y estados tipados sin bloquear el hilo visual.
- [ ] Validar edición, orden, conflictos, cancelación, progreso, errores, accesibilidad y cierre en plataformas aplicables.
- [ ] **R7 completado.**

### R8 — Empaquetado y cierre

- [ ] Generar AppImage independiente y MSI/EXE con WiX; comprobar arranque, instalación, actualización desde Kotlin y desinstalación nativos.
- [ ] Conservar nombre, rutas de datos, `io.github.ThEeLxD192.EmuSync`, UUID Windows `a7b3c291-5f4e-4b28-98e1-d63f5e824c90` y nombres de artefactos. Base de versión: 1.2.0.
- [ ] Actualizar simultáneamente Docker, recursos, scripts de versión, staging y workflows; no activar publicación desde la rama.
- [ ] Revisar matriz y evidencias Linux/Windows, retirar Kotlin y herramientas exclusivas solo tras verificar equivalencia, y actualizar documentación.
- [ ] **R8 completado.**

## Matriz de equivalencia

Completar progresivamente: escenario o garantía | prueba Kotlin y fixture | prueba Rust | Linux | Windows | resultado/limitación. Usar el catálogo histórico de la skill Kotlin como inventario; justificar combinaciones o sustituciones por contrato, sin imponer igual número de pruebas ni cobertura porcentual. Los resultados antiguos y simulaciones no son ejecuciones nativas nuevas.

Las áreas mínimas son JSON/legacy, historial/backups, rutas seguras, OAuth/CAS, transferencias y dos escritores, cancelación/rollback, conflictos, juego offline, procesos/Steam, UI, actualizador, empaquetado y cierre de recursos.

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
