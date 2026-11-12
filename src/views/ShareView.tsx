import { useEffect, useState } from "react";
import { Copy } from "lucide-react";
import { api, errorText } from "../lib/api";
import { plural, providerLabel } from "../lib/format";
import { NO_ADDONS, useStore } from "../lib/store";
import type { Installation, PackPreview } from "../lib/types";

const FLAVOR_NAMES: Record<string, string> = {
  forever: "WoW Forever",
  retail: "World of Warcraft",
  classicEra: "WoW Classic Era",
  classic: "WoW Classic",
  anniversary: "WoW Anniversary",
};

export function ShareView({ installation }: { installation: Installation }) {
  const addons = useStore((s) => s.addons[installation.id] ?? NO_ADDONS);
  const toast = useStore((s) => s.toast);
  const refresh = useStore((s) => s.refreshAddons);
  const shareable = addons.filter((a) => a.managed && a.source && a.source.provider !== "manual");

  const [name, setName] = useState("");
  const [chosen, setChosen] = useState<Set<string>>(() => new Set(shareable.map((a) => a.id)));
  const [code, setCode] = useState("");
  const [incoming, setIncoming] = useState("");
  const [preview, setPreview] = useState<PackPreview | null>(null);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    setChosen(new Set(shareable.map((a) => a.id)));
    // Re-select everything when the list of shareable addons changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shareable.length, installation.id]);

  function toggle(id: string) {
    setChosen((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function createCode() {
    try {
      const made = await api.exportPack(installation.id, name, [...chosen]);
      setCode(made);
      await navigator.clipboard.writeText(made).catch(() => undefined);
      toast("success", "Share code copied. Paste it to your friends.");
    } catch (error) {
      toast("error", errorText(error));
    }
  }

  async function previewCode() {
    try {
      setPreview(await api.previewPack(installation.id, incoming));
    } catch (error) {
      setPreview(null);
      toast("error", errorText(error));
    }
  }

  async function importCode() {
    setImporting(true);
    try {
      const result = await api.importPack(installation.id, incoming);
      if (result.succeeded.length) toast("success", `Installed ${plural(result.succeeded.length, "addon")}.`);
      for (const failure of result.failed) toast("error", `${failure.name}: ${failure.error}`);
      setPreview(await api.previewPack(installation.id, incoming));
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setImporting(false);
      void refresh(installation.id);
    }
  }

  const missing = preview?.items.filter((i) => !i.installed) ?? [];

  return (
    <>
      <h2 className="view-title">Share addon setups</h2>
      <p className="lede">
        Send friends a code and they get the same addons, installed and kept up to date. Codes list addons only: no
        settings or personal data.
      </p>
      <div className="panel-grid">
        <section className="panel">
          <h3 className="section-title">Share your setup</h3>
          <label className="field" style={{ marginTop: 10 }}>
            Name (optional)
            <input className="input" placeholder="Raid night" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          {shareable.length === 0 ? (
            <p className="muted">Install addons through Hearthforge first. Addons installed from a file can't be shared.</p>
          ) : (
            <div className="checklist">
              {shareable.map((addon) => (
                <label key={addon.id}>
                  <input type="checkbox" checked={chosen.has(addon.id)} onChange={() => toggle(addon.id)} />
                  <span style={{ flex: 1 }}>{addon.name}</span>
                  <span className="small faint">{providerLabel[addon.source!.provider]}</span>
                </label>
              ))}
            </div>
          )}
          <button className="btn btn--primary" disabled={chosen.size === 0} onClick={() => void createCode()}>
            <Copy size={15} /> Copy share code
          </button>
          {code && (
            <textarea
              className="textarea code-box"
              style={{ marginTop: 12 }}
              rows={3}
              readOnly
              value={code}
              onFocus={(e) => e.currentTarget.select()}
              aria-label="Share code"
            />
          )}
        </section>

        <section className="panel">
          <h3 className="section-title">Use a friend's setup</h3>
          <label className="field" style={{ marginTop: 10 }}>
            Share code
            <textarea
              className="textarea code-box"
              rows={4}
              placeholder="Paste a code that starts with HF1:"
              value={incoming}
              onChange={(e) => {
                setIncoming(e.target.value);
                setPreview(null);
              }}
            />
          </label>
          {!preview ? (
            <button className="btn" style={{ marginTop: 12 }} disabled={!incoming.trim()} onClick={() => void previewCode()}>
              Preview
            </button>
          ) : (
            <>
              {!preview.flavorMatches && (
                <p className="details__problem" style={{ marginTop: 12 }}>
                  This setup was made for {FLAVOR_NAMES[preview.flavor] ?? preview.flavor}. Addons without a{" "}
                  {installation.flavorName} build will fail to install.
                </p>
              )}
              <div className="checklist">
                {preview.items.map((item) => (
                  <label key={`${item.source.provider}:${item.source.projectId}`} style={{ cursor: "default" }}>
                    <span style={{ flex: 1, minWidth: 0 }}>
                      {item.name}
                      <span className="small faint">
                        {" "}
                        from {providerLabel[item.source.provider]} ({item.source.projectId})
                      </span>
                    </span>
                    <span className={`status ${item.installed ? "status--ok" : "status--idle"}`}>
                      {item.installed ? "Installed" : "Will install"}
                    </span>
                  </label>
                ))}
              </div>
              <button
                className="btn btn--primary"
                disabled={importing || missing.length === 0}
                onClick={() => void importCode()}
              >
                {importing
                  ? "Installing"
                  : missing.length
                    ? `Install ${plural(missing.length, "addon")}`
                    : "Everything is installed"}
              </button>
            </>
          )}
        </section>
      </div>
    </>
  );
}
