//! GitHub releases. Most popular WoW addons publish with the BigWigs packager,
//! which attaches a `release.json` listing the zip for each game version.
//!
//! For the stable channel we read `releases/latest/download/release.json`
//! directly from github.com, which doesn't touch the API rate limit at all.
//! Everything else goes through the REST API with ETag caching.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use futures::stream::{self, StreamExt};
use serde::Deserialize;

use crate::error::is_not_found;
use crate::games::wow::flavor::{ordered_labels, release_flavor_label, Flavor, FlavorInfo, FLAVORS};
use crate::http::{ensure_success, github_api};
use crate::model::{AddonDetails, Channel, ProviderId, RemoteRelease, SearchHit, SourceRef, TextFormat};
use crate::state::AppState;

#[derive(Deserialize, Debug)]
pub struct ReleaseJson {
    pub releases: Vec<ReleaseEntry>,
}

#[derive(Deserialize, Debug)]
pub struct ReleaseEntry {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub filename: String,
    #[serde(default)]
    pub nolib: bool,
    #[serde(default)]
    pub metadata: Vec<ReleaseMetadata>,
}

#[derive(Deserialize, Debug)]
pub struct ReleaseMetadata {
    pub flavor: String,
    #[serde(default)]
    pub interface: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<DateTime<Utc>>,
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize, Debug)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GhRepo {
    pub full_name: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub stargazers_count: u64,
    pub owner: GhOwner,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub pushed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<GhLicense>,
    #[serde(default)]
    pub has_issues: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GhLicense {
    #[serde(default)]
    pub spdx_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize)]
struct GhReadme {
    content: String,
    path: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GhOwner {
    pub login: String,
}

#[derive(Deserialize)]
struct GhSearch {
    items: Vec<GhRepo>,
}

/// Accepts `owner/repo` or any github.com URL pointing into a repository.
pub fn parse_repo(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_end_matches('/');
    let path = if let Some(idx) = trimmed.find("github.com/") {
        &trimmed[idx + "github.com/".len()..]
    } else if trimmed.contains("://") {
        return None;
    } else {
        trimmed
    };
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?.trim_end_matches(".git");
    let valid = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (valid(owner) && valid(repo)).then(|| format!("{owner}/{repo}"))
}

pub fn avatar_url(owner: &str) -> String {
    format!("https://github.com/{owner}.png?size=96")
}

fn owner_of(repo: &str) -> &str {
    repo.split('/').next().unwrap_or(repo)
}

fn repo_name(repo: &str) -> &str {
    repo.split('/').nth(1).unwrap_or(repo)
}

/// The packaged zip for this game version. Full builds are preferred over "nolib" ones.
pub fn pick_release_entry<'a>(release_json: &'a ReleaseJson, info: &FlavorInfo) -> Option<&'a ReleaseEntry> {
    release_json
        .releases
        .iter()
        .filter(|r| {
            r.metadata.iter().any(|m| {
                let flavor = m.flavor.to_ascii_lowercase();
                info.release_flavors.contains(&flavor.as_str())
                    && m.interface.map(|i| info.accepts_interface(i)).unwrap_or(true)
            })
        })
        .min_by_key(|r| r.nolib)
}

