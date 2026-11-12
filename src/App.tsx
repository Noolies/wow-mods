import { useEffect, useState } from "react";
import { AddonDetailsPanel } from "./components/AddonDetailsPanel";
import { Toasts } from "./components/Bits";
import { AppBanner, GameHeader, Rail, Tabs } from "./components/Shell";
import { api, errorText, onFileDrop } from "./lib/api";
import { useSelected, useStore } from "./lib/store";
import { BackupsView } from "./views/BackupsView";
import { FindView } from "./views/FindView";
import { InstalledView } from "./views/InstalledView";
import { SettingsView } from "./views/SettingsView";
import { ShareView } from "./views/ShareView";
import { Welcome } from "./views/Welcome";

export function App() {
  const boot = useStore((s) => s.boot);
  const bootError = useStore((s) => s.bootError);
  const init = useStore((s) => s.init);
  const view = useStore((s) => s.view);
  const selected = useSelected();
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    void onFileDrop({
      over: setDragging,
      drop: async (paths) => {
        const { selectedId, toast, refreshAddons } = useStore.getState();
        if (!selectedId) return;
        const zips = paths.filter((p) => p.toLowerCase().endsWith(".zip"));
        if (zips.length === 0) {
          toast("error", "Drop a .zip file containing an addon.");
          return;
        }
        for (const path of zips) {
          try {
            const record = await api.installFromFile(selectedId, path);
            toast("success", `Installed ${record.name}.`);
          } catch (error) {
            toast("error", errorText(error));
          }
        }
        void refreshAddons(selectedId);
      },
    }).then((unlisten) => (stop = unlisten));
    return () => stop?.();
  }, []);

  if (bootError) {
    return (
      <div className="empty">
        <h1 className="empty__title">Hearthforge couldn't start</h1>
        <p>{bootError}</p>
      </div>
    );
  }
  if (!boot) return null;
  if (boot.installations.length === 0) {
    return (
      <>
        <Welcome />
        <Toasts />
      </>
    );
  }

  return (
    <div className="shell">
      <Rail />
      <main className="main">
        <AppBanner />
        {view === "settings" || !selected ? (
          <div className="scroll">
            <SettingsView />
          </div>
        ) : (
          <>
            <GameHeader installation={selected} />
            <Tabs installation={selected} />
            <div className="scroll">
              <div className="page">
                {view === "installed" && <InstalledView installation={selected} />}
                {view === "find" && <FindView installation={selected} />}
                {view === "share" && <ShareView installation={selected} />}
                {view === "backups" && <BackupsView installation={selected} />}
              </div>
            </div>
          </>
        )}
      </main>
      <AddonDetailsPanel />
      <Toasts />
      {dragging && <div className="drop-overlay">Drop to install into {selected?.label ?? "your game"}</div>}
    </div>
  );
}
