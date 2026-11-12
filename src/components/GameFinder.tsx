import { useEffect, useRef, useState } from "react";
import { api, errorText, inTauri, pickFolder } from "../lib/api";
import { plural, shortVersion } from "../lib/format";
import { useStore } from "../lib/store";
import { GameIcon } from "./Bits";
import type { DetectedInstall } from "../lib/types";

/** Finds WoW clients on this PC (or in a chosen folder) and adds the selected ones. */
export function GameFinder({ autoScan = false, onAdded }: { autoScan?: boolean; onAdded?: () => void }) {
  const toast = useStore((s) => s.toast);
  const reloadBoot = useStore((s) => s.reloadBoot);
  const [found, setFound] = useState<DetectedInstall[] | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [scanning, setScanning] = useState(false);
  const scanned = useRef(false);

  function remember(results: DetectedInstall[]) {
    setFound(results);
    setChosen(new Set(results.filter((r) => !r.alreadyAdded && r.flavor === "forever").map((r) => r.path)));
    if (results.length > 0 && !results.some((r) => r.flavor === "forever")) {
      setChosen(new Set(results.filter((r) => !r.alreadyAdded).map((r) => r.path)));
    }
  }

  async function scan() {
    setScanning(true);
    try {
      remember(await api.detectInstallations());
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setScanning(false);
    }
  }

  useEffect(() => {
    if (autoScan && !scanned.current) {
      scanned.current = true;
      void scan();
    }
    // Scan once on first show.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function choose() {
    const path = await pickFolder("Choose your World of Warcraft folder");
    if (!path) {
      if (!inTauri) toast("info", "Choosing folders works in the desktop app.");
      return;
    }
    try {
      const results = await api.inspectFolder(path);
      const merged = [...(found ?? []).filter((f) => !results.some((r) => r.path === f.path)), ...results];
      remember(merged);
    } catch (error) {
      toast("error", errorText(error));
    }
  }

  async function add() {
    const picks = (found ?? []).filter((f) => chosen.has(f.path));
    try {
      await api.addInstallations(picks.map((p) => ({ path: p.path, flavor: p.flavor, label: p.label })));
      toast("success", `Added ${plural(picks.length, "game")}.`);
      await reloadBoot();
      setFound(null);
      onAdded?.();
    } catch (error) {
      toast("error", errorText(error));
    }
  }

  return (
    <div>
      {found && found.length > 0 && (
        <div className="found" role="group" aria-label="Games found">
          {found.map((f) => (
            <label key={f.path}>
              <input
                type="checkbox"
                disabled={f.alreadyAdded}
                checked={f.alreadyAdded || chosen.has(f.path)}
                onChange={() =>
                  setChosen((c) => {
                    const next = new Set(c);
                    if (next.has(f.path)) next.delete(f.path);
                    else next.add(f.path);
                    return next;
                  })
                }
              />
              <span className="found__game">
                <GameIcon icon={f.icon} flavor={f.flavor} size={36} />
                <span style={{ minWidth: 0 }}>
                  <strong>{f.label}</strong>
                  {f.version && <span className="small faint"> {shortVersion(f.version)}</span>}
                  <div className="found__path" title={f.path}>
                    {f.path}
                  </div>
                </span>
              </span>
              <span className="small muted">
                {f.alreadyAdded ? "Already added" : f.addonCount ? plural(f.addonCount, "addon") : "No addons yet"}
              </span>
            </label>
          ))}
        </div>
      )}
      {found && found.length === 0 && (
        <p className="muted" style={{ margin: "0 0 16px" }}>
          No WoW games were found automatically. Choose your World of Warcraft folder instead.
        </p>
      )}
      <div className="welcome__actions" style={{ justifyContent: "flex-start" }}>
        {found && chosen.size > 0 ? (
          <button className="btn btn--primary" onClick={() => void add()}>
            Add {plural(chosen.size, "game")}
          </button>
        ) : (
          <button className="btn btn--primary" disabled={scanning} onClick={() => void scan()}>
            {scanning ? "Looking for games" : found ? "Look again" : "Find my games"}
          </button>
        )}
        <button className="btn" onClick={() => void choose()}>
          Choose a folder
        </button>
      </div>
    </div>
  );
}
