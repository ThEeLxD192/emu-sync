# Técnicas de pruebas para EmuSync

Elegir el nivel por el riesgo que se necesita observar. Las pruebas rápidas de reglas
y las pruebas de integración de adaptadores se complementan.

## Reglas y resultados independientes

En `SyncPlannerTest`, una partida local más nueva durante la sincronización posterior
debe producir `UPLOAD_LOCAL`. Una aserción explícita sobre esa decisión detecta que
se invierta la política. Obtener el esperado llamando de nuevo al planificador o a su
helper de decisión no aporta esa garantía.

Los casos de fechas iguales, duplicados o conflictos se conservan si representan
reglas distintas. Usar `@ParameterizedTest` para tablas homogéneas cuando mejore la
lectura; no agrupar escenarios independientes en un test enorme solo para reducir
el contador de pruebas. Las aserciones de orden son pertinentes si el resultado
promete determinismo.

## Corrutinas y estado

- Usar `runTest` para lógica suspendida controlable y compartir su scheduler con los
  dispatchers inyectados. Evitar `Thread.sleep` para ordenar eventos de esa lógica.
- Usar `CompletableDeferred` o señales explícitas para liberar una operación en el
  orden que el escenario requiere. `advanceUntilIdle` no sustituye una condición de
  sincronización cuando hay tareas persistentes o trabajo en dispatchers reales.
- Comprobar cancelación observable: propagación de `CancellationException`, estado
  restaurado, temporales eliminados y capacidad de reintentar, según el contrato.
- Para resultados tardíos, bloquear una respuesta antigua, completar una operación
  nueva y liberar la antigua. Afirmar que el resultado válido no se sobrescribe.
- Cancelar y cerrar jobs, scopes y recursos que la prueba cree; no dejar trabajo vivo
  que contamine otras pruebas.

`UpdateConcurrencyTest` contiene escenarios de respuestas tardías, descarga duplicada
y reintento tras cancelación. El contador de peticiones puede ser válido para probar
que una descarga duplicada se evita: no es una prohibición general de contar llamadas.

## Ktor y protocolos

- Usar `MockEngine` para cuerpos, cabeceras, revisiones/ETag, códigos de respuesta y
  reintentos del adaptador, conservando el cliente y la serialización reales.
- Afirmar sobre peticiones cuando representen obligaciones del protocolo, por ejemplo
  una condición que evita sobrescribir una partida modificada remotamente.
- Mantener pruebas de CIO/servidor loopback para timeouts, desconexiones y cancelación
  del transporte real. Un motor simulado no demuestra esas propiedades.
- No usar Google Drive real, GitHub real ni credenciales personales en la suite normal.

## Persistencia, procesos y plataforma

- Usar `@TempDir` para probar JSON, escritura atómica, restauración, VDF y limpieza.
  Leer el resultado persistido es apropiado cuando el formato o los bytes escritos
  son el contrato externo, incluso si no existe otro método público para leerlos.
- Emplear fixtures con contenido esperado conocido para compatibilidad de formatos;
  no generar el esperado con el mismo serializador/escritor bajo prueba.
- Conservar procesos reales aislados para quoting, seguimiento, códigos de salida y
  exclusión de instancias. Sustituirlos por fakes perdería esas garantías del sistema.
- Usar puertos asignados por el sistema y límites de tiempo acotados para I/O real.
  El tiempo virtual no controla procesos ni sockets del sistema operativo.
- Separar una omisión por plataforma no disponible de un éxito. La integración
  PowerShell omitida en Linux debe ejecutarse en el entorno Windows correspondiente.

## Propiedad de las pruebas durante la migración

Mover cada prueba al módulo propietario del contrato que verifica. Las reglas puras
van a dominio; protocolo y transporte a sus adaptadores; procesos al módulo de
plataforma. Los escenarios que integran presentación con adaptadores concretos
pertenecen a las pruebas de integración de `desktopApp`; presentación puede probarse
con fakes de contratos cuando eso preserve el riesgo que se quiere observar.

Los helpers de prueba permanecen en código de prueba. Compartir fixtures desde su
módulo propietario con `java-test-fixtures` solo si hay consumidores reales en otros
módulos; evitar un módulo global de utilidades de prueba y dependencias circulares.
Referencia: [test fixtures en Gradle](https://docs.gradle.org/current/userguide/java_testing.html#sec:java_test_fixtures).
