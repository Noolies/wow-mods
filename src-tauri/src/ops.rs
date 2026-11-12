//! High-level operations combining providers, the installer and the store.
//! Commands, the tray menu and the background scheduler all go through here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use futures::stream::{self, StreamExt};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::backup::{self, BackupReason};
use crate::games::wow::flavor::Flavor;
use crate::games::wow::launch;
use crate::games::wow::scan::{group_folders, missing_dependencies, scan_addons, scan_folder, ScannedFolder};
use crate::http;
use crate::install;
use crate::model::*;
use crate::providers::{self, catalog, curseforge, github};
use crate::secrets::SecretKind;
use crate::share::{self, Pack, PackAddon};
use crate::state::AppState;

fn emit_progress(app: &AppHandle, progress: TaskProgress) {
    let _ = app.emit("task-progress", progress);
}

fn progress(installation_id: &str, key: &str, label: &str, stage: TaskStage) -> TaskProgress {
    TaskProgress {
        installation_id: installation_id.to_string(),
        key: key.to_string(),
        label: label.to_string(),
        stage,
        received: 0,
        total: None,
        message: None,
    }
}

fn effective_channel(state: &AppState, installation: &Installation, channel: Option<Channel>) -> Channel {
    channel.or(installation.default_channel).unwrap_or_else(|| state.settings().default_channel)
}

// ---------------------------------------------------------------- scanning

/// The "Installed" list: tracked addons first-class, loose folders grouped.
pub fn build_views(state: &AppState, installation: &Installation) -> Result<Vec<AddonView>> {
    let info = installation.flavor.info();
    let client_interface = crate::games::wow::detect::client_info(&installation.path).interface();
    let folders = scan_addons(&installation.addons_dir(), info)?;
    let by_name: HashMap<String, &ScannedFolder> =
        folders.iter().map(|f| (f.name.to_ascii_lowercase(), f)).collect();
    let installed: HashSet<String> = by_name.keys().cloned().collect();
    let records = state.records(&installation.id);
    let updates = state.update_checks(&installation.id);

    let mut claimed: HashSet<String> = HashSet::new();
    let mut views: Vec<AddonView> = Vec::new();

    for record in &records {
        let members: Vec<&ScannedFolder> = record
            .folders
            .iter()
            .filter_map(|f| by_name.get(&f.to_ascii_lowercase()).copied())
            .collect();
        let missing_folders: Vec<String> = record
            .folders
            .iter()
            .filter(|f| !installed.contains(&f.to_ascii_lowercase()))
            .cloned()
            .collect();
        claimed.extend(record.folders.iter().map(|f| f.to_ascii_lowercase()));
        let main_toc = members.first().and_then(|m| m.toc.as_ref());
        views.push(AddonView {
            id: record.id.clone(),
            managed: true,
            name: record.name.clone(),
            version: Some(record.installed_version.clone())
                .filter(|v| !v.is_empty())
                .or_else(|| main_toc.and_then(|t| t.version.clone())),
            author: record.author.clone().or_else(|| main_toc.and_then(|t| t.author.clone())),
            notes: record.summary.clone().or_else(|| main_toc.and_then(|t| t.notes.clone())),
            folders: members.iter().map(|f| f.to_view(info, client_interface)).collect(),
            source: Some(record.source.clone()),
            channel: record.channel,
            pinned: record.pinned,
            auto_update: record.auto_update,
            website: record.website.clone(),
            thumbnail: record.thumbnail.clone(),
            installed_at: Some(record.installed_at),
            release_date: record.release_date,
            compatibility: members.first().map(|m| m.compatibility(info, client_interface)).unwrap_or(Compatibility::Missing),
            missing_dependencies: missing_dependencies(&members, &installed),
            missing_folders,
            suggested_source: None,
            update: updates.get(&record.id).cloned(),
        });
    }

    let loose: Vec<&ScannedFolder> =
        folders.iter().filter(|f| !claimed.contains(&f.name.to_ascii_lowercase())).collect();
    for group in group_folders(&loose) {
        let members: Vec<&ScannedFolder> =
            group.iter().filter_map(|n| by_name.get(&n.to_ascii_lowercase()).copied()).collect();
        let Some(main) = members.first() else { continue };
        let toc = main.toc.as_ref();
        views.push(AddonView {
            id: format!("local:{}", main.name),
            managed: false,
            name: toc.and_then(|t| t.title.clone()).unwrap_or_else(|| main.name.clone()),
            version: toc.and_then(|t| t.version.clone()),
            author: toc.and_then(|t| t.author.clone()),
            notes: toc.and_then(|t| t.notes.clone()),
            folders: members.iter().map(|f| f.to_view(info, client_interface)).collect(),
            source: None,
            channel: None,
            pinned: false,
            auto_update: None,
            website: toc.and_then(|t| t.extra("x-website").map(str::to_string)),
            thumbnail: None,
            installed_at: None,
            release_date: None,
            compatibility: main.compatibility(info, client_interface),
            missing_dependencies: missing_dependencies(&members, &installed),
            missing_folders: Vec::new(),
            suggested_source: suggest_source(state, &members),
            update: None,
        });
    }

    views.sort_by_key(|v| v.name.to_lowercase());
    Ok(views)
}

