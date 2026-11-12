import { useEffect, useMemo, useState } from "react";
import { Search, Star } from "lucide-react";
import { AddonIcon } from "../components/Bits";
import { api, errorText, inTauri, pickZip } from "../lib/api";
import { compactNumber, downloadProgress, joinNames, plural, providerLabel, sourceKey } from "../lib/format";
import { useStore } from "../lib/store";
import type { Installation, SearchHit, SearchResults, SearchScope } from "../lib/types";

export function FindView({ installation }: { installation: Installation }) {
  const initialQuery = useStore((s) => s.findQuery);
  const hasCurseforge = useStore((s) => !!s.boot?.secrets.curseforgeKey);
  const setView = useStore((s) => s.setView);
  const toast = useStore((s) => s.toast);
  const refresh = useStore((s) => s.refreshAddons);

  const [query, setQuery] = useState(initialQuery);
  const [scope, setScope] = useState<SearchScope>("all");
  const [results, setResults] = useState<SearchResults | null>(null);
  const [searching, setSearching] = useState(false);
  const [link, setLink] = useState("");
  const [busy, setBusy] = useState(false);
  const [installingKit, setInstallingKit] = useState(false);

  useEffect(() => setQuery(initialQuery), [initialQuery]);

  useEffect(() => {
    let cancelled = false;
    setSearching(true);
    const timer = setTimeout(async () => {
      try {
        const found = await api.searchAddons(installation.id, query, scope);
        if (!cancelled) setResults(found);
      } catch (error) {
        if (!cancelled) toast("error", errorText(error));
      } finally {
        if (!cancelled) setSearching(false);
      }
    }, query ? 400 : 0);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [installation.id, query, scope, toast]);

  const markInstalled = (hit: SearchHit) =>
    setResults((r) =>
      r ? { ...r, hits: r.hits.map((h) => (sourceKey(h.source) === sourceKey(hit.source) ? { ...h, installed: true } : h)) } : r,
    );

  async function install(hit: SearchHit) {
    try {
      await api.installAddon(installation.id, hit.source, {
        name: hit.name,
        summary: hit.summary,
        author: hit.author,
        thumbnail: hit.thumbnail,
        website: hit.website,
      });
      markInstalled(hit);
      toast("success", `Installed ${hit.name}.`);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      void refresh(installation.id);
    }
  }

  const hits = results?.hits ?? [];
  const starterKit = hits.filter((h) => h.starter && !h.installed);

  async function installStarterKit() {
    setInstallingKit(true);
    for (const hit of starterKit) {
      await install(hit);
    }
    setInstallingKit(false);
  }

  async function installLink() {
    setBusy(true);
    try {
      const record = await api.installFromUrl(installation.id, link);
      toast("success", `Installed ${record.name ?? "the addon"}.`);
      setLink("");
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setBusy(false);
      void refresh(installation.id);
    }
  }

  async function installZip() {
    const path = await pickZip();
    if (!path) {
      if (!inTauri) toast("info", "Choosing files works in the desktop app.");
      return;
    }
    setBusy(true);
    try {
      const record = await api.installFromFile(installation.id, path);
      toast("success", `Installed ${record.name}.`);
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setBusy(false);
      void refresh(installation.id);
    }
  }

  const grouped = useMemo(() => {
    if (query.trim()) return null;
    const featured = hits.filter((h) => h.featured);
    const others = hits.filter((h) => !h.featured);
    const groups = new Map<string, SearchHit[]>();
    for (const hit of featured) {
      const key = hit.category ?? "Other";
      groups.set(key, [...(groups.get(key) ?? []), hit]);
    }
    if (others.length) groups.set("More to explore", others);
    return groups;
  }, [hits, query]);

  return (
    <div className="find-layout">
      <div>
        <label className="search-field search-field--large" style={{ marginBottom: 12 }}>
          <Search size={18} />
          <span className="visually-hidden">Search addons</span>
          <input
            className="input"
            placeholder={`Search addons for ${installation.flavorName}`}
            value={query}
            autoFocus
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <div className="toolbar">
          <div className="segmented" role="group" aria-label="Sources">
            {(
              [
                ["all", "All sources"],
                ["catalog", "Recommended"],
                ["github", "GitHub"],
                ["curseforge", "CurseForge"],
                ["tukui", "Tukui"],
              ] as [SearchScope, string][]
            ).map(([id, label]) => (
              <button
                key={id}
                aria-pressed={scope === id}
                disabled={id === "curseforge" && !hasCurseforge}
                title={id === "curseforge" && !hasCurseforge ? "Add a CurseForge API key in Settings first" : undefined}
                onClick={() => setScope(id)}
              >
                {label}
              </button>
            ))}
          </div>
          <span className="small faint">{searching ? "Searching" : results ? plural(hits.length, "result") : ""}</span>
        </div>

        {results?.warnings.map((w) => (
          <p key={w} className="small faint" style={{ margin: "0 0 8px" }}>
            {w}
          </p>
        ))}

        {!query.trim() && starterKit.length > 0 && scope !== "github" && (
          <div className="starter">
            <div>
              <h3>New to {installation.flavorName}?</h3>
              <p>
                Start with {joinNames(starterKit.map((h) => h.name))}: the addons most players install first.
              </p>
            </div>
            <button className="btn btn--primary" disabled={installingKit} onClick={() => void installStarterKit()}>
              {installingKit ? "Installing" : "Install starter kit"}
            </button>
          </div>
        )}

        {grouped
          ? [...grouped.entries()].map(([category, items]) => (
              <section key={category}>
                <h2 className="category">{category}</h2>
                {items.map((hit) => (
                  <HitRow key={sourceKey(hit.source)} hit={hit} installation={installation} onInstall={install} />
                ))}
              </section>
            ))
          : hits.map((hit) => (
              <HitRow key={sourceKey(hit.source)} hit={hit} installation={installation} onInstall={install} />
            ))}

        {results && hits.length === 0 && !searching && (
          <div className="empty">
            <h2 className="empty__title">No matches</h2>
            <p>
              Try a shorter search, or paste the addon's GitHub link on the right. Addons that only publish on
              CurseForge need a CurseForge API key in Settings.
            </p>
          </div>
        )}
      </div>

      <aside className="aside">
        <div className="aside__block">
          <h3>Install from a link</h3>
          <p>A GitHub or Tukui project link, or a direct link to a .zip file.</p>
          <div className="stack">
            <input
              className="input"
              placeholder="github.com/owner/project"
              value={link}
              onChange={(e) => setLink(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && link.trim()) void installLink();
              }}
            />
            <button className="btn" disabled={busy || !link.trim()} onClick={() => void installLink()}>
              Install from link
            </button>
          </div>
        </div>
        <div className="aside__block">
          <h3>Install a zip file</h3>
          <p>Or drop a zip anywhere in this window. Zip installs don't update automatically.</p>
          <button className="btn" disabled={busy} onClick={() => void installZip()}>
            Choose a zip file
          </button>
        </div>
        {!hasCurseforge && (
          <div className="aside__block">
            <h3>CurseForge</h3>
            <p>Some addons are only on CurseForge. Add your own free API key to search there too, no CurseForge app needed.</p>
            <button className="btn btn--small" onClick={() => setView("settings")}>
              Add a CurseForge key
            </button>
          </div>
        )}
      </aside>
    </div>
  );
}

