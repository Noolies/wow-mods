// Typed wrappers around the Rust commands. In a plain browser (vite dev without
// Tauri) calls go to an in-memory mock so the UI can be developed and reviewed.

import type {
  AddonOptions,
  AddonDetails,
  AddonRecord,
  AddonView,
  BackupInfo,
  BackupList,
  BatchResult,
  Bootstrap,
  CachedUpdates,
  Channel,
  DetectedInstall,
  Flavor,
  FolderTarget,
  Installation,
  InstallationPatch,
  PackPreview,
  PlayOutcome,
  SearchResults,
  SearchScope,
  SecretKind,
  SecretStatus,
  Settings,
  SourceHint,
  SourceRef,
  UpdateCheck,
} from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Listen = <T>(event: string, handler: (payload: T) => void) => Promise<() => void>;

let backend: Promise<{ invoke: Invoke; listen: Listen }> | null = null;

function load() {
  if (!backend) {
    backend = inTauri
      ? Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]).then(([core, event]) => ({
          invoke: core.invoke as Invoke,
          listen: (async (name: string, handler: (payload: unknown) => void) =>
            event.listen(name, (e) => handler(e.payload))) as Listen,
        }))
      : import("./mock").then((m) => ({ invoke: m.invoke as Invoke, listen: m.listen as Listen }));
  }
  return backend;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await load();
  return invoke<T>(command, args);
}

export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const { listen } = await load();
  return listen<T>(event, handler);
}

/** Error text from a rejected command (the Rust side sends plain strings). */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}

export const api = {
  bootstrap: () => call<Bootstrap>("bootstrap"),
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }),
  setSecret: (kind: SecretKind, value: string | null) => call<SecretStatus>("set_secret", { kind, value }),
  refreshCatalog: () => call<boolean>("refresh_catalog"),
  openFolder: (target: FolderTarget, installationId?: string) =>
    call<void>("open_folder", { target, installationId: installationId ?? null }),
  revealAddon: (installationId: string, folder: string) => call<void>("reveal_addon", { installationId, folder }),

  detectInstallations: () => call<DetectedInstall[]>("detect_installations"),
  inspectFolder: (path: string) => call<DetectedInstall[]>("inspect_folder", { path }),
  addInstallations: (installations: { path: string; flavor: Flavor; label: string }[]) =>
    call<Installation[]>("add_installations", { installations }),
  updateInstallation: (id: string, patch: InstallationPatch) =>
    call<Installation>("update_installation", { id, patch }),
  removeInstallation: (id: string) => call<void>("remove_installation", { id }),
  selectInstallation: (id: string) => call<void>("select_installation", { id }),
  gameRunning: (installationId: string) => call<boolean>("game_running", { installationId }),
  play: (installationId: string) => call<PlayOutcome>("play", { installationId }),

  listAddons: (installationId: string) => call<AddonView[]>("list_addons", { installationId }),
  cachedUpdates: (installationId: string) => call<CachedUpdates>("cached_updates", { installationId }),
  checkUpdates: (installationId: string) => call<UpdateCheck[]>("check_updates", { installationId }),
  installAddon: (installationId: string, source: SourceRef, hint?: SourceHint, channel?: Channel | null) =>
    call<AddonRecord>("install_addon", { installationId, source, hint: hint ?? null, channel: channel ?? null }),
  updateAddons: (installationId: string, addonIds?: string[]) =>
    call<BatchResult>("update_addons", { installationId, addonIds: addonIds ?? null }),
  uninstallAddon: (installationId: string, addonId: string) =>
    call<void>("uninstall_addon", { installationId, addonId }),
  setAddonOptions: (installationId: string, addonId: string, options: AddonOptions) =>
    call<AddonRecord>("set_addon_options", { installationId, addonId, options }),
  linkAddon: (installationId: string, addonId: string, source: SourceRef) =>
    call<AddonRecord>("link_addon", { installationId, addonId, source }),
  linkAll: (installationId: string) => call<number>("link_all", { installationId }),
  searchAddons: (installationId: string, query: string, scope: SearchScope) =>
    call<SearchResults>("search_addons", { installationId, query, scope }),
  addonDetails: (installationId: string, source: SourceRef) =>
    call<AddonDetails>("addon_details", { installationId, source }),
  installFromUrl: (installationId: string, url: string) =>
    call<AddonRecord>("install_from_url", { installationId, url }),
  installFromFile: (installationId: string, path: string) =>
    call<AddonRecord>("install_from_file", { installationId, path }),

  exportPack: (installationId: string, name: string, addonIds?: string[]) =>
    call<string>("export_pack", { installationId, name, addonIds: addonIds ?? null }),
  previewPack: (installationId: string, code: string) => call<PackPreview>("preview_pack", { installationId, code }),
  importPack: (installationId: string, code: string) => call<BatchResult>("import_pack", { installationId, code }),

  listBackups: (installationId: string) => call<BackupList>("list_backups", { installationId }),
  createBackup: (installationId: string) => call<BackupInfo | null>("create_backup", { installationId }),
  restoreBackup: (installationId: string, backupId: string) =>
    call<BackupInfo | null>("restore_backup", { installationId, backupId }),
  deleteBackup: (installationId: string, backupId: string) =>
    call<void>("delete_backup", { installationId, backupId }),
};

/** Opens a web page in the default browser. */
export async function openUrl(url: string) {
  if (inTauri) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}

/** Native folder / file pickers (no-ops in the browser preview). */
export async function pickFolder(title: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({ directory: true, multiple: false, title });
  return typeof result === "string" ? result : null;
}

export async function pickZip(): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({
    multiple: false,
    title: "Choose an addon zip file",
    filters: [{ name: "Zip archives", extensions: ["zip"] }],
  });
  return typeof result === "string" ? result : null;
}

export async function confirmDialog(message: string, title: string, okLabel: string): Promise<boolean> {
  if (!inTauri) return window.confirm(message);
  const { ask } = await import("@tauri-apps/plugin-dialog");
  return ask(message, { title, kind: "warning", okLabel, cancelLabel: "Cancel" });
}

/** Files dropped onto the window (Tauri only). */
export async function onFileDrop(handlers: {
  over: (active: boolean) => void;
  drop: (paths: string[]) => void;
}): Promise<() => void> {
  if (!inTauri) return () => {};
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") handlers.over(true);
    else if (payload.type === "leave") handlers.over(false);
    else if (payload.type === "drop") {
      handlers.over(false);
      handlers.drop(payload.paths);
    }
  });
}

export async function showWindow() {
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().show();
}