fn name_tokens(file_name: &str) -> Vec<String> {
    file_name
        .to_ascii_lowercase()
        .trim_end_matches(".zip")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Picks a zip by name when there's no release.json: a flavour-tagged asset
/// (`-forever.zip`), else the single untagged zip. Returns (index, verified).
fn pick_asset_by_name(names: &[&str], info: &FlavorInfo) -> Option<(usize, bool)> {
    let zips: Vec<(usize, Vec<String>)> = names
        .iter()
        .enumerate()
        .filter(|(_, n)| n.to_ascii_lowercase().ends_with(".zip"))
        .map(|(i, n)| (i, name_tokens(n)))
        .collect();
    for keyword in info.asset_keywords {
        if let Some((i, _)) = zips.iter().find(|(_, tokens)| tokens.iter().any(|t| t == keyword)) {
            return Some((*i, true));
        }
    }
    let all_keywords: Vec<&str> = FLAVORS.iter().flat_map(|f| f.asset_keywords.iter().copied()).collect();
    let untagged: Vec<usize> = zips
        .iter()
        .filter(|(_, tokens)| !tokens.iter().any(|t| all_keywords.contains(&t.as_str())))
        .map(|(i, _)| *i)
        .collect();
    match (untagged.as_slice(), zips.len()) {
        ([only], _) => Some((*only, false)),
        (_, 1) => Some((zips[0].0, false)),
        _ => None,
    }
}

async fn fetch_release_json(state: &AppState, url: &str) -> Result<ReleaseJson> {
    let response = state.http.get(url).send().await.context("Couldn't reach GitHub. Check your internet connection.")?;
    let response = ensure_success(response, "GitHub").await?;
    response.json::<ReleaseJson>().await.context("The project's release.json couldn't be read")
}

/// `Questie-v12.7.0.zip` -> `v12.7.0` when release.json has no version field.
fn version_from_filename(name: &str, file_name: &str) -> String {
    let stem = file_name.strip_suffix(".zip").unwrap_or(file_name);
    let prefix = format!("{}-", name.to_ascii_lowercase());
    if stem.len() > prefix.len() && stem.to_ascii_lowercase().starts_with(&prefix) {
        stem[prefix.len()..].to_string()
    } else {
        stem.to_string()
    }
}

fn download_url(repo: &str, tag_path: &str, file_name: &str) -> String {
    let mut url = url::Url::parse("https://github.com/").expect("static URL");
    {
        let mut segments = url.path_segments_mut().expect("https URL has a path");
        segments.extend(repo.split('/'));
        segments.push("releases");
        segments.extend(tag_path.split('/'));
        segments.push(file_name);
    }
    url.to_string()
}

fn no_build(repo: &str, info: &FlavorInfo) -> anyhow::Error {
    anyhow!("{} doesn't have a {} build in its latest release yet.", repo_name(repo), info.name)
}

pub async fn resolve(state: &AppState, project: &str, flavor: Flavor, channel: Channel) -> Result<RemoteRelease> {
    let repo = parse_repo(project).ok_or_else(|| anyhow!("\"{project}\" isn't a GitHub repository."))?;
    let info = flavor.info();

    if channel == Channel::Stable {
        let url = format!("https://github.com/{repo}/releases/latest/download/release.json");
        match fetch_release_json(state, &url).await {
            Ok(release_json) => {
                let entry = pick_release_entry(&release_json, info).ok_or_else(|| no_build(&repo, info))?;
                let name = entry.name.clone().unwrap_or_else(|| repo_name(&repo).to_string());
                let version = entry.version.clone().unwrap_or_else(|| version_from_filename(&name, &entry.filename));
                return Ok(RemoteRelease {
                    source: SourceRef::new(ProviderId::Github, &repo),
                    name,
                    version: version.clone(),
                    release_id: version,
                    date: None,
                    download_url: download_url(&repo, "latest/download", &entry.filename),
                    file_name: entry.filename.clone(),
                    changelog: None,
                    changelog_url: Some(format!("https://github.com/{repo}/releases/latest")),
                    website: Some(format!("https://github.com/{repo}")),
                    author: Some(owner_of(&repo).to_string()),
                    summary: None,
                    thumbnail: Some(avatar_url(owner_of(&repo))),
                    prerelease: false,
                    dependencies: Vec::new(),
                    verified_flavor: true,
                });
            }
            Err(error) if is_not_found(&error) => {} // no release.json: use the API below
            Err(error) => return Err(error),
        }
    }
    resolve_via_api(state, &repo, info, channel).await
}

async fn resolve_via_api(state: &AppState, repo: &str, info: &FlavorInfo, channel: Channel) -> Result<RemoteRelease> {
    let body = github_api(state, &format!("https://api.github.com/repos/{repo}/releases?per_page=15"))
        .await
        .map_err(|error| {
            if is_not_found(&error) {
                anyhow!("Couldn't find the GitHub project {repo}. Check the name; private projects aren't supported.")
            } else {
                error
            }
        })?;
    let releases: Vec<GhRelease> = serde_json::from_str(&body).context("GitHub sent release data Hearthforge couldn't read")?;
    let release = releases
        .iter()
        .filter(|r| !r.draft && (channel.allows_prerelease() || !r.prerelease))
        .find(|r| !r.assets.is_empty())
        .ok_or_else(|| anyhow!("{} has no published releases with downloads on GitHub.", repo_name(repo)))?;

    let (asset_index, verified, version) =
        if let Some(manifest) = release.assets.iter().find(|a| a.name.eq_ignore_ascii_case("release.json")) {
            let release_json = fetch_release_json(state, &manifest.browser_download_url).await?;
            let entry = pick_release_entry(&release_json, info).ok_or_else(|| no_build(repo, info))?;
            let index = release
                .assets
                .iter()
                .position(|a| a.name == entry.filename)
                .ok_or_else(|| anyhow!("The release lists {} but doesn't include it.", entry.filename))?;
            (index, true, entry.version.clone().unwrap_or_else(|| release.tag_name.clone()))
        } else {
            let names: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            let (index, verified) = pick_asset_by_name(&names, info).ok_or_else(|| no_build(repo, info))?;
            (index, verified, release.tag_name.clone())
        };
    let asset = &release.assets[asset_index];
    Ok(RemoteRelease {
        source: SourceRef::new(ProviderId::Github, repo),
        name: repo_name(repo).to_string(),
        version: version.clone(),
        release_id: version,
        date: release.published_at,
        download_url: asset.browser_download_url.clone(),
        file_name: asset.name.clone(),
        changelog: release.body.clone().map(|b| b.chars().take(20_000).collect()),
        changelog_url: Some(release.html_url.clone()),
        website: Some(format!("https://github.com/{repo}")),
        author: Some(owner_of(repo).to_string()),
        summary: None,
        thumbnail: Some(avatar_url(owner_of(repo))),
        prerelease: release.prerelease,
        dependencies: Vec::new(),
        verified_flavor: verified,
    })
}

pub async fn repo_info(state: &AppState, repo: &str) -> Result<GhRepo> {
    let body = github_api(state, &format!("https://api.github.com/repos/{repo}")).await?;
    serde_json::from_str(&body).context("GitHub sent project data Hearthforge couldn't read")
}

/// Whether the latest stable release has a build for this flavour, without using the API.
/// `None` when the project doesn't publish a release.json.
pub async fn latest_supports(state: &AppState, repo: &str, flavor: Flavor) -> Option<bool> {
    let url = format!("https://github.com/{repo}/releases/latest/download/release.json");
    match fetch_release_json(state, &url).await {
        Ok(release_json) => Some(pick_release_entry(&release_json, flavor.info()).is_some()),
        Err(_) => None,
    }
}

/// The README as markdown, plus its folder within the repository (for relative links).
async fn readme(state: &AppState, repo: &str) -> Option<(String, String)> {
    use base64::Engine;
    let body = github_api(state, &format!("https://api.github.com/repos/{repo}/readme")).await.ok()?;
    let readme: GhReadme = serde_json::from_str(&body).ok()?;
    let encoded: String = readme.content.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    let text: String = String::from_utf8_lossy(&bytes).chars().take(80_000).collect();
    let folder = readme.path.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
    Some((text, folder))
}

fn info_for_flavor_label(flavor: Flavor) -> String {
    let interface = flavor.info().interface_range.0;
    crate::games::wow::flavor::interface_label(interface).unwrap_or(flavor.info().name).to_string()
}

/// Reads the project's TOC straight from the repository (raw file host, no API quota)
/// to tell whether it targets this game version. Tries the game-specific TOC names first.
async fn probe_toc(state: &AppState, repo: &str, names: &[String], info: &FlavorInfo) -> Option<bool> {
    let mut tried = std::collections::HashSet::new();
    for name in names.iter().filter(|n| !n.is_empty()) {
        let mut files: Vec<String> = info.toc_suffixes.iter().map(|s| format!("{name}_{s}.toc")).collect();
        files.insert(files.len().saturating_sub(1), format!("{name}.toc"));
        for file in files {
            if !tried.insert(file.to_ascii_lowercase()) {
                continue;
            }
            let url = format!("https://raw.githubusercontent.com/{repo}/HEAD/{file}");
            let Ok(response) = state.http.get(&url).send().await else { continue };
            if !response.status().is_success() {
                continue;
            }
            let Ok(text) = response.text().await else { continue };
            let toc = crate::games::wow::toc::parse_toc(&text);
            return Some(toc.is_compatible(info));
        }
    }
    None
}

/// The full details panel for a GitHub project: README, latest release, game versions.
pub async fn details(state: &AppState, project: &str, flavor: Flavor) -> Result<AddonDetails> {
    let repo = parse_repo(project).ok_or_else(|| anyhow!("\"{project}\" isn't a GitHub repository."))?;
    let info = repo_info(state, &repo).await.map_err(|error| {
        if is_not_found(&error) {
            anyhow!("Couldn't find the GitHub project {repo}. It may have been renamed or made private.")
        } else {
            error
        }
    })?;
    let releases_url = format!("https://api.github.com/repos/{repo}/releases?per_page=10");
    let (readme, releases) = tokio::join!(readme(state, &repo), github_api(state, &releases_url));
    let releases: Vec<GhRelease> =
        releases.ok().and_then(|body| serde_json::from_str(&body).ok()).unwrap_or_default();
    let release = releases
        .iter()
        .find(|r| !r.draft && !r.prerelease && !r.assets.is_empty())
        .or_else(|| releases.iter().find(|r| !r.draft && !r.assets.is_empty()));

    let mut details = AddonDetails::new(SourceRef::new(ProviderId::Github, &repo), info.name.clone());
    let owner = info.owner.login.clone();
    details.summary = info.description.clone();
    details.author = Some(owner.clone());
    details.thumbnail = Some(avatar_url(&owner));
    details.website = info.homepage.clone().filter(|h| !h.trim().is_empty()).or_else(|| Some(info.html_url.clone()));
    details.source_url = Some(info.html_url.clone());
    details.issues_url = info.has_issues.then(|| format!("{}/issues", info.html_url));
    details.license = info
        .license
        .as_ref()
        .and_then(|l| l.spdx_id.clone().filter(|id| id != "NOASSERTION").or_else(|| l.name.clone()))
        .filter(|name| !name.eq_ignore_ascii_case("other"));
    details.stars = Some(info.stargazers_count);
    details.updated_at = info.pushed_at;
    details.tags = info
        .topics
        .iter()
        .filter(|t| !matches!(t.as_str(), "wow" | "addon" | "addons" | "world-of-warcraft" | "wow-addon" | "lua"))
        .take(8)
        .cloned()
        .collect();

    if let Some((markdown, folder)) = readme {
        details.description = Some(markdown);
        details.description_format = TextFormat::Markdown;
        details.link_base = Some(format!("https://github.com/{repo}/blob/HEAD/{folder}"));
        details.image_base = Some(format!("https://raw.githubusercontent.com/{repo}/HEAD/{folder}"));
    }

    if let Some(release) = release {
        let info_for_flavor = flavor.info();
        details.latest_version = Some(release.tag_name.clone());
        details.latest_date = release.published_at;
        details.release_notes = release.body.clone().filter(|b| !b.trim().is_empty());
        details.release_notes_format = TextFormat::Markdown;
        details.release_url = Some(release.html_url.clone());
        let manifest = release.assets.iter().find(|a| a.name.eq_ignore_ascii_case("release.json"));
        match manifest {
            Some(manifest) => {
                if let Ok(release_json) = fetch_release_json(state, &manifest.browser_download_url).await {
                    details.game_versions = ordered_labels(
                        release_json
                            .releases
                            .iter()
                            .flat_map(|r| r.metadata.iter())
                            .filter_map(|m| release_flavor_label(&m.flavor)),
                    );
                    let entry = pick_release_entry(&release_json, info_for_flavor);
                    details.supports_flavor = Some(entry.is_some());
                    if let Some(entry) = entry {
                        let name = entry.name.clone().unwrap_or_else(|| repo_name(&repo).to_string());
                        details.latest_version = Some(
                            entry.version.clone().unwrap_or_else(|| version_from_filename(&name, &entry.filename)),
                        );
                        details.download_size =
                            release.assets.iter().find(|a| a.name == entry.filename).map(|a| a.size);
                    }
                }
            }
            None => {
                let names: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
                details.game_versions = ordered_labels(
                    names.iter().flat_map(|n| name_tokens(n)).filter_map(|t| release_flavor_label(&t)),
                );
                match pick_asset_by_name(&names, info_for_flavor) {
                    Some((index, verified)) => {
                        details.download_size = Some(release.assets[index].size);
                        details.supports_flavor = if verified {
                            Some(true)
                        } else {
                            // A single untagged zip: the project's own TOC says which game it targets.
                            let addon = release.assets[index].name.split(['-', '_']).next().unwrap_or("").to_string();
                            probe_toc(state, &repo, &[addon, repo_name(&repo).to_string()], info_for_flavor).await
                        };
                        if details.supports_flavor == Some(true) && details.game_versions.is_empty() {
                            details.game_versions = vec![info_for_flavor_label(flavor)];
                        }
                    }
                    None => details.supports_flavor = Some(false),
                }
            }
        }
    }
    Ok(details)
}

fn looks_like_wow_addon(repo: &GhRepo) -> bool {
    let topics_hit = repo.topics.iter().any(|t| {
        let t = t.to_ascii_lowercase();
        t.contains("wow") || t.contains("warcraft") || t == "addon" || t == "addons"
    });
    let description = repo.description.clone().unwrap_or_default().to_ascii_lowercase();
    let words: Vec<&str> = description.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    topics_hit || words.contains(&"wow") || description.contains("warcraft") || words.contains(&"addon")
}

pub async fn search(state: &AppState, query: &str, flavor: Flavor) -> Result<Vec<SearchHit>> {
    let mut url = url::Url::parse("https://api.github.com/search/repositories").expect("static URL");
    url.query_pairs_mut()
        .append_pair("q", &format!("{} in:name,description,topics fork:false archived:false", query.trim()))
        .append_pair("per_page", "25");
    let body = github_api(state, url.as_str()).await?;
    let results: GhSearch = serde_json::from_str(&body).context("GitHub sent search results Hearthforge couldn't read")?;
    let candidates: Vec<GhRepo> = results.items.into_iter().filter(looks_like_wow_addon).take(12).collect();

    let mut hits: Vec<SearchHit> = stream::iter(candidates)
        .map(|repo| async move {
            let supports = latest_supports(state, &repo.full_name, flavor).await;
            SearchHit {
                source: SourceRef::new(ProviderId::Github, &repo.full_name),
                name: repo.name.clone(),
                summary: repo.description.clone(),
                author: Some(repo.owner.login.clone()),
                thumbnail: Some(avatar_url(&repo.owner.login)),
                website: Some(repo.html_url.clone()),
                downloads: None,
                stars: Some(repo.stargazers_count),
                updated_at: repo.pushed_at,
                supports_flavor: supports,
                installed: false,
                category: None,
                featured: false,
                starter: false,
            }
        })
        .buffered(6)
        .collect()
        .await;
    // Confirmed builds first, then unknown, then projects without a build for this game version.
    hits.sort_by_key(|h| match h.supports_flavor {
        Some(true) => 0,
        None => 1,
        Some(false) => 2,
    });
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repository_references() {
        assert_eq!(parse_repo("Questie/Questie").as_deref(), Some("Questie/Questie"));
        assert_eq!(
            parse_repo("https://github.com/BigWigsMods/BigWigs/releases/tag/v1").as_deref(),
            Some("BigWigsMods/BigWigs")
        );
        assert_eq!(parse_repo("github.com/tullamods/Dominos.git").as_deref(), Some("tullamods/Dominos"));
        assert_eq!(parse_repo("https://example.com/a/b"), None);
        assert_eq!(parse_repo("justone"), None);
    }

    #[test]
    fn picks_the_forever_build_from_release_json() {
        let json = r#"{"releases":[
            {"name":"Grid2","version":"4.1.11","filename":"Grid2-4.1.11.zip","nolib":false,
             "metadata":[{"flavor":"mainline","interface":120105}]},
            {"name":"Grid2","version":"4.1.11","filename":"Grid2-4.1.11-forever.zip","nolib":false,
             "metadata":[{"flavor":"forever","interface":16001}]},
            {"name":"Grid2","version":"4.1.11","filename":"Grid2-4.1.11-forever-nolib.zip","nolib":true,
             "metadata":[{"flavor":"forever","interface":16001}]}
        ]}"#;
        let parsed: ReleaseJson = serde_json::from_str(json).unwrap();
        let entry = pick_release_entry(&parsed, Flavor::Forever.info()).unwrap();
        assert_eq!(entry.filename, "Grid2-4.1.11-forever.zip");
        let retail = pick_release_entry(&parsed, Flavor::Retail.info()).unwrap();
        assert_eq!(retail.filename, "Grid2-4.1.11.zip");
        assert!(pick_release_entry(&parsed, Flavor::ClassicEra.info()).is_none());
    }

    #[test]
    fn picks_assets_by_name_without_release_json() {
        let names = ["MoveAny-1.0-classic.zip", "MoveAny-1.0-forever.zip", "MoveAny-1.0.zip"];
        assert_eq!(pick_asset_by_name(&names, Flavor::Forever.info()), Some((1, true)));
        assert_eq!(pick_asset_by_name(&names, Flavor::Retail.info()), Some((2, false)));
        let single = ["Prat-3.0-3.9.1.zip"];
        assert_eq!(pick_asset_by_name(&single, Flavor::Forever.info()), Some((0, false)));
        let ambiguous = ["A-1.zip", "B-1.zip"];
        assert_eq!(pick_asset_by_name(&ambiguous, Flavor::Forever.info()), None);
    }

    #[test]
    fn derives_versions_from_file_names() {
        assert_eq!(version_from_filename("Questie", "Questie-v12.7.0+v1.0.14.zip"), "v12.7.0+v1.0.14");
        assert_eq!(version_from_filename("Other", "Questie-v1.zip"), "Questie-v1");
    }

    #[test]
    fn builds_encoded_download_urls() {
        assert_eq!(
            download_url("Questie/Questie", "latest/download", "Questie-v12.7.0+v1.0.14.zip"),
            "https://github.com/Questie/Questie/releases/latest/download/Questie-v12.7.0+v1.0.14.zip"
        );
        assert_eq!(
            download_url("a/b", "latest/download", "My Addon.zip"),
            "https://github.com/a/b/releases/latest/download/My%20Addon.zip"
        );
    }
}
