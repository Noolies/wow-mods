import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { X } from "lucide-react";
import { api, errorText, openUrl } from "../lib/api";
import {
  bytes,
  compactNumber,
  downloadProgress,
  joinNames,
  providerLabel,
  relativeTime,
  sourceKey,
} from "../lib/format";
import { renderRichText } from "../lib/richtext";
import { NO_ADDONS, useSelected, useStore } from "../lib/store";
import type { AddonDetails, TextFormat } from "../lib/types";
import { AddonIcon } from "./Bits";

const normalise = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, "");

/** Drops a leading heading that repeats the panel's own title or section name. */
function withoutRepeatedHeading(html: string, repeats: string[]): string {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild;
  const first = root?.firstElementChild;
  if (root && first && /^H[1-3]$/.test(first.tagName) && repeats.some((r) => normalise(r) === normalise(first.textContent ?? ""))) {
    first.remove();
    return root.innerHTML;
  }
  return html;
}

function RichText({
  text,
  format,
  linkBase,
  imageBase,
  repeats = [],
}: {
  text: string;
  format: TextFormat;
  linkBase: string | null;
  imageBase: string | null;
  repeats?: string[];
}) {
  const repeatKey = repeats.join("|");
  const html = useMemo(
    () => withoutRepeatedHeading(renderRichText(text, format, linkBase, imageBase), repeatKey ? repeatKey.split("|") : []),
    [text, format, linkBase, imageBase, repeatKey],
  );
  return (
    <div
      className="prose"
      // Sanitised by DOMPurify in renderRichText.
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={(event) => {
        const link = (event.target as HTMLElement).closest("a");
        if (link) {
          event.preventDefault();
          if (link.href) void openUrl(link.href);
        }
      }}
    />
  );
}