/// Where an untracked addon probably comes from: the curated list, then TOC metadata.
fn suggest_source(state: &AppState, members: &[&ScannedFolder]) -> Option<SourceRef> {
    for member in members {
        if let Some(entry) = catalog::source_for_folder(state, &member.name) {
            return Some(entry.source);
        }
    }
    let toc = members.first()?.toc.as_ref()?;
    for key in ["x-github", "x-repository", "x-source", "x-website", "x-url", "x-homepage", "x-git"] {
        if let Some(value) = toc.extra(key) {
            if value.contains("github.com") {
                if let Some(repo) = github::parse_repo(value) {
                    return Some(SourceRef::new(ProviderId::Github, repo));
                }
            }
        }
    }
    if state.secret(SecretKind::CurseforgeKey).is_some() {
        if let Some(id) = toc.extra("x-curse-project-id").filter(|id| id.parse::<u64>().is_ok()) {
            return Some(SourceRef::new(ProviderId::Curseforge, id));
        }
    }
    None
}

// ---------------------------------------------------------------- installing

/// Installs (or reinstalls) an addon from a source, then any required
/// dependencies the provider reports.
pub async fn install(
    app: &AppHandle,
    state: &AppState,
    installation_id: &str,
    source: SourceRef,
    channel: Option<Channel>,
    hint: SourceHint,
) -> Result<AddonRecord> {
    let (record, dependencies) = install_one(app, state, installation_id, source, channel, hint).await?;
    for dependency in dependencies {
        let tracked = state.records(installation_id).iter().any(|r| r.source.same_as(&dependency));
        if !tracked {
            if let Err(error) =
                install_one(app, state, installation_id, dependency.clone(), None, SourceHint::default()).await
            {
                log::warn!("Couldn't install dependency {}: {error:#}", dependency.project_id);
            }
        }
    }
    Ok(record)
}

async fn install_one(
    app: &AppHandle,
    state: &AppState,
    installation_id: &str,
    source: SourceRef,
    channel: Option<Channel>,
    hint: SourceHint,
) -> Result<(AddonRecord, Vec<SourceRef>)> {
    let installation = state.installation(installation_id)?;
    let existing = state.records(installation_id).into_iter().find(|r| r.source.same_as(&source));
    let channel = channel.or(existing.as_ref().and_then(|r| r.channel));
    let key = existing.as_ref().map(|r| r.id.clone()).unwrap_or_else(|| source.key());
    let label = existing
        .as_ref()
        .map(|r| r.name.clone())
        .or_else(|| hint.name.clone())
        .unwrap_or_else(|| source.project_id.clone());

    emit_progress(app, progress(installation_id, &key, &label, TaskStage::Resolving));
    let release = match providers::resolve(state, &source, installation.flavor, effective_channel(state, &installation, channel)).await {
        Ok(release) => release,
        Err(error) => {
            let mut failed = progress(installation_id, &key, &label, TaskStage::Failed);
            failed.message = Some(format!("{error:#}"));
            emit_progress(app, failed);
            return Err(error);
        }
    };
    let dependencies = release.dependencies.clone();
    let record = install_release(app, state, &installation, release, existing, channel, hint, &key, &label).await?;
    Ok((record, dependencies))
}

