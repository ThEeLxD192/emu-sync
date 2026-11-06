use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VdfValue {
    String(String),
    Int(i32),
    Object(BTreeMap<String, VdfValue>),
}

/// Represents a single Non-Steam Game shortcut entry from shortcuts.vdf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamShortcut {
    pub app_id: i32,
    pub app_name: String,
    pub exe: String,
    pub start_dir: String,
    pub icon: String,
    pub shortcut_path: String,
    pub launch_options: String,
    pub is_hidden: bool,
    pub allow_desktop_config: bool,
    pub allow_overlay: bool,
    pub open_vr: bool,
    pub devkit: bool,
    pub devkit_game_id: String,
    pub devkit_override_app_id: i32,
    pub last_play_time: i32,
    pub flatpak_app_id: String,
    pub sort_as: String,
    pub tags: BTreeMap<String, String>,
    pub extra_fields: BTreeMap<String, VdfValue>,
}

impl Default for SteamShortcut {
    fn default() -> Self {
        Self {
            app_id: 0,
            app_name: String::new(),
            exe: String::new(),
            start_dir: String::new(),
            icon: String::new(),
            shortcut_path: String::new(),
            launch_options: String::new(),
            is_hidden: false,
            allow_desktop_config: true,
            allow_overlay: true,
            open_vr: false,
            devkit: false,
            devkit_game_id: String::new(),
            devkit_override_app_id: 0,
            last_play_time: 0,
            flatpak_app_id: String::new(),
            sort_as: String::new(),
            tags: BTreeMap::new(),
            extra_fields: BTreeMap::new(),
        }
    }
}
