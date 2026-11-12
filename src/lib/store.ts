import { create } from "zustand";
import { api, errorText, inTauri, on } from "./api";
import { plural } from "./format";
import type { Update } from "@tauri-apps/plugin-updater";
import type { AddonView, Bootstrap, Installation, PlayStage, Settings, SourceHint, SourceRef, TaskProgress } from "./types";

export type View = "installed" | "find" | "share" | "backups" | "settings";

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
}

export type AppUpdateStatus = "idle" | "checking" | "available" | "downloading" | "ready" | "error";

interface AppUpdate {
  status: AppUpdateStatus;
  version?: string;
  notes?: string;
  progress?: number;
}

interface State {
  boot: Bootstrap | null;
  bootError: string | null;
  selectedId: string | null;
  view: View;
  findQuery: string;
  /** The addon whose details panel is open. */
  details: { source: SourceRef; hint: SourceHint } | null;
  addons: Record<string, AddonView[] | undefined>;
  loading: Record<string, boolean>;
  checking: Record<string, boolean>;
  lastChecked: Record<string, string | undefined>;
  tasks: Record<string, TaskProgress>;
  running: Record<string, boolean>;
  playStage: Record<string, PlayStage | null>;
  toasts: Toast[];
  appUpdate: AppUpdate;

  init: () => Promise<void>;
  reloadBoot: () => Promise<void>;
  select: (id: string) => void;
  setView: (view: View) => void;
  openFind: (query: string) => void;
  openDetails: (source: SourceRef, hint?: SourceHint) => void;
  closeDetails: () => void;
  refreshAddons: (installationId: string) => Promise<void>;
  checkUpdates: (installationId: string) => Promise<void>;
  updateAll: (installationId: string) => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  play: (installationId: string) => Promise<void>;
  toast: (kind: Toast["kind"], text: string) => void;
  dismiss: (id: number) => void;
  checkAppUpdate: (manual: boolean) => Promise<void>;
  installAppUpdate: () => Promise<void>;
}

let toastId = 0;
/** When each game was last started, so the button stays on "Playing" while Battle.net boots it. */
const launchedAt: Record<string, number> = {};
const LAUNCH_GRACE_MS = 90_000;

/** Stable empty values for selectors: a fresh `[]` each render would loop forever. */
export const NO_ADDONS: AddonView[] = [];
export const NO_INSTALLATIONS: Installation[] = [];
let pendingUpdate: Update | null = null;

export const useStore = create<State>((set, get) => ({
  boot: null,
  bootError: null,
  selectedId: null,
  view: "installed",
  findQuery: "",
  details: null,
  addons: {},
  loading: {},
  checking: {},
  lastChecked: {},
  tasks: {},
  running: {},
  playStage: {},
  toasts: [],
  appUpdate: { status: "idle" },

  init: async () => {
    try {
      const boot = await api.bootstrap();
      const selectedId =
        boot.installations.find((i) => i.id === boot.settings.lastInstallation)?.id ?? boot.installations[0]?.id ?? null;
      set({ boot, selectedId });
      applyTheme(boot.settings.theme);
      subscribe();
      if (selectedId) void get().refreshAddons(selectedId);
      if (boot.settings.appAutoUpdate) setTimeout(() => void get().checkAppUpdate(false), 4000);
    } catch (error) {
      set({ bootError: errorText(error) });
    }
  },

  reloadBoot: async () => {
    const boot = await api.bootstrap();
    const current = get().selectedId;
    const selectedId = boot.installations.some((i) => i.id === current) ? current : (boot.installations[0]?.id ?? null);
    set({ boot, selectedId });
    if (selectedId) void get().refreshAddons(selectedId);
  },

  select: (id) => {
    set({ selectedId: id, view: get().view === "settings" ? "installed" : get().view });
    void api.selectInstallation(id).catch(() => undefined);
    void get().refreshAddons(id);
  },

  setView: (view) => set({ view }),

  openFind: (query) => set({ view: "find", findQuery: query }),

  openDetails: (source, hint = {}) => set({ details: { source, hint } }),

  closeDetails: () => set({ details: null }),

  refreshAddons: async (installationId) => {
    set((s) => ({ loading: { ...s.loading, [installationId]: true } }));
    try {
      const [list, cached] = await Promise.all([api.listAddons(installationId), api.cachedUpdates(installationId)]);
      const latest = cached.checks.map((c) => c.checkedAt).sort().at(-1);
      set((s) => ({
        addons: { ...s.addons, [installationId]: list },
        checking: { ...s.checking, [installationId]: cached.checking },
        lastChecked: { ...s.lastChecked, [installationId]: latest ?? s.lastChecked[installationId] },
      }));
    } catch (error) {
      get().toast("error", errorText(error));
    } finally {
      set((s) => ({ loading: { ...s.loading, [installationId]: false } }));
    }
  },

  checkUpdates: async (installationId) => {
    set((s) => ({ checking: { ...s.checking, [installationId]: true } }));
    try {
      await api.checkUpdates(installationId);
      set((s) => ({ lastChecked: { ...s.lastChecked, [installationId]: new Date().toISOString() } }));
    } catch (error) {
      get().toast("error", errorText(error));
    } finally {
      set((s) => ({ checking: { ...s.checking, [installationId]: false } }));
      await get().refreshAddons(installationId);
    }
  },

  updateAll: async (installationId) => {
    try {
      const result = await api.updateAddons(installationId);
      if (result.failed.length) {
        get().toast("error", `${plural(result.failed.length, "addon")} didn't update. ${result.failed[0].name}: ${result.failed[0].error}`);
      }
      if (result.succeeded.length) get().toast("success", `Updated ${plural(result.succeeded.length, "addon")}.`);
    } catch (error) {
      get().toast("error", errorText(error));
    } finally {
      await get().refreshAddons(installationId);
    }
  },

  saveSettings: async (patch) => {
    const boot = get().boot;
    if (!boot) return;
    const next = { ...boot.settings, ...patch };
    set({ boot: { ...boot, settings: next } });
    try {
      const saved = await api.saveSettings(next);
      set((s) => (s.boot ? { boot: { ...s.boot, settings: saved } } : {}));
      if (patch.theme) applyTheme(saved.theme);
    } catch (error) {
      set((s) => (s.boot ? { boot: { ...s.boot, settings: boot.settings } } : {}));
      get().toast("error", errorText(error));
    }
  },

  play: async (installationId) => {
    set((s) => ({ playStage: { ...s.playStage, [installationId]: "backingUp" } }));
    try {
      const outcome = await api.play(installationId);
      for (const warning of outcome.warnings) get().toast("info", warning);
      if (outcome.updated > 0) get().toast("success", `Updated ${plural(outcome.updated, "addon")} before starting the game.`);
      launchedAt[installationId] = Date.now();
      set((s) => ({ running: { ...s.running, [installationId]: true } }));
    } catch (error) {
      get().toast("error", errorText(error));
    } finally {
      setTimeout(() => set((s) => ({ playStage: { ...s.playStage, [installationId]: null } })), 1200);
      void get().refreshAddons(installationId);
    }
  },

  toast: (kind, text) => {
    const id = ++toastId;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, kind, text }] }));
    setTimeout(() => get().dismiss(id), kind === "error" ? 9000 : 5000);
  },

  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  checkAppUpdate: async (manual) => {
    const boot = get().boot;
    if (!inTauri || !boot?.updaterConfigured) {
      if (manual) get().toast("info", "Automatic app updates aren't set up for this build yet.");
      return;
    }
    set({ appUpdate: { status: "checking" } });
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (!update) {
        set({ appUpdate: { status: "idle" } });
        if (manual) get().toast("success", `Hearthforge ${boot.appVersion} is the latest version.`);
        return;
      }
      pendingUpdate = update;
      set({ appUpdate: { status: "downloading", version: update.version, notes: update.body ?? undefined, progress: 0 } });
      let total = 0;
      let received = 0;
      await update.download((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          received += event.data.chunkLength;
          if (total) set((s) => ({ appUpdate: { ...s.appUpdate, progress: Math.round((received / total) * 100) } }));
        }
      });
      set((s) => ({ appUpdate: { ...s.appUpdate, status: "ready" } }));
    } catch (error) {
      set({ appUpdate: { status: manual ? "error" : "idle" } });
      if (manual) get().toast("error", `Couldn't check for app updates: ${errorText(error)}`);
    }
  },

  installAppUpdate: async () => {
    if (!pendingUpdate) return;
    try {
      await pendingUpdate.install();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (error) {
      get().toast("error", `Couldn't finish the update: ${errorText(error)}`);
    }
  },
}));

