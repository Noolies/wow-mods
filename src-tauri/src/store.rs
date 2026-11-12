//! Persistent app state: settings, installations and tracked addons, kept in a
//! single JSON file in the app data folder. Writes are atomic (temp file + rename)
//! and the previous copy is kept as `state.json.bak`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{AddonRecord, Installation, Settings};

const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct StoreData {
    pub schema: u32,
    pub settings: Settings,
    pub installations: Vec<Installation>,
    /// Tracked addons keyed by installation id.
    pub addons: HashMap<String, Vec<AddonRecord>>,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            settings: Settings::default(),
            installations: Vec::new(),
            addons: HashMap::new(),
        }
    }
}

pub struct Store {
    pub data: StoreData,
    path: PathBuf,
}

impl Store {
    /// Loads the state file, falling back to the backup copy, then to defaults.
    /// A file that can't be parsed is set aside rather than overwritten.
    pub fn load(path: PathBuf) -> Store {
        let backup = backup_path(&path);
        let data = match read(&path) {
            Ok(Some(data)) => data,
            Ok(None) => StoreData::default(),
            Err(error) => {
                log::error!("State file unreadable ({error:#}); trying the backup copy");
                let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
                let _ = fs::rename(&path, path.with_extension(format!("corrupt-{stamp}.json")));
                match read(&backup) {
                    Ok(Some(data)) => data,
                    _ => StoreData::default(),
                }
            }
        };
        Store { data, path }
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("Couldn't create the app data folder")?;
        }
        let json = serde_json::to_vec_pretty(&self.data)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).context("Couldn't write the state file")?;
        if self.path.exists() {
            let _ = fs::copy(&self.path, backup_path(&self.path));
        }
        fs::rename(&tmp, &self.path).context("Couldn't replace the state file")?;
        Ok(())
    }

    pub fn installation(&self, id: &str) -> Result<&Installation> {
        self.data
            .installations
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow!("That game isn't set up any more. Add it again from Settings."))
    }

    pub fn installation_mut(&mut self, id: &str) -> Result<&mut Installation> {
        self.data
            .installations
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow!("That game isn't set up any more. Add it again from Settings."))
    }

    pub fn records(&self, installation_id: &str) -> &[AddonRecord] {
        self.data.addons.get(installation_id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn records_mut(&mut self, installation_id: &str) -> &mut Vec<AddonRecord> {
        self.data.addons.entry(installation_id.to_string()).or_default()
    }

    pub fn record_mut(&mut self, installation_id: &str, addon_id: &str) -> Result<&mut AddonRecord> {
        self.records_mut(installation_id)
            .iter_mut()
            .find(|r| r.id == addon_id)
            .ok_or_else(|| anyhow!("That addon isn't tracked any more. Refresh the list and try again."))
    }

    /// Inserts a record, replacing any record for the same source or with the same id.
    pub fn upsert_record(&mut self, installation_id: &str, record: AddonRecord) {
        let records = self.records_mut(installation_id);
        records.retain(|r| r.id != record.id && !r.source.same_as(&record.source));
        records.push(record);
        records.sort_by_key(|r| r.name.to_lowercase());
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn read(path: &Path) -> Result<Option<StoreData>> {
    match fs::read(path) {
        Ok(bytes) => {
            let data: StoreData = serde_json::from_slice(&bytes).context("Invalid state JSON")?;
            Ok(Some(data))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_recovers_from_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        let mut store = Store::load(path.clone());
        store.data.settings.check_interval_hours = 7;
        store.save().unwrap();
        store.data.settings.check_interval_hours = 9;
        store.save().unwrap(); // previous copy becomes the .bak

        fs::write(&path, b"{ not json").unwrap();
        let recovered = Store::load(path.clone());
        assert_eq!(recovered.data.settings.check_interval_hours, 7);
    }
}
