use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::PathBuf};
use tauri::Manager;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: String,
    pub key_path: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub profiles: Vec<Profile>,
    pub hosts: BTreeMap<String, String>,
    pub appearance: serde_json::Value,
}
pub fn config_path(app: &tauri::AppHandle) -> Result<PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("settings.json"))
}
pub fn load(app: &tauri::AppHandle) -> Result<Store> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(Store::default());
    }
    let data = std::fs::read(&path)?;
    match serde_json::from_slice(&data) {
        Ok(store) => Ok(store),
        Err(error) => {
            let backup = path.with_extension("bak");
            if backup.exists() {
                Ok(serde_json::from_slice(&std::fs::read(backup)?)?)
            } else {
                Err(error.into())
            }
        }
    }
}
pub fn save(app: &tauri::AppHandle, store: &Store) -> Result<()> {
    // Write and sync a complete sibling, then atomically replace the destination.
    let path = config_path(app)?;
    let next = path.with_extension("next");
    let mut file = std::fs::File::create(&next)?;
    file.write_all(&serde_json::to_vec_pretty(store)?)?;
    file.sync_all()?;
    drop(file);
    let backup = path.with_extension("bak");
    if path.exists() {
        std::fs::copy(&path, &backup)?;
    }
    std::fs::rename(&next, &path)?;
    Ok(())
}
pub fn credential(id: &str, field: &str) -> Result<keyring::Entry> {
    uuid::Uuid::parse_str(id)?;
    Ok(keyring::Entry::new(
        "SSH Virtual Desktop",
        &format!("{id}:{field}"),
    )?)
}
