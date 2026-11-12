# Releasing Hearthforge

Releases are built by GitHub Actions when you push a version tag. Each release
publishes the Windows installer, a macOS build and the `latest.json` file that
installed copies of Hearthforge check to update themselves.

## One-time setup

1. **Create the GitHub repository** `Noolies/wow-mods` and push `main`.
   Friends download from GitHub Releases, and the in-app updater reads
   `releases/latest/download/latest.json`, so the repository (or at least its
   releases) must be **public**. The curated addon list is also fetched from the
   public `main` branch.
2. **Set up update signing.** Updates are signed so the app only installs builds you
   made. Run, from the repo root:

   ```powershell
   ./scripts/setup-updater.ps1
   ```

   It creates the signing key in `~/.tauri/wow-mods.key` (outside the repo), writes
   the public key into `src-tauri/tauri.conf.json`, and stores the private key and
   password as the `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` Actions secrets using `gh`.
   **Back up `~/.tauri/wow-mods.key` and its password.** If you lose them, existing
   installs can't verify new updates and everyone would have to reinstall manually.
3. Commit the changed `src-tauri/tauri.conf.json`.

Until step 2 is done, builds work but the app shows "Automatic app updates aren't
set up for this build".

## Each release

1. Bump `version` in `package.json` (the app reads its version from there) and, to
   keep things tidy, in `src-tauri/Cargo.toml`.
2. Commit, then tag and push:

   ```bash
   git tag v0.2.0
   git push origin main --tags
   ```

3. The **Release** workflow builds Windows and macOS, creates the GitHub release and
   uploads `latest.json`. Copies of Hearthforge with automatic updates on download the
   new version in the background and show "Restart and update".

## Code signing (optional, recommended later)

Unsigned Windows installers trigger SmartScreen ("Windows protected your PC") until
the download builds reputation. Signing removes that warning. The usual options are
Azure Trusted Signing (low monthly cost) or a standard code-signing certificate; both
plug into `bundle.windows` in `tauri.conf.json` and the release workflow.

The macOS build is unsigned too. Mac friends need to right-click the app and choose
**Open** the first time, or run `xattr -cr /Applications/Hearthforge.app`.
Notarising requires an Apple Developer account.

## Local builds

```bash
npm run app:build
```

The installer lands in `src-tauri/target/release/bundle/nsis/`. Local builds don't
create updater artefacts (`createUpdaterArtifacts` is only switched on by
`src-tauri/tauri.release.conf.json` in CI), so no signing key is needed.
