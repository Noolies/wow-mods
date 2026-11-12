// In-memory stand-in for the Rust backend, used when the UI runs in a normal
// browser (npm run dev). Add `?welcome` to the URL to see first-run onboarding.

import type {
  AddonView,
  BackupInfo,
  Bootstrap,
  DetectedInstall,
  Installation,
  SearchHit,
  Settings,
  SourceRef,
  TaskProgress,
  UpdateCheck,
} from "./types";

type Handler = (payload: unknown) => void;
const listeners = new Map<string, Set<Handler>>();

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const set = listeners.get(event) ?? new Set<Handler>();
  set.add(handler as Handler);
  listeners.set(event, set);
  return () => set.delete(handler as Handler);
}

function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((h) => h(payload));
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const now = () => new Date().toISOString();
const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

const settings: Settings = {
  theme: "dark",
  checkOnStartup: true,
  backgroundChecks: true,
  checkIntervalHours: 3,
  autoInstallUpdates: false,
  updateBeforeLaunch: true,
  backupBeforeLaunch: true,
  backupRetention: 10,
  backupDir: null,
  afterLaunch: "tray",
  closeToTray: true,
  startWithSystem: false,
  defaultChannel: "stable",
  appAutoUpdate: true,
  notifications: true,
  catalogUrl: null,
  lastInstallation: "forever",
};

const welcome = typeof location !== "undefined" && location.search.includes("welcome");

let installations: Installation[] = welcome
  ? []
  : [
      {
        id: "forever",
        game: "wow",
        flavor: "forever",
        label: "WoW Forever",
        path: "C:\\Program Files (x86)\\World of Warcraft\\_forever_",
        launchMethod: "battlenet",
        defaultChannel: null,
        addedAt: ago(9000),
        flavorName: "WoW Forever",
        available: true,
        clientVersion: "1.60.1.70888",
        icon: null,
      },
      {
        id: "retail",
        game: "wow",
        flavor: "retail",
        label: "World of Warcraft",
        path: "C:\\Program Files (x86)\\World of Warcraft\\_retail_",
        launchMethod: "battlenet",
        defaultChannel: null,
        addedAt: ago(9000),
        flavorName: "World of Warcraft",
        available: true,
        clientVersion: "12.1.5.71011",
        icon: null,
      },
    ];

const gh = (projectId: string): SourceRef => ({ provider: "github", projectId });
const avatar = (owner: string) => `https://github.com/${owner}.png?size=96`;
const folder = (name: string, version = "1.0", compatibility: AddonView["compatibility"] = "compatible") => ({
  name,
  title: name,
  version,
  interfaces: compatibility === "compatible" ? [16001] : [120105],
  tocFile: `${name}.toc`,
  otherTocs: [],
  compatibility,
  dependencies: [],
  loadOnDemand: false,
});

function managed(
  id: string,
  name: string,
  source: SourceRef,
  version: string,
  notes: string,
  extra: Partial<AddonView> = {},
): AddonView {
  return {
    id,
    managed: true,
    name,
    version,
    author: source.projectId.split("/")[0],
    notes,
    folders: [folder(name.replace(/\s+/g, ""), version)],
    source,
    channel: null,
    pinned: false,
    autoUpdate: null,
    website: source.provider === "github" ? `https://github.com/${source.projectId}` : "https://tukui.org/elvui",
    thumbnail: source.provider === "github" ? avatar(source.projectId.split("/")[0]) : null,
    installedAt: ago(4000),
    releaseDate: ago(6000),
    compatibility: "compatible",
    missingDependencies: [],
    missingFolders: [],
    suggestedSource: null,
    update: { addonId: id, available: false, latestVersion: version, latestDate: null, changelog: null, changelogUrl: null, error: null, checkedAt: ago(4) },
    ...extra,
  };
}

