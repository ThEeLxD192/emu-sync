# Playbook de Pruebas Adversariales para la Migración Rust de EmuSync

Guía práctica de vectores de ataque, patrones de construcción y listas de verificación para diseñar pruebas adversariales de alto valor en el workspace Cargo de EmuSync, sin introducir frameworks externos ni acoplar capas prohibidas por `cargo xtask`.

---

## 1. Anatomía de una Prueba Adversarial en EmuSync

Toda prueba adversarial del proyecto se construye en cuatro fases explícitas:

1. **Precondición e Invariante Protegida**:
   - Definir el estado válido previo en un entorno aislado (`tempfile::tempdir()`, configuración sintética, partida original con contenido conocido, historial inicial).
   - Identificar qué **no puede ocurrir jamás** (por ejemplo: perder los bytes de la partida local, registrar un checkpoint de historial cuando la transferencia falló o cambió la cuenta OAuth, liberar el lock mientras el rollback remoto sigue ejecutándose, o permitir cerrar la app con una subida incierta sin reconciliar).
2. **Punto de Inyección Hostil (Sincronizado, sin `sleep`)**:
   - Detener la operación exactamente en la ventana vulnerable mediante primitivas deterministas (`std::sync::Barrier`, `tokio::sync::Notify`, `tokio::sync::oneshot` o lectura parcial de un `TcpListener` loopback).
3. **Acción Saboteadora**:
   - Ejecutar el evento hostil mientras la operación está en la ventana vulnerable: cortar el socket TCP, cambiar un directorio por un symlink, rotar o desvincular credenciales OAuth (ABA), disparar `CancellationToken::cancel()`, emitir `UiAction::ClearStatus`, solicitar `CloseRequested` en `RuntimeOwner`, o matar el subproceso con `SIGKILL`.
4. **Verificación Cuádruple del Post-Estado**:
   - **Resultado retornado**: error tipado o estado de recuperación esperado (no un `Ok` silencioso ni un string genérico que destruya la capacidad de recuperación).
   - **Integridad en disco**: la partida original conserva sus bytes exactos, o existe un respaldo verificado en `.sync-backups` / `.sync-recovery`; los archivos temporales de staging han sido eliminados.
   - **Estado e historial**: `.sync-state.json` y `config.json` no contienen registros falsos ni mutaciones parciales.
   - **Exclusión y recursos**: el bloqueo sigue retenido si requiere reconciliación (`RecoveryRequired` / `ResourceBusy`), o se liberó limpiamente solo después de finalizar el rollback asíncrono.

---

## 2. Vectores Adversariales por Capa del Workspace

### A. Costura de Composición y Ciclo de Vida (`emusync-desktop-app` + `emusync-application`)

Es la frontera de mayor riesgo porque conecta interfaz, runtime Tokio, coordinadores y adaptadores:

- **Conservación de `RetainedFiles` y errores de transferencia**:
  - *Vector*: `SyncOrchestrator` convierte los errores de `LocalSaveFiles` a texto para la interfaz, pero `RetainedFiles` guarda internamente el `TrackedError` (con el snapshot temporal y la sesión/identidad remota).
  - *Ataque*: Provocar un fallo de resultado incierto en una subida iniciada desde `DesktopCoordinator`.
  - *Contrato exigido*: `DesktopCoordinator` no debe descartar `RetainedFiles` (`let (orchestrator, _) = ...`); debe extraer el fallo mediante `RetainedFiles::finish()`, entregarlo al supervisor (`RuntimeOwner` / `OperationSupervisor`) e impedir el cierre silencioso o nuevas sincronizaciones sobre esa partida hasta su reconciliación.
- **Cierre de ventana (`CloseRequested`) con operación en vuelo**:
  - *Vector*: El usuario cierra la ventana mientras hay una transferencia o migración remota activa.
  - *Ataque*: Iniciar sincronización en `DesktopCoordinator`, retener la petición en el servidor HTTP loopback y solicitar el cierre al propietario del runtime (`begin_close()` / `poll_close()`).
  - *Contrato exigido*: `poll_close()` debe devolver `CloseState::Pending` mientras el worker limpia o espera la respuesta crítica, y `CloseState::RecoveryRequired` si quedó una subida incierta retenida. Nunca debe devolver `CloseState::Ready` mientras una tarea lanzada por el coordinador siga ejecutando I/O.
