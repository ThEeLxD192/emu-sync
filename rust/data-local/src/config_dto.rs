use emusync_domain::{
    config::{AppConfig, Credentials},
    library::{Emulator, EntryCommon, GameEntry, NativeGame, SaveTarget, SaveTargetType},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Document {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    google_drive: Option<Drive>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    entries: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Drive {
    client_id: String,
    client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Entry {
    Emulator(Box<EmulatorDto>),
    Native(Box<NativeDto>),
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Common {
    name: String,
    executable_path: String,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    save_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    save_targets: Vec<Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cloud_folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    drive_file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    steam_app_id: Option<i32>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    path: String,
    #[serde(rename = "type")]
    kind: TargetType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cloud_path: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TargetType {
    File,
    Directory,
    Unknown,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmulatorDto {
    #[serde(flatten)]
    common: Common,
    roms_directory: String,
    extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    save_paths_by_rom: IndexMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    save_targets_by_rom: IndexMap<String, Vec<Target>>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    cover_path_by_rom: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    title_by_rom: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    steam_process_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fullscreen_args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    covers_directory: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeDto {
    #[serde(flatten)]
    common: Common,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wait_for_process: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover_path: Option<String>,
}
impl From<Target> for SaveTarget {
    fn from(t: Target) -> Self {
        Self {
            path: t.path,
            kind: match t.kind {
                TargetType::File => SaveTargetType::File,
                TargetType::Directory => SaveTargetType::Directory,
                TargetType::Unknown => SaveTargetType::Unknown,
            },
            cloud_path: t.cloud_path,
        }
    }
}
impl From<SaveTarget> for Target {
    fn from(t: SaveTarget) -> Self {
        Self {
            path: t.path,
            kind: match t.kind {
                SaveTargetType::File => TargetType::File,
                SaveTargetType::Directory => TargetType::Directory,
                SaveTargetType::Unknown => TargetType::Unknown,
            },
            cloud_path: t.cloud_path,
        }
    }
}
impl From<Common> for EntryCommon {
    fn from(c: Common) -> Self {
        Self {
            name: c.name,
            executable_path: c.executable_path,
            arguments: c.arguments,
            save_paths: c.save_paths,
            save_targets: c.save_targets.into_iter().map(Into::into).collect(),
            group: c.group,
            cloud_folder: c.cloud_folder,
            drive_file_id: c.drive_file_id,
            steam_app_id: c.steam_app_id,
        }
    }
}
impl From<EntryCommon> for Common {
    fn from(c: EntryCommon) -> Self {
        Self {
            name: c.name,
            executable_path: c.executable_path,
            arguments: c.arguments,
            save_paths: c.save_paths,
            save_targets: c.save_targets.into_iter().map(Into::into).collect(),
            group: c.group,
            cloud_folder: c.cloud_folder,
            drive_file_id: c.drive_file_id,
            steam_app_id: c.steam_app_id,
        }
    }
}
impl From<Document> for AppConfig {
    fn from(d: Document) -> Self {
        Self {
            credentials: d.google_drive.map(|d| Credentials {
                client_id: d.client_id,
                client_secret: d.client_secret,
                refresh_token: d.refresh_token,
            }),
            entries: d
                .entries
                .into_iter()
                .map(|entry| match entry {
                    Entry::Native(n) => GameEntry::Native(NativeGame {
                        common: n.common.into(),
                        wait_for_process: n.wait_for_process,
                        cover_path: n.cover_path,
                    }),
                    Entry::Emulator(e) => GameEntry::Emulator(Emulator {
                        common: e.common.into(),
                        roms_directory: e.roms_directory,
                        extensions: e.extensions,
                        save_paths_by_rom: e.save_paths_by_rom.into_iter().collect(),
                        save_targets_by_rom: e
                            .save_targets_by_rom
                            .into_iter()
                            .map(|(k, v)| (k, v.into_iter().map(Into::into).collect()))
                            .collect(),
                        cover_path_by_rom: e.cover_path_by_rom.into_iter().collect(),
                        title_by_rom: e.title_by_rom.into_iter().collect(),
                        steam_process_name: e.steam_process_name,
                        fullscreen_args: e.fullscreen_args,
                        covers_directory: e.covers_directory,
                    }),
                })
                .collect(),
        }
    }
}
impl From<AppConfig> for Document {
    fn from(d: AppConfig) -> Self {
        Self {
            google_drive: d.credentials.map(|d| Drive {
                client_id: d.client_id,
                client_secret: d.client_secret,
                refresh_token: d.refresh_token,
            }),
            entries: d
                .entries
                .into_iter()
                .map(|entry| match entry {
                    GameEntry::Native(n) => Entry::Native(Box::new(NativeDto {
                        common: n.common.into(),
                        wait_for_process: n.wait_for_process,
                        cover_path: n.cover_path,
                    })),
                    GameEntry::Emulator(e) => Entry::Emulator(Box::new(EmulatorDto {
                        common: e.common.into(),
                        roms_directory: e.roms_directory,
                        extensions: e.extensions,
                        save_paths_by_rom: e.save_paths_by_rom.into_iter().collect(),
                        save_targets_by_rom: e
                            .save_targets_by_rom
                            .into_iter()
                            .map(|(k, v)| (k, v.into_iter().map(Into::into).collect()))
                            .collect(),
                        cover_path_by_rom: e.cover_path_by_rom.into_iter().collect(),
                        title_by_rom: e.title_by_rom.into_iter().collect(),
                        steam_process_name: e.steam_process_name,
                        fullscreen_args: e.fullscreen_args,
                        covers_directory: e.covers_directory,
                    })),
                })
                .collect(),
        }
    }
}
