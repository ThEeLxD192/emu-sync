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
    post_sync_message: "{game} closed normally. {progress}

{reason}", "{game} se cerró con normalidad. {progress}

{reason}";
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
}