- **Cancelación desde UI (`UiAction::ClearStatus`) durante rollback**:
  - *Vector*: El usuario cancela una operación y lanza inmediatamente otra acción.
  - *Ataque*: Cancelar durante una descarga a mitad de stream o durante una migración de carpetas (`rename_entry` / `rename_game`) tras haber movido el primer archivo.
  - *Contrato exigido*: La cancelación señala el token, ejecuta el rollback inverso o limpia el archivo de staging y **solo cuando el worker termina** libera la exclusión y restablece el estado libre.
- **Configuración interactiva de guardados (`SaveSetup`)**:
  - *Vector*: Lanzar un juego cuyos `save_targets` están vacíos o requieren confirmación inicial.
  - *Ataque*: Invocar `LaunchGame` sobre una entrada sin destinos configurados y cancelar o completar el diálogo.
  - *Contrato exigido*: No debe ejecutar el juego en silencio con objetivos vacíos; debe solicitar la configuración a la interfaz, persistirla si el usuario confirma, o abortar el lanzamiento si cancela.

### B. Transporte, OAuth y Concurrencia Remota (`emusync-data-drive`)

Usar siempre sockets reales sobre `127.0.0.1:0` (`TcpListener`) para probar el comportamiento real de `reqwest` y Tokio:

- **Pérdida de respuesta tras `PUT` condicional o reanudable**:
  - *Ataque*: El servidor lee todo el cuerpo del `PUT` (con `If-Match` o bloque final) y cierra el socket sin enviar cabeceras HTTP, o responde `500`/`503`.
  - *Contrato exigido*: Clasificar como resultado incierto (`!leaves_remote_unchanged()`). Jamás reenviar un `PUT` incondicional ni reiniciar una sesión desde cero creando duplicados; conservar sesión/snapshot para reconciliación por ID o nombre+SHA-256.
- **Contención de dos escritores (`If-Match` y ETags)**:
  - *Ataque*: Dos clientes leen la misma revisión remota; el primero actualiza el ETag y el segundo envía su `PUT` con el ETag antiguo, o el servidor devuelve un ETag débil (`W/"..."`), comodín (`*`) o malformado.
  - *Contrato exigido*: El segundo escritor falla con conflicto de precondición (`412 Precondition Failed`) sin tocar el contenido remoto; los ETags débiles o vacíos se rechazan antes de abrir el snapshot o enviar bytes.
- **Problema ABA en Credenciales y Biblioteca durante OAuth/Transferencia**:
  - *Ataque*: Iniciar un refresco de token o una subida/descarga; antes de que termine, el usuario edita la biblioteca en `config.json`, o desconecta la cuenta y conecta otra (o la misma en una nueva sesión).
  - *Contrato exigido*: El CAS y `CredentialTransaction` deben preservar las ediciones concurrentes de la biblioteca y abortar el commit del historial o reemplazo local si la revisión de credenciales cambió durante el vuelo (`ConnectionChanged`).
- **Respuestas truncadas, identidades falsas y rangos inválidos**:
  - *Ataque*: En descarga, anunciar `Content-Length: 1024` y cerrar en el byte 512, o entregar bytes cuyo SHA-256 no coincide con el metadato remoto. En subida reanudable, responder `308 Resume Incomplete` con un `Range` mayor al tamaño del snapshot, o responder `200 OK` con un `id` vacío o `name` distinto al solicitado.
  - *Contrato exigido*: Descartar el archivo temporal de staging, conservar intacta la partida local y no registrar éxito en el historial.

### C. Sistema de Archivos, Rutas y Recuperación Entre Procesos (`emusync-data-local`)

- **Ataques TOCTOU con enlaces simbólicos (`PinnedRemoteDestination` y `StagedRestore`)**:
  - *Ataque*: Abrir o preparar el destino de restauración dentro del directorio permitido y, antes de escribir o publicar el archivo, renombrar un subdirectorio intermedio y sustituirlo por un symlink que apunte fuera de la raíz (o insertar un symlink en el archivo final).
  - *Contrato exigido*: La operación debe detectar el cambio de inodo/descriptor o rechazar el enlace, abortar sin escribir fuera de la raíz seleccionada, conservar el respaldo y no registrar checkpoint en `.sync-state.json`.
- **Muerte abrupta del proceso (`SIGKILL`) y entradas corruptas en `RecoveryStore`**:
  - *Ataque*: Matar un proceso hijo con `SIGKILL` tras persistir la intención de subida en `<config>.sync-recovery/op-*` pero antes de recibir respuesta; o inyectar en `.sync-recovery` un manifiesto truncado, de versión futura (`v99`), con un snapshot cuyo SHA-256 no coincide, o con un ID de operación `../escape`.
  - *Contrato exigido*: Mientras exista una entrada pendiente para esa partida, cualquier intento de subida nueva falla inmediatamente con `ResourceBusy` sin emitir peticiones de red. Las entradas corruptas o de otra cuenta se reportan como no verificables y **no se borran ni sobrescriben**. `remove` rechaza cualquier identificador que no sea `op-*` limpio.
