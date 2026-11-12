import { Settings as Cog } from "lucide-react";
import { plural, relativeTime, shortVersion } from "../lib/format";
import { NO_ADDONS, NO_INSTALLATIONS, useStore, type View } from "../lib/store";
import type { Installation, PlayStage } from "../lib/types";
import { Mark, Ridgeline } from "./Art";
import { GameIcon } from "./Bits";

export function Rail() {
  const installations = useStore((s) => s.boot?.installations ?? NO_INSTALLATIONS);
  const selectedId = useStore((s) => s.selectedId);
  const view = useStore((s) => s.view);
  const addons = useStore((s) => s.addons);
  const select = useStore((s) => s.select);
  const setView = useStore((s) => s.setView);

  return (
    <nav className="rail" aria-label="Games">
      <Mark className="rail__mark" />
      <div className="rail__games">
        {installations.map((inst) => {
          const updates = (addons[inst.id] ?? []).filter((a) => a.managed && a.update?.available && !a.pinned).length;
          return (
            <button
              key={inst.id}
              className={`game-tile${inst.icon ? " game-tile--art" : ""}`}
              aria-current={inst.id === selectedId && view !== "settings"}
              title={inst.label}
              aria-label={updates ? `${inst.label}, ${plural(updates, "update")} ready` : inst.label}
              onClick={() => select(inst.id)}
            >
              <GameIcon icon={inst.icon} flavor={inst.flavor} size={40} />
              {updates > 0 && <span className="game-tile__badge">{updates}</span>}
            </button>
          );
        })}
        <button className="game-tile game-tile--quiet" title="Add a game" aria-label="Add a game" onClick={() => setView("settings")}>
          +
        </button>
      </div>
      <button
        className="game-tile game-tile--quiet"
        aria-current={view === "settings"}
        title="Settings"
        aria-label="Settings"
        onClick={() => setView("settings")}
      >
        <Cog size={20} />
      </button>
    </nav>
  );
}

const STAGE_LABEL: Record<PlayStage, string> = {
  backingUp: "Backing up",
  updating: "Updating addons",
  launching: "Starting",
  done: "Started",
};

export function GameHeader({ installation }: { installation: Installation }) {
  const addons = useStore((s) => s.addons[installation.id]);
  const checking = useStore((s) => !!s.checking[installation.id]);
  const lastChecked = useStore((s) => s.lastChecked[installation.id]);
  const running = useStore((s) => !!s.running[installation.id]);
  const stage = useStore((s) => s.playStage[installation.id] ?? null);
  const updateBeforeLaunch = useStore((s) => s.boot?.settings.updateBeforeLaunch ?? true);
  const play = useStore((s) => s.play);

  const list = addons ?? [];
  const updates = list.filter((a) => a.managed && a.update?.available && !a.pinned).length;

  let note: string | null;
  if (running) note = "The game is running";
  else if (stage) note = null;
  else if (!installation.available) note = "Game folder not found";
  else if (updates > 0 && updateBeforeLaunch) note = `Updates ${plural(updates, "addon")} first`;
  else note = installation.launchMethod === "battlenet" ? "Opens through Battle.net" : "Starts the game directly";

  return (
    <header className="game-header">
      <Ridgeline className="game-header__art" />
      <div className="game-header__info">
        <div className="game-header__name">
          {installation.icon && <GameIcon icon={installation.icon} flavor={installation.flavor} size={64} />}
          <h1 className="game-header__title">{installation.label}</h1>
        </div>
        <div className="game-header__path" title={installation.path}>
          {installation.path}
        </div>
        <div className="game-header__status">
          {addons && (
            <span>
              <strong className="tnum">{list.length}</strong> {list.length === 1 ? "addon" : "addons"}
            </span>
          )}
          {checking ? (
            <span>Checking for updates</span>
          ) : updates > 0 ? (
            <span>
              <strong className="tnum">{updates}</strong> {updates === 1 ? "update" : "updates"} ready
            </span>
          ) : lastChecked ? (
            <span>Everything is up to date</span>
          ) : null}
          {lastChecked && !checking && <span>Checked {relativeTime(lastChecked)}</span>}
          {installation.clientVersion && (
            <span title={`Build ${installation.clientVersion}`}>Game version {shortVersion(installation.clientVersion)}</span>
          )}
        </div>
      </div>
      <div className="game-header__actions">
        <button
          className={`play${running ? " play--running" : ""}${stage && stage !== "done" ? " play--igniting" : ""}`}
          disabled={running || !!stage || !installation.available}
          onClick={() => void play(installation.id)}
        >
          <span className="play__label">
            {running && <span className="running-dot" aria-hidden="true" />}
            {running ? "Playing" : stage ? STAGE_LABEL[stage] : "Play"}
          </span>
          {note && <span className="play__note">{note}</span>}
        </button>
      </div>
    </header>
  );
}

export function Tabs({ installation }: { installation: Installation }) {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const addons = useStore((s) => s.addons[installation.id] ?? NO_ADDONS);
  const updates = addons.filter((a) => a.managed && a.update?.available && !a.pinned).length;
  const tabs: { id: View; label: string; count?: number; alert?: boolean }[] = [
    { id: "installed", label: "Installed", count: updates || addons.length, alert: updates > 0 },
    { id: "find", label: "Find addons" },
    { id: "share", label: "Share" },
    { id: "backups", label: "Backups" },
  ];
  return (
    <div className="tabs" role="tablist">
      {tabs.map((tab) => (
        <button key={tab.id} role="tab" className="tab" aria-selected={view === tab.id} onClick={() => setView(tab.id)}>
          {tab.label}
          {tab.count ? (
            <span className={`tab__count tnum${tab.alert ? " tab__count--alert" : ""}`}>
              {tab.alert ? `${tab.count} ready` : tab.count}
            </span>
          ) : null}
        </button>
      ))}
    </div>
  );
}

export function AppBanner() {
  const update = useStore((s) => s.appUpdate);
  const install = useStore((s) => s.installAppUpdate);
  if (update.status === "downloading") {
    return (
      <div className="app-banner" role="status">
        <span className="app-banner__text">
          Downloading Hearthforge {update.version}
          {update.progress ? ` (${update.progress}%)` : ""}
        </span>
      </div>
    );
  }
  if (update.status === "ready") {
    return (
      <div className="app-banner" role="status">
        <span className="app-banner__text">Hearthforge {update.version} is ready to install.</span>
        <button className="btn btn--primary btn--small" onClick={() => void install()}>
          Restart and update
        </button>
      </div>
    );
  }
  return null;
}
