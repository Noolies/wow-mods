//! IPC commands called from the frontend. Thin wrappers over `ops`, with
//! blocking filesystem work moved off the async runtime.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::backup::{self, BackupInfo, BackupReason};
use crate::error::{AppError, CmdResult};
use crate::games::wow::detect::{self, path_key};
use crate::games::wow::flavor::Flavor;
use crate::games::WOW;
use crate::model::*;
use crate::ops::{self, PackPreview, PlayOutcome};
use crate::providers::{self, catalog, SearchScope};
use crate::secrets::SecretKind;
use crate::state::AppState;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::msg(format!("A background task stopped unexpectedly: {e}")))?
        .map_err(AppError::from)
}

// ---------------------------------------------------------------- app

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationView {
    #[serde(flatten)]
    pub installation: Installation,
    pub flavor_name: &'static str,
    pub available: bool,
    /// Installed game version from Blizzard's files, e.g. `1.60.1.70888`.
    pub client_version: Option<String>,
    /// The game's own icon (a data URL read from its executable).
    pub icon: Option<String>,
}

fn installation_view(state: &AppState, installation: Installation) -> InstallationView {
    InstallationView {
        icon: state.client_icon(&installation.path),
        flavor_name: installation.flavor.info().name,
        available: installation.path.is_dir(),
        client_version: detect::client_info(&installation.path).version,
        installation,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatus {
    pub github_token: bool,
    pub curseforge_key: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub app_version: String,
    pub settings: Settings,
    pub installations: Vec<InstallationView>,
    pub secrets: SecretStatus,
    pub updater_configured: bool,
    pub data_dir: String,
    pub platform: &'static str,
    pub catalog_version: u32,
}

fn secret_status(state: &AppState) -> SecretStatus {
    SecretStatus {
        github_token: state.secret(SecretKind::GithubToken).is_some(),
        curseforge_key: state.secret(SecretKind::CurseforgeKey).is_some(),
    }
}

fn updater_configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .map(|k| k.len() > 40 && !k.contains("NOT_CONFIGURED"))
        .unwrap_or(false)
}

#[tauri::command]
pub fn bootstrap(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Bootstrap> {
    log::info!("Interface connected");
    Ok(Bootstrap {
        app_version: state.app_version.clone(),
        settings: state.settings(),
        installations: state.installations().into_iter().map(|i| installation_view(&state, i)).collect(),
        secrets: secret_status(&state),
        updater_configured: updater_configured(&app),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        platform: std::env::consts::OS,
        catalog_version: state.catalog.lock().unwrap().version,
    })
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let previous = state.settings();
    if previous.start_with_system != settings.start_with_system {
        #[cfg(desktop)]
        {
            use tauri_plugin_autostart::ManagerExt;
            let autolaunch = app.autolaunch();
            let result = if settings.start_with_system { autolaunch.enable() } else { autolaunch.disable() };
            result.map_err(|e| AppError::msg(format!("Couldn't change the start-up setting: {e}")))?;
        }
    }
    let mut settings = settings;
    settings.check_interval_hours = settings.check_interval_hours.clamp(1, 48);
    settings.backup_retention = settings.backup_retention.clamp(1, 100);
    let saved = state.mutate(|store| {
        store.data.settings = settings.clone();
        Ok(settings)
    })?;
    crate::tray::refresh(&app);
    Ok(saved)
}

#[tauri::command]
pub fn set_secret(state: State<'_, AppState>, kind: SecretKind, value: Option<String>) -> CmdResult<SecretStatus> {
    state.set_secret(kind, value.as_deref())?;
    Ok(secret_status(&state))
}

#[tauri::command]
pub async fn refresh_catalog(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(catalog::refresh(&state).await?)
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum FolderTarget {
    Addons,
    Wtf,
    Client,
    Backups,
    Logs,
    Data,
}

#[tauri::command]
pub fn open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    target: FolderTarget,
    installation_id: Option<String>,
) -> CmdResult<()> {
    let installation = || -> CmdResult<Installation> {
        let id = installation_id.as_deref().ok_or_else(|| AppError::msg("Pick a game first."))?;
        Ok(state.installation(id)?)
    };
    let path: PathBuf = match target {
        FolderTarget::Addons => installation()?.addons_dir(),
        FolderTarget::Wtf => installation()?.wtf_dir(),
        FolderTarget::Client => installation()?.path,
        FolderTarget::Backups => {
            let inst = installation()?;
            backup::backup_dir(&state.settings(), &state.data_dir, &inst)
        }
        FolderTarget::Logs => app.path().app_log_dir().map_err(|e| AppError::msg(e.to_string()))?,
        FolderTarget::Data => state.data_dir.clone(),
    };
    std::fs::create_dir_all(&path).ok();
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::msg(format!("Couldn't open the folder: {e}")))
}

#[tauri::command]
pub fn reveal_addon(state: State<'_, AppState>, app: AppHandle, installation_id: String, folder: String) -> CmdResult<()> {
    if folder.contains(['/', '\\']) || folder.contains("..") {
        return Err(AppError::msg("That isn't an addon folder."));
    }
    let path = state.installation(&installation_id)?.addons_dir().join(folder);
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| AppError::msg(format!("Couldn't show the folder: {e}")))
}

