//! UI presentation models and status definitions.

use crate::theme::{ERROR, ON_SURFACE_DIM, PRIMARY, SUCCESS, WARNING};
use egui::Color32;
use emusync_domain::library::{Emulator, GameEntry, NativeGame, SaveTarget};
use std::path::{Path, PathBuf};

/// Cloud sync status for an entry/emulator system (Steam Cloud style).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CloudSyncStatus {
    #[default]
    Idle,
    Checking,
    InSync,
    OutOfSync,
    Syncing,
    Conflict,
    NotConfigured,
    Error,
}

impl CloudSyncStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Checking => "Checking...",
            Self::InSync => "Up to date",
            Self::OutOfSync => "Out of sync",
            Self::Syncing => "Syncing...",
            Self::Conflict => "Conflict",
            Self::NotConfigured => "Not configured",
            Self::Error => "Sync error",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::InSync => SUCCESS,
            Self::OutOfSync | Self::Conflict => WARNING,
            Self::Syncing | Self::Checking => PRIMARY,
            Self::Error => ERROR,
            Self::Idle | Self::NotConfigured => ON_SURFACE_DIM,
        }
    }
}

/// Lifecycle status for game launching, cloud sync and error presentation.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AppStatus {
    #[default]
    Idle,
    Loading,
    Syncing {
        message: String,
    },
    Playing,
    Error {
        message: String,
    },
    PostSyncFailed {
        game_name: String,
        reason: String,
        completed_save_count: usize,
        pending_save_count: usize,
    },
    Conflict {
        local_date: String,
        cloud_date: String,
        game_name: String,
    },
}

/// Represents a displayable game item in the grid, unifying scanned ROMs and native PC games.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameItem {
    pub name: String,
    pub entry: GameEntry,
    pub rom_file: Option<PathBuf>,
    pub cover_path: Option<String>,
}

impl GameItem {
    pub fn new(name: impl Into<String>, entry: GameEntry, rom_file: Option<PathBuf>) -> Self {
        Self {
            name: name.into(),
            entry,
            rom_file,
            cover_path: None,
        }
    }

    pub fn with_cover_path(mut self, cover_path: Option<String>) -> Self {
        self.cover_path = cover_path;
        self
    }

    pub fn effective_title(&self) -> String {
        match &self.entry {
            GameEntry::Emulator(emulator) => {
                let rom_name = self
                    .rom_file
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| self.name.clone());

                emulator
                    .title_by_rom
                    .iter()
                    .find(|(k, _)| k == &rom_name)
                    .map(|(_, v)| v.trim())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        Path::new(&rom_name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| self.name.clone())
                    })
            }
            GameEntry::Native(native) => native.common.name.clone(),
        }
    }

    /// Cover shown in the grid: the scanned cover (which already prefers the custom one),
    /// else the custom cover.
    pub fn effective_cover_path(&self) -> Option<String> {
        self.cover_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .or_else(|| self.custom_cover_path())
    }

    /// Only the cover the user chose for this game, never an auto-detected one
    /// (Kotlin's `GameItem.effectiveCoverPath`).
    pub fn custom_cover_path(&self) -> Option<String> {
        let custom = match &self.entry {
            GameEntry::Native(native) => native.cover_path.as_deref(),
            GameEntry::Emulator(emulator) => {
                let rom_name = self.rom_file_name()?;
                emulator
                    .cover_path_by_rom
                    .iter()
                    .find(|(k, _)| *k == rom_name)
                    .map(|(_, v)| v.as_str())
            }
        };
        custom
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
    }

    /// File name of the ROM, if this game comes from one.
    pub fn rom_file_name(&self) -> Option<String> {
        self.rom_file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().into_owned())
    }

    /// ROM file name without its extension, or the game name (Kotlin's
    /// `romFile?.nameWithoutExtension ?: name`).
    pub fn raw_name(&self) -> String {
        self.rom_file
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone())
    }

    pub fn is_emulator(&self) -> bool {
        matches!(self.entry, GameEntry::Emulator(_))
    }

    pub fn as_emulator(&self) -> Option<&Emulator> {
        match &self.entry {
            GameEntry::Emulator(e) => Some(e),
            GameEntry::Native(_) => None,
        }
    }

    pub fn as_native(&self) -> Option<&NativeGame> {
        match &self.entry {
            GameEntry::Native(n) => Some(n),
            GameEntry::Emulator(_) => None,
        }
    }
}

/// Request for setting up save targets before game launch or after cloud download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSetupRequest {
    pub item: GameItem,
    pub notice: Option<String>,
    pub cloud_file_names: Vec<String>,
    /// The game's own save targets, used to prefill the form.
    pub targets: Vec<SaveTarget>,
}

/// Form entry type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EntryType {
    #[default]
    Emulator,
    Native,
}

/// Target field being requested for the path picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathTarget {
    Executable,
    RomsDirectory,
    SavePath(usize),
    CoverPath,
    RestoreDestination,
}

/// Pick mode (file vs directory).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickMode {
    File,
    Directory,
}