const addons: Record<string, AddonView[]> = {
  forever: [
    managed("questie", "Questie", gh("Questie/Questie"), "v12.6.2", "Shows quest givers, objectives and turn-ins on your map and minimap.", {
      update: {
        addonId: "questie",
        available: true,
        latestVersion: "v12.7.0+v1.0.14",
        latestDate: ago(800),
        changelog: "Forever 1.0.14\n- Added Skyborne Elf starting zone quests\n- Fixed objective arrows in Gilneas\n- Database updates for the new dungeons",
        changelogUrl: "https://github.com/Questie/Questie/releases/latest",
        error: null,
        checkedAt: ago(4),
      },
    }),
    managed("bigwigs", "BigWigs", gh("BigWigsMods/BigWigs"), "v427.9", "Raid boss warnings and timers that stay out of your way.", {
      folders: [folder("BigWigs", "v427.9"), folder("BigWigs_Core", "v427.9"), folder("BigWigs_Plugins", "v427.9"), folder("BigWigs_Options", "v427.9")],
    }),
    managed("bartender", "Bartender4", gh("Nevcairiel/Bartender4"), "4.18.4", "Replaces the action bars with flexible bars you can move and resize."),
    managed("elvui", "ElvUI", { provider: "tukui", projectId: "elvui" }, "16.01", "A full interface replacement: bars, unit frames, bags, chat and more.", {
      author: "Elv",
      thumbnail: "https://storage.tukui.org/ElvUI-Logo-Square.png",
      folders: [folder("ElvUI", "16.01"), folder("ElvUI_Libraries", "16.01"), folder("ElvUI_Options", "16.01")],
      update: { addonId: "elvui", available: true, latestVersion: "16.02", latestDate: ago(300), changelog: null, changelogUrl: "https://api.tukui.org/v1/changelog/elvui", error: null, checkedAt: ago(4) },
    }),
    managed("plater", "Plater Nameplates", gh("Tercioo/Plater-Nameplates"), "Plater-v661", "Configurable nameplates with threat colours and cast bars.", {
      pinned: true,
      update: { addonId: "plater", available: true, latestVersion: "Plater-v663", latestDate: ago(9000), changelog: null, changelogUrl: null, error: null, checkedAt: ago(4) },
    }),
    managed("masque", "Masque", gh("SFX-WoW/Masque"), "12.1.8", "Skins for action buttons, auras and bags."),
    managed("rxp", "RestedXP Guides", gh("RestedXP/RXPGuides"), "v4.11.36", "Step-by-step levelling routes shown right in the game.", {
      update: { addonId: "rxp", available: false, latestVersion: null, latestDate: null, changelog: null, changelogUrl: null, error: "GitHub's hourly limit for anonymous requests is used up (it resets at 2:41 PM). Add a GitHub token in Settings to raise the limit.", checkedAt: ago(4) },
    }),
    {
      ...managed("macros", "Guild Macros", { provider: "manual", projectId: "GuildMacros.zip" }, "2.1", "Shared macros for raid night.", { update: null, thumbnail: null }),
    },
    {
      id: "local:Leatrix_Plus",
      managed: false,
      name: "Leatrix Plus",
      version: "1.60.04",
      author: "Leatrix",
      notes: "Dozens of small quality-of-life fixes.",
      folders: [folder("Leatrix_Plus", "1.60.04")],
      source: null,
      channel: null,
      pinned: false,
      autoUpdate: null,
      website: null,
      thumbnail: null,
      installedAt: null,
      releaseDate: null,
      compatibility: "compatible",
      missingDependencies: [],
      missingFolders: [],
      suggestedSource: null,
      update: null,
    },
    {
      id: "local:MoveAny",
      managed: false,
      name: "MoveAny",
      version: "v1.14.10",
      author: "d4kir92",
      notes: "Move, scale and hide almost any part of the default interface.",
      folders: [folder("MoveAny", "v1.14.10")],
      source: null,
      channel: null,
      pinned: false,
      autoUpdate: null,
      website: null,
      thumbnail: null,
      installedAt: null,
      releaseDate: null,
      compatibility: "compatible",
      missingDependencies: [],
      missingFolders: [],
      suggestedSource: gh("d4kir92/MoveAny"),
      update: null,
    },
    {
      id: "local:Details",
      managed: false,
      name: "Details! Damage Meter",
      version: "Details.20261020.13824.162",
      author: "Terciob",
      notes: "Damage, healing and threat meters.",
      folders: [folder("Details", "13824", "outOfDate"), folder("Details_DataStorage", "13824", "outOfDate")],
      source: null,
      channel: null,
      pinned: false,
      autoUpdate: null,
      website: null,
      thumbnail: null,
      installedAt: null,
      releaseDate: null,
      compatibility: "outOfDate",
      missingDependencies: [],
      missingFolders: [],
      suggestedSource: null,
      update: null,
    },
  ],
  retail: [],
};
addons.forever.sort((a, b) => a.name.localeCompare(b.name));