#[allow(clippy::too_many_arguments)]
async fn install_release(
    app: &AppHandle,
    state: &AppState,
    installation: &Installation,
    release: RemoteRelease,
    existing: Option<AddonRecord>,
    channel: Option<Channel>,
    hint: SourceHint,
    key: &str,
    label: &str,
) -> Result<AddonRecord> {
    let fail = |error: anyhow::Error| {
        let mut failed = progress(&installation.id, key, label, TaskStage::Failed);
        failed.message = Some(format!("{error:#}"));
        emit_progress(app, failed);
        error
    };

    let downloads = state.data_dir.join("downloads");
    std::fs::create_dir_all(&downloads).context("Couldn't create the downloads folder").map_err(&fail)?;
    let zip_path = downloads.join(format!("{}.zip", uuid::Uuid::new_v4().simple()));

    emit_progress(app, progress(&installation.id, key, label, TaskStage::Downloading));
    let downloaded = http::download(&state.http, &release.download_url, &zip_path, Default::default(), |received, total| {
        let mut p = progress(&installation.id, key, label, TaskStage::Downloading);
        p.received = received;
        p.total = total;
        emit_progress(app, p);
    })
    .await;
    if let Err(error) = downloaded {
        let _ = std::fs::remove_file(&zip_path);
        return Err(fail(error.context(format!("Couldn't download {label}"))));
    }

    emit_progress(app, progress(&installation.id, key, label, TaskStage::Installing));
    let folders = {
        let lock = state.fs_lock(&installation.id);
        let _guard = lock.lock().await;
        let replace = existing.as_ref().map(|r| r.folders.clone()).unwrap_or_default();
        let addons_dir = installation.addons_dir();
        let zip = zip_path.clone();
        let result = tauri::async_runtime::spawn_blocking(move || install::install_archive(&zip, &addons_dir, &replace))
            .await
            .map_err(|e| anyhow!("The install task stopped unexpectedly: {e}"))
            .and_then(|r| r);
        let _ = std::fs::remove_file(&zip_path);
        result.with_context(|| format!("Couldn't install {label}")).map_err(&fail)?
    };

    let info = installation.flavor.info();
    let main_folder = folders.first().cloned().unwrap_or_default();
    let main = scan_folder(&installation.addons_dir().join(&main_folder), &main_folder, info);
    let main_toc = main.toc.as_ref();
    let entry = catalog::entry_for_source(state, &release.source);

    let name = existing
        .as_ref()
        .map(|r| r.name.clone())
        .or_else(|| entry.as_ref().map(|e| e.name.clone()))
        .or_else(|| hint.name.clone())
        .or_else(|| main_toc.and_then(|t| t.title.clone()))
        .unwrap_or_else(|| release.name.clone());
    let record = AddonRecord {
        id: existing.as_ref().map(|r| r.id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name,
        source: release.source.clone(),
        folders,
        installed_version: release.version.clone(),
        installed_release_id: release.release_id.clone(),
        installed_at: Utc::now(),
        release_date: release.date,
        channel,
        pinned: existing.as_ref().map(|r| r.pinned).unwrap_or(false),
        auto_update: existing.as_ref().and_then(|r| r.auto_update),
        author: release
            .author
            .clone()
            .or_else(|| entry.as_ref().and_then(|e| e.author.clone()))
            .or_else(|| hint.author.clone())
            .or_else(|| main_toc.and_then(|t| t.author.clone())),
        summary: entry
            .as_ref()
            .map(|e| e.summary.clone())
            .or_else(|| hint.summary.clone())
            .or_else(|| release.summary.clone())
            .or_else(|| main_toc.and_then(|t| t.notes.clone())),
        website: release.website.clone().or_else(|| hint.website.clone()),
        thumbnail: hint
            .thumbnail
            .clone()
            .or_else(|| entry.as_ref().and_then(|e| e.thumbnail.clone()))
            .or_else(|| release.thumbnail.clone()),
    };
    state.mutate(|store| {
        store.upsert_record(&installation.id, record.clone());
        Ok(())
    })?;
    state.updates.lock().unwrap().entry(installation.id.clone()).or_default().insert(
        record.id.clone(),
        UpdateCheck {
            addon_id: record.id.clone(),
            available: false,
            latest_version: Some(release.version.clone()),
            latest_date: release.date,
            changelog: release.changelog.clone(),
            changelog_url: release.changelog_url.clone(),
            error: None,
            checked_at: Utc::now(),
        },
    );
    emit_progress(app, progress(&installation.id, key, label, TaskStage::Done));
    log::info!("Installed {} {} into {}", record.name, record.installed_version, installation.label);
    Ok(record)
}

/// Reinstalls several tracked addons at their newest version.
pub async fn update_addons(
    app: &AppHandle,
    state: &AppState,
    installation_id: &str,
    addon_ids: &[String],
) -> Result<BatchResult> {
    let records: Vec<AddonRecord> = state
        .records(installation_id)
        .into_iter()
        .filter(|r| addon_ids.contains(&r.id) && r.source.provider != ProviderId::Manual)
        .collect();
    let results: Vec<(String, Result<(AddonRecord, Vec<SourceRef>)>)> = stream::iter(records)
        .map(|record| async move {
            let outcome =
                install_one(app, state, installation_id, record.source.clone(), record.channel, SourceHint::default()).await;
            (record.name, outcome)
        })
        .buffer_unordered(3)
        .collect()
        .await;
    let mut batch = BatchResult { succeeded: Vec::new(), failed: Vec::new() };
    for (name, outcome) in results {
        match outcome {
            Ok(_) => batch.succeeded.push(name),
            Err(error) => batch.failed.push(BatchFailure { name, error: format!("{error:#}") }),
        }
    }
    let _ = app.emit("addons-changed", installation_id);
    Ok(batch)
}

// ---------------------------------------------------------------- update checks

fn normalise_version(version: &str) -> String {
    version.trim().trim_start_matches(['v', 'V']).to_ascii_lowercase()
}

fn is_newer(record: &AddonRecord, release: &RemoteRelease) -> bool {
    if !record.installed_release_id.is_empty() {
        return record.installed_release_id != release.release_id;
    }
    normalise_version(&record.installed_version) != normalise_version(&release.version)
}

fn error_check(addon_id: &str, error: &anyhow::Error) -> UpdateCheck {
    UpdateCheck {
        addon_id: addon_id.to_string(),
        available: false,
        latest_version: None,
        latest_date: None,
        changelog: None,
        changelog_url: None,
        error: Some(format!("{error:#}")),
        checked_at: Utc::now(),
    }
}

/// Checks every tracked addon. Concurrent calls for the same installation share
/// the results of the check already running.
pub async fn check_updates(app: &AppHandle, state: &AppState, installation_id: &str) -> Result<Vec<UpdateCheck>> {
    if !state.checking.lock().unwrap().insert(installation_id.to_string()) {
        return Ok(state.update_checks(installation_id).into_values().collect());
    }
    let _ = app.emit("check-started", installation_id);
    let result = check_updates_inner(state, installation_id).await;
    state.checking.lock().unwrap().remove(installation_id);
    let _ = app.emit("updates-changed", installation_id);
    result
}

async fn check_updates_inner(state: &AppState, installation_id: &str) -> Result<Vec<UpdateCheck>> {
    let installation = state.installation(installation_id)?;
    let records: Vec<AddonRecord> = state
        .records(installation_id)
        .into_iter()
        .filter(|r| r.source.provider != ProviderId::Manual)
        .collect();
    let (from_curseforge, others): (Vec<AddonRecord>, Vec<AddonRecord>) =
        records.into_iter().partition(|r| r.source.provider == ProviderId::Curseforge);

    let mut checks: Vec<UpdateCheck> = Vec::new();
    if !from_curseforge.is_empty() {
        let items: Vec<(String, u64, String, Channel)> = from_curseforge
            .iter()
            .filter_map(|r| {
                Some((
                    r.id.clone(),
                    r.source.project_id.parse().ok()?,
                    r.installed_release_id.clone(),
                    effective_channel(state, &installation, r.channel),
                ))
            })
            .collect();
        match curseforge::check_many(state, &items, installation.flavor).await {
            Ok(found) => checks.extend(found),
            Err(error) => checks.extend(from_curseforge.iter().map(|r| error_check(&r.id, &error))),
        }
    }

    let installation_ref = &installation;
    let other_checks: Vec<UpdateCheck> = stream::iter(others)
        .map(|record: AddonRecord| async move {
            let channel = effective_channel(state, installation_ref, record.channel);
            match providers::resolve(state, &record.source, installation_ref.flavor, channel).await {
                Ok(release) => UpdateCheck {
                    addon_id: record.id.clone(),
                    available: is_newer(&record, &release),
                    latest_version: Some(release.version),
                    latest_date: release.date,
                    changelog: release.changelog,
                    changelog_url: release.changelog_url,
                    error: None,
                    checked_at: Utc::now(),
                },
                Err(error) => error_check(&record.id, &error),
            }
        })
        .buffer_unordered(6)
        .collect()
        .await;
    checks.extend(other_checks);

    let map: HashMap<String, UpdateCheck> = checks.iter().map(|c| (c.addon_id.clone(), c.clone())).collect();
    state.updates.lock().unwrap().insert(installation_id.to_string(), map);
    log::info!(
        "Checked {} addons for {}: {} updates, {} errors",
        checks.len(),
        installation.label,
        checks.iter().filter(|c| c.available).count(),
        checks.iter().filter(|c| c.error.is_some()).count()
    );
    Ok(checks)
}

/// Tracked addons with an update ready. `automatic_only` limits it to addons
/// allowed to update on their own (per-addon setting, else the global one).
pub fn pending_updates(state: &AppState, installation_id: &str, automatic_only: bool) -> Vec<String> {
    let settings = state.settings();
    let checks = state.update_checks(installation_id);
    state
        .records(installation_id)
        .iter()
        .filter(|r| !r.pinned && r.source.provider != ProviderId::Manual)
        .filter(|r| checks.get(&r.id).map(|c| c.available).unwrap_or(false))
        .filter(|r| !automatic_only || r.auto_update.unwrap_or(settings.auto_install_updates))
        .map(|r| r.id.clone())
        .collect()
}

// ---------------------------------------------------------------- removing and adopting

pub async fn uninstall(app: &AppHandle, state: &AppState, installation_id: &str, addon_id: &str) -> Result<()> {
    let installation = state.installation(installation_id)?;
    let lock = state.fs_lock(installation_id);
    let _guard = lock.lock().await;

    let (folders, record_id) = if addon_id.starts_with("local:") {
        let views = build_views(state, &installation)?;
        let view = views
            .iter()
            .find(|v| v.id == addon_id)
            .ok_or_else(|| anyhow!("That addon is no longer in the AddOns folder."))?;
        (view.folders.iter().map(|f| f.name.clone()).collect::<Vec<_>>(), None)
    } else {
        let records = state.records(installation_id);
        let record = records
            .iter()
            .find(|r| r.id == addon_id)
            .ok_or_else(|| anyhow!("That addon isn't tracked any more."))?;
        let shared: HashSet<String> = records
            .iter()
            .filter(|r| r.id != record.id)
            .flat_map(|r| r.folders.iter().map(|f| f.to_ascii_lowercase()))
            .collect();
        let folders = record.folders.iter().filter(|f| !shared.contains(&f.to_ascii_lowercase())).cloned().collect();
        (folders, Some(record.id.clone()))
    };

    let addons_dir = installation.addons_dir();
    tauri::async_runtime::spawn_blocking(move || install::remove_folders(&addons_dir, &folders))
        .await
        .map_err(|e| anyhow!("The removal task stopped unexpectedly: {e}"))??;

    if let Some(record_id) = record_id {
        state.mutate(|store| {
            store.records_mut(installation_id).retain(|r| r.id != record_id);
            Ok(())
        })?;
        if let Some(checks) = state.updates.lock().unwrap().get_mut(installation_id) {
            checks.remove(&record_id);
        }
    }
    let _ = app.emit("addons-changed", installation_id);
    Ok(())
}

/// Starts tracking an addon that was installed by hand or by another app.
pub async fn link(state: &AppState, installation_id: &str, addon_id: &str, source: SourceRef) -> Result<AddonRecord> {
    let installation = state.installation(installation_id)?;
    let views = build_views(state, &installation)?;
    let view = views
        .into_iter()
        .find(|v| v.id == addon_id && !v.managed)
        .ok_or_else(|| anyhow!("That addon is already tracked or no longer installed."))?;
    if state.records(installation_id).iter().any(|r| r.source.same_as(&source)) {
        bail!("Another installed addon already uses that source.");
    }
    let release = providers::resolve(state, &source, installation.flavor, effective_channel(state, &installation, None))
        .await
        .ok();
    let entry = catalog::entry_for_source(state, &source);
    let record = AddonRecord {
        id: uuid::Uuid::new_v4().to_string(),
        name: entry.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| view.name.clone()),
        source: source.clone(),
        folders: view.folders.iter().map(|f| f.name.clone()).collect(),
        installed_version: view.version.clone().unwrap_or_default(),
        installed_release_id: String::new(),
        installed_at: Utc::now(),
        release_date: None,
        channel: None,
        pinned: false,
        auto_update: None,
        author: view.author.clone().or_else(|| release.as_ref().and_then(|r| r.author.clone())),
        summary: entry
            .as_ref()
            .map(|e| e.summary.clone())
            .or_else(|| release.as_ref().and_then(|r| r.summary.clone()))
            .or_else(|| view.notes.clone()),
        website: release.as_ref().and_then(|r| r.website.clone()).or(view.website.clone()),
        thumbnail: entry
            .as_ref()
            .and_then(|e| e.thumbnail.clone())
            .or_else(|| release.as_ref().and_then(|r| r.thumbnail.clone())),
    };
    state.mutate(|store| {
        store.upsert_record(installation_id, record.clone());
        Ok(())
    })?;
    if let Some(release) = release {
        state.updates.lock().unwrap().entry(installation_id.to_string()).or_default().insert(
            record.id.clone(),
            UpdateCheck {
                addon_id: record.id.clone(),
                available: is_newer(&record, &release),
                latest_version: Some(release.version),
                latest_date: release.date,
                changelog: release.changelog,
                changelog_url: release.changelog_url,
                error: None,
                checked_at: Utc::now(),
            },
        );
    }
    Ok(record)
}

