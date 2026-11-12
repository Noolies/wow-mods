import { useMemo, useState } from "react";
import { LoaderCircle, RefreshCw, Search } from "lucide-react";
import { AddonIcon, Option, Switch } from "../components/Bits";
import { api, confirmDialog, errorText, openUrl } from "../lib/api";
import {
  addonStatus,
  channelLabel,
  downloadProgress,
  parseGithubRepo,
  plural,
  providerLabel,
  relativeTime,
  type RowStatus,
} from "../lib/format";
import { useStore } from "../lib/store";
import type { AddonView, Channel, Installation } from "../lib/types";

type Filter = "all" | "updates" | "untracked" | "problems";

/** Same rule as the header and the backend: tracked, update ready, not pinned. */
const hasUpdate = (a: AddonView) => a.managed && !!a.update?.available && !a.pinned;

export function InstalledView({ installation }: { installation: Installation }) {
  const addons = useStore((s) => s.addons[installation.id]);
  const loading = useStore((s) => !!s.loading[installation.id]);
  const checking = useStore((s) => !!s.checking[installation.id]);
  const tasks = useStore((s) => s.tasks);
  const checkUpdates = useStore((s) => s.checkUpdates);
  const updateAll = useStore((s) => s.updateAll);
  const refresh = useStore((s) => s.refreshAddons);
  const setView = useStore((s) => s.setView);
  const toast = useStore((s) => s.toast);

  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [linking, setLinking] = useState(false);

  const list = addons ?? [];
  const statuses = useMemo(
    () => new Map(list.map((a) => [a.id, addonStatus(a, installation.flavorName, tasks[a.id])])),
    [list, tasks, installation.flavorName],
  );
  const counts = {
    all: list.length,
    updates: list.filter(hasUpdate).length,
    untracked: list.filter((a) => !a.managed).length,
    problems: list.filter((a) => statuses.get(a.id)?.kind === "problem").length,
  };
  const linkable = list.filter((a) => !a.managed && a.suggestedSource).length;

  const visible = list.filter((a) => {
    const status = statuses.get(a.id);
    if (filter === "updates" && !hasUpdate(a)) return false;
    if (filter === "untracked" && a.managed) return false;
    if (filter === "problems" && status?.kind !== "problem") return false;
    if (!query.trim()) return true;
    const q = query.trim().toLowerCase();
    return (
      a.name.toLowerCase().includes(q) ||
      (a.notes ?? "").toLowerCase().includes(q) ||
      a.folders.some((f) => f.name.toLowerCase().includes(q))
    );
  });

  async function linkAll() {
    setLinking(true);
    try {
      const linked = await api.linkAll(installation.id);
      toast("success", linked ? `Now tracking ${plural(linked, "addon")}.` : "No sources could be identified.");
      await refresh(installation.id);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setLinking(false);
    }
  }

  if (!installation.available) {
    return (
      <div className="empty">
        <h2 className="empty__title">This game folder is missing</h2>
        <p>
          Hearthforge can't find {installation.path}. If you moved the game, remove it in Settings and add the new
          folder.
        </p>
        <button className="btn" onClick={() => setView("settings")}>
          Open Settings
        </button>
      </div>
    );
  }

  if (!addons) {
    return (
      <div className="empty">
        <p>{loading ? "Reading your AddOns folder" : ""}</p>
      </div>
    );
  }

  if (list.length === 0) {
    return (
      <div className="empty">
        <h2 className="empty__title">No addons yet</h2>
        <p>
          Pick from addons known to work on {installation.flavorName}, paste a GitHub link, or drop a zip file anywhere in
          this window.
        </p>
        <button className="btn btn--primary" onClick={() => setView("find")}>
          Find addons
        </button>
      </div>
    );
  }

  return (
    <>
      {linkable > 0 && (
        <div className="notice notice--brass">
          <div className="notice__text">
            <strong>{plural(linkable, "addon")}</strong> {linkable === 1 ? "was" : "were"} installed outside Hearthforge.
            Track {linkable === 1 ? "it" : "them"} to get updates automatically.
          </div>
          <button className="btn btn--small" disabled={linking} onClick={() => void linkAll()}>
            {linking ? "Tracking" : "Track all"}
          </button>
        </div>
      )}

      <div className="toolbar">
        <label className="search-field">
          <Search size={16} />
          <span className="visually-hidden">Filter installed addons</span>
          <input
            className="input"
            placeholder="Filter installed addons"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <div className="segmented" role="group" aria-label="Show">
          {(
            [
              ["all", "All"],
              ["updates", "Updates"],
              ["untracked", "Not tracked"],
              ["problems", "Problems"],
            ] as [Filter, string][]
          ).map(([id, label]) => (
            <button key={id} aria-pressed={filter === id} onClick={() => setFilter(id)}>
              {label} <span className="tnum faint">{counts[id]}</span>
            </button>
          ))}
        </div>
        <div className="toolbar__end">
          <button className="btn" disabled={checking} onClick={() => void checkUpdates(installation.id)}>
            <RefreshCw size={15} className={checking ? "spin" : undefined} />
            {checking ? "Checking" : "Check for updates"}
          </button>
          <button
            className="btn btn--primary"
            disabled={counts.updates === 0}
            onClick={() => void updateAll(installation.id)}
          >
            {counts.updates > 0 ? `Update all (${counts.updates})` : "Update all"}
          </button>
        </div>
      </div>

      <div className="list" role="list">
        {visible.map((addon) => (
          <AddonRow
            key={addon.id}
            addon={addon}
            installation={installation}
            status={statuses.get(addon.id)!}
            open={open === addon.id}
            onToggle={() => setOpen(open === addon.id ? null : addon.id)}
          />
        ))}
        {visible.length === 0 && (
          <div className="empty">
            <p>Nothing here matches. Try another filter.</p>
          </div>
        )}
      </div>
    </>
  );
}

