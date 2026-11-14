//! Why a form cannot be saved yet, shown in the interface language.

use crate::i18n::Texts;

/// The first problem that keeps a category or game form from being saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormError {
    /// A cloud path is absolute or climbs out with `..`.
    CloudPathInvalid,
    /// An excluded folder is not inside its save folder.
    ExcludedFolderOutside,
    /// The name or the executable path is blank.
    RequiredFieldsMissing,
    /// The arguments have unclosed quotes or escapes.
    ArgumentsInvalid,
    /// An emulator needs its ROMs directory and at least one extension.
    EmulatorFieldsMissing,
    /// A native game needs at least one save path.
    NativeSaveTargetMissing,
    /// A cloud restore needs a destination folder.
    DestinationMissing,
}

impl FormError {
    pub fn message(self, t: &Texts) -> &'static str {
        match self {
            Self::CloudPathInvalid => t.form_cloud_path_invalid,
            Self::ExcludedFolderOutside => t.form_excluded_folder_outside,
            Self::RequiredFieldsMissing => t.form_required_fields,
            Self::ArgumentsInvalid => t.form_arguments_invalid,
            Self::EmulatorFieldsMissing => t.form_emulator_fields,
            Self::NativeSaveTargetMissing => t.form_native_save_target,
            Self::DestinationMissing => t.select_destination_first,
        }
    }
}
