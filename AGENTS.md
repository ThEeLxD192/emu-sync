# Guía de trabajo de EmuSync

## Alcance y estado actual

EmuSync es una aplicación Kotlin/Compose Desktop para Linux y Windows. La migración
acordada usa módulos Gradle JVM y conserva el comportamiento y los formatos actuales.
La primera entrega fue esta guía y la skill local de pruebas. H0–H5 están
completados. La implementación de H6 está verificada en Linux; quedan pendientes
la validación nativa de Windows. El commit de H6 está autorizado; H7–H8 siguen pendientes.
Documentar un hito no significa ejecutarlo.

Línea base inspeccionada el 2026-10-30, sobre `484012f`:

- Un proyecto Gradle raíz, sin subproyectos incluidos.
- 85 archivos Kotlin de producción y 57 archivos Kotlin de pruebas/fixtures.
- Gradle 9.8.2, Java 25, Kotlin 2.4.21 y Compose 1.7.3.
- Ktor 3.1.1, kotlinx.serialization 1.8.0 y coroutines 1.10.1.
- Detekt 2.0.0-alpha.6 y Spotless 8.10.4 con ktfmt 0.64.
- Reportes locales existentes: 431 casos, 429 aprobados, cero fallos/errores y dos
  omitidos por requerir PowerShell. Son evidencia local, no validación de Windows.
- Detekt principal y de pruebas: cero hallazgos en los reportes SARIF existentes.

Las cifras son una fotografía inicial, no objetivos que impidan añadir pruebas útiles
o que obliguen a eliminar pruebas para reducir su número.

## Estructura actual

```text
src/main/kotlin/
  Main.kt                    # Arranque y composición de la aplicación
  com/emusync/
    AppInfo.kt               # Información y versión de la aplicación
    composition/             # Construcción temporal de servicios; traslado a desktopApp en H7
    ui/                      # Estado y controladores
      components/            # Componentes de interfaz
      dialogs/               # Diálogos y estado de formularios
      theme/                 # Colores y tema
src/main/resources/          # Iconos y metadatos AppStream
src/test/kotlin/com/emusync/  # Presentación e integraciones entre adaptadores
config/                      # Configuración de Detekt
gradle/
  wrapper/                   # Gradle Wrapper
  libs.versions.toml         # Versiones compartidas, sin cambios respecto a la base
build-logic/                 # Convenciones JVM/calidad; build incluido
  src/main/kotlin/           # Plugins precompilados .gradle.kts
domain/                      # Biblioteca, reglas de sync y contratos neutrales (H3)
  src/{main,test}/kotlin/     # com/emusync/domain/{config,library,sync,operation,execution,update}
application/                 # Casos de uso y coordinación neutral (H5)
  src/main/kotlin/            # com/emusync/application/{sync,library,connection,operation,update}
  src/test/kotlin/            # Reglas de destinos junto a sus consumidores
data/local/                  # Configuración/DTO y saves: historial, snapshots, backups y rutas
  src/{main,test}/kotlin/     # com/emusync/config y com/emusync/local/saves
data/drive/                  # OAuth, búsquedas, carpetas y transferencias de Google Drive
  src/{main,test}/kotlin/     # com/emusync/drive
  src/testFixtures/kotlin/    # Árbol Drive compartido por pruebas de protocolo/integración
data/updates/                # Consulta GitHub y descarga de actualizaciones (H6)
  src/{main,test}/kotlin/     # com/emusync/update; DTO internos y cliente HTTP prestado
platform/desktop/            # Adaptadores JVM de escritorio (H6)
  src/{main,test}/kotlin/     # runner, steam, scanner, update, desktop/picker y entorno
presentation/, desktopApp/   # Módulos vacíos preparados para H7
scripts/                     # Distribución, versión, hooks y sus pruebas
.githooks/                   # Hook de precommit versionado
.github/workflows/           # Validación y releases en Linux/Windows
.agents/skills/              # Skills locales versionadas
```

La raíz también contiene los archivos Gradle, Dockerfile, docker-compose.yml,
config.example.json, README.md y configuración de formato/Git. `docs/`, `build/`,
`config.json` y el historial/backups locales están ignorados: no guardar allí la única
copia del seguimiento versionado ni añadir información personal o credenciales.

## Arquitectura objetivo

