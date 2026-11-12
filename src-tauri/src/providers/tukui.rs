//! Tukui.org: home of ElvUI and Tukui. A small public API with one zip per addon
//! covering every game version it supports.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use serde::Deserialize;

use crate::games::wow::flavor::{interface_label, ordered_labels, patch_to_interface, Flavor, FlavorInfo};
use crate::http::get_json;
use crate::model::{AddonDetails, ProviderId, RemoteRelease, SearchHit, SourceRef, TextFormat};
use crate::state::AppState;

const LIST_URL: &str = "https://api.tukui.org/v1/addons";
const CACHE_FOR: Duration = Duration::from_secs(20 * 60);

#[derive(Deserialize, Debug, Clone)]
pub struct TukuiAddon {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    pub url: String,
    pub version: String,
    #[serde(default)]
    pub changelog_url: Option<String>,
    #[serde(default)]
    pub patch: Vec<String>,
    #[serde(default)]
    pub last_update: Option<String>,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub small_desc: Option<String>,
    #[serde(default)]
    pub logo_square_url: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub screenshot_url: Option<String>,
    #[serde(default)]
    pub gallery_url: Vec<String>,
    #[serde(default)]
    pub ticket_url: Option<String>,
    #[serde(default)]
    pub git_url: Option<String>,
    #[serde(default)]
    pub donate_url: Option<String>,
}

impl TukuiAddon {
    pub fn supports(&self, info: &FlavorInfo) -> bool {
        self.patch.iter().filter_map(|p| patch_to_interface(p)).any(|i| info.accepts_interface(i))
    }

    fn updated(&self) -> Option<chrono::DateTime<Utc>> {
        let date = NaiveDate::parse_from_str(self.last_update.as_deref()?, "%Y-%m-%d").ok()?;
        Utc.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).single()
    }
}

pub async fn list(state: &AppState) -> Result<Vec<TukuiAddon>> {
    if let Some((fetched, addons)) = state.tukui_cache.lock().unwrap().as_ref() {
        if fetched.elapsed() < CACHE_FOR {
            return Ok(addons.clone());
        }
    }
    let addons: Vec<TukuiAddon> = get_json(&state.http, LIST_URL, "Tukui").await?;
    *state.tukui_cache.lock().unwrap() = Some((Instant::now(), addons.clone()));
    Ok(addons)
}

pub async fn resolve(state: &AppState, slug: &str, flavor: Flavor) -> Result<RemoteRelease> {
    let addons = list(state).await?;
    let addon = addons
        .iter()
        .find(|a| a.slug.eq_ignore_ascii_case(slug.trim()))
        .ok_or_else(|| anyhow!("Tukui doesn't list \"{slug}\"."))?;
    let info = flavor.info();
    if !addon.supports(info) {
        return Err(anyhow!("{} doesn't support {} yet.", addon.name, info.name));
    }
    Ok(RemoteRelease {
        source: SourceRef::new(ProviderId::Tukui, &addon.slug),
        name: addon.name.clone(),
        version: addon.version.clone(),
        release_id: addon.version.clone(),
        date: addon.updated(),
        download_url: addon.url.clone(),
        file_name: format!("{}-{}.zip", addon.name, addon.version),
        changelog: None,
        changelog_url: addon.changelog_url.clone(),
        website: addon.web_url.clone(),
        author: addon.author.clone(),
        summary: addon.small_desc.clone(),
        thumbnail: addon.logo_square_url.clone(),
        prerelease: false,
        dependencies: Vec::new(),
        verified_flavor: true,
    })
}

/// The newest entry of a markdown changelog: everything before the second heading.
fn first_section(markdown: &str) -> String {
    let mut seen_heading = false;
    let mut out = Vec::new();
    for line in markdown.lines() {
        if line.starts_with('#') {
            if seen_heading {
                break;
            }
            seen_heading = true;
        }
        out.push(line);
    }
    out.join("\n").trim().to_string()
}

pub async fn details(state: &AppState, slug: &str, flavor: Flavor) -> Result<AddonDetails> {
    let addons = list(state).await?;
    let addon = addons
        .iter()
        .find(|a| a.slug.eq_ignore_ascii_case(slug.trim()))
        .ok_or_else(|| anyhow!("Tukui doesn't list \"{slug}\"."))?;
    let mut details = AddonDetails::new(SourceRef::new(ProviderId::Tukui, &addon.slug), addon.name.clone());
    details.summary = addon.small_desc.clone();
    details.author = addon.author.clone();
    details.thumbnail = addon.logo_square_url.clone();
    details.website = addon.web_url.clone();
    details.source_url = addon.git_url.clone();
    details.issues_url = addon.ticket_url.clone();
    details.donate_url = addon.donate_url.clone();
    details.updated_at = addon.updated();
    details.description = addon.desc.clone();
    details.description_format = TextFormat::Markdown;
    details.screenshots = addon.screenshot_url.iter().chain(addon.gallery_url.iter()).cloned().collect();
    details.game_versions =
        ordered_labels(addon.patch.iter().filter_map(|p| patch_to_interface(p)).filter_map(interface_label));
    details.supports_flavor = Some(addon.supports(flavor.info()));
    details.latest_version = Some(addon.version.clone());
    details.latest_date = addon.updated();
    details.release_url = addon.changelog_url.clone();
    if let Some(url) = &addon.changelog_url {
        if let Ok(response) = state.http.get(url).send().await {
            if let Ok(text) = response.error_for_status().map(|r| r.text()) {
                if let Ok(markdown) = text.await {
                    details.release_notes = Some(first_section(&markdown)).filter(|s| !s.is_empty());
                    details.release_notes_format = TextFormat::Markdown;
                }
            }
        }
    }
    Ok(details)
}

pub async fn search(state: &AppState, query: &str, flavor: Flavor) -> Result<Vec<SearchHit>> {
    let q = query.trim().to_lowercase();
    let info = flavor.info();
    Ok(list(state)
        .await?
        .into_iter()
        .filter(|a| {
            q.is_empty()
                || a.name.to_lowercase().contains(&q)
                || a.slug.contains(&q)
                || a.small_desc.as_deref().unwrap_or("").to_lowercase().contains(&q)
        })
        .map(|a| SearchHit {
            source: SourceRef::new(ProviderId::Tukui, &a.slug),
            supports_flavor: Some(a.supports(info)),
            name: a.name.clone(),
            summary: a.small_desc.clone(),
            author: a.author.clone(),
            thumbnail: a.logo_square_url.clone(),
            website: a.web_url.clone(),
            downloads: None,
            stars: None,
            updated_at: a.updated(),
            installed: false,
            category: Some("User interface".to_string()),
            featured: false,
            starter: false,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_supported_patches() {
        let addon: TukuiAddon = serde_json::from_value(serde_json::json!({
            "slug": "elvui", "name": "ElvUI", "url": "https://example.invalid/elvui.zip", "version": "16.01",
            "patch": ["12.1.5", "1.60.1", "5.5.4", "1.15.9", "2.5.6"], "last_update": "2026-11-03"
        }))
        .unwrap();
        assert!(addon.supports(Flavor::Forever.info()));
        assert!(addon.supports(Flavor::ClassicEra.info()));
        assert!(addon.updated().is_some());
    }

    #[test]
    fn keeps_only_the_newest_changelog_entry() {
        let changelog = "### Version 16.01 [ November 3rd ]\n* Fix A\n* Fix B\n\n### Version 16.00\n* Old\n";
        assert_eq!(first_section(changelog), "### Version 16.01 [ November 3rd ]\n* Fix A\n* Fix B");
    }
}
