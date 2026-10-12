# glogger-twinkleoftoes — branch, build & release pipeline (fork-owned)

Skill for managing the `main` branch of `cprivitere/glogger-twinkleoftoes`:
merging work in, building the `glogger.Release` daily driver, cutting signed
releases, and keeping data intact. This fork owns its config and CI; upstream
(`crisp-oddio/glogger-oddio`) is an optional read-only reference.

## Branch map

| Branch | Purpose | Push? |
|---|---|---|
| `main` | Source of truth + release branch: all fork work and releases. | YES (to `origin`) |
| `dev` | Legacy shared line inherited from upstream; not the release branch. | as needed |
| `upstream/*` | `crisp-oddio` remotes — read-only reference. | never |

Absorb upstream fixes with `git fetch upstream && git merge upstream/<branch>`
and resolve conflicts toward this fork's values (config/CI files it owns).

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
npm run release <patch|minor|major|x.y.z>
```

Must be run on `main` with a clean tree. It runs `scripts/bump-version.sh`
(sets the version in `tauri.conf.json`, `package.json`, `Cargo.toml`, and the
release/experimental window titles), writes `RELEASE_NOTES.md` from the
commits since the previous release tag (`scripts/generate-changelog.sh`;
`--notes <file>` uses hand-written notes instead — the script prompts if the
generated notes list no changes), commits `release: v<version>` (bump + notes),
tags it, and pushes both the branch and the tag to `origin`. CI
(`.github/workflows/release.yml`) then builds the **signed** `glogger.Release`
bundles on three runners — Windows NSIS, macOS aarch64 (`dmg` + `app.tar.gz`),
Linux (`deb` + `AppImage`) — and publishes them to
`https://github.com/cprivitere/glogger-twinkleoftoes/releases/tag/v<version>`
as `glogger-<version>-{windows-setup.exe,macos.dmg,macos.app.tar.gz,linux.deb,linux.AppImage}`
(each updater artifact with a `.sig`), then refreshes the update channel (next
section). The release body is the download table + the tagged `RELEASE_NOTES.md`
+ the boilerplate, and the same notes go into `latest.json.notes`. A dry run of
the same matrix without publishing:

```bash
gh workflow run release.yml --repo cprivitere/glogger-twinkleoftoes \
  -f ref=v<version> -f publish=false
```

The pipeline refuses to publish when the tag != the committed
`tauri.conf.json` version, when any platform's `.sig` is missing, or (for a
dispatched publish) when the tag doesn't exist on `origin`. The `validate` job
also runs the game-interaction hard-line scanner
(`scripts/check-glogger-constraints.ps1`, `pwsh`), so a TOS/CoC violation fails
the release — the same scanner runs from `.githooks/pre-push`, which is
**opt-in per clone**: install it with `scripts/setup-hooks.sh` (verify:
`git config --get core.hooksPath` → `.githooks`; it blocks a push carrying a
probe violation, and `GLOGGER_SKIP_CONSTRAINTS=1` bypasses it loudly — never do
that to get a release out, CI re-checks anyway). A tag push also
fails `validate` when `RELEASE_NOTES.md` is missing or doesn't cover the range
since the previous tag (a warning only on `workflow_dispatch`, where historical
tags are smoke-tested). Commit prefixes drive the notes: `feat:`/`impv:`/`fix:`
appear, `docs:`/`build:`/`test:`/`chore:`/`release:` do not — so put anything a
user should read about in a `feat:`/`impv:`/`fix:` commit.

### When a tag's build fails

CI cannot rebuild a tag in place: a tag-triggered run executes the workflow file
*at that tag*, and the `validate` job only compares the tag name against
`tauri.conf.json` — so a failed build leaves the tag pointing at a tree that may
itself be broken. Two options, both needing a deliberate call:

- **Re-point the tag** at a fixed commit (what v1.1.0 needed):
  `git tag -d v<ver> && git tag v<ver> && git push origin :refs/tags/v<ver> && git push origin refs/tags/v<ver>`.
  Keep the version unchanged so the `validate` gate still passes; the old failed
  run stays in history against the stale commit.
- **Bump** (`npm run release patch`) and leave the broken tag behind.

Whichever you pick, dry-run the matrix first (command above), and prefer a
version whose `tauri.conf.json` matches the tag you are about to push.

### Platform coverage

| Platform | Shipped artifact | Notes |
|---|---|---|
| Windows x86_64 | `windows-setup.exe` (NSIS) | the daily driver; updater key `windows-x86_64` |
| macOS aarch64 | `macos.dmg` + `macos.app.tar.gz` | unsigned (no Apple cert) → right-click → Open; updater key `darwin-aarch64`; Intel Macs unsupported (upstream parity) |
| Linux x86_64 | `linux.deb` + `linux.AppImage` | AppImage is the self-updating one; updater key `linux-x86_64`; needs WebKitGTK 4.1 + `WEBKIT_DISABLE_DMABUF_RENDERER` (set automatically, see `lib.rs`) |
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
| Repo secrets | `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (set via `gh secret set … --repo cprivitere/glogger-twinkleoftoes --body "$(cat <file>)"`) |
| Public key (compiled into the app) | `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` — minisign key ID `E39F2150244B82CA` |
| Update endpoint (compiled into the app) | `https://github.com/cprivitere/glogger-twinkleoftoes/releases/download/updater-latest/latest.json` |
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
- Commit prefixes: `feat:` / `fix:` / `impv:` / `docs:` / `test:` / `build:`.

## Upstream reference path

`upstream` (crisp-oddio/glogger-oddio) is a read-only reference remote.
Absorb fixes with `git fetch upstream && git merge upstream/<branch>`; expect
conflicts in the config/CI files this fork owns. Never open a PR against
upstream. The local pre-push constraint scanner
(`scripts/check-glogger-constraints.ps1`) runs automatically on push.
