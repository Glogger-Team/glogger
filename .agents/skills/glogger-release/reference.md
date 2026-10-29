# glogger — branch, build & release pipeline (canonical)

Skill for managing the `main` branch of `Glogger-Team/glogger`:
merging work in, building the `glogger.Release` daily driver, cutting signed
releases, and keeping data intact. This repo owns its config and CI; upstream
(`crisp-oddio/glogger-oddio`) is an optional read-only reference.

## Branch map

| Branch | Purpose | Push? |
|---|---|---|
| `main` | Source of truth + release branch: all repo work and releases. | YES (to `origin`) |
| `dev` | Legacy shared line inherited from upstream; not the release branch. | as needed |
| `canonical/*` | `Glogger-Team/glogger` tracking — the canonical remote. | never (push to `origin`) |

Absorb upstream fixes with `git fetch upstream && git merge upstream/<branch>`
and resolve conflicts toward this repo's values (config/CI files it owns).

## Key identifiers & profiles

| Config file | Identifier | Appdata dir | Use |
|---|---|---|---|
| `src-tauri/tauri.release.conf.json` (overlay on base) | `glogger.Release` | `%APPDATA%\glogger.Release` | **Installed daily driver — protect its data** |
| `src-tauri/tauri.experimental.conf.json` (overlay) | `glogger.Experimental` | `%APPDATA%\glogger.Experimental` | Tester sandbox — re-seeds from the Release dir on each version bump; do NOT use for real data |
| `src-tauri/tauri.conf.json` (default) | `glogger.Dev` | `%APPDATA%\glogger.Dev` | `npm run tauri dev` scratch DB |

The release config is a thin overlay (identifier + window title) on the base
`tauri.conf.json`, which owns productName, updater pubkey/endpoint and
`bundle.createUpdaterArtifacts`. The base dev window title is
`glogger v<ver> DEV`.

## Commands

```powershell
# Build the Release installer (from main):
npm run tauri:build
# → src-tauri/target/release/bundle/nsis/*.exe

# Quick dev-run (debug build):
npm run tauri dev            # glogger.Dev DB — testing only
npm run tauri:dev:experimental  # Experimental profile — re-seeds on version bump

# Verify before/after merges:
cd src-tauri && cargo test --lib
# frontend type gate:
npm run build
```

## Routine: "I want fix X in my daily glogger"

1. **Source the fix.** If it lives on a side branch: `git merge fix/<name>`
   into `main`. If it's uncommitted work: commit it to `main` in logical
   chunks first (small commits, honest messages).
2. **Verify:** `cd src-tauri && cargo test --lib` and `npm run build`
   (vue-tsc) if frontend changed.
3. **Build:** `npm run tauri:build` (~3–10 min; first build ~20 min).
4. **Install:** run the produced NSIS setup from
   `src-tauri/target/release/bundle/nsis/`. Installing over the previous
   build preserves `%APPDATA%\glogger.Release` untouched.
5. **Smoke:** launch, check window title `glogger v<version>`, confirm live
   data (new events since the last session appear).

## Routine: publish a release

```bash
npm run release vX.Y.Z
```

Run it on `main` with a clean, up-to-date tree. It creates and pushes one
annotated tag — nothing is committed. CI (`.github/workflows/release.yml`)
does the rest: syncs the tag version into the build workspaces, generates the
notes from the commits since the last *published* release
(`scripts/generate-changelog.sh`, based via
`scripts/last-published-tag.sh`), builds the **signed** `glogger.Release`
bundles on three runners via tauri-action direct mode — creating the GitHub
Release under default tauri-action asset names (Windows NSIS, macOS aarch64,
Linux) — then the single-writer channel job waits for all three platforms'
binaries plus their `.sig` files, assembles `latest.json` from them, attaches
it to the release, and refreshes the update channel (next section). Until
that channel job succeeds the published Release is missing `latest.json`
(and the Flatpak, which the follow-up job builds from the tag's committed
files with its own sync). A failed platform build leaves a visibly partial
Release under the tag; the next attempt is a new version tag — the previous
tag is left behind, never retagged. The release body opens with the channel
line, then the generated notes, then a
Flatpak follow-up sentence, the attribution paragraph, and the macOS unsigned
footnote; the same notes go into `latest.json.notes`. The `.githooks/commit-msg`
prefix list drives the notes — put anything a user should read about in a
prefix the notes include. A dry run of the same matrix without publishing:

