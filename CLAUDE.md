# wow-mods (Hearthforge)

Personal project. Hearthforge is a Tauri 2 desktop addon manager and launcher, built
first for WoW Forever (Blizzard's Classic+ client, patch 1.60, launched 4 Nov 2026) as
an alternative to the CurseForge app for a group of friends. The slug is `wow-mods`;
the product name is Hearthforge.

## Stack

- `src-tauri/`: Rust engine (Tauri 2, reqwest with rustls + ring, zip, keyring).
- `src/`: React 19 + TypeScript UI, zustand store, hand-written CSS tokens in `src/styles.css`.
- `src/lib/mock.ts` backs the UI when it runs in a plain browser (`npm run dev`).

## Commands

- `npm run app:dev`: desktop app with hot reload.
- `npm run dev`: UI only in a browser with mock data (`?welcome` shows first run).
- `npm run typecheck`, `npm test`, `npm run test:rust`.
- Live network tests: `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored live_`.

## WoW Forever facts the code relies on

- Client folder `_forever_` (the beta used `_classic_beta_`); AddOns at `_forever_/Interface/AddOns`.
  The game executable is `_forever_\Wow.exe`.
- Battle.net product code `wowf`, written in `_forever_/.flavor.info`. Detection trusts it over the folder name.
- The installed version comes from `<WoW folder>/.build.info` (1.60.1.70888 on 12 Nov 2026), giving the
  client's interface number. Like the game, addons with a lower interface count as out of date.
- Interface number 1600x (16001 at launch).
- TOC suffixes the client looks for: `_Forever` / `_Camelot`, then the `_Mainline` family, then plain.
- BigWigs-packager `release.json` flavour: `forever`.
- CurseForge game version type id: 88568.
- Battle.net launch: `Battle.net.exe --exec="launch WoWF"` (from a community Lutris script; not yet confirmed on a real launch).
- Detection finds installs through Battle.net's records (`C:\ProgramData\Battle.net\Agent\product.db`,
  `%APPDATA%\Battle.net\Battle.net.config`); Forever doesn't write a Windows uninstall entry.

These live in `src-tauri/src/games/wow/flavor.rs`. Re-check them if Blizzard changes the client.

## Rules for this repo

- Australian spelling in UI copy, docs and comments.
- Game icons are read at runtime from the installed client's executable (`games/wow/icon.rs`).
  Never commit or bundle Blizzard logos or artwork.
- Secrets: API keys and tokens only ever go to the OS credential store (`secrets.rs`).
  The updater signing key lives in `~/.tauri/` and GitHub Actions secrets, never in the repo.
- UI copy is plain, active and sentence case. Errors say what happened and how to fix it.
- Keep `catalog/catalog.json` limited to addons whose latest release is verified to
  have a Forever build. Bump its `version` when you change it.
- Run `npm run typecheck`, `npm test` and `npm run test:rust` before committing.
- Releases: see `docs/RELEASING.md`.
