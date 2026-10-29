# Update Notifications

## Overview

Glogger has two update systems:

1. **App auto-updater** — checks GitHub releases for new app versions and allows one-click download + install via the Tauri updater plugin.
2. **CDN game data monitoring** — polls for Project: Gorgon CDN version changes and surfaces a blocking modal when stale data is detected.

## App Auto-Updater

### How It Works

- **Source:** GitHub Releases on [Glogger-Team/glogger](https://github.com/Glogger-Team/glogger) — the endpoint compiled into the app is the `updater-latest` channel tag (`releases/download/updater-latest/latest.json`).
- Uses the [Tauri updater plugin](https://v2.tauri.app/plugin/updater/) (`tauri-plugin-updater`), which checks that `latest.json` manifest and verifies the per-platform signatures before installing.
- The manifest includes download URLs and cryptographic signatures for each platform. Signatures are verified before installation.
- Failures (no network, rate-limited, GitHub down) are silently ignored.

### Polling Schedule

- **Initial check:** 5 seconds after app startup
- **Recurring:** Every 60 minutes while the app is running
- Managed by the frontend update store (`src/stores/updateStore.ts`)

### User Notification

Two notification surfaces:

1. **Toast (one-time):** When a new version is first detected, an info toast appears: *"Glogger vX.Y.Z is available!"*

2. **Bottom bar (persistent):** A small indicator in the status bar shows "Update to vX.Y.Z". Clicking it downloads and installs the update, then restarts the app. A progress percentage is shown during download.

The user can dismiss the indicator via the X button. It resurfaces after 5 hours.

### Signing

- Release builds are signed with a keypair. The public key is in `tauri.conf.json` under `plugins.updater.pubkey`. The private key is a GitHub Actions secret (`TAURI_SIGNING_PRIVATE_KEY`). `tauri-apps/tauri-action` produces the `.sig` files; the release workflow's single-writer channel job builds `latest.json` once from all three platforms' `.sig` files (version, per-platform URL + signature, `pub_date`) and its `notes` field is the CI-generated release notes.

## CDN Game Data Monitoring

### Why This Matters

If Project: Gorgon pushes a CDN data update while glogger is running, the app continues using stale data. This can cause incorrect item names, broken recipe lookups, and missing entity information.

### How It Works

- After game data loads, a background poll starts checking the CDN version endpoint (`http://client.projectgorgon.com/fileversion.txt`).
- **Poll interval:** Every 15 minutes (CDN updates are infrequent).
- If the remote version differs from the loaded version, `cdnUpdateAvailable` is set in the game data store.

### User Notification

A **blocking modal dialog** appears over the entire app:

- Shows the version change (e.g., "v621 -> v622")
- Warns that stale data may cause incorrect information
- Two buttons:
  - **Restart Now** — re-downloads CDN data and relaunches the app
  - **Remind Me Later** — dismisses the modal for 30 minutes, then re-shows

The modal is intentionally blocking (no backdrop-click-to-dismiss) to ensure users don't run with stale data indefinitely.

## Key Files

| Layer | File | Purpose |
|-------|------|---------|
| Config | `src-tauri/tauri.conf.json` | Updater plugin config (pubkey, endpoints, install mode) |
| Store | `src/stores/updateStore.ts` | App update polling, download/install lifecycle |
| Store | `src/stores/gameDataStore.ts` | CDN version polling, `cdnUpdateAvailable` state |
| Rust | `src-tauri/src/cdn_commands.rs` | `check_cdn_version` command |
| UI | `src/App.vue` | Bottom bar update indicator, CDN modal wiring |
| UI | `src/components/Shared/CdnUpdateModal.vue` | Blocking CDN update modal |
| CI | `.github/workflows/release.yml` | Build + sign + publish; generates `latest.json` (notes generated in CI since the last published release) and refreshes the `updater-latest` channel |