// ---------------------------------------------------------------- games

#[tauri::command]
pub async fn detect_installations(state: State<'_, AppState>) -> CmdResult<Vec<DetectedInstall>> {
    let known: Vec<String> =
        state.installations().iter().map(|i| path_key(&i.path)).collect();
    let found = blocking(|| Ok(detect::detect_all())).await?;
    Ok(found
        .into_iter()
        .map(|c| DetectedInstall {
            already_added: known.contains(&path_key(&c.path)),
            flavor: c.flavor,
            label: c.label,
            icon: state.client_icon(&c.path),
            path: c.path,
            addon_count: c.addon_count,
            version: c.version,
        })
        .collect())
}

#[tauri::command]
pub async fn inspect_folder(state: State<'_, AppState>, path: String) -> CmdResult<Vec<DetectedInstall>> {
    let known: Vec<String> =
        state.installations().iter().map(|i| path_key(&i.path)).collect();
    let found = blocking(move || detect::resolve_user_path(&PathBuf::from(path))).await?;
    Ok(found
        .into_iter()
        .map(|c| DetectedInstall {
            already_added: known.contains(&path_key(&c.path)),
            flavor: c.flavor,
            label: c.label,
            icon: state.client_icon(&c.path),
            path: c.path,
            addon_count: c.addon_count,
            version: c.version,
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddInstallation {
    pub path: String,
    pub flavor: Flavor,
    pub label: String,
}

#[tauri::command]
pub fn add_installations(
    app: AppHandle,
    state: State<'_, AppState>,
    installations: Vec<AddInstallation>,
) -> CmdResult<Vec<InstallationView>> {
    let added = state.mutate(|store| {
        for request in installations {
            let path = detect::tidy(&PathBuf::from(&request.path));
            let exists = store.data.installations.iter().any(|i| path_key(&i.path) == path_key(&path));
            if exists || !path.is_dir() {
                continue;
            }
            store.data.installations.push(Installation {
                id: uuid::Uuid::new_v4().to_string(),
                game: WOW.to_string(),
                flavor: request.flavor,
                label: request.label,
                path,
                launch_method: LaunchMethod::Battlenet,
                default_channel: None,
                added_at: Some(chrono::Utc::now()),
            });
        }
        store.data.installations.sort_by_key(|i| (i.flavor != Flavor::Forever, i.label.clone()));
        if store.data.settings.last_installation.is_none() {
            store.data.settings.last_installation = store.data.installations.first().map(|i| i.id.clone());
        }
        Ok(store.data.installations.clone())
    })?;
    crate::tray::refresh(&app);
    Ok(added.into_iter().map(|i| installation_view(&state, i)).collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationPatch {
    pub label: Option<String>,
    pub launch_method: Option<LaunchMethod>,
    pub flavor: Option<Flavor>,
    /// `null` clears the override, missing leaves it alone.
    #[serde(default, with = "double_option")]
    pub default_channel: Option<Option<Channel>>,
}

#[tauri::command]
pub fn update_installation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: InstallationPatch,
) -> CmdResult<InstallationView> {
    let updated = state.mutate(|store| {
        let installation = store.installation_mut(&id)?;
        if let Some(label) = patch.label.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()) {
            installation.label = label;
        }
        if let Some(method) = patch.launch_method {
            installation.launch_method = method;
        }
        if let Some(flavor) = patch.flavor {
            installation.flavor = flavor;
        }
        if let Some(channel) = patch.default_channel {
            installation.default_channel = channel;
        }
        Ok(installation.clone())
    })?;
    state.updates.lock().unwrap().remove(&id);
    crate::tray::refresh(&app);
    Ok(installation_view(&state, updated))
}

#[tauri::command]
pub fn remove_installation(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.mutate(|store| {
        store.data.installations.retain(|i| i.id != id);
        store.data.addons.remove(&id);
        if store.data.settings.last_installation.as_deref() == Some(id.as_str()) {
            store.data.settings.last_installation = store.data.installations.first().map(|i| i.id.clone());
        }
        Ok(())
    })?;
    state.updates.lock().unwrap().remove(&id);
    crate::tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn select_installation(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.mutate(|store| {
        store.installation(&id)?;
        store.data.settings.last_installation = Some(id);
        Ok(())
    })?;
    crate::tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn game_running(state: State<'_, AppState>, installation_id: String) -> CmdResult<bool> {
    let installation = state.installation(&installation_id)?;
    Ok(state.is_game_running(&installation))
}

#[tauri::command]
pub async fn play(app: AppHandle, state: State<'_, AppState>, installation_id: String) -> CmdResult<PlayOutcome> {
    Ok(ops::play(&app, &state, &installation_id).await?)
}

// ---------------------------------------------------------------- addons

#[tauri::command]
pub async fn list_addons(app: AppHandle, installation_id: String) -> CmdResult<Vec<AddonView>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let installation = state.installation(&installation_id)?;
        ops::build_views(&state, &installation)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedUpdates {
    pub checks: Vec<UpdateCheck>,
    pub checking: bool,
}

#[tauri::command]
pub fn cached_updates(state: State<'_, AppState>, installation_id: String) -> CmdResult<CachedUpdates> {
    Ok(CachedUpdates {
        checks: state.update_checks(&installation_id).into_values().collect(),
        checking: state.checking.lock().unwrap().contains(&installation_id),
    })
}

#[tauri::command]
pub async fn check_updates(app: AppHandle, state: State<'_, AppState>, installation_id: String) -> CmdResult<Vec<UpdateCheck>> {
    Ok(ops::check_updates(&app, &state, &installation_id).await?)
}

#[tauri::command]
pub async fn install_addon(
    app: AppHandle,
    state: State<'_, AppState>,
    installation_id: String,
    source: SourceRef,
    channel: Option<Channel>,
    hint: Option<SourceHint>,
) -> CmdResult<AddonRecord> {
    let record = ops::install(&app, &state, &installation_id, source, channel, hint.unwrap_or_default()).await?;
    let _ = tauri::Emitter::emit(&app, "addons-changed", &installation_id);
    Ok(record)
}

#[tauri::command]
pub async fn update_addons(
    app: AppHandle,
    state: State<'_, AppState>,
    installation_id: String,
    addon_ids: Option<Vec<String>>,
) -> CmdResult<BatchResult> {
    let ids = addon_ids.unwrap_or_else(|| ops::pending_updates(&state, &installation_id, false));
    Ok(ops::update_addons(&app, &state, &installation_id, &ids).await?)
}

#[tauri::command]
pub async fn uninstall_addon(
    app: AppHandle,
    state: State<'_, AppState>,
    installation_id: String,
    addon_id: String,
) -> CmdResult<()> {
    Ok(ops::uninstall(&app, &state, &installation_id, &addon_id).await?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddonOptions {
    pub pinned: Option<bool>,
    #[serde(default, with = "double_option")]
    pub channel: Option<Option<Channel>>,
    #[serde(default, with = "double_option")]
    pub auto_update: Option<Option<bool>>,
}

#[tauri::command]
pub fn set_addon_options(
    state: State<'_, AppState>,
    installation_id: String,
    addon_id: String,
    options: AddonOptions,
) -> CmdResult<AddonRecord> {
    let record = state.mutate(|store| {
        let record = store.record_mut(&installation_id, &addon_id)?;
        if let Some(pinned) = options.pinned {
            record.pinned = pinned;
        }
        if let Some(channel) = options.channel {
            record.channel = channel;
        }
        if let Some(auto_update) = options.auto_update {
            record.auto_update = auto_update;
        }
        Ok(record.clone())
    })?;
    Ok(record)
}

#[tauri::command]
pub async fn link_addon(
    state: State<'_, AppState>,
    installation_id: String,
    addon_id: String,
    source: SourceRef,
) -> CmdResult<AddonRecord> {
    Ok(ops::link(&state, &installation_id, &addon_id, source).await?)
}

#[tauri::command]
pub async fn link_all(state: State<'_, AppState>, installation_id: String) -> CmdResult<usize> {
    Ok(ops::link_all(&state, &installation_id).await?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn search_addons(
    state: State<'_, AppState>,
    installation_id: String,
    query: String,
    scope: SearchScope,
) -> CmdResult<SearchResults> {
    let installation = state.installation(&installation_id)?;
    let outcome = providers::search(&state, &query, installation.flavor, scope).await;
    let records = state.records(&installation_id);
    let hits = outcome
        .hits
        .into_iter()
        .map(|mut hit| {
            hit.installed = records.iter().any(|r| r.source.same_as(&hit.source));
            hit
        })
        .collect();
    Ok(SearchResults { hits, warnings: outcome.warnings })
}

#[tauri::command]
pub async fn addon_details(
    state: State<'_, AppState>,
    installation_id: String,
    source: SourceRef,
) -> CmdResult<AddonDetails> {
    let installation = state.installation(&installation_id)?;
    Ok(providers::details(&state, &source, installation.flavor).await?)
}

#[tauri::command]
pub async fn install_from_url(
    app: AppHandle,
    state: State<'_, AppState>,
    installation_id: String,
    url: String,
) -> CmdResult<AddonRecord> {
    let record = ops::install_from_url(&app, &state, &installation_id, &url).await?;
    let _ = tauri::Emitter::emit(&app, "addons-changed", &installation_id);
    Ok(record)
}

#[tauri::command]
pub async fn install_from_file(
    app: AppHandle,
    state: State<'_, AppState>,
    installation_id: String,
    path: String,
) -> CmdResult<AddonRecord> {
    Ok(ops::install_from_file(&app, &state, &installation_id, &PathBuf::from(path)).await?)
}

// ---------------------------------------------------------------- sharing

#[tauri::command]
pub fn export_pack(
    state: State<'_, AppState>,
    installation_id: String,
    addon_ids: Option<Vec<String>>,
    name: String,
) -> CmdResult<String> {
    Ok(ops::export_pack(&state, &installation_id, addon_ids, &name)?)
}

#[tauri::command]
pub fn preview_pack(state: State<'_, AppState>, installation_id: String, code: String) -> CmdResult<PackPreview> {
    Ok(ops::preview_pack(&state, &installation_id, &code)?)
}

#[tauri::command]
pub async fn import_pack(
    app: AppHandle,
    state: State<'_, AppState>,
    installation_id: String,
    code: String,
) -> CmdResult<BatchResult> {
    Ok(ops::import_pack(&app, &state, &installation_id, &code).await?)
}

// ---------------------------------------------------------------- backups

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupList {
    pub folder: String,
    pub backups: Vec<BackupInfo>,
    pub has_settings: bool,
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>, installation_id: String) -> CmdResult<BackupList> {
    let installation = state.installation(&installation_id)?;
    let dir = backup::backup_dir(&state.settings(), &state.data_dir, &installation);
    let has_settings = installation.wtf_dir().is_dir();
    let folder = dir.to_string_lossy().into_owned();
    let backups = blocking(move || backup::list(&dir)).await?;
    Ok(BackupList { folder, backups, has_settings })
}

#[tauri::command]
pub async fn create_backup(state: State<'_, AppState>, installation_id: String) -> CmdResult<Option<BackupInfo>> {
    let installation = state.installation(&installation_id)?;
    let dir = backup::backup_dir(&state.settings(), &state.data_dir, &installation);
    blocking(move || backup::create(&dir, &installation, BackupReason::Manual)).await
}

#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    installation_id: String,
    backup_id: String,
) -> CmdResult<Option<BackupInfo>> {
    let installation = state.installation(&installation_id)?;
    if state.is_game_running(&installation) {
        return Err(AppError::msg(format!(
            "Close {} first: the game saves its settings when it exits and would overwrite the restore.",
            installation.label
        )));
    }
    let dir = backup::backup_dir(&state.settings(), &state.data_dir, &installation);
    blocking(move || backup::restore(&dir, &installation, &backup_id)).await
}

#[tauri::command]
pub async fn delete_backup(state: State<'_, AppState>, installation_id: String, backup_id: String) -> CmdResult<()> {
    let installation = state.installation(&installation_id)?;
    let dir = backup::backup_dir(&state.settings(), &state.data_dir, &installation);
    blocking(move || backup::delete(&dir, &backup_id)).await
}

/// Lets JSON distinguish "field missing" (leave alone) from `null` (clear).
mod double_option {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}