function statusClass(status: RowStatus): string {
  switch (status.kind) {
    case "ok":
      return "status status--ok";
    case "problem":
      return "status status--warn";
    case "busy":
      return "status status--busy";
    case "update":
    case "pinned":
      return "status status--brass";
    default:
      return "status status--idle";
  }
}

function AddonRow({
  addon,
  installation,
  status,
  open,
  onToggle,
}: {
  addon: AddonView;
  installation: Installation;
  status: RowStatus;
  open: boolean;
  onToggle: () => void;
}) {
  const task = useStore((s) => s.tasks[addon.id]);
  const toast = useStore((s) => s.toast);
  const refresh = useStore((s) => s.refreshAddons);
  const pct = downloadProgress(task);
  const busy = status.kind === "busy";

  async function update(event?: React.MouseEvent) {
    event?.stopPropagation();
    try {
      await api.updateAddons(installation.id, [addon.id]);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      await refresh(installation.id);
    }
  }

  async function track(event: React.MouseEvent) {
    event.stopPropagation();
    if (!addon.suggestedSource) return;
    try {
      await api.linkAddon(installation.id, addon.id, addon.suggestedSource);
      toast("success", `Tracking ${addon.name} from ${providerLabel[addon.suggestedSource.provider]}.`);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      await refresh(installation.id);
    }
  }

  let end: React.ReactNode;
  if (busy) {
    end = (
      <span className={statusClass(status)}>
        {status.label}
        {pct !== null ? ` ${pct}%` : ""}
      </span>
    );
  } else if (hasUpdate(addon)) {
    end = (
      <button className="btn btn--primary btn--small" onClick={(e) => void update(e)}>
        Update
      </button>
    );
  } else if (!addon.managed && addon.suggestedSource && status.kind !== "problem") {
    end = (
      <button className="btn btn--small" onClick={(e) => void track(e)}>
        Track updates
      </button>
    );
  } else {
    end = <span className={statusClass(status)}>{status.label}</span>;
  }

  const latest = addon.update?.latestVersion;
  return (
    <div className="row" role="listitem" data-open={open}>
      <div
        className="row__main"
        onClick={onToggle}
        role="button"
        tabIndex={0}
        aria-expanded={open}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
      >
        <AddonIcon name={addon.name} src={addon.thumbnail} />
        <div style={{ minWidth: 0 }}>
          <div className="row__name">{addon.name}</div>
          <div className="row__summary">{addon.notes ?? addon.folders.map((f) => f.name).join(", ")}</div>
        </div>
        <div className="row__version tnum" title={addon.version ?? undefined}>
          {addon.managed && addon.update?.available && latest ? (
            <>
              {addon.version} → <b>{latest}</b>
            </>
          ) : (
            (addon.version ?? "Unknown version")
          )}
        </div>
        <div className="source">{addon.source ? providerLabel[addon.source.provider] : "Local folder"}</div>
        <div className="row__end">{end}</div>
      </div>
      {busy && (
        <div className={`row__progress${pct === null ? " row__progress--indeterminate" : ""}`}>
          <span style={pct !== null ? { width: `${pct}%` } : undefined} />
        </div>
      )}
      {open && <AddonDetails addon={addon} installation={installation} status={status} onUpdate={() => void update()} />}
    </div>
  );
}

