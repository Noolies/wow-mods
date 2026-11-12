import type { AddonView, BackupReason, Channel, ProviderId, SourceRef, TaskProgress } from "./types";

/** Same key the Rust side uses for progress events (`SourceRef::key`). */
export function sourceKey(source: SourceRef): string {
  return `${source.provider}:${source.projectId.toLowerCase()}`;
}

/** `owner/repo` from a GitHub link or shorthand, or null. */
export function parseGithubRepo(input: string): string | null {
  const text = input.trim();
  const match =
    text.match(/^(?:https?:\/\/)?(?:www\.)?github\.com\/([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)/i) ??
    text.match(/^([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)$/);
  return match ? `${match[1]}/${match[2].replace(/\.git$/i, "")}` : null;
}

/** "A", "A and B", "A, B and C". */
export function joinNames(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** "1.60.1.70888" -> "1.60.1" */
export function shortVersion(version: string | null | undefined): string | null {
  if (!version) return null;
  return version.split(".").slice(0, 3).join(".");
}

export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

export function relativeTime(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "never";
  const then = new Date(iso);
  const seconds = Math.round((now.getTime() - then.getTime()) / 1000);
  if (Number.isNaN(seconds)) return "never";
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${plural(minutes, "minute")} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${plural(hours, "hour")} ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return days === 1 ? "yesterday" : `${days} days ago`;
  return then.toLocaleDateString("en-AU", { day: "numeric", month: "short", year: "numeric" });
}

export function dateTime(iso: string): string {
  return new Date(iso).toLocaleString("en-AU", {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "numeric",
    minute: "2-digit",
  });
}

export function bytes(size: number): string {
  if (size < 1024) return `${size} B`;
  const units = ["KB", "MB", "GB"];
  let value = size / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

export function compactNumber(value: number): string {
  if (value < 1000) return String(value);
  if (value < 1_000_000) return `${(value / 1000).toFixed(value < 10_000 ? 1 : 0)}k`;
  return `${(value / 1_000_000).toFixed(1)}M`;
}

export const providerLabel: Record<ProviderId, string> = {
  github: "GitHub",
  curseforge: "CurseForge",
  tukui: "Tukui",
  manual: "From a file",
};

export const channelLabel: Record<Channel, string> = {
  stable: "Stable",
  beta: "Beta",
  alpha: "Alpha",
};

export const backupReasonLabel: Record<BackupReason, string> = {
  manual: "Backed up by you",
  launch: "Before playing",
  update: "Before updating",
  restore: "Before a restore",
};

export function monogram(name: string): string {
  const letters = name.replace(/[^A-Za-z0-9 ]/g, "").trim();
  return (letters[0] ?? "?").toUpperCase();
}

export type StatusKind = "busy" | "update" | "ok" | "pinned" | "idle" | "problem";

export interface RowStatus {
  kind: StatusKind;
  label: string;
  detail?: string;
}

/** What the status column says about an addon, most urgent first. */
export function addonStatus(addon: AddonView, flavorName: string, task?: TaskProgress): RowStatus {
  if (task && task.stage !== "done" && task.stage !== "failed") {
    const label = task.stage === "downloading" ? "Downloading" : task.stage === "installing" ? "Installing" : "Preparing";
    return { kind: "busy", label };
  }
  if (addon.managed && addon.missingFolders.length > 0) {
    return { kind: "problem", label: "Files missing", detail: `Missing: ${addon.missingFolders.join(", ")}. Reinstall to fix it.` };
  }
  if (addon.compatibility === "missing") {
    return { kind: "problem", label: `Not for ${flavorName}`, detail: `This addon has no files ${flavorName} will load.` };
  }
  if (addon.compatibility === "outOfDate") {
    return {
      kind: "problem",
      label: "Built for another version",
      detail: `${flavorName} will mark this addon as out of date and won't load it unless you allow out-of-date addons.`,
    };
  }
  if (addon.compatibility === "olderPatch") {
    return {
      kind: "problem",
      label: "Out of date",
      detail: `Built for an earlier ${flavorName} patch. The game won't load it until the author releases an update, unless you tick "Load out of date AddOns" in the game's AddOns menu.`,
    };
  }
  if (addon.missingDependencies.length > 0) {
    return {
      kind: "problem",
      label: `Needs ${addon.missingDependencies[0]}`,
      detail: `Required addons not installed: ${addon.missingDependencies.join(", ")}.`,
    };
  }
  if (!addon.managed) return { kind: "idle", label: "Not tracked" };
  if (addon.source?.provider === "manual") return { kind: "idle", label: "From a file" };
  if (addon.update?.available && addon.pinned) return { kind: "pinned", label: "Pinned" };
  if (addon.update?.available) return { kind: "update", label: "Update ready" };
  if (addon.update?.error) return { kind: "problem", label: "Check failed", detail: addon.update.error };
  if (addon.update) return { kind: "ok", label: "Up to date" };
  return { kind: "idle", label: "Not checked yet" };
}

export function downloadProgress(task: TaskProgress | undefined): number | null {
  if (!task || task.stage !== "downloading" || !task.total) return null;
  return Math.min(100, Math.round((task.received / task.total) * 100));
}