```bash
gh workflow run release.yml --repo Glogger-Team/glogger \
  -f ref=<branch> -f publish=false
```

The pipeline refuses to publish a tag that is not `v<semver>`, or any release
whose per-platform `.sig` is missing (dispatch publishes additionally require
the tag to exist on `origin`). The `validate` job also runs the
game-interaction hard-line scanner
(`scripts/check-glogger-constraints.ps1`, `pwsh` — TOS prose lives in
`docs/research/glogger-game-interaction-limits.md`), so a TOS/CoC violation fails
the release — the same scanner runs from `.githooks/pre-push`, which is
opt-in per clone (install: `scripts/setup-hooks.sh`; it blocks a violating
push, `GLOGGER_SKIP_CONSTRAINTS=1` bypasses it loudly, CI re-checks anyway).

### When a release fails

Cut releases from `main`, but never move or reuse a published tag, and never
push a tag fix directly:

- **Build failed before publish:** fix on a branch, merge to `main`, then cut
  the next version tag (`npm run release vX.Y.(Z+1)`). Leave the broken tag
  behind.
- **Wrong tag pushed (release not yet published):** cancel the running workflow
  first (it keeps publishing with `cancel-in-progress: false`), confirm no
  Release exists for the tag, then delete it locally and on origin
  (`git tag -d <tag> && git push origin :refs/tags/<tag>`) and retag
  correctly. Safe only while the release was never published.
- **Published tag must never move.** If a published release is bad, fix
  forward with a new version tag.

Whichever you pick, dry-run the matrix first (command above).

### Platform coverage

| Platform | Shipped artifact | Notes |
|---|---|---|
| Windows x86_64 | NSIS setup (default tauri-action name) | the daily driver; updater key `windows-x86_64` |
| macOS aarch64 | `.dmg` + updater `.app.tar.gz` (default tauri-action names) | unsigned (no Apple cert) → right-click → Open; updater key `darwin-aarch64`; Intel Macs unsupported (upstream parity) |
| Linux x86_64 | `.deb` + `.AppImage` (default tauri-action names) | AppImage is the self-updating one; updater key `linux-x86_64`; needs WebKitGTK 4.1 + `WEBKIT_DISABLE_DMABUF_RENDERER` (set automatically, see `lib.rs`) |
| Linux Flatpak | `glogger.flatpak` | built by `.github/workflows/flatpak.yml` (gnome-50 builder image) and attached by the same release run; no updater entry |
| Windows ARM64 / other arches | — | not built |

Local `npm run tauri:build` still produces only the Windows NSIS bundle
(`src-tauri/target/release/bundle/nsis/`); this machine has no macOS/Linux
cross toolchain, so the other installers exist only as CI artifacts.

## Auto-update & signing

The `glogger.Release` profile ships a working in-app updater
(`tauri-plugin-updater`, polled 5 s after startup + hourly; banner in the
header, full UI in Help → Changelog).

| Piece | Value |
|---|---|
| Private key (build-time signing) | `C:\Users\cprivitere\.tauri\glogger.key` (password in `glogger.key.password` next to it) — **outside the repo; this is the only copy** |
| Repo secrets | `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (set via `gh secret set … --repo Glogger-Team/glogger --body "$(cat <file>)"`) |
| Public key (compiled into the app) | `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` — minisign key ID `E39F2150244B82CA` |
| Update endpoint (compiled into the app) | `https://github.com/Glogger-Team/glogger/releases/download/updater-latest/latest.json` |
| Manifest producer | `.github/workflows/release.yml` → *Generate latest.json updater manifest* → uploaded to the `updater-latest` pointer release (`--clobber`) |
| Manually installed `.sig` verification | `tauri signer sign` produces the same minisign payload; CI verifies `latest.json` parses |