function AddonDetails({
  addon,
  installation,
  status,
  onUpdate,
}: {
  addon: AddonView;
  installation: Installation;
  status: RowStatus;
  onUpdate: () => void;
}) {
  const settings = useStore((s) => s.boot!.settings);
  const toast = useStore((s) => s.toast);
  const refresh = useStore((s) => s.refreshAddons);
  const openFind = useStore((s) => s.openFind);
  const openDetails = useStore((s) => s.openDetails);
  const [link, setLink] = useState("");
  const [working, setWorking] = useState(false);
  const isManual = addon.source?.provider === "manual";
  const gameDefault = installation.defaultChannel ?? settings.defaultChannel;

  async function setOptions(options: Parameters<typeof api.setAddonOptions>[2]) {
    try {
      await api.setAddonOptions(installation.id, addon.id, options);
      await refresh(installation.id);
    } catch (error) {
      toast("error", errorText(error));
    }
  }

  async function remove() {
    const ok = await confirmDialog(
      `Remove ${addon.name}? Its folders are deleted from AddOns. Its saved settings stay in your WTF folder.`,
      "Remove addon",
      "Remove",
    );
    if (!ok) return;
    setWorking(true);
    try {
      await api.uninstallAddon(installation.id, addon.id);
      toast("success", `Removed ${addon.name}.`);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setWorking(false);
      await refresh(installation.id);
    }
  }

  async function reinstall() {
    if (!addon.source) return;
    setWorking(true);
    try {
      await api.installAddon(installation.id, addon.source);
      toast("success", `Reinstalled ${addon.name}.`);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setWorking(false);
      await refresh(installation.id);
    }
  }

  async function trackFromLink() {
    const repo = parseGithubRepo(link);
    if (!repo) {
      toast("error", "Paste a GitHub project link, like github.com/owner/project.");
      return;
    }
    setWorking(true);
    try {
      await api.linkAddon(installation.id, addon.id, { provider: "github", projectId: repo });
      toast("success", `Tracking ${addon.name} from GitHub.`);
      await refresh(installation.id);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setWorking(false);
    }
  }

  const update = addon.update;
  return (
    <div className="details">
      <div>
        {status.detail && <p className="details__problem">{status.detail}</p>}
        {addon.notes && addon.notes.length > 90 && <p className="details__notes">{addon.notes}</p>}
        {update?.available && update.changelog && (
          <>
            <div className="option__label">What's new in {update.latestVersion}</div>
            <pre className="changelog">{update.changelog}</pre>
          </>
        )}
        <ul className="folders" aria-label="Folders">
          {addon.folders.map((f) => (
            <li key={f.name}>
              <span>
                {f.name}
                {f.loadOnDemand ? <span className="faint"> (loads when needed)</span> : null}
              </span>
              <span className="faint tnum">
                {f.interfaces.length ? `Interface ${f.interfaces.join(", ")}` : f.otherTocs.join(", ") || "No TOC file"}
              </span>
            </li>
          ))}
          {addon.missingFolders.map((name) => (
            <li key={name}>
              <span>{name}</span>
              <span className="status status--warn">Missing</span>
            </li>
          ))}
        </ul>
        <div className="details__actions">
          {update?.available && (
            <button className="btn btn--primary btn--small" onClick={onUpdate}>
              Update to {update.latestVersion}
            </button>
          )}
          {addon.managed && !isManual && (
            <button className="btn btn--small" disabled={working} onClick={() => void reinstall()}>
              Reinstall
            </button>
          )}
          {addon.managed && addon.source && !isManual && (
            <button
              className="btn btn--small"
              onClick={() =>
                openDetails(addon.source!, {
                  name: addon.name,
                  summary: addon.notes,
                  author: addon.author,
                  thumbnail: addon.thumbnail,
                  website: addon.website,
                })
              }
            >
              Details
            </button>
          )}
          {addon.folders[0] && (
            <button className="btn btn--small" onClick={() => void api.revealAddon(installation.id, addon.folders[0].name)}>
              Open folder
            </button>
          )}
          {addon.website && (
            <button className="btn btn--small btn--ghost" onClick={() => void openUrl(addon.website!)}>
              Website
            </button>
          )}
          {update?.changelogUrl && (
            <button className="btn btn--small btn--ghost" onClick={() => void openUrl(update.changelogUrl!)}>
              Release notes
            </button>
          )}
          <button className="btn btn--small btn--ghost btn--danger" disabled={working} onClick={() => void remove()}>
            Remove
          </button>
        </div>
      </div>

      <div>
        {addon.managed && !isManual && (
          <>
            <Option label="Release channel" hint="Beta and alpha builds arrive sooner but can be unstable.">
              <select
                className="select"
                value={addon.channel ?? "default"}
                onChange={(e) =>
                  void setOptions({ channel: e.target.value === "default" ? null : (e.target.value as Channel) })
                }
              >
                <option value="default">Default ({channelLabel[gameDefault]})</option>
                <option value="stable">Stable</option>
                <option value="beta">Beta</option>
                <option value="alpha">Alpha</option>
              </select>
            </Option>
            <Option label="Update automatically" hint="Installs updates in the background while the game is closed.">
              <Switch
                label="Update automatically"
                checked={addon.autoUpdate ?? settings.autoInstallUpdates}
                onChange={(value) => void setOptions({ autoUpdate: value })}
              />
            </Option>
            <Option label="Pin this version" hint="Skips updates until you unpin it.">
              <Switch label="Pin this version" checked={addon.pinned} onChange={(value) => void setOptions({ pinned: value })} />
            </Option>
            <p className="small faint">
              From {providerLabel[addon.source!.provider]} ({addon.source!.projectId})
              {addon.installedAt ? `, installed ${relativeTime(addon.installedAt)}` : ""}.
            </p>
          </>
        )}
        {isManual && (
          <p className="muted small">
            Installed from a file, so it can't update itself. Install it from Find addons to get updates.
          </p>
        )}
        {!addon.managed && (
          <>
            <div className="option__label">Track updates for this addon</div>
            <p className="option__hint" style={{ margin: "2px 0 10px" }}>
              It was installed outside Hearthforge.{" "}
              {addon.suggestedSource
                ? `It looks like it comes from ${providerLabel[addon.suggestedSource.provider]} (${addon.suggestedSource.projectId}).`
                : "Paste its GitHub link, or search for it."}
            </p>
            <div className="stack">
              {!addon.suggestedSource && (
                <div className="secret-row" style={{ marginTop: 0 }}>
                  <input
                    className="input"
                    placeholder="github.com/owner/project"
                    value={link}
                    onChange={(e) => setLink(e.target.value)}
                  />
                  <button className="btn" disabled={working || !link.trim()} onClick={() => void trackFromLink()}>
                    {working ? <LoaderCircle size={15} className="spin" /> : null}
                    Track
                  </button>
                </div>
              )}
              <button className="btn btn--small" onClick={() => openFind(addon.name)}>
                Search for {addon.name}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