| Módulo Gradle | Responsabilidad |
| --- | --- |
| `:domain` | Modelos neutrales, reglas de sincronización y contratos externos. |
| `:application` | Casos de uso de biblioteca, ejecución, sincronización y actualizaciones. |
| `:data:local` | Configuración, historial, recuperación y persistencia local. |
| `:data:drive` | OAuth, protocolo de Google Drive y transferencias. |
| `:data:updates` | Consulta de versiones y descarga de actualizaciones. |
| `:platform:desktop` | Procesos, Steam, navegador, selectores nativos e instalación. |
| `:presentation` | Compose, estados visuales, controladores, componentes y temas. |
| `:desktopApp` | Arranque, construcción de dependencias, recursos y distribución. |

Dependencias permitidas:

```text
application -> domain
data:local / data:drive / data:updates / platform:desktop -> domain
presentation -> application + domain
desktopApp -> presentation + application + domain + adaptadores concretos
```

Los adaptadores implementan contratos de dominio y reciben sus colaboradores por
constructor. No dependen de presentación ni se instancian entre sí. `domain` puede
usar Kotlin y abstracciones de corrutinas cuando hagan falta; no usa Compose, Ktor,
AWT ni operaciones concretas de archivos, HTTP o procesos. `application` coordina
esas capacidades mediante contratos, sin construir implementaciones concretas.

La composición manual de dependencias y su cierre pertenecen a `desktopApp`. Cada
cliente HTTP, scope y recurso tiene un propietario explícito. Mantener el enfoque
actual de estado con `StateFlow`; esta migración no añade un framework de DI ni
obliga a cambiar a otro tipo de ViewModel.

### Reglas de modularidad y legibilidad

- Organizar cada módulo por funcionalidad. No convertir `util`, `common`, `manager`
  o una carpeta global de modelos en destinos para responsabilidades sin relación.
- Los componentes Compose reutilizables reciben estado y callbacks. Los estados
  observables se exponen como solo lectura; los controladores gestionan operaciones.
- No pasar `GameItem`, estados visuales o callbacks de diálogos a Drive o dominio.
  Traducir entre resultados neutrales y presentación en el límite de la UI.
- Cada `data class`, enum y raíz sellada reutilizable tiene un archivo con su nombre
  dentro de la funcionalidad propietaria. Las variantes anidadas de una jerarquía
  sellada pueden permanecer juntas; un helper privado exclusivamente local puede
  quedarse junto a su consumidor. No convertir clases en `data class` por esta regla.
- Separar modelos de dominio, DTO de configuración/red y estados de presentación
  cuando tengan contratos distintos. Usar conversiones explícitas y conservar las
  claves y valores serializados existentes.
- Compartir reglas que sean realmente comunes, por ejemplo entre alta y edición de
  entradas. No extraer una interfaz por clase ni una abstracción para usos hipotéticos.
- Diseñar contratos por capacidad necesaria. Un futuro proveedor implementará esas
  capacidades sin exponer DTO de Google ni introducir ramas de proveedor en Compose.
- Mantener implementaciones y helpers como `internal` o privados cuando sea posible.
  Publicar solo contratos y tipos necesarios entre módulos, sin romper el aislamiento
  con accesos especiales del compilador para las pruebas.
- Durante la transición, el módulo raíz puede consumir las partes ya extraídas. Los
  módulos nuevos no deben importar código que todavía viva en la raíz. Retirar las
  fachadas temporales al completar el hito que las sustituye.

