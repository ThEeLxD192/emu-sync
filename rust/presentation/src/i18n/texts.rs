//! Every interface text, English and Spanish side by side. `{name}` marks a value that
//! `i18n::fill` puts in.

macro_rules! texts {
    ($($key:ident: $en:literal, $es:literal;)*) => {
        /// The interface texts of one language.
        pub struct Texts {
            $(pub $key: &'static str,)*
        }

        pub(super) const EN: Texts = Texts { $($key: $en,)* };
        pub(super) const ES: Texts = Texts { $($key: $es,)* };
    };
}

texts! {
    // Header
    safe_close: "Safe Close", "Cerrar con seguridad";
    drive_linked: "Drive Linked", "Drive conectado";
    connect_drive: "Connect Drive", "Conectar Drive";
    update_to: "Update v{version}", "Actualizar a v{version}";
    restart_app: "Restart EmuSync", "Reiniciar EmuSync";
    downloading_percent: "Downloading {percent}%", "Descargando {percent} %";
    checking_updates: "Checking updates…", "Buscando actualizaciones…";
    check_updates: "Check Updates", "Buscar actualizaciones";
    language: "Language", "Idioma";

    // Shared buttons
    save: "Save", "Guardar";
    cancel: "Cancel", "Cancelar";
    folder: "Folder", "Carpeta";

    // Game dialog
    edit_game: "Edit Game", "Editar juego";
    edit_game_subtitle: "Configure custom title, cover art, and specific save files/folders.", "Configura un título propio, la portada y archivos o carpetas de partida propios del juego.";
    game_title: "Game Title", "Título del juego";
    raw_name: "Raw Name", "Nombre original";
    auto_clean: "Auto-Clean", "Limpiar nombre";
    rom_file: "ROM File: {rom}", "Archivo ROM: {rom}";
    cover_art: "Cover Art", "Portada";
    custom_cover: "Custom Cover", "Portada propia";
    auto_detected: "Auto-detected", "Detectada";
    no_cover: "No Cover", "Sin portada";
    cover_hint: "Individual cover image for this game, or empty for automatic detection.", "Imagen de portada para este juego; déjala vacía para detectarla automáticamente.";
    custom_image_file: "Custom Image File:", "Imagen propia:";
    detected_image_file: "Detected Image File:", "Imagen detectada:";
    auto_detecting: "Auto-detecting...", "Detectando...";
    using_custom_image: "Using your specific custom image for this game.", "Se usa tu propia imagen para este juego.";
    found_cover_automatically: "Found automatically next to ROM or emulator covers.", "Encontrada automáticamente junto a la ROM o entre las portadas del emulador.";
    select_cover_hint: "Select an image file to display as box art.", "Elige una imagen para usarla como portada.";
    change_image: "Change Image...", "Cambiar imagen...";
    select_image: "Select Image...", "Elegir imagen...";
    reset_to_auto: "Reset to Auto", "Volver a la automática";
    no_cover_found: "No Cover Found", "Sin portada";
    cloud_save_destination: "Cloud Save Destination", "Destino de la partida de la nube";
    save_paths_title: "Save Files/Directories Paths", "Rutas de archivos y carpetas de partida";
    restore_destination_hint: "Choose one folder. The cloud files below will be restored as individual files; other files in this folder will not be synced.", "Elige una carpeta. Los archivos de la nube de abajo se restaurarán como archivos sueltos; el resto de archivos de esa carpeta no se sincronizará.";
    emulator_game_paths_hint: "Only paths specific to this game. Emulator-wide paths are configured in the system settings.", "Solo las rutas propias de este juego. Las rutas comunes del emulador se configuran en los ajustes de la categoría.";
    native_game_paths_hint: "Specific save files or directories synchronized for this game.", "Archivos o carpetas de partida que se sincronizan para este juego.";
    restore_only_listed: "Only the cloud files listed below", "Solo los archivos de la nube de la lista";
    restore_whole_folder: "The entire folder", "Toda la carpeta";
    restore_whole_folder_hint: "Every file in this folder will be synchronized. Use a dedicated folder if it contains saves from other games.", "Se sincronizarán todos los archivos de esta carpeta. Usa una carpeta exclusiva si contiene partidas de otros juegos.";
    restore_only_listed_hint: "Only the cloud files listed below will be synchronized; other files in the folder will be left alone.", "Solo se sincronizarán los archivos de la nube de la lista; el resto de archivos de la carpeta no se tocará.";
    choose_destination_placeholder: "Choose a destination folder with the Folder button", "Elige una carpeta de destino con el botón Carpeta";
    select_destination_first: "Select a destination folder before saving.", "Elige una carpeta de destino antes de guardar.";

    // Category dialog
    edit_entry: "Edit Entry", "Editar categoría";
    add_entry: "Add New Entry", "Nueva categoría";
    add: "Add", "Añadir";
    entry_type: "Type", "Tipo";
    emulator: "Emulator", "Emulador";
    native: "Native", "Nativo";
    name: "Name", "Nombre";
    emulator_name_placeholder: "e.g. Yuzu or Eden", "p. ej., Yuzu o Eden";
    native_name_placeholder: "e.g. Spelunky Classic", "p. ej., Spelunky Classic";
    group_label: "Folder / Platform Group (Optional)", "Carpeta o grupo de plataforma (opcional)";
    group_placeholder: "e.g. Switch, PlayStation (shares cloud saves folder)", "p. ej., Switch, PlayStation (comparten carpeta de partidas en la nube)";
    assign_existing_group: "Assign to existing folder:", "Asignar a una carpeta existente:";
    drive_folder_preview: "☁️ Google Drive saves folder: \"{folder}\"", "☁️ Carpeta de partidas en Google Drive: \"{folder}\"";
    emulator_executable: "Emulator Executable Path", "Ruta del ejecutable del emulador";
    executable: "Executable Path", "Ruta del ejecutable";
    browse: "Browse", "Examinar";
    arguments_label: "Arguments (quote values that contain spaces)", "Argumentos (pon entre comillas los valores con espacios)";
    arguments_example: "Example: --profile \"My Profile\" keeps My Profile as one argument.", "Ejemplo: --profile \"Mi perfil\" mantiene Mi perfil como un solo argumento.";
    rom_placeholder_hint: "Use {ROM} to insert the requested game's path.", "Usa {ROM} para insertar la ruta del juego que se abre.";
    roms_directory: "ROMs Directory", "Carpeta de ROMs";
    extensions_label: "File Extensions (comma-separated)", "Extensiones de archivo (separadas por comas)";
    fullscreen_placeholder: "e.g. -bigpicture -fullscreen", "p. ej., -bigpicture -fullscreen";
    fullscreen_label: "Fullscreen / Big Picture Flags (auto-detected if empty)", "Opciones de pantalla completa / Big Picture (se detectan si se deja vacío)";
    fullscreen_hint: "Forces the emulator to bypass its Qt GUI. Auto-detected: \"{detected}\"", "Hace que el emulador se salte su interfaz Qt. Detectado: \"{detected}\"";
    none: "(none)", "(ninguno)";
    wait_process_label: "Wait for Process Name (Optional)", "Esperar al proceso (opcional)";
    wait_process_hint: "Useful for Steam games: EmuSync will wait for this process to close.", "Útil para juegos de Steam: EmuSync esperará a que se cierre este proceso.";
    cover_image_optional: "Cover Image (Optional)", "Imagen de portada (opcional)";

    // Save path fields
    file: "File", "Archivo";
    choose_type: "Choose type", "Elegir tipo";
    cloud_prefix_placeholder: "Folder prefix shared across devices", "Prefijo de carpeta común a todos los dispositivos";
    cloud_name_placeholder: "File name, including any subfolders", "Nombre del archivo, con sus subcarpetas";
    cloud_path_label: "Cloud path (optional)", "Ruta en la nube (opcional)";
    cloud_path_invalid: "Use a relative path without '..'.", "Usa una ruta relativa sin '..'.";
    cloud_path_automatic: "Saved in the cloud as “{automatic}” so this folder's files stay apart. Use the same cloud path on each device.", "Se guarda en la nube como “{automatic}” para que los archivos de esta carpeta no se mezclen. Usa la misma ruta en la nube en cada dispositivo.";
    cloud_path_same: "Use the same cloud path for this save on each device.", "Usa la misma ruta en la nube para esta partida en cada dispositivo.";
    remove_path: "Remove Path", "Quitar ruta";
    add_another_path: "+ Add Another Path", "+ Añadir otra ruta";
    excluded_folders_label: "Excluded folders (optional)", "Carpetas excluidas (opcional)";
    excluded_folder_placeholder: "Folder inside this save folder", "Carpeta dentro de esta carpeta de partida";
    excluded_folder_outside: "Choose a folder inside this save folder.", "Elige una carpeta dentro de esta carpeta de partida.";
    add_excluded_folder: "+ Add Excluded Folder", "+ Añadir carpeta excluida";
    excluded_folders_hint: "Files in these folders are never uploaded or downloaded.", "Los archivos de estas carpetas nunca se suben ni se descargan.";
    hide: "Hide", "Ocultar";
    show: "Show", "Mostrar";

    // Update dialog
    update_available: "Update Available", "Actualización disponible";
    update_version_current: "Version {version} (current: {current})", "Versión {version} (actual: {current})";
    release_notes: "Release Notes:", "Novedades:";
    update_now: "Update Now", "Actualizar ahora";
    later: "Later", "Más tarde";
    downloading_update: "Downloading update...", "Descargando la actualización...";
    downloading_from_github: "Downloading version {version} from GitHub", "Descargando la versión {version} desde GitHub";
    update_complete: "Update Complete!", "¡Actualización lista!";
    update_ready_to_apply: "The new version is ready to apply.", "La nueva versión está lista para aplicarse.";
    update_ready_to_install: "The new version is ready to install.", "La nueva versión está lista para instalarse.";
    update_steam_hint: "In Steam Deck Game Mode, closing EmuSync applies the update. You can then relaunch it from Steam to start the new version.", "En el modo juego de Steam Deck, la actualización se aplica al cerrar EmuSync. Después puedes volver a abrirlo desde Steam para usar la nueva versión.";
    update_restart_hint: "To apply changes and enjoy the new version, restart EmuSync.", "Reinicia EmuSync para aplicar los cambios y usar la nueva versión.";
    close_to_apply: "Close to Apply", "Cerrar y aplicar";
    restart_now: "Restart Now", "Reiniciar ahora";
    restart_later: "Restart Later", "Reiniciar más tarde";
    update_error: "Update Error", "Error al actualizar";
    close: "Close", "Cerrar";
    checking_for_updates: "Checking for updates...", "Buscando actualizaciones...";
    up_to_date: "You are on the latest version of EmuSync.", "Ya tienes la última versión de EmuSync.";

    // Status overlay
    playing: "Playing...", "Jugando...";
    playing_hint: "The game is running. This window will update when you close it.", "El juego está abierto. Esta ventana se actualizará cuando lo cierres.";
    loading: "Loading...", "Cargando...";
    syncing: "Syncing...", "Sincronizando...";
    drive_not_updated: "Google Drive was not updated.", "Google Drive no se actualizó.";
    post_sync_progress: "{completed} save(s) synced successfully; {pending} save(s) still need to be uploaded.", "{completed} partida(s) sincronizada(s) correctamente; faltan por subir {pending} partida(s).";
    post_sync_message: "{game} closed normally. {progress}\n\n{reason}", "{game} se cerró con normalidad. {progress}\n\n{reason}";
    save_conflict: "Save Conflict", "Conflicto de partidas";
    save_conflict_hint: "Both copies changed, or their shared history could not be verified. Choose which progress to keep. Restoring from the cloud preserves a local recovery copy.", "Las dos copias cambiaron o no se pudo comprobar su historial común. Elige qué progreso conservar. Si restauras desde la nube, se guarda una copia local de recuperación.";
    local_save: "Local Save", "Partida local";
    cloud_save: "Cloud Save", "Partida en la nube";
    choose_save_folders: "Choose Save Folders", "Elige las carpetas de partida";
    choose_save_folders_hint: "These cloud saves are not in any of this game's save folders on this device yet. Choose the folder each one belongs to; it will be downloaded there and no local file is overwritten.", "Estas partidas de la nube aún no están en ninguna carpeta de partida de este juego en este dispositivo. Elige la carpeta a la que pertenece cada una; se descargará allí sin sobrescribir ningún archivo local.";
    restore: "Restore", "Restaurar";
    file_count_one: "1 file", "1 archivo";
    file_count_other: "{count} files", "{count} archivos";
    save_kept_locally: "Save kept locally", "Partida guardada solo en local";
    retry_upload: "Retry upload", "Reintentar la subida";
    keep_local: "Keep local", "Dejarla en local";
    sync_notice: "Sync Notice", "Aviso de sincronización";

    // Drive dialog
    missing_credentials: "Please enter both Client ID and Client Secret.", "Introduce el Client ID y el Client Secret.";
    drive_sync_title: "Google Drive Sync", "Sincronización con Google Drive";
    account_linked: "Account linked and active", "Cuenta conectada y activa";
    drive_setup_subtitle: "Configure cross-device save synchronization", "Configura la sincronización de partidas entre dispositivos";
    drive_setup_info: "Requires an OAuth 2.0 Client ID (Desktop App) from Google Cloud Console with the Google Drive API enabled.", "Necesita un Client ID de OAuth 2.0 (aplicación de escritorio) de Google Cloud Console con la API de Google Drive activada.";
    open_folder: "Open Folder", "Abrir carpeta";
    config_location: "Config Location:", "Ubicación de la configuración:";
    unlink_account: "Unlink Account", "Desconectar cuenta";
    save_and_connect: "Save & Connect", "Guardar y conectar";

    // Game grid
    select_category: "Select a category", "Elige una categoría";
    no_roms_found: "No ROMs found", "No se encontraron ROMs";
    add_roms_to: "Add ROMs to: {folder}", "Añade ROMs a: {folder}";
    game_count_one: "1 game", "1 juego";
    game_count_other: "{count} games", "{count} juegos";
    delete_category: "Delete Category", "Eliminar categoría";
    edit_category: "Edit Category", "Editar categoría";
    open_emulator: "Open Emulator", "Abrir emulador";
    check_sync: "Check sync", "Comprobar sincronización";
    in_sync: "In Sync", "Sincronizado";
    out_of_sync_sync_all: "Out of sync • Sync All", "Sin sincronizar • Sincronizar todo";
    conflict_sync: "Conflict • Sync", "Conflicto • Sincronizar";
    checking: "Checking...", "Comprobando...";
    sync_all: "Sync All", "Sincronizar todo";
    edit_saves: "Edit Saves", "Editar partidas";

    // Sync status labels
    status_idle: "Idle", "En espera";
    status_up_to_date: "Up to date", "Al día";
    status_out_of_sync: "Out of sync", "Sin sincronizar";
    status_conflict: "Conflict", "Conflicto";
    status_not_configured: "Not configured", "Sin configurar";
    status_error: "Sync error", "Error de sincronización";

    // Form errors
    form_cloud_path_invalid: "Cloud paths must be relative and cannot contain '..'.", "Las rutas en la nube deben ser relativas y no pueden contener '..'.";
    form_excluded_folder_outside: "Excluded folders must be inside their save folder.", "Las carpetas excluidas deben estar dentro de su carpeta de partida.";
    form_required_fields: "Please fill in all required fields.", "Rellena todos los campos obligatorios.";
    form_arguments_invalid: "Arguments format is invalid (unclosed quotes or escapes).", "Los argumentos no son válidos (comillas o escapes sin cerrar).";
    form_emulator_fields: "Please specify the ROMs directory and at least one extension.", "Indica la carpeta de ROMs y al menos una extensión.";
    form_native_save_target: "Native PC games require at least one save target path.", "Los juegos nativos de PC necesitan al menos una ruta de partida.";

    // Delete confirmation
    delete_entry_title: "Delete \"{name}\"?", "¿Eliminar \"{name}\"?";
    delete_entry_detail: "The category is removed from EmuSync.", "La categoría se quita de EmuSync.";
    delete_entry_steam: " Its Steam shortcut is removed too.", " También se quita su acceso directo de Steam.";
    delete_entry_kept: "Saves, ROMs and Google Drive files are not deleted.", "No se borran las partidas, las ROMs ni los archivos de Google Drive.";
    delete: "Delete", "Eliminar";

    // Sidebar
    library: "LIBRARY", "BIBLIOTECA";
    add_category: "Add entry", "Añadir categoría";
    expand: "Expand", "Desplegar";
    collapse: "Collapse", "Plegar";
    unregister_steam: "Unregister from Steam", "Quitar de Steam";
    register_steam: "Register in Steam", "Añadir a Steam";
    move_down: "Move down", "Bajar";
    move_up: "Move up", "Subir";

    // Synchronization messages
    sync_checking_cloud_saves: "Checking cloud saves...", "Comprobando las partidas en la nube...";
    sync_connecting_drive: "Connecting to Google Drive...", "Conectando con Google Drive...";
    sync_syncing_game: "Syncing saves for {game}...", "Sincronizando las partidas de {game}...";
    sync_uploading_game: "Uploading saves for {game}...", "Subiendo las partidas de {game}...";
    sync_restoring_game: "Restoring cloud saves for {game}...", "Restaurando las partidas de la nube de {game}...";
    sync_syncing_target: "Syncing {name}...", "Sincronizando {name}...";
    sync_checking_folder: "Checking cloud folder for {title}...", "Comprobando la carpeta en la nube de {title}...";
    sync_renaming_folder: "Renaming cloud folder to {title}...", "Renombrando la carpeta en la nube a {title}...";
    sync_checking_files: "Checking cloud files in {title}...", "Comprobando los archivos en la nube de {title}...";
    sync_moving_saves: "Moving cloud saves for {name}...", "Moviendo las partidas en la nube de {name}...";
    sync_failed: "Error: {detail}", "Error: {detail}";
    sync_game_not_tracked: "{game} did not start or its process could not be tracked. Cloud upload was not attempted.", "{game} no se inició o no se pudo seguir su proceso. No se intentó subir nada a la nube.";
    sync_category_name_invalid: "Choose a non-empty category name that is not already in use.", "Elige un nombre de categoría que no esté vacío ni en uso.";
    sync_drive_auth_failed: "Drive authentication failed: {detail}", "Falló la autenticación con Drive: {detail}";
    sync_cloud_rename_failed: "Cloud rename error: {detail}", "Error al renombrar en la nube: {detail}";
    sync_shared_cloud_folder: "The cloud folder '{folder}' is shared with another entry. Keep the existing cloud folder when editing this entry to protect their saves.", "Otra categoría comparte la carpeta en la nube '{folder}'. Conserva la carpeta en la nube actual al editar esta categoría para proteger sus partidas.";
    sync_drive_not_connected: "Google Drive is not connected. Click 'Connect Drive' in the top bar to sign in.", "Google Drive no está conectado. Pulsa 'Conectar Drive' en la barra superior para iniciar sesión.";
    sync_drive_connect_failed: "Could not connect to Google Drive ({detail}). Please reconnect your account by clicking 'Connect Drive'.", "No se pudo conectar con Google Drive ({detail}). Vuelve a conectar tu cuenta con 'Conectar Drive'.";
    sync_save_name_taken: "The save name '{name}' is already used by another configured file target.", "El nombre de partida '{name}' ya lo usa otro archivo configurado.";
    sync_directory_save_conflict: "The directory save '{path}' conflicts with another configured file target. Rename or reconfigure one of the saves before syncing it.", "La partida de carpeta '{path}' choca con otro archivo configurado. Renombra o reconfigura una de las partidas antes de sincronizarla.";
    sync_unknown_drive_error: "Unknown Drive error", "Error desconocido de Drive";
    sync_invalid_cloud_path: "Invalid cloud path '{path}'. Use a relative path without '..' or absolute components.", "Ruta en la nube no válida: '{path}'. Usa una ruta relativa sin '..' ni partes absolutas.";
    sync_conflicting_local_destinations: "Different cloud paths cannot restore to the same local save path. Choose separate destinations.", "Rutas distintas en la nube no pueden restaurarse en la misma ruta local. Elige destinos separados.";
    sync_file_is_parent: "A save file cannot also be a parent directory for another save. Choose separate destinations.", "Un archivo de partida no puede ser a la vez la carpeta de otra partida. Elige destinos separados.";
    sync_overlapping_scopes: "Each save directory needs a separate cloud path. Overlapping paths would mix their saves.", "Cada carpeta de partida necesita su propia ruta en la nube. Si se solapan, sus partidas se mezclarían.";
    sync_conflicting_remote_paths: "Cloud files resolve to conflicting local paths. Choose separate destinations before restoring them.", "Los archivos de la nube apuntan a rutas locales que chocan. Elige destinos separados antes de restaurarlos.";
    sync_unplaced_legacy: "Cannot safely match cloud saves to these older save folders.\n\n{folders}\n\n{files}\n\nSync this game from the library to choose the folder for each cloud file. These folders were not synchronized.", "No se pueden emparejar con seguridad las partidas de la nube con estas carpetas antiguas.\n\n{folders}\n\n{files}\n\nSincroniza este juego desde la biblioteca para elegir la carpeta de cada archivo de la nube. Estas carpetas no se sincronizaron.";
    sync_configured_folders: "Configured save folders", "Carpetas de partida configuradas";
    sync_unmatched_cloud_files: "Cloud files with no matching local save", "Archivos de la nube sin partida local";
    sync_more_items: "… ({count} more)", "… ({count} más)";
    sync_local_access_failed: "Local access error: {detail}", "Error de acceso local: {detail}";

    // Desktop messages
    msg_updates_portable: "This portable copy does not update itself. Download the latest portable ZIP from the project's releases and replace the folder.", "Esta copia portable no se actualiza sola. Descarga el ZIP portable más reciente de las versiones del proyecto y sustituye la carpeta.";
    msg_updates_disabled: "Updates are disabled in development builds. Set EMUSYNC_UPDATE_BASE_URL to a test release server.", "Las actualizaciones están desactivadas en las compilaciones de desarrollo. Define EMUSYNC_UPDATE_BASE_URL con un servidor de versiones de prueba.";
    msg_language_not_saved: "Could not save the language.", "No se pudo guardar el idioma.";
    msg_up_to_date: "EmuSync is up to date.", "EmuSync está al día.";
    msg_finish_before_update: "Finish the current operation before restarting to update.", "Termina la operación en curso antes de reiniciar para actualizar.";
    msg_pending_recovery: "Pending recovery of {count} unconfirmed upload(s).", "Recuperación pendiente de {count} subida(s) sin confirmar.";
    msg_found_recovery_records: "Found {count} pending sync recovery record(s).", "Se encontraron {count} registro(s) de recuperación de sincronización pendientes.";
    msg_config_load_failed: "Failed to load config: {error}", "No se pudo cargar la configuración: {error}";
    msg_busy_launch_game: "Cannot launch game: another operation is in progress or requires recovery.", "No se puede abrir el juego: hay otra operación en curso o pendiente de recuperación.";
    msg_busy_launch_emulator: "Cannot launch emulator: another operation is in progress.", "No se puede abrir el emulador: hay otra operación en curso.";
    msg_busy_sync: "Cannot synchronize: another operation is in progress or requires recovery.", "No se puede sincronizar: hay otra operación en curso o pendiente de recuperación.";
    msg_busy_add_category: "Cannot add category: another operation is in progress or requires recovery.", "No se puede añadir la categoría: hay otra operación en curso o pendiente de recuperación.";
    msg_busy_edit_category: "Cannot edit category: another operation is in progress or requires recovery.", "No se puede editar la categoría: hay otra operación en curso o pendiente de recuperación.";
    msg_busy_delete_category: "Cannot delete category: another operation is in progress.", "No se puede eliminar la categoría: hay otra operación en curso.";
    msg_busy_reorder: "Cannot reorder categories: another operation is in progress.", "No se pueden reordenar las categorías: hay otra operación en curso.";
    msg_busy_edit_game: "Cannot edit game: another operation is in progress or requires recovery.", "No se puede editar el juego: hay otra operación en curso o pendiente de recuperación.";
    msg_no_restore_location: "No valid restore location for the cloud files in that folder.", "No hay un destino de restauración válido para los archivos de la nube en esa carpeta.";
    msg_busy_toggle_steam: "Cannot toggle Steam: another operation is in progress or requires recovery.", "No se puede cambiar Steam: hay otra operación en curso o pendiente de recuperación.";
    msg_busy_retry: "Cannot retry sync: operation in progress or recovery required.", "No se puede reintentar la sincronización: hay una operación en curso o pendiente de recuperación.";
    msg_category_missing: "Category no longer exists.", "La categoría ya no existe.";
    msg_game_setup_failed: "Failed to save game setup: {error}", "No se pudo guardar la configuración del juego: {error}";
    msg_syncing_file_percent: "Syncing {name} ({percent}%)", "Sincronizando {name} ({percent} %)";
    msg_syncing_file: "Syncing {name}", "Sincronizando {name}";
    msg_http_config_failed: "HTTP client configuration failed: {error}", "Falló la configuración del cliente HTTP: {error}";
    msg_retry_failed: "Sync failed during retry: {detail}. Recovery is required.", "La sincronización falló al reintentar: {detail}. Hace falta una recuperación.";
    msg_busy_connect_drive: "Cannot connect Drive: another operation is in progress.", "No se puede conectar Drive: hay otra operación en curso.";
    msg_credentials_empty: "Client ID and Client Secret cannot be empty.", "El Client ID y el Client Secret no pueden estar vacíos.";
    msg_credentials_not_saved: "Failed to save credentials: {error}", "No se pudieron guardar las credenciales: {error}";
    msg_opening_browser: "Opening browser for Google Drive authorization...", "Abriendo el navegador para autorizar Google Drive...";
    msg_http_client_error: "HTTP client error: {error}", "Error del cliente HTTP: {error}";
    msg_drive_connected: "Google Drive connected successfully.", "Google Drive se conectó correctamente.";
    msg_drive_authorization_failed: "Google Drive authorization failed: {error}", "Falló la autorización de Google Drive: {error}";
    msg_busy_disconnect_drive: "Cannot disconnect Drive: an operation is currently active.", "No se puede desconectar Drive: hay una operación en curso.";
    msg_drive_disconnected: "Google Drive disconnected.", "Google Drive se desconectó.";
    msg_drive_disconnect_failed: "Failed to disconnect Drive: {error}", "No se pudo desconectar Drive: {error}";

    // Library messages
    msg_category_name_unique: "Choose a unique, non-empty category name.", "Elige un nombre de categoría único y no vacío.";
    msg_category_added: "Category added successfully.", "Categoría añadida correctamente.";
    msg_category_add_failed: "Failed to add category: {error}", "No se pudo añadir la categoría: {error}";
    msg_category_name_empty: "Category name cannot be empty.", "El nombre de la categoría no puede estar vacío.";
    msg_category_name_taken: "Category name is already in use.", "Ese nombre de categoría ya está en uso.";
    msg_category_updated: "Category updated.", "Categoría actualizada.";
    msg_category_update_failed: "Failed to update category: {error}", "No se pudo actualizar la categoría: {error}";
    msg_migrating_folder: "Migrating cloud folder from '{old}' to '{new}'...", "Moviendo la carpeta en la nube de '{old}' a '{new}'...";
    msg_migration_failed: "Cloud migration failed: {error}", "Falló el traslado en la nube: {error}";
    msg_migration_failed_transfer: "Cloud migration failed during transfer.", "El traslado en la nube falló durante la transferencia.";
    msg_category_renamed: "Category updated and cloud folders renamed.", "Categoría actualizada y carpetas en la nube renombradas.";
    msg_remote_renamed_local_failed: "Remote cloud folder was renamed to '{new}', but saving local configuration failed: {error}", "La carpeta en la nube se renombró a '{new}', pero no se pudo guardar la configuración local: {error}";
    msg_renamed_save_failed: "Cloud folder was renamed, but local configuration save failed: {error}. Recovery is required.", "La carpeta en la nube se renombró, pero no se pudo guardar la configuración local: {error}. Hace falta una recuperación.";
    msg_migration_failed_unchanged: "Cloud migration failed. Category was not modified.", "Falló el traslado en la nube. La categoría no se modificó.";
    msg_migration_error_unchanged: "Cloud migration failed: {error}. Category was not modified.", "Falló el traslado en la nube: {error}. La categoría no se modificó.";
    msg_steam_unregister_failed: "Failed to unregister from Steam: {error}", "No se pudo quitar de Steam: {error}";
    msg_category_deleted: "Category '{name}' deleted.", "Categoría '{name}' eliminada.";
    msg_category_delete_failed: "Failed to delete category: {error}", "No se pudo eliminar la categoría: {error}";
    msg_reorder_failed: "Failed to reorder: {error}", "No se pudo reordenar: {error}";
    msg_overrides_saved: "Game overrides saved.", "Ajustes del juego guardados.";
    msg_overrides_failed: "Failed to save overrides: {error}", "No se pudieron guardar los ajustes del juego: {error}";
    msg_overrides_renamed: "Game overrides saved and cloud folder renamed.", "Ajustes del juego guardados y carpeta en la nube renombrada.";
    msg_game_remote_renamed_local_failed: "Remote cloud folder for game was renamed to '{new}', but saving local configuration failed: {error}", "La carpeta en la nube del juego se renombró a '{new}', pero no se pudo guardar la configuración local: {error}";
    msg_game_migration_failed_unchanged: "Cloud folder migration failed. Game overrides were not saved.", "Falló el traslado de la carpeta en la nube. Los ajustes del juego no se guardaron.";
    msg_game_migration_error_unchanged: "Cloud folder migration failed: {error}. Game overrides were not saved.", "Falló el traslado de la carpeta en la nube: {error}. Los ajustes del juego no se guardaron.";

    // Game and sync messages
    msg_configuration_load_failed: "Failed to load configuration: {error}", "No se pudo cargar la configuración: {error}";
    msg_game_category_missing: "Game category no longer exists in configuration.", "La categoría del juego ya no existe en la configuración.";
    msg_targets_failed: "Failed to resolve save targets: {error}", "No se pudieron resolver las rutas de partida: {error}";
    msg_preparing_game: "Preparing {game}...", "Preparando {game}...";
    msg_game_exit_code: "{game} did not start or exited with error code {code}.", "{game} no se inició o terminó con el código de error {code}.";
    msg_session_finished: "Session finished ({seconds}s)", "Sesión terminada ({seconds} s)";
    msg_launch_failed: "Launch failed: {error}", "No se pudo abrir: {error}";
    msg_sync_init_failed: "Failed to initialize cloud synchronization: {error}", "No se pudo iniciar la sincronización con la nube: {error}";
    msg_sync_failed_session: "Sync failed during game session.", "La sincronización falló durante la partida.";
    msg_sync_error: "Synchronization error: {error}", "Error de sincronización: {error}";
    msg_drive_not_configured: "Google Drive is not configured.", "Google Drive no está configurado.";
    msg_synchronizing_entry: "Synchronizing '{name}'...", "Sincronizando '{name}'...";
    msg_sync_failed_transfer: "Sync failed during transfer: {detail}. Recovery is required.", "La sincronización falló durante la transferencia: {detail}. Hace falta una recuperación.";
    msg_entry_synchronized: "'{name}' synchronized.", "'{name}' sincronizado.";
    msg_sync_failed: "Sync failed: {reason}", "La sincronización falló: {reason}";
    msg_no_emulator: "No emulator executable configured.", "No hay ningún ejecutable de emulador configurado.";
    msg_emulator_launch_failed: "Failed to launch emulator: {error}", "No se pudo abrir el emulador: {error}";

    // Platform messages
    msg_steam_added: "Added '{name}' to Steam.", "'{name}' añadido a Steam.";
    msg_steam_removed: "Removed '{name}' from Steam.", "'{name}' quitado de Steam.";
    msg_steam_failed: "Steam operation failed: {error}", "Falló la operación de Steam: {error}";
    select_path: "Select path", "Seleccionar ruta";
    msg_file_chooser_failed: "Could not open file chooser: {error}", "No se pudo abrir el selector de archivos: {error}";
    msg_save_paths_unreadable: "Could not read save paths: {error}", "No se pudieron leer las rutas de partida: {error}";
    msg_history_open_failed: "Failed to open sync history: {error}", "No se pudo abrir el historial de sincronización: {error}";
    msg_recovery_open_failed: "Failed to open recovery store: {error}", "No se pudo abrir el almacén de recuperación: {error}";
    msg_credentials_snapshot_failed: "Failed to snapshot credentials: {error}", "No se pudo leer el estado de las credenciales: {error}";
    msg_recovery_before_close: "A sync operation needs recovery before closing.", "Una sincronización necesita recuperarse antes de cerrar.";
}