/// Links every untracked addon whose source can be identified. Returns how many.
pub async fn link_all(state: &AppState, installation_id: &str) -> Result<usize> {
    let installation = state.installation(installation_id)?;
    let views = build_views(state, &installation)?;
    let mut seen = HashSet::new();
    let suggestions: Vec<(String, SourceRef)> = views
        .iter()
        .filter(|v| !v.managed)
        .filter_map(|v| Some((v.id.clone(), v.suggested_source.clone()?)))
        .filter(|(_, s)| seen.insert(s.key()))
        .collect();
    let mut linked = 0;
    for (addon_id, source) in suggestions {
        match link(state, installation_id, &addon_id, source).await {
            Ok(_) => linked += 1,
            Err(error) => log::info!("Didn't link {addon_id}: {error:#}"),
        }
    }
    Ok(linked)
}

// ---------------------------------------------------------------- other install routes

/// Installs from a pasted link: GitHub, CurseForge or Tukui pages, or a direct .zip URL.
pub async fn install_from_url(app: &AppHandle, state: &AppState, installation_id: &str, url: &str) -> Result<AddonRecord> {
    let input = url.trim();
    let lower = input.to_ascii_lowercase();
    if lower.contains("github.com/") || (!lower.contains("://") && input.matches('/').count() == 1) {
        let repo = github::parse_repo(input).ok_or_else(|| anyhow!("That doesn't look like a GitHub project link."))?;
        let hint = match github::repo_info(state, &repo).await {
            Ok(info) => SourceHint {
                name: Some(info.name.clone()),
                summary: info.description.clone(),
                author: Some(info.owner.login.clone()),
                thumbnail: Some(github::avatar_url(&info.owner.login)),
                website: Some(info.html_url.clone()),
            },
            Err(_) => SourceHint::default(),
        };
        return install(app, state, installation_id, SourceRef::new(ProviderId::Github, repo), None, hint).await;
    }
    if lower.contains("tukui.org") {
        let slug = input
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("That doesn't look like a Tukui addon link."))?;
        return install(app, state, installation_id, SourceRef::new(ProviderId::Tukui, slug.to_lowercase()), None, SourceHint::default()).await;
    }
    if lower.contains("curseforge.com") {
        bail!("To install from CurseForge, search for the addon in Find addons (it needs a CurseForge API key in Settings).");
    }
    if lower.starts_with("https://") && lower.split('?').next().unwrap_or("").ends_with(".zip") {
        return install_manual_url(app, state, installation_id, input).await;
    }
    bail!("Paste a GitHub or Tukui link, or a direct https link to a .zip file.")
}

