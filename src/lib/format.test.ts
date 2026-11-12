import { describe, expect, it } from "vitest";
import {
  addonStatus,
  bytes,
  compactNumber,
  joinNames,
  monogram,
  parseGithubRepo,
  plural,
  relativeTime,
  shortVersion,
  sourceKey,
} from "./format";
import type { AddonView } from "./types";

const base: AddonView = {
  id: "a",
  managed: true,
  name: "Questie",
  version: "v1",
  author: null,
  notes: null,
  folders: [],
  source: { provider: "github", projectId: "Questie/Questie" },
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
};

const check = { addonId: "a", latestVersion: "v2", latestDate: null, changelog: null, changelogUrl: null, checkedAt: "" };

describe("format helpers", () => {
  it("pluralises and formats sizes", () => {
    expect(plural(1, "addon")).toBe("1 addon");
    expect(plural(3, "addon")).toBe("3 addons");
    expect(joinNames(["A"])).toBe("A");
    expect(joinNames(["A", "B", "C"])).toBe("A, B and C");
    expect(bytes(512)).toBe("512 B");
    expect(bytes(1536)).toBe("1.5 KB");
    expect(bytes(25 * 1024 * 1024)).toBe("25 MB");
    expect(compactNumber(1543)).toBe("1.5k");
    expect(compactNumber(25_000)).toBe("25k");
    expect(monogram("!BugGrabber")).toBe("B");
    expect(monogram("Questie")).toBe("Q");
    expect(shortVersion("1.60.1.70888")).toBe("1.60.1");
    expect(shortVersion(null)).toBeNull();
  });

  it("parses GitHub references and builds progress keys", () => {
    expect(parseGithubRepo("Questie/Questie")).toBe("Questie/Questie");
    expect(parseGithubRepo("https://github.com/BigWigsMods/BigWigs/releases")).toBe("BigWigsMods/BigWigs");
    expect(parseGithubRepo("github.com/tullamods/Dominos.git")).toBe("tullamods/Dominos");
    expect(parseGithubRepo("https://example.com/a/b")).toBeNull();
    expect(parseGithubRepo("not a repo")).toBeNull();
    expect(sourceKey({ provider: "github", projectId: "Questie/Questie" })).toBe("github:questie/questie");
  });

  it("describes times relative to now", () => {
    const now = new Date("2026-11-12T10:00:00Z");
    expect(relativeTime("2026-11-12T09:59:40Z", now)).toBe("just now");
    expect(relativeTime("2026-11-12T09:55:00Z", now)).toBe("5 minutes ago");
    expect(relativeTime("2026-11-12T07:00:00Z", now)).toBe("3 hours ago");
    expect(relativeTime("2026-11-11T09:00:00Z", now)).toBe("yesterday");
    expect(relativeTime(null, now)).toBe("never");
  });

  it("puts the most urgent status first", () => {
    expect(addonStatus(base, "WoW Forever").label).toBe("Not checked yet");
    expect(addonStatus({ ...base, update: { ...check, available: false, error: null } }, "WoW Forever").kind).toBe("ok");
    expect(addonStatus({ ...base, update: { ...check, available: true, error: null } }, "WoW Forever").kind).toBe("update");
    expect(
      addonStatus({ ...base, pinned: true, update: { ...check, available: true, error: null } }, "WoW Forever").kind,
    ).toBe("pinned");
    expect(addonStatus({ ...base, compatibility: "outOfDate" }, "WoW Forever").kind).toBe("problem");
    expect(addonStatus({ ...base, compatibility: "olderPatch" }, "WoW Forever").label).toBe("Out of date");
    expect(addonStatus({ ...base, missingDependencies: ["Ace3"] }, "WoW Forever").label).toBe("Needs Ace3");
    expect(addonStatus({ ...base, managed: false, source: null }, "WoW Forever").label).toBe("Not tracked");
    expect(
      addonStatus(base, "WoW Forever", {
        installationId: "i",
        key: "a",
        label: "Questie",
        stage: "downloading",
        received: 1,
        total: 2,
        message: null,
      }).kind,
    ).toBe("busy");
  });
});
