//! Addon sources. Each provider can resolve the newest build of a project for a
//! game version, and search for projects.

pub mod catalog;
pub mod curseforge;
pub mod github;
pub mod tukui;

use anyhow::{bail, Result};

use crate::games::wow::flavor::Flavor;
use crate::model::{AddonDetails, Channel, ProviderId, RemoteRelease, SearchHit, SourceRef};
use crate::state::AppState;

pub async fn resolve(state: &AppState, source: &SourceRef, flavor: Flavor, channel: Channel) -> Result<RemoteRelease> {
    match source.provider {
        ProviderId::Github => github::resolve(state, &source.project_id, flavor, channel).await,
        ProviderId::Curseforge => curseforge::resolve(state, &source.project_id, flavor, channel).await,
        ProviderId::Tukui => tukui::resolve(state, &source.project_id, flavor).await,
        ProviderId::Manual => bail!("Addons installed from a file can't be updated automatically."),
    }
}

const DETAILS_CACHE_FOR: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Everything for an addon's details panel, cached for a few minutes per project.
pub async fn details(state: &AppState, source: &SourceRef, flavor: Flavor) -> Result<AddonDetails> {
    let key = format!("{}|{:?}", source.key(), flavor);
    if let Some((fetched, cached)) = state.details_cache.lock().unwrap().get(&key) {
        if fetched.elapsed() < DETAILS_CACHE_FOR {
            return Ok(cached.clone());
        }
    }
    let details = match source.provider {
        ProviderId::Github => github::details(state, &source.project_id, flavor).await?,
        ProviderId::Curseforge => curseforge::details(state, &source.project_id, flavor).await?,
        ProviderId::Tukui => tukui::details(state, &source.project_id, flavor).await?,
        ProviderId::Manual => bail!("Addons installed from a file don't have online details."),
    };
    state
        .details_cache
        .lock()
        .unwrap()
        .insert(key, (std::time::Instant::now(), details.clone()));
    Ok(details)
}

/// Which sources a search should cover.
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchScope {
    All,
    Catalog,
    Github,
    Curseforge,
    Tukui,
}

pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub warnings: Vec<String>,
}

pub async fn search(state: &AppState, query: &str, flavor: Flavor, scope: SearchScope) -> SearchOutcome {
    let wants = |s: SearchScope| scope == SearchScope::All || scope == s;
    let mut hits: Vec<SearchHit> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if wants(SearchScope::Catalog) {
        hits.extend(catalog::search(state, query, flavor));
    }

    let curseforge_ready = state.secret(crate::secrets::SecretKind::CurseforgeKey).is_some();
    let (gh, cf, tk) = tokio::join!(
        async {
            if wants(SearchScope::Github) && !query.trim().is_empty() {
                Some(github::search(state, query, flavor).await)
            } else {
                None
            }
        },
        async {
            if wants(SearchScope::Curseforge) && (curseforge_ready || scope == SearchScope::Curseforge) {
                Some(curseforge::search(state, query, flavor).await)
            } else {
                None
            }
        },
        async {
            if wants(SearchScope::Tukui) {
                Some(tukui::search(state, query, flavor).await)
            } else {
                None
            }
        },
    );
    for (label, result) in [("GitHub", gh), ("CurseForge", cf), ("Tukui", tk)] {
        match result {
            Some(Ok(found)) => hits.extend(found),
            Some(Err(error)) => warnings.push(format!("{label}: {error:#}")),
            None => {}
        }
    }

    // Keep the first hit per project (the catalogue comes first and has the best metadata).
    let mut seen = std::collections::HashSet::new();
    hits.retain(|h| seen.insert(h.source.key()));
    SearchOutcome { hits, warnings }
}
