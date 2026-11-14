//! Interface texts in every language the interface can be shown in.
//!
//! Each text is written once per language, side by side in `texts.rs`, so a language
//! cannot miss one: the tables would not compile. The language in use travels with the
//! egui context, so any widget reads it from the `Ui` it is drawn in.

mod sync;
mod texts;

use egui::{Context, Id};
pub use emusync_domain::config::Language;
pub use sync::sync_text;
pub use texts::Texts;

/// The texts of `language`.
pub fn texts(language: Language) -> &'static Texts {
    match language {
        Language::English => &texts::EN,
        Language::Spanish => &texts::ES,
    }
}

fn language_id() -> Id {
    Id::unique("emusync-language")
}

/// Makes `language` the one the following frames are drawn in.
pub fn set_language(ctx: &Context, language: Language) {
    ctx.data_mut(|data| data.insert_temp(language_id(), language));
}

/// The language the interface is drawn in; English until one is set.
pub fn language(ctx: &Context) -> Language {
    ctx.data(|data| data.get_temp(language_id()))
        .unwrap_or_default()
}

/// The texts of the language the interface is drawn in.
pub fn tr(ctx: &Context) -> &'static Texts {
    texts(language(ctx))
}

/// Replaces each `{name}` in `template` with its value.
pub fn fill(template: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut text = template.to_owned();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}
