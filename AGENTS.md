# Guía de trabajo de EmuSync

## Alcance y estado actual

EmuSync es una aplicación Kotlin/Compose Desktop para Linux y Windows. La migración
acordada usa módulos Gradle JVM y conserva el comportamiento y los formatos actuales.
La primera entrega fue esta guía y la skill local de pruebas. H0 y H1 están
completados; H2 está verificado técnicamente y espera confirmación de commit.
H3–H8 siguen pendientes. Documentar un hito no significa ejecutarlo.

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
    config/                  # Configuración y persistencia
    model/                   # Configuración serializable y modelos de entradas
    drive/                   # OAuth, Drive, reglas y flujos de sincronización
    runner/                  # Lanzamiento y seguimiento de procesos
    scanner/                 # Descubrimiento de ROMs
    steam/                   # VDF, accesos directos y lanzadores
    update/                  # Consulta, descarga e instalación de actualizaciones
    util/                    # Argumentos y entorno AppImage
    ui/                      # Estado y controladores
      components/            # Componentes de interfaz
      dialogs/               # Diálogos y estado de formularios
      theme/                 # Colores y tema
src/main/resources/          # Iconos y metadatos AppStream
src/test/kotlin/com/emusync/  # Pruebas y fixtures de esas funcionalidades
config/                      # Configuración de Detekt
gradle/
  wrapper/                   # Gradle Wrapper
  libs.versions.toml         # Versiones compartidas, sin cambios respecto a la base
build-logic/                 # Convenciones JVM/calidad; build incluido
  src/main/kotlin/           # Plugins precompilados .gradle.kts
domain/, application/        # Módulos vacíos preparados para H3/H5
data/{local,drive,updates}/  # Módulos vacíos preparados para H4/H6
platform/desktop/            # Módulo vacío preparado para H6
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
./gradlew test --tests 'com.emusync.drive.SyncPlannerTest' --no-daemon
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

**H2 completado técnicamente. Commit H2: pendiente de confirmación explícita.**

### H3 — Dominio, tipos y contratos

- [ ] Extraer reglas/modelos neutrales, separando tipos reutilizables en sus archivos.
- [ ] Definir contratos pequeños para persistencia, nube y capacidades de plataforma.
- [ ] Romper la dependencia de sincronización hacia tipos y diálogos de UI.
- [ ] Mover las pruebas de reglas junto al dominio y verificar sus contratos.
- [ ] **H3 completado.**

Aceptación: `domain` compila sin UI, Ktor ni adaptadores concretos; las dependencias
de paquetes UI/Drive ya no son circulares y los flujos actuales siguen compilando.

Evidencia: pendiente. Commit: pendiente.

### H4 — Persistencia local y Google Drive

- [ ] Extraer configuración, historial y recuperación a `data:local`.
- [ ] Extraer OAuth, protocolo y transferencias a `data:drive`.
- [ ] Inyectar contratos de credenciales y colaboradores, separando DTO de dominio.
- [ ] Verificar JSON, revisiones, escrituras atómicas y condicionales, cancelación,
  conflictos, rutas seguras y recuperación ante fallos.
- [ ] **H4 completado.**

Aceptación: adaptadores utilizables sin presentación y datos existentes compatibles.
Los tests de protocolo usan límites controlados y los de transporte conservan su
comprobación real cuando sea necesaria.

Evidencia: pendiente. Commit: pendiente.

### H5 — Casos de uso

- [ ] Extraer ejecución, sincronización previa/posterior y operaciones manuales.
- [ ] Extraer edición y migración de entradas, conexión y coordinación de actualizaciones.
- [ ] Expresar progreso, resultados y resolución de conflictos con contratos neutrales.
- [ ] Verificar exclusión de operaciones, cancelación, reintentos y resultados obsoletos.
- [ ] **H5 completado.**

Aceptación: `application` solo depende de `domain`; no construye clientes HTTP ni
adaptadores, y no depende de tipos visuales. Los controladores delegan los flujos.

Evidencia: pendiente. Commit: pendiente.

### H6 — Plataforma y actualizaciones

- [ ] Extraer procesos, Steam, navegador y servicios nativos a `platform:desktop`.
- [ ] Separar consulta/descarga (`data:updates`) de instalación y reinicio (plataforma).
- [ ] Conservar pruebas reales de procesos, VDF, argumentos, transporte e instalación.
- [ ] Verificar Linux y Windows en sus entornos; registrar omisiones justificadas.
- [ ] **H6 completado.**

Aceptación: contratos implementados sin dependencia visual, preservando aislamiento
de lanzadores, integridad de descargas, recuperación y códigos de salida.

Evidencia: pendiente. Commit: pendiente.

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
