//! The curated list of addons known to have builds for each game version.
//! A copy ships inside the app; a newer copy is fetched from the project repo so
//! the list can grow without releasing a new version of Hearthforge.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::games::wow::flavor::Flavor;
use crate::http::get_json;
use crate::model::{ProviderId, SearchHit, SourceRef};
use crate::providers::github::avatar_url;
use crate::state::AppState;

const EMBEDDED: &str = include_str!("../../../catalog/catalog.json");
pub const DEFAULT_URL: &str = "https://raw.githubusercontent.com/Noolies/wow-mods/main/catalog/catalog.json";
const CACHE_FILE: &str = "catalog.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub category: String,
    pub source: SourceRef,
    pub flavors: Vec<Flavor>,
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub starter: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub version: u32,
    pub updated: String,
    pub entries: Vec<CatalogEntry>,
}

pub fn embedded() -> Catalog {
    serde_json::from_str(EMBEDDED).expect("the bundled catalogue is valid JSON")
}

/// The newer of the bundled catalogue and the last downloaded copy.
pub fn load_cached(data_dir: &Path) -> Catalog {
    let bundled = embedded();
    std::fs::read(data_dir.join(CACHE_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Catalog>(&bytes).ok())
        .filter(|cached| cached.version > bundled.version)
        .unwrap_or(bundled)
}

/// Downloads the published catalogue; keeps it when it's newer. Returns true if it changed.
pub async fn refresh(state: &AppState) -> Result<bool> {
    let url = state.settings().catalog_url.unwrap_or_else(|| DEFAULT_URL.to_string());
    let remote: Catalog = get_json(&state.http, &url, "The addon list").await?;
    let current_version = state.catalog.lock().unwrap().version;
    if remote.version <= current_version || remote.entries.is_empty() {
        return Ok(false);
    }
    let bytes = serde_json::to_vec_pretty(&remote)?;
    std::fs::write(state.data_dir.join(CACHE_FILE), bytes).context("Couldn't save the addon list")?;
    *state.catalog.lock().unwrap() = remote;
    Ok(true)
}

fn thumbnail(entry: &CatalogEntry) -> Option<String> {
    entry.thumbnail.clone().or_else(|| match entry.source.provider {
        ProviderId::Github => entry.source.project_id.split('/').next().map(avatar_url),
        _ => None,
    })
}

fn website(entry: &CatalogEntry) -> Option<String> {
    match entry.source.provider {
        ProviderId::Github => Some(format!("https://github.com/{}", entry.source.project_id)),
        ProviderId::Tukui => Some(format!("https://tukui.org/{}", entry.source.project_id)),
        ProviderId::Curseforge => None,
        ProviderId::Manual => None,
    }
}

pub fn to_hit(entry: &CatalogEntry, flavor: Flavor) -> SearchHit {
    SearchHit {
        source: entry.source.clone(),
        name: entry.name.clone(),
        summary: Some(entry.summary.clone()),
        author: entry.author.clone(),
        thumbnail: thumbnail(entry),
        website: website(entry),
        downloads: None,
        stars: None,
        updated_at: None,
        supports_flavor: Some(entry.flavors.contains(&flavor)),
        installed: false,
        category: Some(entry.category.clone()),
        featured: true,
        starter: entry.starter,
    }
}

/// Catalogue entries for this game version matching the query (all of them for an empty query).
pub fn search(state: &AppState, query: &str, flavor: Flavor) -> Vec<SearchHit> {
    let q = query.trim().to_lowercase();
    let catalog = state.catalog.lock().unwrap();
    catalog
        .entries
        .iter()
        .filter(|e| e.flavors.contains(&flavor))
        .filter(|e| {
            q.is_empty()
                || e.name.to_lowercase().contains(&q)
                || e.summary.to_lowercase().contains(&q)
                || e.category.to_lowercase().contains(&q)
                || e.folders.iter().any(|f| f.to_lowercase().contains(&q))
        })
        .map(|e| to_hit(e, flavor))
        .collect()
}

/// The catalogue source owning an addon folder, e.g. `DBM-Core` -> Deadly Boss Mods.
pub fn source_for_folder(state: &AppState, folder: &str) -> Option<CatalogEntry> {
    let catalog = state.catalog.lock().unwrap();
    catalog
        .entries
        .iter()
        .find(|e| e.folders.iter().any(|f| f.eq_ignore_ascii_case(folder)))
        .cloned()
}

pub fn entry_for_source(state: &AppState, source: &SourceRef) -> Option<CatalogEntry> {
    let catalog = state.catalog.lock().unwrap();
    catalog.entries.iter().find(|e| e.source.same_as(source)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalogue_is_valid_and_unique() {
        let catalog = embedded();
        assert!(!catalog.entries.is_empty());
        let mut ids = std::collections::HashSet::new();
        for entry in &catalog.entries {
            assert!(ids.insert(entry.id.clone()), "duplicate id {}", entry.id);
            assert!(!entry.folders.is_empty(), "{} lists no folders", entry.id);
            assert!(entry.flavors.contains(&Flavor::Forever), "{} isn't for Forever", entry.id);
        }
    }
}
