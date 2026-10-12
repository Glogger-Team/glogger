---
name: glogger-release
description: Operate the glogger-twinkleoftoes fork — integrate tested work into the main branch, build/install the glogger.Release daily driver (npm run tauri:build), cut a signed release (npm run release), keep %APPDATA%\glogger.Release data safe, and treat upstream (crisp-oddio) as an optional read-only reference.
---

# Glogger Fork — Daily Driver & Release Pipeline

## Overview
This repo (`cprivitere/glogger-twinkleoftoes`) is a standalone fork of `crisp-oddio/glogger-oddio`. It owns its config, release pipeline, update channel and branch line: `main` is both the source of truth and the release branch. Upstream is an optional read-only reference (`git fetch upstream && git merge upstream/<branch>`), never a PR target.

## Quick reference

| Task | Command |
|---|---|
| Integrate tested work | `git checkout main` → `git merge <branch-or-commit>` (or commit logical chunks) |
| Verify before build | `cd src-tauri && cargo test --lib`; `npm run build` if frontend touched |
| Build daily driver | `npm run tauri:build` (from repo root; unsigned locally unless a key is set — see *Auto-update*) |
| Installer output | Local Windows build: `src-tauri/target/release/bundle/nsis/*.exe` (macOS/Linux installers only come from CI — this box has no cross toolchain) |
| Publish a release | `npm run release <patch\|minor\|major\|x.y.z>` (bumps, writes `RELEASE_NOTES.md`, commits, tags `v<version>`, pushes; CI builds + signs + publishes Windows/macOS(aarch64)/Linux with those notes and refreshes the update channel) |
| Dry-run the release pipeline | `gh workflow run release.yml --repo cprivitere/glogger-twinkleoftoes -f ref=<tag\|main> -f publish=false` — builds all three platforms, creates no release |
| Install/update | Run the NSIS setup — appdata `%APPDATA%\glogger.Release` is preserved on reinstall. Once a signed build is installed, the app updates itself (banner in the header / Help → Changelog) |
| Signing key material | `~/.tauri/glogger.key` + `.password` (outside the repo) ↔ repo secrets `TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`; pubkey in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`, key ID `E39F2150244B82CA`) |
| Fresh data copy | Close glogger → copy `%APPDATA%\glogger.Release\{glogger.db,glogger.db-wal,glogger.db-shm,settings.json}` |
| Type check only | `npm run build` (vue-tsc) |

## Decision rules

- **Which profile is the user running?** Window title: `glogger v<ver>` = the installed daily driver (appdata `glogger.Release`, data persists forever). `EXPERIMENTAL` = sandbox that re-seeds from the Release dir on each version bump — never treat its data as valuable. `DEV` = `npm run tauri dev` scratch DB (`glogger.Dev`).
- **Where does new work live?** Committed work goes on `main` in logical chunks (commit prefixes: `feat:`/`fix:`/`impv:`/`docs:`/`test:`/`build:`).
- **Upstream is a reference remote:** absorb upstream fixes with `git fetch upstream && git merge upstream/<branch>`. Expect conflicts in the config/CI files this fork owns (`tauri.conf.json` updater block, `tauri.release.conf.json`, `scripts/*`, `.github/workflows/release.yml`, `AGENTS.md`) — resolve toward this fork's values. Never open a PR against `crisp-oddio/glogger-oddio`.
- **DB safety:** copy DBs only when the source app is closed; copy `glogger.db` + `-wal` + `-shm` together; verify with count-diff (`words_of_power`, `character_resuscitations`). Migrations are upward-only — never open a newer-schema DB with an older build.

## Build pipeline details

1. Pre-flight: confirm branch = `main`, tree clean (`git status --short`).
2. Verify: `cargo test --lib` (src-tauri dir) + `npm run build` if TS changed.
3. Build: `npm run tauri:build`. NSIS bundle lands in `src-tauri/target/release/bundle/nsis/`.
4. Install: run the setup exe (installs to `%LOCALAPPDATA%\glogger\`, does not touch the data dir).
5. Smoke: launch → title `glogger v<ver>` → confirm recent events (log positions advance).

### Platforms shipped (CI matrix in `.github/workflows/release.yml`)

| Runner | Target | Artifacts |
|---|---|---|
| `windows-latest` | `x86_64-pc-windows-msvc` | `glogger-<v>-windows-setup.exe` (+`.sig`) |
| `macos-latest` | `aarch64-apple-darwin` (`--target`) | `glogger-<v>-macos.dmg`, `glogger-<v>-macos.app.tar.gz` (+`.sig`) |
| `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `glogger-<v>-linux.deb`, `glogger-<v>-linux.AppImage` (+`.sig`) |
| `ubuntu-latest` + gnome-50 container (`.github/workflows/flatpak.yml`) | flatpak-builder | `glogger.flatpak`, attached by the same release run via `workflow_call` |

Each matrix job runs `cargo test --lib` on its own OS before building, so a release also proves the
crate compiles and tests on macOS/Linux. `latest.json` carries `windows-x86_64`, `darwin-aarch64` and
`linux-x86_64` (the Flatpak has no updater entry — it is a separate, unmanaged install).
macOS builds are unsigned (no Apple certificate) → users right-click → Open. Intel Macs are not covered
(aarch64 only, same as upstream). The `validate` job fails a tag release whose tag != `tauri.conf.json`
version; the Flatpak Workflow can be smoke-tested with
`gh workflow run flatpak.yml -f tag=<tag> -f publish=false`.

## Data recovery / resync

If the Release DB drifts stale: close ALL gloggers, copy the four files into `%APPDATA%\glogger.Release\` (overwrite), relaunch. Migrations run on the copy at launch (upward-only).

## Known hazards (this machine, Windows)

- Hooks are **opt-in per clone**: `scripts/setup-hooks.sh` sets `core.hooksPath=.githooks`. `pre-push` runs the TOS/CoC hard-line scanner (`scripts/check-glogger-constraints.ps1`) and blocks the push on a violation; the release workflow's `validate` job re-runs it, so an uninstalled hook cannot ship a violation.

- PowerShell 5.1 mangles `git commit -m` here-strings with double quotes → `git commit -F <file>`.
- `src-tauri/Cargo.toml` phantom CRLF diff in `git status` — leave uncommitted.
- npm only (Node 24). No bun/yarn/pnpm.
- Do not run two glogger instances against the same data dir simultaneously.
- `version:bump` does NOT wipe the daily-driver data; it DOES wipe Experimental data — that's by design.
- Updater signing (`bundle.createUpdaterArtifacts: true` in `tauri.conf.json`): a build **fails** without `TAURI_SIGNING_PRIVATE_KEY`, but `npm run tauri:build` runs `scripts/release-build.sh`, which injects a `createUpdaterArtifacts:false` override when the variable is unset — so local builds stay unsigned and working. Never publish an unsigned build: each matrix job's *Collect installer* step hard-fails when a platform's `.sig` is missing.
- **Auto-update correctness:** the `pubkey` in `src-tauri/tauri.conf.json` must be the public half of the repo's `TAURI_SIGNING_PRIVATE_KEY` secret, and the endpoint must stay the `updater-latest` channel tag. If either drifts, every update is silently rejected (`check()` succeeds, install fails verification) — see *Auto-update & signing* in `reference.md`.

## Deep reference

Branch map, seed-gate internals, and upstream-merge details: read `skill://glogger-release/reference.md` (same directory as this file).