async fn install_manual_url(app: &AppHandle, state: &AppState, installation_id: &str, url: &str) -> Result<AddonRecord> {
    let downloads = state.data_dir.join("downloads");
    std::fs::create_dir_all(&downloads)?;
    let zip_path = downloads.join(format!("{}.zip", uuid::Uuid::new_v4().simple()));
    let label = url.rsplit('/').next().unwrap_or(url).split('?').next().unwrap_or(url).to_string();
    emit_progress(app, progress(installation_id, url, &label, TaskStage::Downloading));
    let result = http::download(&state.http, url, &zip_path, Default::default(), |_, _| {}).await;
    if let Err(error) = result {
        let _ = std::fs::remove_file(&zip_path);
        let mut failed = progress(installation_id, url, &label, TaskStage::Failed);
        failed.message = Some(format!("{error:#}"));
        emit_progress(app, failed);
        return Err(error);
    }
    let outcome = install_local_zip(state, installation_id, &zip_path, &label, url).await;
    let _ = std::fs::remove_file(&zip_path);
    let stage = if outcome.is_ok() { TaskStage::Done } else { TaskStage::Failed };
    emit_progress(app, progress(installation_id, url, &label, stage));
    outcome
}

/// Installs a zip the user picked or dropped onto the window.
pub async fn install_from_file(app: &AppHandle, state: &AppState, installation_id: &str, path: &Path) -> Result<AddonRecord> {
    if !path.is_file() {
        bail!("That file no longer exists.");
    }
    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let record = install_local_zip(state, installation_id, path, &label, &label).await?;
    let _ = app.emit("addons-changed", installation_id);
    Ok(record)
}

