import { useCallback, useEffect, useState } from "react";
import { api, confirmDialog, errorText } from "../lib/api";
import { backupReasonLabel, bytes, dateTime, relativeTime } from "../lib/format";
import { useStore } from "../lib/store";
import type { BackupList, Installation } from "../lib/types";

export function BackupsView({ installation }: { installation: Installation }) {
  const settings = useStore((s) => s.boot!.settings);
  const setView = useStore((s) => s.setView);
  const toast = useStore((s) => s.toast);
  const [data, setData] = useState<BackupList | null>(null);
  const [working, setWorking] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setData(await api.listBackups(installation.id));
    } catch (error) {
      toast("error", errorText(error));
    }
  }, [installation.id, toast]);

  useEffect(() => {
    void load();
  }, [load]);

  async function backUpNow() {
    setWorking("new");
    try {
      const made = await api.createBackup(installation.id);
      toast(made ? "success" : "info", made ? `Backed up (${bytes(made.size)}).` : "There are no settings to back up yet.");
      await load();
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setWorking(null);
    }
  }

  async function restore(id: string, createdAt: string) {
    const ok = await confirmDialog(
      `Restore the settings backup from ${dateTime(createdAt)}? Your current settings are backed up first, so you can undo this.`,
      "Restore settings",
      "Restore",
    );
    if (!ok) return;
    setWorking(id);
    try {
      await api.restoreBackup(installation.id, id);
      toast("success", "Settings restored. They apply the next time you log in.");
      await load();
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setWorking(null);
    }
  }

  async function remove(id: string) {
    const ok = await confirmDialog("Delete this backup? This can't be undone.", "Delete backup", "Delete");
    if (!ok) return;
    setWorking(id);
    try {
      await api.deleteBackup(installation.id, id);
      await load();
    } catch (error) {
      toast("error", errorText(error));
    } finally {
      setWorking(null);
    }
  }

  return (
    <>
      <h2 className="view-title">Settings backups</h2>
      <p className="lede">
        Backups keep your keybindings, macros, chat setup and every addon's saved settings (the WTF folder), so a bad
        update or a reinstall never costs you your UI.
      </p>
      <div className="toolbar" style={{ marginTop: 16 }}>
        <button className="btn btn--primary" disabled={working !== null || data?.hasSettings === false} onClick={() => void backUpNow()}>
          {working === "new" ? "Backing up" : "Back up now"}
        </button>
        <button className="btn" onClick={() => void api.openFolder("backups", installation.id)}>
          Open backup folder
        </button>
        <span className="small muted">
          {settings.backupBeforeLaunch
            ? `A backup is made each time you press Play. The newest ${settings.backupRetention} automatic backups are kept; backups you make are never deleted automatically.`
            : "Backups before playing are turned off."}{" "}
          <button className="btn btn--ghost btn--small" onClick={() => setView("settings")}>
            Change
          </button>
        </span>
      </div>

      {data && !data.hasSettings && (
        <div className="empty">
          <h2 className="empty__title">Nothing to back up yet</h2>
          <p>{installation.label} creates its settings folder the first time you log in.</p>
        </div>
      )}

      {data && data.hasSettings && data.backups.length === 0 && (
        <div className="empty">
          <h2 className="empty__title">No backups yet</h2>
          <p>Make one now, or press Play and Hearthforge backs up your settings first.</p>
        </div>
      )}

      {data && data.backups.length > 0 && (
        <div className="list">
          {data.backups.map((backup) => (
            <div key={backup.id} className="backup">
              <div>
                <div className="row__name">{dateTime(backup.createdAt)}</div>
                <div className="row__summary">{relativeTime(backup.createdAt)}</div>
              </div>
              <div className="small muted">{backupReasonLabel[backup.reason]}</div>
              <div className="small muted tnum">{bytes(backup.size)}</div>
              <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}>
                <button className="btn btn--small" disabled={working !== null} onClick={() => void restore(backup.id, backup.createdAt)}>
                  {working === backup.id ? "Working" : "Restore"}
                </button>
                <button className="btn btn--small btn--ghost btn--danger" disabled={working !== null} onClick={() => void remove(backup.id)}>
                  Delete
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      {data && <p className="small faint" style={{ marginTop: 14 }}>Saved in {data.folder}</p>}
    </>
  );
}
