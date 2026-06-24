# ADR-0022: Tauri auto-updater for multi-machine testing

## Context

The app is being tested on 2+ machines, and manually copying the binary after
each change is impractical. Need a way for instances to update themselves when
a new version is pushed.

## Decision

Use Tauri's built-in updater plugin:

1. **Plugin setup**: Added `tauri-plugin-updater` to Cargo.toml and registered
   in `lib.rs`.

2. **Update endpoint**: Configured to check
   `https://github.com/jpurusho/filesync/releases/latest/download/latest.json`
   for new versions.

3. **UI integration**: Added "Check Updates" button in header that:
   - Checks for updates on app launch (silent)
   - Shows "Update Available" button when new version exists
   - Downloads and installs the update when clicked
   - Prompts user to restart after install

4. **GitHub Actions workflow**: `.github/workflows/release.yml` triggers on
   `v*` tags, builds the DMG, generates `latest.json` with version metadata,
   and publishes both to GitHub Releases.

5. **Signature**: Left `pubkey` empty for now (unsigned updates). Can add code
   signing later if needed for distribution beyond testing.

## Consequences

- Push a tag (`git tag v0.1.1 && git push origin v0.1.1`) → GitHub Actions
  builds and publishes → all instances can click "Update Available" to upgrade.
- No manual DMG copying between machines.
- Updates are unsigned (users see a security warning on macOS Gatekeeper).
  Acceptable for internal testing; would need proper code signing for
  public release.
