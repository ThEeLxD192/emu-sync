use emusync_data_local::{ConfigManager, HistoryCheckpoint, SyncHistoryStore};
use emusync_domain::config::ConfigurationStore;
use std::{env, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mode = args.next().ok_or("missing mode")?;
    let dir = PathBuf::from(args.next().ok_or("missing directory")?);
    let path = dir.join("config.json");
    if mode == "write" {
        fs::create_dir_all(&dir)?;
        fs::write(
            &path,
            r#"{"googleDrive":{"clientId":"rust","clientSecret":"secret"},"entries":[{"type":"emulator","name":"Rust system","executablePath":"emu","romsDirectory":"roms","extensions":["gb"],"arguments":[],"titleByRom":{"first":"First","second":"Second"}}]}"#,
        )?;
        let manager = ConfigManager::new(&path)?;
        let config = manager.load()?;
        manager.save(&config)?;
        let history = SyncHistoryStore::new(&path)?;
        history.record(
            "rust",
            HistoryCheckpoint {
                local_sha256: "rust-hash".into(),
                remote_id: "remote".into(),
                remote_modified_time: "2026-10-01T12:00:00Z".into(),
                remote_sha256: None,
                remote_version: None,
            },
        )?;
    } else if mode == "read" {
        let config = ConfigManager::new(&path)?.load()?;
        assert_eq!(config.credentials.unwrap().client_id, "kotlin");
        assert_eq!(config.entries.len(), 2);
        assert_eq!(config.entries[0].common().arguments, Vec::<String>::new());
        let history = SyncHistoryStore::new(&path)?;
        assert_eq!(history.get("rust")?.unwrap().local_sha256, "rust-hash");
        assert_eq!(history.get("kotlin")?.unwrap().local_sha256, "kotlin-hash");
    } else {
        return Err("unknown mode".into());
    }
    Ok(())
}