/** Slide-in panel with everything known about an addon, opened from search results or the installed list. */
export function AddonDetailsPanel() {
  const target = useStore((s) => s.details);
  const close = useStore((s) => s.closeDetails);
  const installation = useSelected();
  const addons = useStore((s) => (installation ? (s.addons[installation.id] ?? NO_ADDONS) : NO_ADDONS));
  const toast = useStore((s) => s.toast);
  const refresh = useStore((s) => s.refreshAddons);
  const task = useStore((s) => (target ? s.tasks[sourceKey(target.source)] : undefined));

  const [details, setDetails] = useState<AddonDetails | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [zoomed, setZoomed] = useState<string | null>(null);
  const closeButton = useRef<HTMLButtonElement>(null);

  const load = useCallback(async () => {
    if (!target || !installation) return;
    setDetails(null);
    setError(null);
    try {
      setDetails(await api.addonDetails(installation.id, target.source));
    } catch (failure) {
      setError(errorText(failure));
    }
  }, [target, installation]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!target) return;
    closeButton.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (zoomed) setZoomed(null);
        else close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [target, close, zoomed]);

  if (!target || !installation) return null;

  const installed = addons.find((a) => a.source && sourceKey(a.source) === sourceKey(target.source));
  const working = !!task && task.stage !== "done" && task.stage !== "failed";
  const pct = downloadProgress(task);
  const name = details?.name ?? target.hint.name ?? target.source.projectId;
  const summary = details?.summary ?? target.hint.summary ?? null;
  const author = details?.author ?? target.hint.author ?? null;
  const thumbnail = details?.thumbnail ?? target.hint.thumbnail ?? null;

  async function install() {
    if (!installation || !target) return;
    try {
      await api.installAddon(installation.id, target.source, {
        name,
        summary,
        author,
        thumbnail,
        website: details?.website ?? target.hint.website ?? null,
      });
      toast("success", `Installed ${name}.`);
    } catch (failure) {
      toast("error", errorText(failure));
    } finally {
      void refresh(installation.id);
    }
  }

  async function update() {
    if (!installation || !installed) return;
    try {
      await api.updateAddons(installation.id, [installed.id]);
      toast("success", `Updated ${name}.`);
    } catch (failure) {
      toast("error", errorText(failure));
    } finally {
      void refresh(installation.id);
    }
  }

  let action: React.ReactNode;
  if (working) {
    action = (
      <span className="status status--busy">
        {task?.stage === "downloading" ? `Downloading${pct !== null ? ` ${pct}%` : ""}` : "Installing"}
      </span>
    );
  } else if (installed?.update?.available && !installed.pinned) {
    action = (
      <button className="btn btn--primary" onClick={() => void update()}>
        Update to {installed.update.latestVersion}
      </button>
    );
  } else if (installed) {
    action = <span className="status status--ok">Installed</span>;
  } else {
    action = (
      <button className="btn btn--primary" disabled={details?.supportsFlavor === false} onClick={() => void install()}>
        Install
      </button>
    );
  }

  const support =
    details?.supportsFlavor === true ? (
      <span className="status status--ok">Works on {installation.flavorName}</span>
    ) : details?.supportsFlavor === false ? (
      <span className="status status--warn">No {installation.flavorName} build yet</span>
    ) : details ? (
      <span className="status status--idle">Not confirmed</span>
    ) : null;

  const links: [string, string | null | undefined][] = [
    ["Website", details?.website ?? target.hint.website],
    ["Source code", details?.sourceUrl !== details?.website ? details?.sourceUrl : null],
    ["Report a problem", details?.issuesUrl],
    ["Support the author", details?.donateUrl],
  ];

  return (
    <div className="sheet-backdrop" onClick={close}>
      <aside
        className="sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="sheet-title"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="sheet__head">
          <AddonIcon name={name} src={thumbnail} />
          <div style={{ minWidth: 0 }}>
            <h2 id="sheet-title" className="sheet__title">
              {name}
            </h2>
            <div className="sheet__by">
              {author && <span>by {author}</span>}
              <span>{providerLabel[target.source.provider]}</span>
            </div>
          </div>
          <button ref={closeButton} className="btn btn--ghost" aria-label="Close details" onClick={close}>
            <X size={18} />
          </button>
        </header>

        {summary && <p className="sheet__summary">{summary}</p>}

        <div className="sheet__actions">
          {action}
          {links
            .filter(([, url]) => !!url)
            .map(([label, url]) => (
              <button key={label} className="btn btn--ghost" onClick={() => void openUrl(url!)}>
                {label}
              </button>
            ))}
        </div>

        {error && (
          <div className="notice">
            <div className="notice__text">Couldn't load the details. {error}</div>
            <button className="btn btn--small" onClick={() => void load()}>
              Try again
            </button>
          </div>
        )}
        {!details && !error && <p className="muted">Loading details</p>}

        {details && (
          <>
            <dl className="facts">
              <div>
                <dt>{installation.flavorName}</dt>
                <dd>{support}</dd>
              </div>
              {details.latestVersion && (
                <div>
                  <dt>Latest version</dt>
                  <dd className="tnum" title={details.latestVersion}>
                    {details.latestVersion}
                  </dd>
                </div>
              )}
              {details.latestDate && (
                <div>
                  <dt>Released</dt>
                  <dd>{relativeTime(details.latestDate)}</dd>
                </div>
              )}
              {details.downloadSize !== null && (
                <div>
                  <dt>Download size</dt>
                  <dd className="tnum">{bytes(details.downloadSize)}</dd>
                </div>
              )}
              {details.stars !== null && (
                <div>
                  <dt>GitHub stars</dt>
                  <dd className="tnum">{compactNumber(details.stars)}</dd>
                </div>
              )}
              {details.downloads !== null && (
                <div>
                  <dt>Downloads</dt>
                  <dd className="tnum">{compactNumber(details.downloads)}</dd>
                </div>
              )}
              {details.license && (
                <div>
                  <dt>Licence</dt>
                  <dd>{details.license}</dd>
                </div>
              )}
              {details.updatedAt && (
                <div>
                  <dt>Last changed</dt>
                  <dd>{relativeTime(details.updatedAt)}</dd>
                </div>
              )}
            </dl>

            {details.gameVersions.length > 0 && (
              <p className="sheet__games">Has builds for {joinNames(details.gameVersions)}.</p>
            )}
            {details.tags.length > 0 && (
              <ul className="tags" aria-label="Tags">
                {details.tags.map((tag) => (
                  <li key={tag}>{tag}</li>
                ))}
              </ul>
            )}

            {details.screenshots.length > 0 && (
              <section>
                <h3>Screenshots</h3>
                <div className="shots">
                  {details.screenshots.map((url) => (
                    <button key={url} onClick={() => setZoomed(url)} aria-label="Enlarge screenshot">
                      <img src={url} alt="" loading="lazy" />
                    </button>
                  ))}
                </div>
              </section>
            )}

            {details.description && (
              <section>
                <h3>About</h3>
                <RichText
                  text={details.description}
                  format={details.descriptionFormat}
                  linkBase={details.linkBase}
                  imageBase={details.imageBase}
                  repeats={[details.name, "About", "Description", "Overview"]}
                />
              </section>
            )}

            {details.releaseNotes && (
              <section>
                <h3>What's new{details.latestVersion ? ` in ${details.latestVersion}` : ""}</h3>
                <RichText
                  text={details.releaseNotes}
                  format={details.releaseNotesFormat}
                  linkBase={details.sourceUrl ? `${details.sourceUrl}/blob/HEAD/` : null}
                  imageBase={null}
                />
                {details.releaseUrl && (
                  <button className="btn btn--small btn--ghost" onClick={() => void openUrl(details.releaseUrl!)}>
                    All release notes
                  </button>
                )}
              </section>
            )}
          </>
        )}
      </aside>
      {zoomed && (
        <div
          className="lightbox"
          onClick={(event) => {
            event.stopPropagation();
            setZoomed(null);
          }}
        >
          <img src={zoomed} alt="" />
        </div>
      )}
    </div>
  );
}