Base de diseño: [arquitectura Compose](https://developer.android.com/develop/ui/compose/architecture)
y [modularización](https://developer.android.com/topic/modularization), adaptadas a
Desktop JVM. No se añaden targets Android/iOS ni se actualizan dependencias como
parte de esta reorganización.

## Pruebas y comprobaciones

Usar [kotlin-valuable-tests](.agents/skills/kotlin-valuable-tests/SKILL.md) al diseñar,
revisar, consolidar o auditar pruebas. Consultar su
[matriz de auditoría](.agents/skills/kotlin-valuable-tests/references/test-audit.md)
cuando se trabaje en H1 o se cambie una garantía registrada. La skill se versiona en
el repositorio; no necesita una instalación global ni otras skills comunitarias.

Comandos actuales, ejecutados desde la raíz:

```bash
./gradlew :domain:test --tests 'com.emusync.domain.sync.SyncPlannerTest' --no-daemon
./gradlew qualityCheck --no-daemon
./gradlew check --no-daemon
./gradlew packageReleaseAppImage --no-daemon
./gradlew packageReleaseMsi packageReleaseExe --no-daemon
```

El último comando requiere Windows; AppImage se verifica en Linux. Tras extraer un
módulo, las pruebas focalizadas usan su tarea, por ejemplo `:domain:test --tests ...`.
Conservar las tareas raíz de calidad, pruebas, `check` y empaquetado como puntos de
entrada agregados o delegados. Mantener reportes de todos los módulos en CI.

- Ejecutar las pruebas y análisis afectados por el cambio. Ejecutar `check` al cerrar
  integraciones y al finalizar; no repetir la suite completa sin un motivo nuevo.
- No añadir pruebas por un simple movimiento de archivos o una modificación de
  documentación. Conservar las regresiones útiles y comprobar compatibilidad en
  los límites que cambien.
- Spotless/ktfmt bloquea por formato. Detekt sigue informativo (`ignoreFailures=true`),
  con aceptación de cero hallazgos. No relajar reglas ni añadir supresiones para
  completar hitos; conservar y revisar las supresiones justificadas existentes.
- Los reportes de la raíz se conservan en `build/test-results/test`,
  `build/reports/tests/test` y `build/reports/detekt`; los módulos usan esas rutas
  bajo su directorio. CI recoge los reportes recursivamente, incluido `build-logic`.
  Distinguir resultados recientes
  de reportes previos, tareas `UP-TO-DATE` y comprobaciones que no se pudieron ejecutar.

## Compatibilidad y flujo de trabajo

- Preservar JSON de configuración, revisiones de credenciales, rutas remotas,
  historial, backups, partidas y comportamiento de conflictos/cancelación.
- Conservar identificadores de aplicación, UUID de actualización de Windows y nombres
  de artefactos. No publicar releases ni hacer push sin autorización explícita.
- Antes de mover recursos o metadatos de versión, actualizar conjuntamente los
  consumidores: Dockerfile, scripts de distribución/versiones, workflows y sus rutas
  de staging, informes y artefactos. No dejar esos ajustes para un hito posterior si
  el movimiento actual ya los rompe.
- El precommit valida una exportación del índice. Preservar ese aislamiento: no usar
  stash, reset ni añadir archivos del usuario para que las comprobaciones pasen.
  Incluir nuevas convenciones/configuración Gradle en los disparadores pertinentes.
- Solicitar confirmación explícita antes de cada commit, mostrando los cambios y un
  mensaje propuesto en inglés. Preparar y verificar el resultado antes de pedirla.
  Una aprobación anterior no autoriza automáticamente el siguiente commit.

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
Commit H6: autorizado (`refactor: extract desktop platform and update adapters`);
hash pendiente de registrar en la próxima actualización del seguimiento.**

### H7 — Presentación y composición Desktop

- [ ] Agrupar presentación por biblioteca, conexión y actualizaciones.
- [ ] Compartir componentes/reglas comunes de formularios mediante estado y callbacks.
- [ ] Extraer `desktopApp`, recursos y composición manual de dependencias.
- [ ] Actualizar rutas de distribución, versión, staging y artefactos con sus consumidores.
- [ ] **H7 completado.**

Aceptación: UI sin dependencias concretas de Drive/HTTP, recursos con un propietario,
comandos raíz compatibles y preparación de paquetes en Linux/Windows operativa.

Evidencia: pendiente. Commit: pendiente.

### H8 — Validación final y cierre

- [ ] Ejecutar `check`, revisar Detekt de todos los módulos y confirmar ausencia de ciclos.
- [ ] Revisar la matriz de pruebas y retirar fachadas o dependencias temporales.
- [ ] Verificar AppImage en Linux y MSI/EXE en Windows sin publicar releases.
- [ ] Confirmar compatibilidad de datos, actualizar estructura real y registrar pendientes.
- [ ] **H8 completado.**

Aceptación: todos los hitos verificados, formatos compatibles, cero hallazgos Detekt y
evidencia de distribución por plataforma. Un resultado solo en Linux no cierra Windows.

Evidencia: pendiente. Commit: pendiente.
