# Build Channels

glogger has three build channels, each with an isolated app data directory so they never interfere with each other.

## Dev

- **Identifier:** `glogger.Dev`
- **Data dir:** `%APPDATA%/glogger.Dev/`
- **Window title:** `glogger v{version} DEV`
- **How to run:** `npm run tauri dev`
- **Purpose:** Local development with hot reload. Uses the Vite dev server.

## Release

- **Identifier:** `glogger.Release`
- **Data dir:** `%APPDATA%/glogger.Release/`
- **Window title:** `glogger v{version}` (production) / `glogger beta v{version}` (pre-1.0)
- **How to build:** `npm run tauri:build`
- **CI workflow:** `.github/workflows/release.yml` (manual dispatch)
- **Published to:** `glogger` GitHub repo (Releases)
- **Auto-updater:** Yes — checks `glogger` releases for `latest.json`

## Experimental

- **Identifier:** `glogger.Experimental`
- **Data dir:** `%APPDATA%/glogger.Experimental/`
- **Window title:** `glogger v{version} EXPERIMENTAL`
- **How to build:** `npm run tauri:build:experimental`
- **CI workflow:** `.github/workflows/experimental.yml` (manual dispatch)
- **Published to:** `glogger` GitHub repo (pre-releases)
- **Auto-updater:** Disabled — testers download manually

### Data seeding behavior

On startup, experimental builds check whether the database needs to be seeded from the release install:

1. If no `glogger.db` exists in the experimental data dir, copy it from `glogger.Release/`.
2. If `glogger.db` exists but `last_app_version` in settings doesn't match the current build version, delete the experimental DB and re-copy from release.
3. If the release data dir doesn't exist, start with an empty database.

This means every new experimental version starts with a fresh copy of the tester's real release data. Settings (game paths, preferences) are also copied so the experimental build is immediately functional.

The release install is never modified.

### Version tagging

Experimental releases use a `-exp` tag suffix: `v0.7.12-exp`. This keeps them clearly separated from release tags (`v0.7.12`).

## Configuration files

| File | Purpose |
|------|---------|
| `src-tauri/tauri.conf.json` | Base config (dev defaults, shared settings) |
| `src-tauri/tauri.release.conf.json` | Release overlay (identifier, window title) |
| `src-tauri/tauri.experimental.conf.json` | Experimental overlay (identifier, window title, no updater) |

All three configs have their window titles updated by `scripts/bump-version.py`.
