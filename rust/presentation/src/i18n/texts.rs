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
}
