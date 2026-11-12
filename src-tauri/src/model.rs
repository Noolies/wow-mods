//! Data types shared between the engine, persistence and the frontend.
//! Everything crossing the IPC boundary is camelCase JSON.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::games::wow::flavor::Flavor;

/// Release channel an addon follows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
    Alpha,
}

impl Channel {
    pub fn allows_prerelease(self) -> bool {
        self != Channel::Stable
    }

    /// CurseForge release types: 1 = release, 2 = beta, 3 = alpha.
    pub fn curseforge_max_release_type(self) -> u8 {
        match self {
            Channel::Stable => 1,
            Channel::Beta => 2,
            Channel::Alpha => 3,
        }
    }
}

/// Where an addon comes from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Github,
    Curseforge,
    Tukui,
    /// Installed from a zip file or folder; never updated automatically.
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub provider: ProviderId,
    /// `owner/repo` for GitHub, the numeric project id for CurseForge, the slug for Tukui.
    pub project_id: String,
}

impl SourceRef {
    pub fn new(provider: ProviderId, project_id: impl Into<String>) -> Self {
        Self { provider, project_id: project_id.into() }
    }

    /// Stable key for maps, de-duplication and progress events: `github:owner/repo`.
    /// The UI builds the same key to match progress to a search result.
    pub fn key(&self) -> String {
        let provider = match self.provider {
            ProviderId::Github => "github",
            ProviderId::Curseforge => "curseforge",
            ProviderId::Tukui => "tukui",
            ProviderId::Manual => "manual",
        };
        format!("{provider}:{}", self.project_id.to_lowercase())
    }

    pub fn same_as(&self, other: &SourceRef) -> bool {
        self.provider == other.provider && self.project_id.eq_ignore_ascii_case(&other.project_id)
    }
}

/// Extra details the UI already knows about an addon it asks us to install.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceHint {
    pub name: Option<String>,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub thumbnail: Option<String>,
    pub website: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMethod {
    /// Ask the Battle.net app to start the game (handles login).
    #[default]
    Battlenet,
    /// Run the game executable directly. Battle.net must already be signed in.
    Direct,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AfterLaunch {
    /// Leave the window as it is.
    Keep,
    /// Hide to the system tray.
    #[default]
    Tray,
    /// Quit Hearthforge.
    Quit,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

/// App-wide preferences. Secrets (tokens, API keys) are never stored here:
/// they live in the OS credential store (see `secrets.rs`).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: Theme,
    pub check_on_startup: bool,
    pub background_checks: bool,
    pub check_interval_hours: u32,
    pub auto_install_updates: bool,
    pub update_before_launch: bool,
    pub backup_before_launch: bool,
    pub backup_retention: u32,
    pub backup_dir: Option<String>,
    pub after_launch: AfterLaunch,
    pub close_to_tray: bool,
    pub start_with_system: bool,
    pub default_channel: Channel,
    pub app_auto_update: bool,
    pub notifications: bool,
    pub catalog_url: Option<String>,
    pub last_installation: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            check_on_startup: true,
            background_checks: true,
            check_interval_hours: 3,
            auto_install_updates: false,
            update_before_launch: true,
            backup_before_launch: true,
            backup_retention: 10,
            backup_dir: None,
            after_launch: AfterLaunch::Tray,
            close_to_tray: true,
            start_with_system: false,
            default_channel: Channel::Stable,
            app_auto_update: true,
            notifications: true,
            catalog_url: None,
            last_installation: None,
        }
    }
}

/// A game client the user manages, e.g. `...\World of Warcraft\_forever_`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: String,
    pub game: String,
    pub flavor: Flavor,
    pub label: String,
    pub path: PathBuf,
    #[serde(default)]
    pub launch_method: LaunchMethod,
    #[serde(default)]
    pub default_channel: Option<Channel>,
    #[serde(default)]
    pub added_at: Option<DateTime<Utc>>,
}

impl Installation {
    pub fn interface_dir(&self) -> PathBuf {
        self.path.join("Interface")
    }

    pub fn addons_dir(&self) -> PathBuf {
        self.interface_dir().join("AddOns")
    }

    pub fn wtf_dir(&self) -> PathBuf {
        self.path.join("WTF")
    }
}

/// An addon Hearthforge installed (or adopted) and keeps up to date.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AddonRecord {
    pub id: String,
    pub name: String,
    pub source: SourceRef,
    pub folders: Vec<String>,
    pub installed_version: String,
    /// Provider-specific identity of the installed build. Empty when adopted from an
    /// existing install, in which case versions are compared loosely instead.
    pub installed_release_id: String,
    pub installed_at: DateTime<Utc>,
    #[serde(default)]
    pub release_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub channel: Option<Channel>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub auto_update: Option<bool>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
}