function HitRow({
  hit,
  installation,
  onInstall,
}: {
  hit: SearchHit;
  installation: Installation;
  onInstall: (hit: SearchHit) => Promise<void>;
}) {
  const task = useStore((s) => s.tasks[sourceKey(hit.source)]);
  const openDetails = useStore((s) => s.openDetails);
  const working = !!task && task.stage !== "done" && task.stage !== "failed";
  const pct = downloadProgress(task);

  let support: React.ReactNode;
  if (hit.supportsFlavor === true) support = <span className="status status--ok">Works on {installation.flavorName}</span>;
  else if (hit.supportsFlavor === false) support = <span className="status status--warn">No {installation.flavorName} build yet</span>;
  else support = <span className="status status--idle">Not confirmed for {installation.flavorName}</span>;

  return (
    <div className="hit">
      <AddonIcon name={hit.name} src={hit.thumbnail} />
      <div style={{ minWidth: 0 }}>
        <button
          className="hit__open"
          aria-label={`Details for ${hit.name}`}
          onClick={() =>
            openDetails(hit.source, {
              name: hit.name,
              summary: hit.summary,
              author: hit.author,
              thumbnail: hit.thumbnail,
              website: hit.website,
            })
          }
        >
          <span className="row__name">{hit.name}</span>
          {hit.summary && <span className="row__summary">{hit.summary}</span>}
        </button>
        <div className="hit__meta">
          {support}
          <span>{providerLabel[hit.source.provider]}</span>
          {hit.author && <span>by {hit.author}</span>}
          {hit.stars !== null && (
            <span className="tnum">
              <Star size={11} style={{ verticalAlign: -1 }} /> {compactNumber(hit.stars)}
            </span>
          )}
          {hit.downloads !== null && <span className="tnum">{compactNumber(hit.downloads)} downloads</span>}
        </div>
      </div>
      <div>
        {hit.installed ? (
          <span className="status status--ok">Installed</span>
        ) : working ? (
          <span className="status status--busy">
            {task?.stage === "downloading" ? `Downloading${pct !== null ? ` ${pct}%` : ""}` : "Installing"}
          </span>
        ) : (
          <button
            className="btn btn--primary btn--small"
            disabled={hit.supportsFlavor === false}
            onClick={() => void onInstall(hit)}
          >
            Install
          </button>
        )}
      </div>
    </div>
  );
}