- **Edición local concurrente durante transferencia**:
  - *Ataque*: El usuario o el emulador modifica el archivo de partida local mientras una descarga o subida está en vuelo.
  - *Contrato exigido*: La descarga detecta que la partida local cambió respecto a la observación previa y rehúsa sobrescribirla; la subida registra lo ocurrido con el snapshot pero no marca la nueva edición local como ya sincronizada.

### D. Dominio y Reglas Puras (`emusync-domain`)

- **Esperados independientes y compatibilidad**:
  - *Ataque*: Nombres con caracteres fuera del BMP (pares subrogados UTF-16 frente a escalares UTF-8), rutas con barras mezcladas (`\` y `/`), segmentos `.` o `..`, duplicados remotos donde uno carece de hash o fecha, y timestamps en los límites de tolerancia temporal.
  - *Contrato exigido*: El resultado debe coincidir exactamente con los esperados literales de `rust/domain/tests/fixtures/comparison.tsv` y las reglas de `SyncPlanner`.

### E. Plataforma, Steam y Actualizaciones (`emusync-platform-desktop` + `emusync-data-updates`)

- **Inyección en argumentos de lanzamiento y entorno AppImage**:
  - *Ataque*: Rutas de ROM con comillas dobles, `$`, `` ` ``, espacios, caracteres no UTF-8 o bytes `NUL`; variables `LD_LIBRARY_PATH` contaminadas por el runtime de AppImage heredadas por el emulador o navegador hijo.
  - *Contrato exigido*: Rechazo estricto de `NUL`, escape seguro de metacaracteres de shell y limpieza de variables de AppImage preservando las rutas de bibliotecas legítimas del usuario.
- **Corrupción o truncado en `shortcuts.vdf` y colisiones de wrappers**:
  - *Ataque*: Archivos VDF binarios truncados a mitad de campo, con tipos desconocidos o alias legacy vacíos; dos categorías cuyos nombres normalizados colisionan.
  - *Contrato exigido*: Preservación byte a byte de campos desconocidos en escrituras atómicas, rechazo de VDF truncados sin corromper el original y bloqueo de lanzadores legacy ambiguos.
- **Fallo de actualización y rollback de AppImage**:
  - *Ataque*: Descarga de release vacía (`0 bytes`), truncada o cancelada; fallo al ejecutar el nuevo binario tras reemplazar el AppImage.
  - *Contrato exigido*: El archivo `.part` se limpia sin tocar el binario instalado; si el lanzamiento del nuevo AppImage falla, se restaura automáticamente el `.old.bak` y, si incluso la restauración falla, el respaldo `.old.bak` se conserva en disco para recuperación manual.

---

## 3. Protocolo de Verificación por Mutación Manual Dirigida

No damos por válida una prueba adversarial solo porque compile y pase en verde. Antes de cerrar el cambio:

1. Identificar la línea o guardia de producción que protege el contrato (por ejemplo, la verificación de revisión en `CredentialTransaction`, el chequeo de inodo en `PinnedRemoteDestination`, la retención del error en `RetainedFiles` o el rollback en `CloudFolderMigrator`).
2. Comentar o invertir temporalmente esa condición en la copia de trabajo.
3. Ejecutar el test (`cargo test --locked -p <crate> --test <suite>`): **debe fallar por la aserción exacta que protege el contrato**.
4. Restaurar el código de producción y volver a verificar que pasa en verde y sin warnings de Clippy.

---

## 4. Checklist Rápido antes de dar por cerrada una Prueba

- [ ] ¿El test entra por la API pública del módulo o caso de uso y no acopla detalles privados?
- [ ] ¿El valor esperado es un literal independiente y no el resultado de llamar a la misma función bajo prueba?
- [ ] ¿Se comprueba tanto el retorno como los efectos colaterales (bytes del save en disco, ausencia de temporales, estado de `.sync-state.json`, liberación o retención deliberada del lock)?
- [ ] ¿La sincronización entre hilos/tareas usa barreras o canales deterministas en lugar de `sleep` arbitrarios?
- [ ] ¿Todos los recursos (`TempDir`, `TcpListener`, procesos hijos, `RuntimeOwner`) se limpian al terminar la prueba aunque falle una aserción?
- [ ] ¿Está registrado en la sección «Pendientes Windows» de `AGENTS.md` lo que sigue pendiente en Windows nativo?