/// The newest build a provider offers for one addon.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRelease {
    pub source: SourceRef,
    pub name: String,
    pub version: String,
    pub release_id: String,
    pub date: Option<DateTime<Utc>>,
    pub download_url: String,
    pub file_name: String,
    pub changelog: Option<String>,
    pub changelog_url: Option<String>,
    pub website: Option<String>,
    pub author: Option<String>,
    pub summary: Option<String>,
    pub thumbnail: Option<String>,
    pub prerelease: bool,
    /// Required dependencies the provider knows about (CurseForge only).
    pub dependencies: Vec<SourceRef>,
    /// True when the provider explicitly lists this game version for the build.
    pub verified_flavor: bool,
}

/// Result of checking one tracked addon for updates.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub addon_id: String,
    pub available: bool,
    pub latest_version: Option<String>,
    pub latest_date: Option<DateTime<Utc>>,
    pub changelog: Option<String>,
    pub changelog_url: Option<String>,
    pub error: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Compatibility {
    /// Has a TOC for this game version with a matching interface number.
    Compatible,
    /// Has a TOC, but its interface number is for another game version.
    OutOfDate,
    /// Built for this game, but for an earlier patch than the one installed.
    OlderPatch,
    /// No TOC file this game version will load.
    Missing,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub name: String,
    pub title: Option<String>,
    pub version: Option<String>,
    pub interfaces: Vec<u32>,
    pub toc_file: Option<String>,
    /// TOC files for other game versions, e.g. `Addon_Vanilla.toc`.
    pub other_tocs: Vec<String>,
    pub compatibility: Compatibility,
    pub dependencies: Vec<String>,
    pub load_on_demand: bool,
}

/// One row in the "Installed" list: a managed addon or a group of unmanaged folders.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AddonView {
    pub id: String,
    pub managed: bool,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub notes: Option<String>,
    pub folders: Vec<FolderView>,
    pub source: Option<SourceRef>,
    pub channel: Option<Channel>,
    pub pinned: bool,
    pub auto_update: Option<bool>,
    pub website: Option<String>,
    pub thumbnail: Option<String>,
    pub installed_at: Option<DateTime<Utc>>,
    pub release_date: Option<DateTime<Utc>>,
    pub compatibility: Compatibility,
    pub missing_dependencies: Vec<String>,
    pub missing_folders: Vec<String>,
    pub suggested_source: Option<SourceRef>,
    pub update: Option<UpdateCheck>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub source: SourceRef,
    pub name: String,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub thumbnail: Option<String>,
    pub website: Option<String>,
    pub downloads: Option<u64>,
    pub stars: Option<u64>,
    pub updated_at: Option<DateTime<Utc>>,
    /// `Some(true)` when a build for this game version is confirmed, `None` when unknown.
    pub supports_flavor: Option<bool>,
    pub installed: bool,
    pub category: Option<String>,
    pub featured: bool,
    pub starter: bool,
}

/// How a description or set of release notes is written.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TextFormat {
    Text,
    Markdown,
    Html,
}

/// Everything shown on an addon's details panel while browsing.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AddonDetails {
    pub source: SourceRef,
    pub name: String,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub thumbnail: Option<String>,
    pub website: Option<String>,
    pub source_url: Option<String>,
    pub issues_url: Option<String>,
    pub donate_url: Option<String>,
    pub license: Option<String>,
    pub stars: Option<u64>,
    pub downloads: Option<u64>,
    pub updated_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub description_format: TextFormat,
    pub screenshots: Vec<String>,
    /// Game versions the latest release supports, e.g. "WoW Forever", "Classic Era".
    pub game_versions: Vec<String>,
    /// Whether the latest release has a build for the selected game (`None` = can't tell).
    pub supports_flavor: Option<bool>,
    pub latest_version: Option<String>,
    pub latest_date: Option<DateTime<Utc>>,
    pub release_notes: Option<String>,
    pub release_notes_format: TextFormat,
    pub release_url: Option<String>,
    pub download_size: Option<u64>,
    /// Bases for resolving relative links and images in a GitHub README.
    pub link_base: Option<String>,
    pub image_base: Option<String>,
}

impl AddonDetails {
    pub fn new(source: SourceRef, name: String) -> Self {
        Self {
            source,
            name,
            summary: None,
            author: None,
            thumbnail: None,
            website: None,
            source_url: None,
            issues_url: None,
            donate_url: None,
            license: None,
            stars: None,
            downloads: None,
            updated_at: None,
            tags: Vec::new(),
            description: None,
            description_format: TextFormat::Text,
            screenshots: Vec::new(),
            game_versions: Vec::new(),
            supports_flavor: None,
            latest_version: None,
            latest_date: None,
            release_notes: None,
            release_notes_format: TextFormat::Text,
            release_url: None,
            download_size: None,
            link_base: None,
            image_base: None,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DetectedInstall {
    pub flavor: Flavor,
    pub label: String,
    pub path: PathBuf,
    pub addon_count: usize,
    pub version: Option<String>,
    pub icon: Option<String>,
    pub already_added: bool,
}

/// Progress events streamed to the UI while installing or updating.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub installation_id: String,
    pub key: String,
    pub label: String,
    pub stage: TaskStage,
    pub received: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStage {
    Resolving,
    Downloading,
    Installing,
    Done,
    Failed,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<BatchFailure>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BatchFailure {
    pub name: String,
    pub error: String,
}