Why an `updater-latest` pointer release instead of `releases/latest/download/…`:
the endpoint URL then stays stable while the asset behind it is replaced on
every release — the app never has to change its compiled endpoint.

Invariants (break either one and updates fail silently — `check()` still
reports an update, the install fails at signature verification):

1. `tauri.conf.json`'s `plugins.updater.pubkey` == public half of the secret.
2. The endpoint stays the `updater-latest` tag.

Local builds are **unsigned** by design (`scripts/release-build.sh` adds a
`createUpdaterArtifacts:false` override when no key is in the environment) —
that is harmless, because the updater verifies the artifact it *downloads*, not
the running binary. For a signed local build:

```bash
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/glogger.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat ~/.tauri/glogger.key.password)" \
npm run tauri:build
```

Rotating the key: `npx tauri signer generate -w ~/.tauri/glogger.key -p '<pw>' -f`
(via `node node_modules/@tauri-apps/cli/tauri.js signer generate …` — bare `npx tauri` fails on this machine), update both secrets, then paste the new
`glogger.key.pub` content into `plugins.updater.pubkey` and publish a
release. **Installed builds cannot auto-update across a key rotation** — the old
pubkey is compiled in, so the new-key release must be installed manually once.

## Data safety invariants (NEVER break these)

- **Never** point one profile's install at another profile's DB, and never copy
  DBs while a glogger is running (SQLite WAL — copy only when that source app
  is closed).
- Copying DBs: copy `glogger.db` + `glogger.db-wal` + `glogger.db-shm`
  together, source app closed. Verify with a count-diff on a couple of
  tables (`words_of_power`, `character_resuscitations`).
- `version:bump` only changes title strings — no data impact for the Release
  profile.
- Migrations run **upward only**. A DB that has touched vN keeps vN schema.
  Never open a newer-schema DB with an older glogger build.

## Windows/PowerShell gotchas (this machine)

- PowerShell 5.1 mangles `git commit -m` here-strings with double quotes —
  use `git commit -F <file>`.
- `src-tauri/Cargo.toml` shows a phantom CRLF-only diff in `git status` —
  leave it uncommitted (permanent known artifact).
- npm only (Node 24); no bun/yarn/pnpm.
- Prefixes/hook install: see `.githooks/commit-msg` and `scripts/setup-hooks.sh` (cited, not restated).

## Upstream reference path
`upstream` (crisp-oddio/glogger-oddio) is a read-only reference remote.
Absorb fixes with `git fetch upstream && git merge upstream/<branch>`; expect
conflicts in the config/CI files this repo owns. Never open a PR against
upstream. Enforcement: `scripts/check-glogger-constraints.ps1` runs from `.githooks/pre-push` — the script owns the machine patterns.

## Credit lines (release bodies + README)

- Original app: **Daniel Auchenpaugh** (`danielout/glogger`).
- Upstream fork: credit the handle **oddio** (`crisp-oddio/glogger-oddio`) —
  that is their own public self-credit ("oddio of arisetsu"), and their GitHub
  profile carries no display name. The git author identity behind their commits
  does carry a personal name, which they publish nowhere; default to the handle unless they ask for the name.
- Credit the **work**, not just the pipeline: their commits are
  ancestors of our releases (count via `git log --author=crisp-oddio --oneline | wc -l`), so the release
  boilerplate reads "Features, fixes and the release pipeline inherited from
  the oddio fork by oddio". Keep the wording in `release.yml` (template) and
  `README.md` (footer) in sync when it changes.