const catalogHits: SearchHit[] = [
  ["Questie/Questie", "Questie", "Quests", "Shows quest givers, objectives and turn-ins on your map and minimap.", true],
  ["BigWigsMods/BigWigs", "BigWigs", "Boss encounters", "Raid boss warnings and timers that stay out of your way.", true],
  ["BigWigsMods/LittleWigs", "LittleWigs", "Boss encounters", "BigWigs warnings and timers for dungeon bosses.", false],
  ["DeadlyBossMods/DeadlyBossMods", "Deadly Boss Mods", "Boss encounters", "Raid and dungeon boss alerts, timers and voice warnings.", false],
  ["Nevcairiel/Bartender4", "Bartender4", "Action bars", "Replaces the action bars with flexible bars you can move and resize.", false],
  ["tullamods/Dominos", "Dominos", "Action bars", "Lightweight action bars you can move, scale and fade.", false],
  ["tullamods/tullaRange", "tullaRange", "Action bars", "Tints action buttons when the target is out of range or you lack power.", true],
  ["SFX-WoW/Masque", "Masque", "Action bars", "Skins for action buttons, auras and bags.", false],
  ["Tercioo/Plater-Nameplates", "Plater Nameplates", "Nameplates", "Configurable nameplates with threat colours and cast bars.", false],
  ["michaelnpsp/Grid2", "Grid2", "Unit frames", "Compact, highly configurable party and raid frames.", false],
  ["RestedXP/RXPGuides", "RestedXP Guides", "Levelling", "Step-by-step levelling routes shown right in the game.", false],
  ["Nevcairiel/Mapster", "Mapster", "Map", "A cleaner world map with coordinates and fog-of-war options.", true],
  ["d4kir92/MoveAny", "MoveAny", "Interface", "Move, scale and hide almost any part of the default interface.", false],
].map(([projectId, name, category, summary, starter]) => ({
  source: gh(projectId as string),
  name: name as string,
  summary: summary as string,
  author: (projectId as string).split("/")[0],
  thumbnail: avatar((projectId as string).split("/")[0]),
  website: `https://github.com/${projectId}`,
  downloads: null,
  stars: null,
  updatedAt: null,
  supportsFlavor: true,
  installed: false,
  category: category as string,
  featured: true,
  starter: starter as boolean,
}));

let backups: BackupInfo[] = [
  { id: "WTF-1-launch.zip", path: "", createdAt: ago(90), size: 18_400_000, reason: "launch" },
  { id: "WTF-2-launch.zip", path: "", createdAt: ago(1500), size: 18_200_000, reason: "launch" },
  { id: "WTF-3-manual.zip", path: "", createdAt: ago(5200), size: 17_900_000, reason: "manual" },
];

function isInstalled(installationId: string, source: SourceRef) {
  return (addons[installationId] ?? []).some(
    (a) => a.source && a.source.provider === source.provider && a.source.projectId.toLowerCase() === source.projectId.toLowerCase(),
  );
}

async function fakeInstall(installationId: string, source: SourceRef, name: string) {
  const key = `${source.provider}:${source.projectId.toLowerCase()}`;
  const progress = (stage: TaskProgress["stage"], received = 0) =>
    emit("task-progress", { installationId, key, label: name, stage, received, total: 4_000_000, message: null });
  progress("resolving");
  await wait(400);
  for (let i = 1; i <= 5; i++) {
    progress("downloading", i * 800_000);
    await wait(180);
  }
  progress("installing");
  await wait(350);
  const list = addons[installationId] ?? [];
  const existing = list.find((a) => a.source && a.source.projectId === source.projectId);
  if (existing) {
    existing.version = existing.update?.latestVersion ?? existing.version;
    existing.update = existing.update ? { ...existing.update, available: false } : null;
  } else {
    list.push(managed(key, name, source, "1.0.0", "Installed just now."));
    list.sort((a, b) => a.name.localeCompare(b.name));
  }
  addons[installationId] = list;
  progress("done");
  emit("addons-changed", installationId);
}

const MOCK_README = [
  "## About",
  "",
  "**Questie** shows quest givers, objectives and turn-ins on your **map and minimap**, so you spend less time reading and more time questing.",
  "",
  "- Quest givers and turn-ins marked on the map",
  "- Objective areas highlighted as you go",
  "- Tracker with distance and progress",
  "",
  "> Works with WoW Forever's new zones, including the Skyborne Elf starting area.",
  "",
  "See the [wiki](docs/wiki.md) for settings, or run `/questie` in game.",
].join(String.fromCharCode(10));

const MOCK_NOTES = [
  "### Forever 1.0.14",
  "- Added Skyborne Elf starting zone quests",
  "- Fixed objective arrows in Gilneas",
  "- Database updates for the new dungeons",
].join(String.fromCharCode(10));

