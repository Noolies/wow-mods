//! Shared application state, managed by Tauri and passed to every command.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};

use crate::model::{AddonRecord, Installation, Settings, UpdateCheck};
use crate::providers::catalog::Catalog;
use crate::providers::tukui::TukuiAddon;
use crate::secrets::{self, SecretKind};
use crate::store::Store;

pub struct AppState {
    pub store: Mutex<Store>,
    pub http: reqwest::Client,
    pub data_dir: PathBuf,
    pub app_version: String,
    /// GitHub API responses keyed by URL: (ETag, body). 304 replies don't count against the rate limit.
    pub github_cache: Mutex<HashMap<String, (String, String)>>,
    /// Latest update-check results: installation id -> addon id -> result.
    pub updates: Mutex<HashMap<String, HashMap<String, UpdateCheck>>>,
    /// Installations with an update check in flight.
    pub checking: Mutex<HashSet<String>>,
    /// One lock per installation so installs never race on the same AddOns folder.
    fs_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    secrets: Mutex<HashMap<SecretKind, Option<String>>>,
    pub tukui_cache: Mutex<Option<(Instant, Vec<TukuiAddon>)>>,
    pub catalog: Mutex<Catalog>,
    /// Addon details panels, keyed by source and game version, with when they were fetched.
    pub details_cache: Mutex<HashMap<String, (Instant, crate::model::AddonDetails)>>,
    /// Game icons read from each client's executable, keyed by client folder.
    icons: Mutex<HashMap<String, Option<String>>>,
    /// Process list for "is the game running?" (None if the platform can't provide one).
    pub system: Mutex<Option<sysinfo::System>>,
}

impl AppState {
    pub fn init(app: &AppHandle) -> Result<Self> {
        // HEARTHFORGE_DATA_DIR keeps all state elsewhere: portable installs and test runs.
        let data_dir = match std::env::var_os("HEARTHFORGE_DATA_DIR").filter(|v| !v.is_empty()) {
            Some(dir) => PathBuf::from(dir),
            None => app.path().app_data_dir().context("No app data folder available")?,
        };
        std::fs::create_dir_all(&data_dir).context("Couldn't create the app data folder")?;
        let app_version = app.package_info().version.to_string();
        let store = Store::load(data_dir.join("state.json"));
        let catalog = crate::providers::catalog::load_cached(&data_dir);
        Ok(Self {
            store: Mutex::new(store),
            http: crate::http::build_client(&app_version)?,
            data_dir,
            app_version,
            github_cache: Mutex::new(HashMap::new()),
            updates: Mutex::new(HashMap::new()),
            checking: Mutex::new(HashSet::new()),
            fs_locks: Mutex::new(HashMap::new()),
            secrets: Mutex::new(HashMap::new()),
            tukui_cache: Mutex::new(None),
            catalog: Mutex::new(catalog),
            details_cache: Mutex::new(HashMap::new()),
            icons: Mutex::new(HashMap::new()),
            system: Mutex::new(sysinfo::System::new().ok()),
        })
    }

    /// A state with no Tauri app behind it, for tests.
    #[cfg(test)]
    pub fn for_tests(data_dir: &std::path::Path) -> Self {
        Self {
            store: Mutex::new(Store::load(data_dir.join("state.json"))),
            http: crate::http::build_client("test").unwrap(),
            data_dir: data_dir.to_path_buf(),
            app_version: "test".into(),
            github_cache: Mutex::new(HashMap::new()),
            updates: Mutex::new(HashMap::new()),
            checking: Mutex::new(HashSet::new()),
            fs_locks: Mutex::new(HashMap::new()),
            secrets: Mutex::new(HashMap::from([(SecretKind::GithubToken, None), (SecretKind::CurseforgeKey, None)])),
            tukui_cache: Mutex::new(None),
            catalog: Mutex::new(crate::providers::catalog::embedded()),
            details_cache: Mutex::new(HashMap::new()),
            icons: Mutex::new(HashMap::new()),
            system: Mutex::new(None),
        }
    }

    /// Runs `f` with the store locked, then saves it.
    pub fn mutate<R>(&self, f: impl FnOnce(&mut Store) -> Result<R>) -> Result<R> {
        let mut store = self.store.lock().unwrap();
        let result = f(&mut store)?;
        store.save()?;
        Ok(result)
    }

    pub fn settings(&self) -> Settings {
        self.store.lock().unwrap().data.settings.clone()
    }

    pub fn installations(&self) -> Vec<Installation> {
        self.store.lock().unwrap().data.installations.clone()
    }

    pub fn installation(&self, id: &str) -> Result<Installation> {
        Ok(self.store.lock().unwrap().installation(id)?.clone())
    }

    pub fn records(&self, installation_id: &str) -> Vec<AddonRecord> {
        self.store.lock().unwrap().records(installation_id).to_vec()
    }

    pub fn fs_lock(&self, installation_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.fs_locks
            .lock()
            .unwrap()
            .entry(installation_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    pub fn secret(&self, kind: SecretKind) -> Option<String> {
        self.secrets.lock().unwrap().entry(kind).or_insert_with(|| secrets::read(kind)).clone()
    }

    pub fn set_secret(&self, kind: SecretKind, value: Option<&str>) -> Result<()> {
        secrets::write(kind, value)?;
        let stored = value.map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
        self.secrets.lock().unwrap().insert(kind, stored);
        if kind == SecretKind::GithubToken {
            self.github_cache.lock().unwrap().clear();
        }
        Ok(())
    }

    pub fn update_checks(&self, installation_id: &str) -> HashMap<String, UpdateCheck> {
        self.updates.lock().unwrap().get(installation_id).cloned().unwrap_or_default()
    }

    /// The client's icon as a data URL, read once per folder and then cached.
    pub fn client_icon(&self, client_dir: &std::path::Path) -> Option<String> {
        let key = crate::games::wow::detect::path_key(client_dir);
        if let Some(cached) = self.icons.lock().unwrap().get(&key) {
            return cached.clone();
        }
        let icon = crate::games::wow::icon::client_icon_data_url(client_dir);
        self.icons.lock().unwrap().insert(key, icon.clone());
        icon
    }

    pub fn is_game_running(&self, installation: &Installation) -> bool {
        let mut system = self.system.lock().unwrap();
        match system.as_mut() {
            Some(system) => crate::games::wow::launch::is_running(system, installation),
            None => false,
        }
    }
}
