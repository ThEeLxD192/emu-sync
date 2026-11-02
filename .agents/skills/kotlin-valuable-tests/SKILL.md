---
name: kotlin-valuable-tests
description: Diseña, revisa y consolida pruebas Kotlin/JUnit de EmuSync según contratos observables y riesgos de producción. Usar al añadir regresiones, revisar concurrencia o auditar la suite; los movimientos mecánicos y cambios triviales no requieren pruebas nuevas.
---

# Pruebas Kotlin con valor

Proteger comportamientos que importan a usuarios y consumidores, con pruebas que
sigan siendo útiles después de una refactorización. Una prueba valiosa permite
explicar qué fallo real detecta; su valor no se deduce del tamaño de la suite.

## Elegir qué comprobar

Antes de añadir o cambiar una prueba, identificar el contrato observable, el riesgo
que cubre y el punto de entrada más pequeño que lo ejercita con fidelidad. Revisar
las pruebas existentes para reutilizar protección en vez de duplicarla.

Priorizar pérdida/sobrescritura de partidas, recuperación, credenciales obsoletas,
escrituras concurrentes, rutas y argumentos, cancelación, reintentos e integridad de
descargas. Para una regresión, reproducir primero el fallo cuando sea viable y
comprobar que la corrección lo resuelve. No exigir TDD para cada función ni recrear
código ya escrito para cumplir un ritual.

No añadir pruebas solo por mover archivos, separar una `data class` o cambiar
documentación. Getters, constantes y delegaciones triviales necesitan pruebas
únicamente si expresan un contrato relevante que todavía no esté protegido.

## Diseñar la prueba

- Entrar por funciones, casos de uso o interfaces que los consumidores usan. No
  afirmar sobre nombres de helpers, métodos privados o texto del código fuente.
- Obtener el resultado esperado de un requisito, ejemplo trabajado o fixture
  independiente; no recalcularlo con el algoritmo o helper que se está probando.
- Comprobar resultados y efectos relevantes. Varias aserciones son apropiadas cuando
  describen una misma garantía, por ejemplo contenido conservado y temporal eliminado.
- Usar componentes reales y fakes pequeños donde representen fielmente un contrato.
  Controlar red, reloj, procesos y otros límites cuando el comportamiento real de ese
  límite no sea el objeto de la prueba. No simular todas las clases colaboradoras.
- Comprobar número u orden de operaciones si es parte del contrato, como evitar una
  segunda descarga o impedir un lanzamiento después de un fallo de sincronización.
- Mantener la configuración enfocada. Si los mocks recrean medio subsistema, evaluar
  una prueba de integración con componentes reales en un entorno aislado.
- Usar nombres que describan condición y comportamiento. Los mensajes exactos se
  comprueban cuando son un contrato estable, no por coincidir con redacción accidental.

Leer [técnicas Kotlin y ejemplos del proyecto](references/kotlin-testing.md) cuando
se trabaje con corrutinas, Ktor, archivos, procesos o fixtures compartidos.

## Auditar o consolidar

Para H1 o una auditoría explícita, usar la
[matriz versionada](references/test-audit.md). Registrar contrato, riesgo, punto de
entrada, nivel, decisión y evidencia por escenario o grupo homogéneo. No clasificar
una clase completa como redundante porque dos métodos se parezcan.

Conservar, combinar o reescribir según la garantía. Retirar una prueba solo si es
tautológica, protege un comportamiento ya eliminado o tiene protección equivalente
identificada. Una prueba unitaria y una de integración pueden verificar riesgos
distintos aunque compartan datos de entrada. Mantener las regresiones históricas
salvo que su garantía equivalente quede trazada.

No hay cuota de eliminación ni meta porcentual de cobertura. Medir número, tiempo,
inestabilidad y coste de mantenimiento como señales; no retirar una prueba real de
procesos o sockets únicamente por tardar más que una prueba pura. Parametrizar
tablas de casos puede mejorar mantenimiento sin reducir los escenarios ejecutados.

## Verificar el valor y comunicarlo

Preguntarse qué cambio plausible de producción haría fallar la prueba: invertir una
decisión, omitir una escritura condicional, aceptar una ruta inválida o ignorar una
cancelación. Si ninguna aserción lo observaría, mejorar la prueba o reconsiderarla.
Una comprobación manual mediante un fallo temporal, cuando haga falta, se realiza
en una copia aislada y nunca se entrega como cambio de producción.

Ejecutar las suites afectadas y registrar comandos/resultados, escenarios protegidos,
decisiones de consolidación y omisiones. Distinguir una ejecución nueva de reportes
existentes o resultados recuperados de caché. No repetir comprobaciones completas
sin un cambio o una incertidumbre nueva que lo justifique.

Esta skill no autoriza commits, publicación, pruebas contra cuentas reales ni
cambios de política del proyecto. Seguir el `AGENTS.md` raíz para esas decisiones.

## Origen

Adaptación local de criterios de pruebas de Matt Pocock y Jesse Vincent/Superpowers.
Consultar [fuentes y licencias](references/sources-and-licenses.md) para revisiones
fijadas y avisos MIT. Los materiales de origen son referencias: esta adaptación no
requiere sus otras skills, aprobaciones por cada prueba ni reglas universales de TDD.