export function applyTheme(theme: Settings["theme"]) {
  const dark = theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

let subscribed = false;

function subscribe() {
  if (subscribed) return;
  subscribed = true;
  const { getState, setState } = useStore;

  void on<TaskProgress>("task-progress", (task) => {
    setState((s) => ({ tasks: { ...s.tasks, [task.key]: task } }));
    if (task.stage === "done" || task.stage === "failed") {
      if (task.stage === "failed" && task.message) getState().toast("error", task.message);
      setTimeout(() => {
        setState((s) => {
          const tasks = { ...s.tasks };
          if (tasks[task.key]?.stage === task.stage) delete tasks[task.key];
          return { tasks };
        });
      }, 1500);
    }
  });

  const refreshIfCurrent = (installationId: string) => {
    void getState().refreshAddons(installationId);
  };
  void on<string>("addons-changed", refreshIfCurrent);
  void on<string>("updates-changed", (id) => {
    setState((s) => ({
      checking: { ...s.checking, [id]: false },
      lastChecked: { ...s.lastChecked, [id]: new Date().toISOString() },
    }));
    refreshIfCurrent(id);
  });
  void on<string>("check-started", (id) => setState((s) => ({ checking: { ...s.checking, [id]: true } })));
  void on<{ installationId: string; stage: PlayStage }>("play-stage", ({ installationId, stage }) =>
    setState((s) => ({ playStage: { ...s.playStage, [installationId]: stage } })),
  );
  void on<string>("toast", (text) => getState().toast("info", text));
  void on<null>("catalog-changed", () => getState().toast("info", "The list of recommended addons was updated."));

  // Poll whether the selected game is running, for the Play button.
  const poll = async () => {
    const id = getState().selectedId;
    if (id) {
      try {
        const starting = Date.now() - (launchedAt[id] ?? 0) < LAUNCH_GRACE_MS;
        const running = (await api.gameRunning(id)) || starting;
        setState((s) => (s.running[id] === running ? {} : { running: { ...s.running, [id]: running } }));
      } catch {
        /* ignore */
      }
    }
  };
  void poll();
  setInterval(poll, 8000);

  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => applyTheme(getState().boot?.settings.theme ?? "dark"));
}

if (import.meta.env.DEV) {
  (window as unknown as { __hearthforge: typeof useStore }).__hearthforge = useStore;
}

export function useSelected(): Installation | null {
  return useStore((s) => s.boot?.installations.find((i) => i.id === s.selectedId) ?? null);
}
