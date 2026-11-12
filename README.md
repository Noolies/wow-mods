# Hearthforge

A lightweight addon manager and launcher for **WoW Forever**, built as a friendlier
alternative to the CurseForge app: no ads, no account, no Overwolf, and an installer
under 5 MB. It also works with retail World of Warcraft and the Classic clients.

![Hearthforge app icon](assets/app-icon.svg)

## For players

### Install

1. Open the [latest release](https://github.com/Noolies/wow-mods/releases/latest).
2. Download `Hearthforge_<version>_x64-setup.exe` and run it. It installs for your
   Windows account only, so it doesn't need admin rights.
3. Windows may show "Windows protected your PC" because the app isn't code-signed yet.
   Choose **More info**, then **Run anyway**.

On first launch Hearthforge looks for your World of Warcraft folder and offers the
WoW Forever client (`World of Warcraft\_forever_`). If it can't find it, choose the
folder yourself.

### What it does

- **One-click installs** of addon builds made for WoW Forever. Hearthforge reads each
  release's metadata and only installs a build the Forever client will load
  (interface 16001), so you don't end up with retail or Classic files by mistake.
- **Recommended addons and a starter kit**: Questie, BigWigs, Deadly Boss Mods,
  Bartender4, Plater, ElvUI and more, all checked to have Forever builds.
- **Search** GitHub, Tukui and (optionally) CurseForge, or paste any GitHub link.
- **Addon details** before you install: the full description, screenshots, latest release notes,
  supported game versions, download size, licence and links.
- **Automatic updates**: checks when it starts and every few hours in the tray.
  Turn on automatic installs globally or per addon, pin a version, or follow beta
  builds.
- **Play button**: backs up your settings, installs waiting updates, then starts the
  game through Battle.net.
- **Settings backups**: zips your `WTF` folder (keybindings, macros, every addon's
  saved settings) before you play. Restore any backup in one click; the current
  settings are backed up first, so a restore can be undone.
- **Share codes**: copy a code for your addon setup, paste a friend's code, and
  install everything they use.
- **Adopts what you already have**: addons installed by hand or by another app are
  listed, and Hearthforge can start tracking them for updates.
- **Problem spotting**: flags addons built for another game version, missing
  dependencies and missing files.
- **Drag and drop** a zip onto the window to install it.
- **Your games' own icons**, the same emblems Battle.net shows, read from your installed game.
- **Tray icon, start with Windows, notifications**, and the app updates itself.

### Moving over from CurseForge

Point Hearthforge at the same game folder. Your addons show up straight away, and the
ones it recognises can be tracked for updates with **Track all**. Many popular WoW Forever
addons publish the same files on GitHub, often with newer versions than CurseForge lists.

### Addons only on CurseForge

GitHub and Tukui need no setup. Some addons are only published on CurseForge; to
install those, create a free API key at [console.curseforge.com](https://console.curseforge.com/)
and paste it in **Settings, Addon sources**. Keys are kept in Windows Credential
Manager, never in plain files.

### Questions

- **Does it change my existing addons?** No. It lists them as "Not tracked" until you
  choose to track or update them.
- **Where do downloads come from?** The addon authors' own releases on GitHub,
  Tukui or CurseForge. Nothing is re-hosted.
- **Where is my data?** Settings and backups live in
  `%APPDATA%\com.noolies.wow-mods`. Logs are in
  `%LOCALAPPDATA%\com.noolies.wow-mods\logs` (Settings, Help, Open log folder).

## For developers

Hearthforge is a [Tauri 2](https://tauri.app/) app: a Rust engine with a React +
TypeScript interface.

### Requirements

- Node.js 24+
- Rust (stable, via [rustup](https://rustup.rs/))
- Windows: Visual Studio Build Tools with the C++ workload (WebView2 ships with Windows 11)

### Commands

```bash
npm install
npm run app:dev        # run the desktop app with hot reload
npm run dev            # UI only, in a browser, backed by mock data (add ?welcome for first run)
npm test               # frontend unit tests
npm run test:rust      # engine unit tests
npm run app:build      # release build and Windows installer
```

Live tests against the real GitHub and Tukui sources (no keys needed):

```bash
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored live_
```

### Layout

| Path | What it holds |
| --- | --- |
| `src-tauri/src/games/wow/` | WoW versions, install detection, TOC parsing, folder scanning, launching |
| `src-tauri/src/providers/` | GitHub, CurseForge, Tukui and the curated list |
| `src-tauri/src/install.rs` | Safe zip installs: staging, rollback, zip-slip protection |
| `src-tauri/src/ops.rs` | Install, update, check, adopt, share and play operations |
| `src-tauri/src/backup.rs` | WTF backups, pruning and restores |
| `src-tauri/src/scheduler.rs` | Background update checks and notifications |
| `src/` | React interface (`views/`, `components/`, `lib/`) |
| `catalog/catalog.json` | The recommended-addons list shown in the app |

### Recommended addons list

`catalog/catalog.json` ships inside the app, and the app also downloads the latest copy
from this repository's `main` branch at start-up. To add an addon: check its latest
release has a WoW Forever build, add an entry, bump `version`, and push. Installed
apps pick it up the next time they start.

### Adding another game

Each game lives in its own module under `src-tauri/src/games/`, with detection,
mod scanning and launching. `Installation.game` records which adapter a game uses.

### Releasing

See [docs/RELEASING.md](docs/RELEASING.md).
