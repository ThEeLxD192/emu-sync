# Convenciones Gradle de EmuSync

Este build incluido consume el mismo `gradle/libs.versions.toml` que la aplicación.
No publica plugins ni artefactos. Su compilación usa el Kotlin DSL incluido en Gradle;
el plugin Kotlin de los proyectos conserva la versión del catálogo.

- `emusync.kotlin-jvm`: Kotlin/JVM, toolchain Java 25, JUnit Platform y dependencias
  comunes de pruebas. Aplica la convención de calidad.
- `emusync.quality`: Spotless/ktfmt y Detekt con configuración raíz compartida.
  Analiza las compilaciones JVM, incluidas las de `java-test-fixtures`; las de pruebas
  y fixtures reciben `config/detekt-test.yml`. Detekt sigue informativo y el formato
  bloquea. No se añaden baselines ni supresiones.

Los ocho módulos tienen fuentes y dependencias según la arquitectura de AGENTS.md.
`desktopApp` conserva aplicación, recursos y paquetes; `presentation` contiene Compose
y sus controladores. La raíz no tiene fuentes y mantiene los comandos agregados de
calidad/pruebas y las delegaciones de ejecución/empaquetado a `desktopApp`.
Los proyectos contenedores `:data` y `:platform` no tienen fuentes ni build propio.
Los agregados raíz incorporan todos los subproyectos con build propio.
`verifyModuleArchitecture`, incluido en `qualityCheck` y por tanto en `check` y el
precommit, verifica las dependencias de producción declaradas e heredadas en
api/implementation/compileOnly/runtimeOnly y los classpaths de main. Rechaza aristas
fuera del grafo de AGENTS.md, familias Ktor/Compose/serialización incompatibles con
cada capa y fuentes en la raíz. Las dependencias de prueba no se someten al grafo
productivo. La tarea es compatible con la caché de configuración.

```bash
./gradlew :qualityCheck --no-daemon
./gradlew :test --no-daemon
./gradlew check --no-daemon
./gradlew :domain:test --tests 'nombre.de.Clase' --no-daemon
```

El último comando se usa cuando el módulo tenga pruebas. `check` también ejecuta
las regresiones de hooks y `verifyGradleConventions`. Esta última crea un proyecto
JVM desechable: demuestra ejecución/fallo de tests desde el agregado raíz, análisis
con tipos de main/test/fixtures, formato bloqueante, bytecode Java 25 y reutilización
de la caché de configuración. Su hallazgo GlobalCoroutineUsage es deliberado, se
verifica en el SARIF temporal y no se incorpora a la aplicación.

Las tareas de scripts aceptan `EMUSYNC_BASH` con la ruta nativa absoluta del
intérprete. CI selecciona Git Bash en Windows para evitar el lanzador de WSL;
la configuración local está en [README](../README.md#code-quality). Los scripts
reutilizan `$BASH` en sus llamadas internas. La suite desechable antepone un
`bash` que falla al PATH y verifica hooks con el intérprete explícito, también
al reutilizar la caché de configuración.

La copia conserva `.gitattributes` y `.editorconfig`. Su repositorio temporal usa
`core.autocrlf=false` y `core.eol=crlf` para comprobar que las reglas LF de Kotlin
y Gradle prevalecen sobre ese valor por defecto. El formato acepta las fuentes LF
y rechaza un fixture CRLF, además del fixture con formato Kotlin incorrecto.

La suite desechable comprueba también que el verificador acepta dependencias de
prueba, rechaza domain → application, Ktor en domain y fuentes añadidas a la raíz,
y reutiliza la caché de configuración.

Spotless de la raíz formatea también los scripts y clases Kotlin de este build. `qualityCheck`
valida los plugins con `build-logic:check`. Los reportes de la aplicación y los
módulos quedan en sus respectivos `build/test-results/` y `build/reports/`; CI los
recoge recursivamente. Los módulos vacíos producen tareas NO-SOURCE, no resultados
ficticios de tests o análisis.

El wrapper sigue en Gradle 9.8.2. `--warning-mode all` y
`-Dorg.gradle.deprecation.trace=true` identifican `Configuration.setVisible` en
`dev.detekt.gradle.plugin.DetektBasePlugin.kt:60` de Detekt 2.0.0-alpha.6. Gradle
anuncia su retirada en Gradle 11. Se conserva esa limitación del plugin y la versión
acordada; no se oculta el aviso ni se cambian reglas para evitarlo.

Referencias oficiales: [plugins precompilados](https://docs.gradle.org/9.8.2/userguide/implementing_gradle_plugins_precompiled.html),
[catálogos de versiones](https://docs.gradle.org/9.8.2/userguide/version_catalogs.html) y
[Detekt Gradle](https://detekt.dev/docs/2.0.0-alpha.6/gettingstarted/gradle/).
