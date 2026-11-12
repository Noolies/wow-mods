import { useState } from "react";
import { GameFinder } from "../components/GameFinder";
import { GameIcon, Option, Switch } from "../components/Bits";
import { api, confirmDialog, errorText, openUrl } from "../lib/api";
import { channelLabel, shortVersion } from "../lib/format";
import { useStore } from "../lib/store";
import type { AfterLaunch, Channel, Installation, LaunchMethod, SecretKind, Theme } from "../lib/types";

export function SettingsView() {
  const boot = useStore((s) => s.boot!);
  const save = useStore((s) => s.saveSettings);
  const checkAppUpdate = useStore((s) => s.checkAppUpdate);
  const appUpdate = useStore((s) => s.appUpdate);
  const settings = boot.settings;
  const isMac = boot.platform === "macos";

  return (
    <div className="page">
      <h1 className="view-title">Settings</h1>

      <section className="settings-section">
        <h2 className="section-title">Games</h2>
        <p className="lede">The game folders Hearthforge manages addons for.</p>
        {boot.installations.map((inst) => (
          <InstallationCard key={inst.id} installation={inst} />
        ))}
        <div style={{ marginTop: 14 }}>
          <GameFinder />
        </div>
      </section>

      <section className="settings-section">
        <h2 className="section-title">Updates</h2>
        <Option label="Check when Hearthforge starts">
          <Switch label="Check when Hearthforge starts" checked={settings.checkOnStartup} onChange={(v) => void save({ checkOnStartup: v })} />
        </Option>
        <Option
          label="Keep checking in the background"
          hint={
            <>
              Every{" "}
              <select
                className="select"
                style={{ height: 26, padding: "0 4px" }}
                value={settings.checkIntervalHours}
                onChange={(e) => void save({ checkIntervalHours: Number(e.target.value) })}
                aria-label="Check interval"
              >
                {[1, 2, 3, 6, 12, 24].map((h) => (
                  <option key={h} value={h}>
                    {h === 1 ? "hour" : `${h} hours`}
                  </option>
                ))}
              </select>{" "}
              while Hearthforge is open or in the tray.
            </>
          }
        >
          <Switch label="Keep checking in the background" checked={settings.backgroundChecks} onChange={(v) => void save({ backgroundChecks: v })} />
        </Option>
        <Option label="Install updates automatically" hint="Updates install by themselves while the game is closed. You can override this per addon.">
          <Switch label="Install updates automatically" checked={settings.autoInstallUpdates} onChange={(v) => void save({ autoInstallUpdates: v })} />
        </Option>
        <Option label="Update addons before playing" hint="Pressing Play installs waiting updates first.">
          <Switch label="Update addons before playing" checked={settings.updateBeforeLaunch} onChange={(v) => void save({ updateBeforeLaunch: v })} />
        </Option>
        <Option label="Default release channel" hint="Beta and alpha builds arrive sooner but can be unstable.">
          <select className="select" value={settings.defaultChannel} onChange={(e) => void save({ defaultChannel: e.target.value as Channel })}>
            {(["stable", "beta", "alpha"] as Channel[]).map((c) => (
              <option key={c} value={c}>
                {channelLabel[c]}
              </option>
            ))}
          </select>
        </Option>
        <Option label="Notifications" hint="Tells you when updates are ready or installed while the window is hidden.">
          <Switch label="Notifications" checked={settings.notifications} onChange={(v) => void save({ notifications: v })} />
        </Option>
      </section>

      <section className="settings-section">
        <h2 className="section-title">Playing</h2>
        <Option label="Back up settings before playing" hint="Saves your WTF folder each time you press Play.">
          <Switch label="Back up settings before playing" checked={settings.backupBeforeLaunch} onChange={(v) => void save({ backupBeforeLaunch: v })} />
        </Option>
        <Option label="Automatic backups to keep" hint="Older automatic backups are deleted. Backups you make yourself are kept.">
          <select className="select" value={settings.backupRetention} onChange={(e) => void save({ backupRetention: Number(e.target.value) })}>
            {[3, 5, 10, 20, 50].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </Option>
        <Option label="When the game starts">
          <select className="select" value={settings.afterLaunch} onChange={(e) => void save({ afterLaunch: e.target.value as AfterLaunch })}>
            <option value="tray">Hide Hearthforge to the tray</option>
            <option value="keep">Keep the window open</option>
            <option value="quit">Quit Hearthforge</option>
          </select>
        </Option>
      </section>

      <section className="settings-section">
        <h2 className="section-title">App</h2>
        <Option label={isMac ? "Open at login" : "Start with Windows"} hint="Starts quietly in the tray so updates are ready before you play.">
          <Switch label="Start with the computer" checked={settings.startWithSystem} onChange={(v) => void save({ startWithSystem: v })} />
        </Option>
        <Option label="Closing the window keeps Hearthforge in the tray" hint="Quit from the tray icon's menu.">
          <Switch label="Close to tray" checked={settings.closeToTray} onChange={(v) => void save({ closeToTray: v })} />
        </Option>
        <Option label="Theme">
          <select className="select" value={settings.theme} onChange={(e) => void save({ theme: e.target.value as Theme })}>
            <option value="dark">Dusk (dark)</option>
            <option value="light">Daybreak (light)</option>
            <option value="system">Match the system</option>
          </select>
        </Option>
        <Option
          label="Update Hearthforge automatically"
          hint={
            boot.updaterConfigured
              ? `Version ${boot.appVersion}. New versions download in the background and install when you restart.`
              : `Version ${boot.appVersion}. Automatic app updates aren't set up for this build.`
          }
        >
          <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
            <button
              className="btn btn--small"
              disabled={!boot.updaterConfigured || appUpdate.status === "checking" || appUpdate.status === "downloading"}
              onClick={() => void checkAppUpdate(true)}
            >
              {appUpdate.status === "checking" ? "Checking" : "Check now"}
            </button>
            <Switch label="Update Hearthforge automatically" checked={settings.appAutoUpdate} onChange={(v) => void save({ appAutoUpdate: v })} />
          </div>
        </Option>
      </section>

      <section className="settings-section">
        <h2 className="section-title">Addon sources</h2>
        <p className="lede">
          GitHub and Tukui work without any setup. Keys are optional and stay in {isMac ? "your Keychain" : "Windows Credential Manager"}, never in plain files.
        </p>
        <SecretField
          kind="githubToken"
          label="GitHub token"
          hint="Raises GitHub's limit from 60 to 5,000 requests an hour. A token with no scopes is enough."
          helpUrl="https://github.com/settings/personal-access-tokens/new"
          helpLabel="Create a token"
        />
        <SecretField
          kind="curseforgeKey"
          label="CurseForge API key"
          hint="Lets you search and install addons that are only on CurseForge, without the CurseForge app."
          helpUrl="https://console.curseforge.com/"
          helpLabel="Get a key"
        />
      </section>

      <section className="settings-section">
        <h2 className="section-title">Help</h2>
        <div className="details__actions">
          <button className="btn btn--small" onClick={() => void api.openFolder("logs")}>
            Open log folder
          </button>
          <button className="btn btn--small" onClick={() => void api.openFolder("data")}>
            Open data folder
          </button>
          <button className="btn btn--small btn--ghost" onClick={() => void openUrl("https://github.com/Noolies/wow-mods/issues")}>
            Report a problem
          </button>
        </div>
        <p className="small faint">Hearthforge {boot.appVersion}. Data folder: {boot.dataDir}</p>
      </section>
    </div>
  );
}

function InstallationCard({ installation }: { installation: Installation }) {
  const reloadBoot = useStore((s) => s.reloadBoot);
  const settings = useStore((s) => s.boot!.settings);
  const toast = useStore((s) => s.toast);
  const [label, setLabel] = useState(installation.label);

  async function patch(change: Parameters<typeof api.updateInstallation>[1]) {
    try {
      await api.updateInstallation(installation.id, change);
      await reloadBoot();
    } catch (error) {
      toast("error", errorText(error));
    }
  }

  async function remove() {
    const ok = await confirmDialog(
      `Stop managing ${installation.label}? Your addons and settings stay where they are; Hearthforge just forgets this folder.`,
      "Remove game",
      "Remove",
    );
    if (!ok) return;
    try {
      await api.removeInstallation(installation.id);
      await reloadBoot();
    } catch (error) {
      toast("error", errorText(error));
    }
  }

  return (
    <div className="install-card">
      <div className="install-card__body">
        <GameIcon icon={installation.icon} flavor={installation.flavor} size={44} />
        <div style={{ minWidth: 0, flex: 1 }}>
        <strong>{installation.label}</strong>
        {installation.label !== installation.flavorName && (
          <span className="small faint"> {installation.flavorName}</span>
        )}
        {installation.clientVersion && (
          <span className="small faint"> version {shortVersion(installation.clientVersion)}</span>
        )}
        <div className="found__path" title={installation.path}>
          {installation.available ? installation.path : `Missing: ${installation.path}`}
        </div>
        <div className="install-card__fields">
          <label className="field">
            Name
            <input
              className="input"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              onBlur={() => label.trim() && label !== installation.label && void patch({ label })}
            />
          </label>
          <label className="field">
            Start the game
            <select
              className="select"
              value={installation.launchMethod}
              onChange={(e) => void patch({ launchMethod: e.target.value as LaunchMethod })}
            >
              <option value="battlenet">Through Battle.net</option>
              <option value="direct">Directly (Battle.net must be signed in)</option>
            </select>
          </label>
          <label className="field">
            Release channel
            <select
              className="select"
              value={installation.defaultChannel ?? "default"}
              onChange={(e) =>
                void patch({ defaultChannel: e.target.value === "default" ? null : (e.target.value as Channel) })
              }
            >
              <option value="default">App default ({channelLabel[settings.defaultChannel]})</option>
              <option value="stable">Stable</option>
              <option value="beta">Beta</option>
              <option value="alpha">Alpha</option>
            </select>
          </label>
        </div>
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <button className="btn btn--small" onClick={() => void api.openFolder("addons", installation.id)}>
          Open AddOns folder
        </button>
        <button className="btn btn--small btn--ghost btn--danger" onClick={() => void remove()}>
          Remove
        </button>
      </div>
    </div>
  );
}

function SecretField({
  kind,
  label,
  hint,
  helpUrl,
  helpLabel,
}: {
  kind: SecretKind;
  label: string;
  hint: string;
  helpUrl: string;
  helpLabel: string;
}) {
  const saved = useStore((s) => s.boot!.secrets[kind]);
  const toast = useStore((s) => s.toast);
  const [value, setValue] = useState("");

  async function store(next: string | null) {
    try {
      const status = await api.setSecret(kind, next);
      useStore.setState((s) => (s.boot ? { boot: { ...s.boot, secrets: status } } : {}));
      setValue("");
      toast("success", next ? `${label} saved.` : `${label} removed.`);
    } catch (error) {
      toast("error", errorText(error));
    }
  }

  return (
    <div className="option" style={{ display: "block" }}>
      <div className="option__label">
        {label} <span className={`status ${saved ? "status--ok" : "status--idle"}`}>{saved ? "Saved" : "Not set"}</span>
      </div>
      <div className="option__hint">
        {hint}{" "}
        <button className="btn btn--ghost btn--small" onClick={() => void openUrl(helpUrl)}>
          {helpLabel}
        </button>
      </div>
      <div className="secret-row">
        <input
          className="input"
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={saved ? "Paste a new value to replace it" : "Paste it here"}
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
        <button className="btn" disabled={!value.trim()} onClick={() => void store(value)}>
          Save
        </button>
        {saved && (
          <button className="btn btn--ghost btn--danger" onClick={() => void store(null)}>
            Remove
          </button>
        )}
      </div>
    </div>
  );
}