async fn install_local_zip(state: &AppState, installation_id: &str, zip: &Path, label: &str, origin: &str) -> Result<AddonRecord> {
    let installation = state.installation(installation_id)?;
    let lock = state.fs_lock(installation_id);
    let _guard = lock.lock().await;
    let addons_dir = installation.addons_dir();
    let zip_owned: PathBuf = zip.to_path_buf();
    let existing = state
        .records(installation_id)
        .into_iter()
        .find(|r| r.source.provider == ProviderId::Manual && r.source.project_id == origin);
    let replace = existing.as_ref().map(|r| r.folders.clone()).unwrap_or_default();
    let folders = tauri::async_runtime::spawn_blocking(move || install::install_archive(&zip_owned, &addons_dir, &replace))
        .await
        .map_err(|e| anyhow!("The install task stopped unexpectedly: {e}"))?
        .with_context(|| format!("Couldn't install {label}"))?;

    let info = installation.flavor.info();
    let main = scan_folder(&installation.addons_dir().join(&folders[0]), &folders[0], info);
    let toc = main.toc.as_ref();
    let record = AddonRecord {
        id: existing.as_ref().map(|r| r.id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: toc.and_then(|t| t.title.clone()).unwrap_or_else(|| folders[0].clone()),
        source: SourceRef::new(ProviderId::Manual, origin),
        folders,
        installed_version: toc.and_then(|t| t.version.clone()).unwrap_or_default(),
        installed_release_id: String::new(),
        installed_at: Utc::now(),
        release_date: None,
        channel: None,
        pinned: false,
        auto_update: None,
        author: toc.and_then(|t| t.author.clone()),
        summary: toc.and_then(|t| t.notes.clone()),
        website: origin.starts_with("https://").then(|| origin.to_string()),
        thumbnail: None,
    };
    state.mutate(|store| {
        store.upsert_record(installation_id, record.clone());
        Ok(())
    })?;
    Ok(record)
}

// ---------------------------------------------------------------- sharing

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    pub name: String,
    pub flavor: Flavor,
    pub flavor_matches: bool,
    pub items: Vec<PackItemView>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackItemView {
    pub source: SourceRef,
    pub name: String,
    pub installed: bool,
}

pub fn export_pack(state: &AppState, installation_id: &str, addon_ids: Option<Vec<String>>, name: &str) -> Result<String> {
    let installation = state.installation(installation_id)?;
    let addons: Vec<PackAddon> = state
        .records(installation_id)
        .into_iter()
        .filter(|r| r.source.provider != ProviderId::Manual)
        .filter(|r| addon_ids.as_ref().map(|ids| ids.contains(&r.id)).unwrap_or(true))
        .map(|r| PackAddon { provider: r.source.provider, project_id: r.source.project_id, name: r.name, channel: r.channel })
        .collect();
    if addons.is_empty() {
        bail!("There are no shareable addons yet. Addons installed from a file can't be shared.");
    }
    share::encode(&Pack { v: 1, name: name.trim().to_string(), flavor: installation.flavor, addons })
}

pub fn preview_pack(state: &AppState, installation_id: &str, code: &str) -> Result<PackPreview> {
    let installation = state.installation(installation_id)?;
    let pack = share::decode(code)?;
    let records = state.records(installation_id);
    Ok(PackPreview {
        name: pack.name.clone(),
        flavor: pack.flavor,
        flavor_matches: pack.flavor == installation.flavor,
        items: pack
            .addons
            .iter()
            .map(|a| {
                let source = SourceRef::new(a.provider, &a.project_id);
                PackItemView { installed: records.iter().any(|r| r.source.same_as(&source)), source, name: a.name.clone() }
            })
            .collect(),
    })
}

pub async fn import_pack(app: &AppHandle, state: &AppState, installation_id: &str, code: &str) -> Result<BatchResult> {
    let pack = share::decode(code)?;
    let tracked = state.records(installation_id);
    let missing: Vec<PackAddon> = pack
        .addons
        .into_iter()
        .filter(|a| a.provider != ProviderId::Manual)
        .filter(|a| !tracked.iter().any(|r| r.source.same_as(&SourceRef::new(a.provider, &a.project_id))))
        .collect();
    let results: Vec<(String, Result<AddonRecord>)> = stream::iter(missing)
        .map(|addon| async move {
            let hint = SourceHint { name: Some(addon.name.clone()), ..Default::default() };
            let source = SourceRef::new(addon.provider, &addon.project_id);
            (addon.name.clone(), install(app, state, installation_id, source, addon.channel, hint).await)
        })
        .buffer_unordered(3)
        .collect()
        .await;
    let mut batch = BatchResult { succeeded: Vec::new(), failed: Vec::new() };
    for (name, outcome) in results {
        match outcome {
            Ok(_) => batch.succeeded.push(name),
            Err(error) => batch.failed.push(BatchFailure { name, error: format!("{error:#}") }),
        }
    }
    let _ = app.emit("addons-changed", installation_id);
    Ok(batch)
}

// ---------------------------------------------------------------- playing

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlayOutcome {
    pub method: String,
    pub updated: usize,
    pub backed_up: bool,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct PlayStage<'a> {
    installation_id: &'a str,
    stage: &'a str,
}

/// Backs up settings, applies addon updates and starts the game, per Settings.
pub async fn play(app: &AppHandle, state: &AppState, installation_id: &str) -> Result<PlayOutcome> {
    let installation = state.installation(installation_id)?;
    let settings = state.settings();
    if state.is_game_running(&installation) {
        bail!("{} is already running.", installation.label);
    }
    let stage = |name: &str| {
        let _ = app.emit("play-stage", PlayStage { installation_id, stage: name });
    };
    let mut warnings: Vec<String> = Vec::new();
    let mut backed_up = false;
    let mut updated = 0;

    if settings.backup_before_launch {
        stage("backingUp");
        let dir = backup::backup_dir(&settings, &state.data_dir, &installation);
        let inst = installation.clone();
        let keep = settings.backup_retention as usize;
        let outcome = tauri::async_runtime::spawn_blocking(move || -> Result<bool> {
            let created = backup::create(&dir, &inst, BackupReason::Launch)?;
            backup::prune(&dir, keep)?;
            Ok(created.is_some())
        })
        .await
        .map_err(|e| anyhow!("{e}"))
        .and_then(|r| r);
        match outcome {
            Ok(created) => backed_up = created,
            Err(error) => warnings.push(format!("Settings backup skipped: {error:#}")),
        }
    }

    if settings.update_before_launch {
        stage("updating");
        match tokio::time::timeout(Duration::from_secs(30), check_updates(app, state, installation_id)).await {
            Ok(Ok(_)) => {
                let ids = pending_updates(state, installation_id, false);
                if !ids.is_empty() {
                    let batch = update_addons(app, state, installation_id, &ids).await?;
                    updated = batch.succeeded.len();
                    warnings.extend(batch.failed.into_iter().map(|f| format!("{} didn't update: {}", f.name, f.error)));
                }
            }
            Ok(Err(error)) => warnings.push(format!("Couldn't check for addon updates: {error:#}")),
            Err(_) => warnings.push("Checking for addon updates took too long, so the game started without them.".into()),
        }
    }

    stage("launching");
    let method = launch::launch(&installation)?;
    state.mutate(|store| {
        store.data.settings.last_installation = Some(installation_id.to_string());
        Ok(())
    })?;
    crate::tray::refresh(app);

    match settings.after_launch {
        AfterLaunch::Tray => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        AfterLaunch::Quit => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                handle.exit(0);
            });
        }
        AfterLaunch::Keep => {}
    }
    stage("done");
    Ok(PlayOutcome { method: method.to_string(), updated, backed_up, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn toc(dir: &Path, name: &str, body: &str) {
        fs::create_dir_all(dir.join(name)).unwrap();
        fs::write(dir.join(name).join(format!("{name}.toc")), body).unwrap();
    }

    #[test]
    fn lists_tracked_and_loose_addons_with_problems_and_suggestions() {
        let tmp = tempfile::tempdir().unwrap();
        let state = AppState::for_tests(tmp.path());
        let client = tmp.path().join("_forever_");
        let addons = client.join("Interface").join("AddOns");
        toc(&addons, "Questie", "## Interface: 16001\n## Title: Questie\n## Version: v1\n");
        toc(&addons, "BigWigs", "## Interface: 16001\n## Title: BigWigs\n");
        toc(&addons, "BigWigs_Core", "## Interface: 16001\n## Dependencies: BigWigs\n");
        toc(&addons, "MoveAny", "## Interface: 16001\n## Title: MoveAny\n");
        toc(&addons, "Details", "## Interface: 120105\n## Title: Details!\n");
        toc(&addons, "Plugin", "## Interface: 16001\n## Dependencies: MissingCore\n");

        let installation = Installation {
            id: "i".into(),
            game: "wow".into(),
            flavor: Flavor::Forever,
            label: "WoW Forever".into(),
            path: client,
            launch_method: LaunchMethod::Battlenet,
            default_channel: None,
            added_at: None,
        };
        state
            .mutate(|store| {
                store.data.installations.push(installation.clone());
                store.upsert_record(
                    "i",
                    AddonRecord {
                        id: "r1".into(),
                        name: "Questie".into(),
                        source: SourceRef::new(ProviderId::Github, "Questie/Questie"),
                        folders: vec!["Questie".into(), "Questie_Gone".into()],
                        installed_version: "v1".into(),
                        installed_release_id: "v1".into(),
                        installed_at: Utc::now(),
                        release_date: None,
                        channel: None,
                        pinned: false,
                        auto_update: None,
                        author: None,
                        summary: None,
                        website: None,
                        thumbnail: None,
                    },
                );
                Ok(())
            })
            .unwrap();

        let views = build_views(&state, &installation).unwrap();
        let find = |name: &str| views.iter().find(|v| v.name == name).unwrap_or_else(|| panic!("{name} missing"));

        let questie = find("Questie");
        assert!(questie.managed);
        assert_eq!(questie.missing_folders, vec!["Questie_Gone".to_string()]);

        let bigwigs = find("BigWigs");
        assert!(!bigwigs.managed);
        assert_eq!(bigwigs.folders.len(), 2, "BigWigs_Core is grouped under BigWigs");
        assert_eq!(
            bigwigs.suggested_source,
            Some(SourceRef::new(ProviderId::Github, "BigWigsMods/BigWigs")),
            "suggested from the curated list"
        );
        assert_eq!(find("MoveAny").suggested_source.as_ref().map(|s| s.project_id.as_str()), Some("d4kir92/MoveAny"));
        assert_eq!(find("Details!").compatibility, Compatibility::OutOfDate);
        assert_eq!(find("Plugin").missing_dependencies, vec!["MissingCore".to_string()]);
        assert_eq!(views.len(), 5);
    }

    #[test]
    fn compares_versions_for_adopted_addons() {
        let mut record = AddonRecord {
            id: "r".into(),
            name: "X".into(),
            source: SourceRef::new(ProviderId::Github, "a/b"),
            folders: vec![],
            installed_version: "v1.2.0".into(),
            installed_release_id: String::new(),
            installed_at: Utc::now(),
            release_date: None,
            channel: None,
            pinned: false,
            auto_update: None,
            author: None,
            summary: None,
            website: None,
            thumbnail: None,
        };
        let mut release = RemoteRelease {
            source: record.source.clone(),
            name: "X".into(),
            version: "1.2.0".into(),
            release_id: "1.2.0".into(),
            date: None,
            download_url: String::new(),
            file_name: String::new(),
            changelog: None,
            changelog_url: None,
            website: None,
            author: None,
            summary: None,
            thumbnail: None,
            prerelease: false,
            dependencies: vec![],
            verified_flavor: true,
        };
        assert!(!is_newer(&record, &release), "v-prefix differences aren't updates");
        release.version = "1.3.0".into();
        assert!(is_newer(&record, &release));
        record.installed_release_id = "1.3.0".into();
        release.release_id = "1.3.0".into();
        assert!(!is_newer(&record, &release));
    }
}
