# Auditoría de pruebas de EmuSync

Estado: **H1 pendiente**. Este archivo registra el inventario inicial y el formato de
trabajo; no afirma que las pruebas ya se hayan auditado ni autoriza su eliminación.

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
| `config` | 3 | Pendiente |
| `drive` | 20 | Pendiente |
| `model` | 3 | Pendiente |
| `runner` | 2 | Pendiente |
| `scanner` | 1 | Pendiente |
| `steam` | 7 | Pendiente |
| `ui` (sin subcarpetas) | 9 | Pendiente |
| `ui/components` | 3 | Pendiente |
| `ui/dialogs` | 3 | Pendiente |
| `update` | 4 | Pendiente |
| `util` | 2 | Pendiente |

Las pruebas de hooks en `scripts/tests/git-hooks-test.sh` también deben revisarse por
sus garantías de índice aislado, instalación y limpieza. No forman parte de los 431
casos JUnit ni del tiempo anterior.

## Matriz de decisiones

Añadir filas por escenario o grupo homogéneo durante H1. Para grupos, enumerar los
métodos/casos y justificar por qué comparten contrato y decisión. Mantener los nombres
y destinos actualizados cuando las pruebas cambien de módulo.

| Ubicación y escenario(s) | Contrato observable | Riesgo y fallo que detecta | Punto de entrada / nivel / dobles | Decisión y motivo | Protección equivalente si se retira | Comando y resultado / fecha / commit |
| --- | --- | --- | --- | --- | --- | --- |

Decisiones admitidas: **conservar**, **combinar**, **reescribir**, **retirar**.
Usar «no aplica» para protección equivalente cuando la prueba se conserve; si protege
un comportamiento eliminado, identificar ese cambio en vez de inventar un reemplazo.

## Garantías que deben quedar trazadas

- Planificación y conflictos: dirección correcta, duplicados y resultados deterministas.
- Transferencias: escrituras condicionales, reintentos, fallos parciales y cancelación.
- Persistencia: JSON compatible, escrituras atómicas, historial y recuperación.
- Autenticación: revisiones de credenciales, autorización obsoleta y desconexión.
- Coordinación: exclusión de operaciones, respuesta tardía, edición segura y reintento.
- Plataforma: argumentos, procesos, VDF/Steam y aislamiento de lanzadores.
- Actualizaciones: integridad, temporales, instalación y reinicio seguro.
- Formularios: validación significativa y reglas compartidas entre alta y edición.
- Hooks: validar el índice sin alterar archivos preparados o sin preparar.

No reducir los riesgos a una sola prueba por fila de esta lista. La matriz debe
identificar escenarios concretos, incluidas regresiones históricas relevantes.

## Cierre de H1

- [ ] Todos los archivos del inventario y las pruebas de hooks tienen clasificación.
- [ ] Toda consolidación o retirada tiene motivo y protección equivalente cuando aplica.
- [ ] Las garantías críticas están trazadas a pruebas concretas.
- [ ] Las suites afectadas pasan y las omisiones tienen causa y plataforma identificadas.
- [ ] Se registran cantidad/tiempos posteriores sin tratarlos como objetivo de reducción.

Ejecuciones y decisiones de H1: todavía no iniciadas.