const handlers: Record<string, (args: Record<string, any>) => unknown> = {
  bootstrap: (): Bootstrap => ({
    appVersion: "0.1.0",
    settings: { ...settings },
    installations,
    secrets: { githubToken: false, curseforgeKey: false },
    updaterConfigured: false,
    dataDir: "C:\\Users\\you\\AppData\\Roaming\\com.noolies.wow-mods",
    platform: "windows",
    catalogVersion: 1,
  }),
  save_settings: ({ settings: next }) => Object.assign(settings, next),
  set_secret: ({ kind, value }) => ({ githubToken: kind === "githubToken" && !!value, curseforgeKey: kind === "curseforgeKey" && !!value }),
  refresh_catalog: () => false,
  open_folder: () => undefined,
  reveal_addon: () => undefined,
  detect_installations: async (): Promise<DetectedInstall[]> => {
    await wait(900);
    return [
      { flavor: "forever", label: "WoW Forever", path: "C:\\Program Files (x86)\\World of Warcraft\\_forever_", addonCount: 11, version: "1.60.1.70888", icon: null, alreadyAdded: false },
      { flavor: "retail", label: "World of Warcraft", path: "C:\\Program Files (x86)\\World of Warcraft\\_retail_", addonCount: 0, version: "12.1.5.71011", icon: null, alreadyAdded: false },
    ];
  },
  inspect_folder: () => [],
  add_installations: ({ installations: requests }) => {
    for (const r of requests as { path: string; flavor: Installation["flavor"]; label: string }[]) {
      installations.push({
        id: r.flavor,
        game: "wow",
        flavor: r.flavor,
        label: r.label,
        path: r.path,
        launchMethod: "battlenet",
        defaultChannel: null,
        addedAt: now(),
        flavorName: r.label,
        available: true,
        clientVersion: null,
        icon: null,
      });
    }
    return installations;
  },
  update_installation: ({ id, patch }) => {
    const inst = installations.find((i) => i.id === id)!;
    Object.assign(inst, patch);
    return inst;
  },
  remove_installation: ({ id }) => {
    installations = installations.filter((i) => i.id !== id);
  },
  select_installation: ({ id }) => {
    settings.lastInstallation = id;
  },
  game_running: () => false,
  play: async ({ installationId }) => {
    emit("play-stage", { installationId, stage: "backingUp" });
    await wait(700);
    emit("play-stage", { installationId, stage: "updating" });
    await wait(900);
    emit("play-stage", { installationId, stage: "launching" });
    await wait(700);
    emit("play-stage", { installationId, stage: "done" });
    return { method: "battlenet", updated: 0, backedUp: true, warnings: [] };
  },
  list_addons: ({ installationId }) => structuredClone(addons[installationId] ?? []),
  cached_updates: ({ installationId }) => ({
    checks: (addons[installationId] ?? []).map((a) => a.update).filter(Boolean) as UpdateCheck[],
    checking: false,
  }),
  check_updates: async ({ installationId }) => {
    emit("check-started", installationId);
    await wait(1200);
    emit("updates-changed", installationId);
    return [];
  },
  install_addon: async ({ installationId, source, hint }) => {
    await fakeInstall(installationId, source, hint?.name ?? source.projectId);
    return {};
  },
  update_addons: async ({ installationId, addonIds }) => {
    const list = addons[installationId] ?? [];
    const targets = list.filter((a) => (addonIds ? addonIds.includes(a.id) : a.update?.available && !a.pinned));
    await Promise.all(targets.map((a) => fakeInstall(installationId, a.source!, a.name)));
    return { succeeded: targets.map((t) => t.name), failed: [] };
  },
  uninstall_addon: ({ installationId, addonId }) => {
    addons[installationId] = (addons[installationId] ?? []).filter((a) => a.id !== addonId);
  },
  set_addon_options: ({ installationId, addonId, options }) => {
    const addon = (addons[installationId] ?? []).find((a) => a.id === addonId)!;
    Object.assign(addon, options);
    return addon;
  },
  link_addon: ({ installationId, addonId, source }) => {
    const addon = (addons[installationId] ?? []).find((a) => a.id === addonId)!;
    Object.assign(addon, { managed: true, source, id: `${addonId}-linked`, update: { addonId, available: true, latestVersion: "v1.14.13", latestDate: null, changelog: null, changelogUrl: null, error: null, checkedAt: now() } });
    return addon;
  },
  link_all: () => 0,
  search_addons: async ({ installationId, query, scope }) => {
    await wait(350);
    const q = String(query).toLowerCase();
    let hits = catalogHits
      .filter((h) => !q || h.name.toLowerCase().includes(q) || (h.summary ?? "").toLowerCase().includes(q))
      .map((h) => ({ ...h, installed: isInstalled(installationId, h.source) }));
    if (scope === "curseforge") hits = [];
    return { hits, warnings: [] };
  },
  addon_details: async ({ source }) => {
    await wait(500);
    const hit = catalogHits.find((h) => h.source.projectId === source.projectId);
    const elvui = source.provider === "tukui";
    return {
      source,
      name: hit?.name ?? (elvui ? "ElvUI" : source.projectId.split("/")[1]),
      summary: hit?.summary ?? "A full interface replacement: bars, unit frames, bags, chat and more.",
      author: hit?.author ?? "Elv",
      thumbnail: hit?.thumbnail ?? "https://storage.tukui.org/ElvUI-Logo-Square.png",
      website: hit?.website ?? "https://tukui.org/elvui",
      sourceUrl: elvui ? "https://github.com/tukui-org/ElvUI" : `https://github.com/${source.projectId}`,
      issuesUrl: `https://github.com/${source.projectId}/issues`,
      donateUrl: elvui ? "https://tukui.org/support" : null,
      license: elvui ? null : "GPL-3.0",
      stars: elvui ? null : 1143,
      downloads: null,
      updatedAt: ago(300),
      tags: elvui ? [] : ["quests", "classic", "forever"],
      description: MOCK_README,
      descriptionFormat: "markdown",
      screenshots: elvui
        ? ["https://storage.tukui.org/ElvUI-1.jpg", "https://storage.tukui.org/ElvUI-2.jpg", "https://storage.tukui.org/ElvUI-3.jpg"]
        : [],
      gameVersions: ["WoW Forever", "Classic Era", "Burning Crusade Classic", "Mists of Pandaria Classic"],
      supportsFlavor: true,
      latestVersion: elvui ? "16.01" : "v12.7.0+v1.0.14",
      latestDate: ago(800),
      releaseNotes: MOCK_NOTES,
      releaseNotesFormat: "markdown",
      releaseUrl: `https://github.com/${source.projectId}/releases/latest`,
      downloadSize: 94_538_855,
      linkBase: `https://github.com/${source.projectId}/blob/HEAD/`,
      imageBase: `https://raw.githubusercontent.com/${source.projectId}/HEAD/`,
    };
  },
  install_from_url: async ({ installationId, url }) => {
    const source = gh(String(url).replace(/^https?:\/\/github.com\//, "").split("/").slice(0, 2).join("/"));
    await fakeInstall(installationId, source, source.projectId.split("/")[1] ?? "Addon");
    return {};
  },
  install_from_file: () => ({}),
  export_pack: () => "HF1:lVLBbsIwDP0VK2ekgQaHHnfYD0wcEIqQa7dEtUmKNx3qv8-0wJgEmzZ5spP3_J6dF_RmA2nXnu2B7ZGv8EHyzlY0P0ul1PNTA6",
  preview_pack: () => ({
    name: "Raid night",
    flavor: "forever",
    flavorMatches: true,
    items: [
      { source: gh("Questie/Questie"), name: "Questie", installed: true },
      { source: gh("DeadlyBossMods/DeadlyBossMods"), name: "Deadly Boss Mods", installed: false },
      { source: gh("tullamods/tullaRange"), name: "tullaRange", installed: false },
    ],
  }),
  import_pack: async ({ installationId }) => {
    await fakeInstall(installationId, gh("DeadlyBossMods/DeadlyBossMods"), "Deadly Boss Mods");
    await fakeInstall(installationId, gh("tullamods/tullaRange"), "tullaRange");
    return { succeeded: ["Deadly Boss Mods", "tullaRange"], failed: [] };
  },
  list_backups: () => ({ folder: "C:\\Users\\you\\AppData\\Roaming\\com.noolies.wow-mods\\backups\\forever", backups, hasSettings: true }),
  create_backup: async () => {
    await wait(800);
    const backup: BackupInfo = { id: `WTF-${Date.now()}-manual.zip`, path: "", createdAt: now(), size: 18_500_000, reason: "manual" };
    backups = [backup, ...backups];
    return backup;
  },
  restore_backup: async () => {
    await wait(900);
    return null;
  },
  delete_backup: ({ backupId }) => {
    backups = backups.filter((b) => b.id !== backupId);
  },
};

export async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const handler = handlers[command];
  if (!handler) throw new Error(`Mock has no handler for ${command}`);
  await wait(60);
  return (await handler(args as Record<string, any>)) as T;
}
