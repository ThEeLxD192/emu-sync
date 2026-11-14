//! The interface language the desktop layer writes its messages in.

use emusync_domain::config::Language;
use emusync_presentation::i18n::{Texts, texts};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// The current interface language, shared by every clone. Messages are put into words
/// when they are sent, in the language current at that moment.
#[derive(Clone, Default)]
pub struct CurrentLanguage(Arc<AtomicUsize>);

impl CurrentLanguage {
    pub fn get(&self) -> Language {
        Language::ALL
            .get(self.0.load(Ordering::Relaxed))
            .copied()
            .unwrap_or_default()
    }

    pub fn set(&self, language: Language) {
        let index = Language::ALL
            .iter()
            .position(|known| *known == language)
            .unwrap_or_default();
        self.0.store(index, Ordering::Relaxed);
    }

    /// The texts of the current language.
    pub fn texts(&self) -> &'static Texts {
        texts(self.get())
    }
}
