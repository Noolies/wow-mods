//! CurseForge, through its official API. Optional: it needs an API key, which the
//! user pastes into Settings and which is kept in the OS credential store.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::ACCEPT;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::games::wow::flavor::{curseforge_type_label, ordered_labels, Flavor, FlavorInfo};
use crate::http::ensure_success;
use crate::model::{AddonDetails, Channel, ProviderId, RemoteRelease, SearchHit, SourceRef, TextFormat, UpdateCheck};
use crate::secrets::SecretKind;
use crate::state::AppState;

const API: &str = "https://api.curseforge.com/v1";
const WOW_GAME_ID: &str = "1";
const REQUIRED_DEPENDENCY: u8 = 3;

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub links: Option<CfLinks>,
    #[serde(default)]
    pub authors: Vec<CfAuthor>,
    #[serde(default)]
    pub logo: Option<CfLogo>,
    #[serde(default)]
    pub download_count: Option<f64>,
    #[serde(default)]
    pub categories: Vec<CfCategory>,
    #[serde(default)]
    pub latest_files: Vec<CfFile>,
    #[serde(default)]
    pub latest_files_indexes: Vec<CfFileIndex>,
    #[serde(default)]
    pub date_modified: Option<DateTime<Utc>>,
    #[serde(default)]
    pub allow_mod_distribution: Option<bool>,
    #[serde(default)]
    pub screenshots: Vec<CfScreenshot>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CfScreenshot {
    pub url: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfLinks {
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CfAuthor {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfLogo {
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CfCategory {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfFileIndex {
    pub file_id: u64,
    pub filename: String,
    pub release_type: u8,
    #[serde(default)]
    pub game_version_type_id: Option<u32>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    pub id: u64,
    pub display_name: String,
    pub file_name: String,
    pub release_type: u8,
    pub file_date: DateTime<Utc>,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<CfDependency>,
    #[serde(default)]
    pub file_length: Option<u64>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfDependency {
    pub mod_id: u64,
    pub relation_type: u8,
}

fn api_key(state: &AppState) -> Result<String> {
    state
        .secret(SecretKind::CurseforgeKey)
        .ok_or_else(|| anyhow!("CurseForge needs an API key. Add one in Settings, under Addon sources."))
}

async fn get<T: DeserializeOwned>(state: &AppState, url: &str) -> Result<T> {
    let response = state
        .http
        .get(url)
        .header("x-api-key", api_key(state)?)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .context("Couldn't reach CurseForge. Check your internet connection.")?;
    let response = ensure_success(response, "CurseForge").await?;
    let envelope: Envelope<T> = response.json().await.context("CurseForge sent data Hearthforge couldn't read")?;
    Ok(envelope.data)
}

async fn post<T: DeserializeOwned, B: Serialize>(state: &AppState, url: &str, body: &B) -> Result<T> {
    let response = state
        .http
        .post(url)
        .header("x-api-key", api_key(state)?)
        .header(ACCEPT, "application/json")
        .json(body)
        .send()
        .await
        .context("Couldn't reach CurseForge. Check your internet connection.")?;
    let response = ensure_success(response, "CurseForge").await?;
    let envelope: Envelope<T> = response.json().await.context("CurseForge sent data Hearthforge couldn't read")?;
    Ok(envelope.data)
}

/// Newest file for this game version within the channel (higher file ids are newer).
pub fn pick_index<'a>(m: &'a CfMod, info: &FlavorInfo, channel: Channel) -> Option<&'a CfFileIndex> {
    m.latest_files_indexes
        .iter()
        .filter(|i| i.game_version_type_id == Some(info.curseforge_type_id))
        .filter(|i| i.release_type <= channel.curseforge_max_release_type())
        .max_by_key(|i| i.file_id)
}

fn forgecdn_url(file_id: u64, file_name: &str) -> String {
    let mut url = url::Url::parse("https://edge.forgecdn.net/files/").expect("static URL");
    url.path_segments_mut()
        .expect("https URL has a path")
        .pop_if_empty()
        .push(&(file_id / 1000).to_string())
        .push(&(file_id % 1000).to_string())
        .push(file_name);
    url.to_string()
}

fn website(m: &CfMod) -> Option<String> {
    m.links.as_ref().and_then(|l| l.website_url.clone())
}

pub async fn resolve(state: &AppState, project_id: &str, flavor: Flavor, channel: Channel) -> Result<RemoteRelease> {
    let id: u64 = project_id.trim().parse().map_err(|_| anyhow!("\"{project_id}\" isn't a CurseForge project id."))?;
    let m: CfMod = get(state, &format!("{API}/mods/{id}")).await?;
    let info = flavor.info();
    let index = pick_index(&m, info, channel)
        .ok_or_else(|| anyhow!("{} doesn't have a {} file on CurseForge yet.", m.name, info.name))?;
    let file: CfFile = get(state, &format!("{API}/mods/{}/files/{}", m.id, index.file_id)).await?;
    let download_url = match file.download_url.clone().filter(|u| !u.is_empty()) {
        Some(url) => url,
        None if m.allow_mod_distribution == Some(false) => {
            bail!("The author of {} only allows downloads through the CurseForge website or app.", m.name)
        }
        None => forgecdn_url(file.id, &file.file_name),
    };
    Ok(RemoteRelease {
        source: SourceRef::new(ProviderId::Curseforge, m.id.to_string()),
        name: m.name.clone(),
        version: file.display_name.clone(),
        release_id: file.id.to_string(),
        date: Some(file.file_date),
        download_url,
        file_name: file.file_name.clone(),
        changelog: None,
        changelog_url: website(&m).map(|w| format!("{w}/files/{}", file.id)),
        website: website(&m),
        author: m.authors.first().map(|a| a.name.clone()),
        summary: m.summary.clone(),
        thumbnail: m.logo.as_ref().and_then(|l| l.thumbnail_url.clone()),
        prerelease: file.release_type > 1,
        dependencies: file
            .dependencies
            .iter()
            .filter(|d| d.relation_type == REQUIRED_DEPENDENCY)
            .map(|d| SourceRef::new(ProviderId::Curseforge, d.mod_id.to_string()))
            .collect(),
        verified_flavor: true,
    })
}

pub async fn details(state: &AppState, project_id: &str, flavor: Flavor) -> Result<AddonDetails> {
    let id: u64 = project_id.trim().parse().map_err(|_| anyhow!("\"{project_id}\" isn't a CurseForge project id."))?;
    let m: CfMod = get(state, &format!("{API}/mods/{id}")).await?;
    let info = flavor.info();
    let index = pick_index(&m, info, Channel::Stable).or_else(|| pick_index(&m, info, Channel::Alpha));
    let (description, file, notes) = tokio::join!(
        async { get::<String>(state, &format!("{API}/mods/{id}/description")).await.ok() },
        async {
            match index {
                Some(index) => get::<CfFile>(state, &format!("{API}/mods/{id}/files/{}", index.file_id)).await.ok(),
                None => None,
            }
        },
        async {
            match index {
                Some(index) => get::<String>(state, &format!("{API}/mods/{id}/files/{}/changelog", index.file_id)).await.ok(),
                None => None,
            }
        },
    );

    let mut details = AddonDetails::new(SourceRef::new(ProviderId::Curseforge, m.id.to_string()), m.name.clone());
    details.summary = m.summary.clone();
    details.author = m.authors.first().map(|a| a.name.clone());
    details.thumbnail = m.logo.as_ref().and_then(|l| l.thumbnail_url.clone());
    details.website = website(&m);
    details.source_url = m.links.as_ref().and_then(|l| l.source_url.clone());
    details.issues_url = m.links.as_ref().and_then(|l| l.issues_url.clone());
    details.downloads = m.download_count.map(|d| d as u64);
    details.updated_at = m.date_modified;
    details.tags = m.categories.iter().map(|c| c.name.clone()).collect();
    details.description = description;
    details.description_format = TextFormat::Html;
    details.screenshots = m.screenshots.iter().map(|s| s.url.clone()).collect();
    details.game_versions = ordered_labels(
        m.latest_files_indexes
            .iter()
            .filter_map(|i| i.game_version_type_id.and_then(curseforge_type_label)),
    );
    details.supports_flavor = Some(index.is_some());
    if let Some(file) = file {
        details.latest_version = Some(file.display_name.clone());
        details.latest_date = Some(file.file_date);
        details.download_size = file.file_length;
        details.release_url = website(&m).map(|w| format!("{w}/files/{}", file.id));
    }
    details.release_notes = notes.filter(|n| !n.trim().is_empty());
    details.release_notes_format = TextFormat::Html;
    Ok(details)
}

pub async fn search(state: &AppState, query: &str, flavor: Flavor) -> Result<Vec<SearchHit>> {
    let info = flavor.info();
    let mut url = url::Url::parse(&format!("{API}/mods/search")).expect("static URL");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("gameId", WOW_GAME_ID)
            .append_pair("gameVersionTypeId", &info.curseforge_type_id.to_string())
            .append_pair("sortField", "2")
            .append_pair("sortOrder", "desc")
            .append_pair("pageSize", "30");
        if !query.trim().is_empty() {
            q.append_pair("searchFilter", query.trim());
        }
    }
    let mods: Vec<CfMod> = get(state, url.as_str()).await?;
    Ok(mods
        .into_iter()
        .map(|m| SearchHit {
            source: SourceRef::new(ProviderId::Curseforge, m.id.to_string()),
            supports_flavor: Some(pick_index(&m, info, Channel::Alpha).is_some()),
            name: m.name.clone(),
            summary: m.summary.clone(),
            author: m.authors.first().map(|a| a.name.clone()),
            thumbnail: m.logo.as_ref().and_then(|l| l.thumbnail_url.clone()),
            website: website(&m),
            downloads: m.download_count.map(|d| d as u64),
            stars: None,
            updated_at: m.date_modified,
            installed: false,
            category: m.categories.first().map(|c| c.name.clone()),
            featured: false,
            starter: false,
        })
        .collect())
}

/// One query for many tracked addons: (addon id, project id, installed file id, channel).
pub async fn check_many(
    state: &AppState,
    items: &[(String, u64, String, Channel)],
    flavor: Flavor,
) -> Result<Vec<UpdateCheck>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<u64> = items.iter().map(|(_, id, _, _)| *id).collect();
    let mods: Vec<CfMod> =
        post(state, &format!("{API}/mods"), &serde_json::json!({ "modIds": ids, "filterPcOnly": true })).await?;
    let info = flavor.info();
    let now = Utc::now();
    Ok(items
        .iter()
        .map(|(addon_id, mod_id, installed, channel)| {
            let found = mods.iter().find(|m| m.id == *mod_id);
            let index = found.and_then(|m| pick_index(m, info, *channel));
            match (found, index) {
                (Some(m), Some(index)) => {
                    let file = m.latest_files.iter().find(|f| f.id == index.file_id);
                    UpdateCheck {
                        addon_id: addon_id.clone(),
                        available: installed.is_empty() || *installed != index.file_id.to_string(),
                        latest_version: Some(
                            file.map(|f| f.display_name.clone())
                                .unwrap_or_else(|| index.filename.trim_end_matches(".zip").to_string()),
                        ),
                        latest_date: file.map(|f| f.file_date),
                        changelog: None,
                        changelog_url: website(m).map(|w| format!("{w}/files/{}", index.file_id)),
                        error: None,
                        checked_at: now,
                    }
                }
                (Some(m), None) => UpdateCheck {
                    addon_id: addon_id.clone(),
                    available: false,
                    latest_version: None,
                    latest_date: None,
                    changelog: None,
                    changelog_url: None,
                    error: Some(format!("{} has no {} file on CurseForge.", m.name, info.name)),
                    checked_at: now,
                },
                _ => UpdateCheck {
                    addon_id: addon_id.clone(),
                    available: false,
                    latest_version: None,
                    latest_date: None,
                    changelog: None,
                    changelog_url: None,
                    error: Some("CurseForge no longer lists this project.".to_string()),
                    checked_at: now,
                },
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_newest_forever_file_within_channel() {
        let m: CfMod = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "Example",
            "latestFilesIndexes": [
                {"fileId": 10, "filename": "a.zip", "releaseType": 1, "gameVersionTypeId": 88568},
                {"fileId": 12, "filename": "b.zip", "releaseType": 2, "gameVersionTypeId": 88568},
                {"fileId": 15, "filename": "c.zip", "releaseType": 1, "gameVersionTypeId": 517}
            ]
        }))
        .unwrap();
        let forever = Flavor::Forever.info();
        assert_eq!(pick_index(&m, forever, Channel::Stable).map(|i| i.file_id), Some(10));
        assert_eq!(pick_index(&m, forever, Channel::Beta).map(|i| i.file_id), Some(12));
        assert_eq!(pick_index(&m, Flavor::Retail.info(), Channel::Stable).map(|i| i.file_id), Some(15));
        assert!(pick_index(&m, Flavor::ClassicEra.info(), Channel::Alpha).is_none());
        assert_eq!(forgecdn_url(4567890, "My Addon.zip"), "https://edge.forgecdn.net/files/4567/890/My%20Addon.zip");
    }
}
