// Mirrors the camelCase JSON produced by the Rust side (src-tauri/src/model.rs).

export type Flavor = "forever" | "retail" | "classicEra" | "classic" | "anniversary";
export type Channel = "stable" | "beta" | "alpha";
export type ProviderId = "github" | "curseforge" | "tukui" | "manual";
export type LaunchMethod = "battlenet" | "direct";
export type AfterLaunch = "keep" | "tray" | "quit";
export type Theme = "dark" | "light" | "system";
export type Compatibility = "compatible" | "outOfDate" | "olderPatch" | "missing";
export type SearchScope = "all" | "catalog" | "github" | "curseforge" | "tukui";
export type SecretKind = "githubToken" | "curseforgeKey";
export type FolderTarget = "addons" | "wtf" | "client" | "backups" | "logs" | "data";

export interface SourceRef {
  provider: ProviderId;
  projectId: string;
}

export interface SourceHint {
  name?: string | null;
  summary?: string | null;
  author?: string | null;
  thumbnail?: string | null;
  website?: string | null;
}

export interface Settings {
  theme: Theme;
  checkOnStartup: boolean;
  backgroundChecks: boolean;
  checkIntervalHours: number;
  autoInstallUpdates: boolean;
  updateBeforeLaunch: boolean;
  backupBeforeLaunch: boolean;
  backupRetention: number;
  backupDir: string | null;
  afterLaunch: AfterLaunch;
  closeToTray: boolean;
  startWithSystem: boolean;
  defaultChannel: Channel;
  appAutoUpdate: boolean;
  notifications: boolean;
  catalogUrl: string | null;
  lastInstallation: string | null;
}

export interface Installation {
  id: string;
  game: string;
  flavor: Flavor;
  label: string;
  path: string;
  launchMethod: LaunchMethod;
  defaultChannel: Channel | null;
  addedAt: string | null;
  flavorName: string;
  available: boolean;
  clientVersion: string | null;
  /** The game's own icon, read from its executable (PNG data URL). */
  icon: string | null;
}

export interface SecretStatus {
  githubToken: boolean;
  curseforgeKey: boolean;
}

export interface Bootstrap {
  appVersion: string;
  settings: Settings;
  installations: Installation[];
  secrets: SecretStatus;
  updaterConfigured: boolean;
  dataDir: string;
  platform: string;
  catalogVersion: number;
}

export interface DetectedInstall {
  flavor: Flavor;
  label: string;
  path: string;
  addonCount: number;
  version: string | null;
  icon: string | null;
  alreadyAdded: boolean;
}

export interface UpdateCheck {
  addonId: string;
  available: boolean;
  latestVersion: string | null;
  latestDate: string | null;
  changelog: string | null;
  changelogUrl: string | null;
  error: string | null;
  checkedAt: string;
}

export interface FolderView {
  name: string;
  title: string | null;
  version: string | null;
  interfaces: number[];
  tocFile: string | null;
  otherTocs: string[];
  compatibility: Compatibility;
  dependencies: string[];
  loadOnDemand: boolean;
}

export interface AddonView {
  id: string;
  managed: boolean;
  name: string;
  version: string | null;
  author: string | null;
  notes: string | null;
  folders: FolderView[];
  source: SourceRef | null;
  channel: Channel | null;
  pinned: boolean;
  autoUpdate: boolean | null;
  website: string | null;
  thumbnail: string | null;
  installedAt: string | null;
  releaseDate: string | null;
  compatibility: Compatibility;
  missingDependencies: string[];
  missingFolders: string[];
  suggestedSource: SourceRef | null;
  update: UpdateCheck | null;
}

export interface AddonRecord {
  id: string;
  name: string;
  source: SourceRef;
  folders: string[];
  installedVersion: string;
  installedReleaseId: string;
  installedAt: string;
  channel: Channel | null;
  pinned: boolean;
  autoUpdate: boolean | null;
}

export interface SearchHit {
  source: SourceRef;
  name: string;
  summary: string | null;
  author: string | null;
  thumbnail: string | null;
  website: string | null;
  downloads: number | null;
  stars: number | null;
  updatedAt: string | null;
  supportsFlavor: boolean | null;
  installed: boolean;
  category: string | null;
  featured: boolean;
  starter: boolean;
}

export type TextFormat = "text" | "markdown" | "html";

export interface AddonDetails {
  source: SourceRef;
  name: string;
  summary: string | null;
  author: string | null;
  thumbnail: string | null;
  website: string | null;
  sourceUrl: string | null;
  issuesUrl: string | null;
  donateUrl: string | null;
  license: string | null;
  stars: number | null;
  downloads: number | null;
  updatedAt: string | null;
  tags: string[];
  description: string | null;
  descriptionFormat: TextFormat;
  screenshots: string[];
  gameVersions: string[];
  supportsFlavor: boolean | null;
  latestVersion: string | null;
  latestDate: string | null;
  releaseNotes: string | null;
  releaseNotesFormat: TextFormat;
  releaseUrl: string | null;
  downloadSize: number | null;
  linkBase: string | null;
  imageBase: string | null;
}

export interface SearchResults {
  hits: SearchHit[];
  warnings: string[];
}

export type TaskStage = "resolving" | "downloading" | "installing" | "done" | "failed";

export interface TaskProgress {
  installationId: string;
  key: string;
  label: string;
  stage: TaskStage;
  received: number;
  total: number | null;
  message: string | null;
}

export interface BatchResult {
  succeeded: string[];
  failed: { name: string; error: string }[];
}

export interface CachedUpdates {
  checks: UpdateCheck[];
  checking: boolean;
}

export interface PackPreview {
  name: string;
  flavor: Flavor;
  flavorMatches: boolean;
  items: { source: SourceRef; name: string; installed: boolean }[];
}

export type BackupReason = "manual" | "launch" | "update" | "restore";

export interface BackupInfo {
  id: string;
  path: string;
  createdAt: string;
  size: number;
  reason: BackupReason;
}

export interface BackupList {
  folder: string;
  backups: BackupInfo[];
  hasSettings: boolean;
}

export interface PlayOutcome {
  method: string;
  updated: number;
  backedUp: boolean;
  warnings: string[];
}

export type PlayStage = "backingUp" | "updating" | "launching" | "done";

export interface InstallationPatch {
  label?: string;
  launchMethod?: LaunchMethod;
  flavor?: Flavor;
  defaultChannel?: Channel | null;
}

export interface AddonOptions {
  pinned?: boolean;
  channel?: Channel | null;
  autoUpdate?: boolean | null;
}
