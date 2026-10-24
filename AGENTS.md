# Repository Guidelines

Guidance for AI assistants working on **glogger** — a Tauri v2 desktop app that ingests Project Gorgon game logs (`Player.log`, `Chat.log`) in real time, parses them into a local SQLite database, and renders dashboards (inventory, crafting, surveys, economy, chat, combat stats) in a Vue 3 UI.

## Architecture & Data Flow

```text
Project Gorgon logs (Player.log / Chat.log)
        │  filesystem watchers (500ms polling thread)
        ▼
DataIngestCoordinator (src-tauri/src/coordinator.rs)
        │  parsers (`parsers.rs` + `*_parser.rs`: player_event, chat_status, chat_combat, arena, shop_log, …)
        ▼
SQLite (WAL) via r2d2 pool          ──►  Tauri events (game-state-updated, ...)
        ▲                                        │
        │  invoke('snake_case_cmd')              ▼
Vue 3 + Pinia frontend (src/)  ◄────────  lib.rs generate_handler!
```

- **Single Rust lib crate** (`glogger_lib` in `src-tauri/`); `main.rs` is a 5-line shim calling `run()`. `lib.rs` is the wiring hub: numbered, commented startup sequence, `tauri::generate_handler![...]` with command groups per `// <area>` comment sections (count via `grep -rc '#\[tauri::command\]' src-tauri/src`).
- **FE→BE**: `invoke('snake_case_cmd', { camelCaseArgs })` from `@tauri-apps/api/core`. Commands mostly sync `fn -> Result<T, String>`, registered in `lib.rs`.
- **BE→FE**: `app_handle.emit(...)` → `listen(...)` in stores. Key contract: `game-state-updated` payload is a **list of domain names** (e.g. `["arena"]`) — handlers must refresh only the named domains.
- **State split**: Rust `app.manage()` holds durable state (`DbPool`, `SettingsManager` JSON-file settings, `GameDataState` CDN data, `DataIngestCoordinator`, `StallOpsLock`). Pinia stores are session caches, hydrated via invoke and kept fresh by events — game-state durability flows through backend settings/DB, not `localStorage` (which holds only UI prefs: pinned NPCs, timers).
- **Startup order matters**: settings → version check → `db::init_pool` (runs migrations) → coordinator → **paused** polling thread (frontend calls `start_background_polling` after historical catch-up) → background CDN init that later emits `game-data-ready` (don't assume game data at first invoke).

## Key Directories

| Path | Purpose |
|---|---|
| `src/components/<Area>/` | Area views: Dashboard, Character, Crafting, Inventory, Chat, Economics, StallTracker, Settings, Startup, Shared, … |
| `src/components/Dashboard/dashboardWidgets.ts` + `widgets/` | Widget registry: `{ id, name, component, configComponent?, defaultSize }` — array order = default card order for new users |
| `src/stores/` | Pinia stores (`gameStateStore` is the largest: live state + event handling) |
| `src/composables/` | `use*.ts` shared logic (navigation is provide/inject, no vue-router) |
| `src/types/` | TS mirrors of Rust payloads |
| `src/dev-panel/` | Second Vue app (own entrypoint) for live testing/debugging |
| `src-tauri/src/db/` | `<domain>_commands.rs` modules + `migrations.rs` |
| `src-tauri/src/coordinator.rs` | Log ingestion core (watchers → parsers → DB → events) |
| `src-tauri/src/survey/` | Survey tracker pipeline + slow replay accuracy tests |
| `src-tauri/src/game_data/`, `cdn_commands.rs` | CDN game-data load/persistence |
| `scripts/` | Version bump, changelog, log analysis utilities |
| `test_data/surveyLogs/` | Replay fixtures (Player.log/Chat.log + ground-truth `results.txt`) |
| `docs/features/`, `docs/plans/` | Feature specs and plans |

## Development Commands

| Action | Command | Notes |
|---|---|---|
| Dev server (CDP-ready) | `export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222; npm run tauri dev` | Default for any session that may need to see the app: flag MUST be exported in the same command (does not carry across calls). WebView2 listens on 9222; attach via `skill://glogger-smoke` recipe. Without the flag there is no CDP endpoint. |
| Dev server (headless) | `npm run tauri dev` | Vite HMR on :1420, auto-rebuilds Rust. Only when no visual verification is planned. |
| Frontend build + typecheck | `npm run build` | `vue-tsc --noEmit && vite build` |
| Rust check | `cd src-tauri && cargo check` | Faster feedback |
| Rust tests | `cd src-tauri && cargo test --lib` | Count reported per run; all fast except `#[ignore]`-gated suites |
|TOS/CoC hard lines|`pwsh scripts/check-glogger-constraints.ps1`|Game-interaction limits gate: local pre-push hook (install per clone: `scripts/setup-hooks.sh`) + the release workflow's `validate` job|
|Grid standards|`npm run check:grids`|Raw-`<table>`/census/exemption gate (`scripts/check-grid-standards.mjs` owns the machine list) — run it when touching a table or the census, and hand its output to a review round (see *Code review loop*)|
| Full Rust tests | `cd src-tauri && cargo test` | No `src-tauri/tests/` dir exists; everything is in-lib |
| Survey replay | `npm run survey-test` | `--ignored` accuracy report, minutes-scale, needs `test_data/` + `docs/CDN-full-examples/` |
| Version bump | `npm run version:bump patch\|minor\|major\|x.y.z`|See version section|
| Release | `npm run release patch\|minor\|major\|x.y.z`|On `main`, clean tree: bumps, writes `RELEASE_NOTES.md`, commits, tags `v<semver>`, pushes; CI publishes the release with those notes. Notes cover the commits since the last *published* release (`scripts/last-published-tag.sh`), so a failed release's tag never orphans its commits from the next notes |
| Changelog | `npm run changelog` | Git-log notes since a base tag (default: nearest tag — pass the last published tag explicitly after a failed release); `npm run release` writes them to `RELEASE_NOTES.md` (publishable to the GitHub Release + `latest.json`). `--notes <file>` overrides |

## Code review loop

**`/skill:frozen-review` is this repo's review call** — a user-level skill (`~/.agents/skills/frozen-review/SKILL.md`, the agents-standard root) that owns the whole loop: the freeze invariant, the target grammar, facts-once, the three intake categories, the report contract and the closing rule. It takes its target from the argument when it names a revision, range, or `worktree` (`/skill:frozen-review b77e4bd` freezes that one commit, `v1.3.2` the range starting after that tag); anything else is focus text and the target is otherwise **figured out**: the delta since its own `review-last` marker, the working tree when the tree is dirty, and nothing at all when the marker already sits on HEAD — so a fix batch is reviewed by asking again with no argument. Skills are discovered at startup (`omp://skills.md`) — there is no file watcher for them either — so a freshly written or moved skill answers `Unknown skill` until a restart. That is why this section keeps glogger's *values*: the fact rows, the gate commands and the accepted-drift ledger below, which the skill reads from here (always in context, no second file to maintain). `.omp/commands/frozen.md` is gone — `/skill:frozen-review` resolves from the agents root, so this repo carries no review command any more and a bare `/frozen` is now just literal text (nothing shadows the skill).

`/review` cannot be re-pointed at those defaults: it is the harness's own command (a `session.customCommands` entry, dispatched before project files), so a project file named `review.md` would be shadowed and dead. `/review` itself does not review anything — it freezes a diff and dictates how the work is distributed. `agentCount` is derived from that snapshot — `< 100` changed lines or `<= 2` files → 1, `< 500` → `min(2, files)`, `< 2000` → `min(4, ceil(files/3))`, `< 5000` → `min(8, ceil(files/2))`, else `min(16, files)` — and `skipDiff` (raw diff over 50 000 chars, or over 20 files) replaces the inlined diff with `max(5, floor(100/files))` lines per file and tells reviewers to run `git diff`/`git show` for their own files instead. Modes, when a UI is available: base branch (PR-style, `mergeBase(base, HEAD)..HEAD`), uncommitted, one commit, a PR (`pr://owner/repo/N` as an argument skips the menu), custom instructions (offered only when no focus text was passed); with no UI it emits the headless form — exactly 1 reviewer over recent changes plus `Focus: <args>`. `/annotate code-review [focus]` runs the same diff pipeline to collect line notes first and hands them to `/review` as the operator focus.

- **Freeze the target, and never mix revisions.** The diff handed to a reviewer and the files it reads must be the same revision. `/skill:frozen-review` derives it — do not re-derive here: `marker..HEAD` with a clean tree (marker is `review-last`, else the newest `review-pass*`; with no marker yet the newest `v*` release tag is the fallback), or the working tree (`git diff HEAD`, reported as uncommitted) when the tree is dirty. What stays forbidden is reviewing a committed range while the checkout has moved past it: a reviewer reading unreviewed files is the failure the freeze exists to prevent — a peer repo's two high findings came from the layer that was being written while that pass ran.
- **Measure the facts once, in the parent, and paste them into every brief.** Rows: `npm run build`, `cd src-tauri && cargo test --lib` (the count), `npm run check:grids`, `pwsh scripts/check-glogger-constraints.ps1`, plus the freeze state (`git rev-parse --short <target>`, `git status --porcelain`, `git log --oneline BASE..TARGET | wc -l`). A red row is a fact to report once, not a per-file finding, and subagents never re-run a row. The cost is measured, not theoretical: re-deriving them was roughly a third of a peer repo's pass wall time and produced three "findings" that were facts — and Session 45's own delta pass re-derived a padding census the audit script already knew, and got a number wrong.
- **Intake is three categories.** (1) behaviour nobody intended, (2) a claim the code or its docs make that the code does not honour, (3) a gate whose documentation is stronger than its check. Anything else is out of scope for the round: say so in the request, and bulk-rule already-decided differences where they can be cited from the request — the accepted-drift ledger in `docs/architecture/ux-standards.md` is the worked example, including its "do not re-file them in a review" line.
- **Report to a fixed contract.** Per finding: severity (critical / high / medium / low / nit) · `file:line` · the defect · evidence (an exact quote or command output) · the fix · `verified` or `[INFERENCE]`. Then one verdict — **ship** / **ship-with-fixes** / **blocked** — an explicit fix-now vs backlog split (fix-now = a defect in code this range changed), and what could not be verified and why.
- **One fix batch, then gates only — and the pass closes when the range is clean.** Re-run the gates on the batch's tree and stop: pointing a reviewer back at its own fix re-files decided steps. A pass closes the review when it returns 0 critical, 0 high and 0 fix-now mediums in the changed code; everything else is backlog, i.e. ordinary work rather than another pass, and at most one further verification-only pass is allowed (verify the fixes, no new scope). If a batch does need eyes, review the *delta* range, never the series.
- **Scale the round to the delta, and tier the fan-out by risk class.** A high `agentCount` is the tool's heuristic over changed lines, not a risk assessment — 8 reviewers is what a 2 800-line series earns by arithmetic, and that is also the most a round here should run. Tier by what can go wrong, not by directory: changed behaviour (which may *run* the thing — the live smoke found what reading could not), wiring/docs, and the gates/CI themselves; one runner, the parent.

## Code Conventions & Common Patterns

- **Commands**: snake_case, verb-first (`get_*`, `save_*`, `scan_*`, `backfill_*`). Adding one: define `#[tauri::command]` fn + **register in `lib.rs`'s `generate_handler!`** (single giant list, easy to forget).
- **Errors**: uniformly `Result<T, String>` with human-readable messages (`map_err(|e| format!(...))`); no custom error enums across the IPC boundary. Collapsed failures must stay distinguishable: use an explicit tri-state (value / absent / `Failed` with cause, `SessionServerLookup`-style) and log path + error at the collapse point; a lookup failure never falls through to the next tier.
- **Async**: sync commands by default; `async fn` only for CDN/IO-heavy work. Background work: `tauri::async_runtime::spawn`; ingestion runs on threads with batched emits and yield guards.
- **Migrations**: append-only in `src-tauri/src/db/migrations.rs` (head in `migrations.rs`). Add `migration_vN_*` + `if current_version < N` block + `record_migration`. **Never edit a shipped migration.**
- **State management**: `defineStore` + composables for shared logic; no vue-router (views toggled via provide/inject navigation; `App.vue` hand-rolls keep-alive with `v-if="visited.has(view)"` + `v-show`). Every Pinia store **must** have an `acceptHMRUpdate` block (see gotchas); add it when touching a store.
- **Dashboard widget**: create `src/components/Dashboard/widgets/<Name>Widget.vue` (+ optional `<Name>Config.vue`), then append to `DASHBOARD_WIDGETS` in `dashboardWidgets.ts`. Widgets are dumb consumers of Pinia stores, refreshed by `game-state-updated` domain payloads.
- **Tailwind v4**: CSS-first via `@tailwindcss/vite`; **no** `tailwind.config.js`/`postcss.config.js`. Classes applied directly in templates.
- **Component naming**: PascalCase with role suffixes (`*Widget.vue`, `*Tab.vue`, `*View.vue`, `*Panel.vue`, `*Overlay.vue`). Rust: `<domain>_commands.rs`, `<domain>_parser.rs`.
- **Rust↔TS contract**: Rust snake_case structs, hand-mirrored camelCase types in `src/types/`. When adding a setting: update both `toBackendSettings`/`fromBackendSettings` converters + defaults in `settingsStore.ts`.
- **Default directories** (never guess; all from `settings.rs::get_default_game_data_path` / `get_default_player_log_path`): game data root is `%APPDATA%\..\LocalLow\Elder Game\Project Gorgon\` on Windows (`~/Library/Application Support/unity.Elder Game.Project Gorgon/` on macOS). `Player.log` sits under the game root on Windows but under the separate `~/Library/Logs/Elder Game/Project Gorgon/` on macOS (game-rotated `Player-prev.log` is its sibling either way); `ChatLogs/`, `Reports/` (`Character_*.json`, `*_items_*.json`), `Books/` hang off the game root. glogger-side lives under Tauri's `app_data_dir` (per-build identifier: `glogger.Dev` / `glogger.Release` / `glogger.Experimental`): `glogger.db` (or custom `db_path`), `settings.json`, `data/` (CDN cache), `icons/`, `gst/`, `tesseract/`, `player-logs/` (rotated-Player.log archive; derived from the settings file's parent, not the game dir). Survey replay fixtures resolve via relative paths (`test_data/`, `docs/CDN-full-examples/`), not these defaults; live-export/smoke `#[ignore]` tests fall back to the real game dirs when fixtures are absent.

## Important Files

| File | Why it matters |
|---|---|
| `src-tauri/src/lib.rs` | Wiring hub: startup sequence, command registration, event emission |
| `src-tauri/src/db/migrations.rs` | Schema; additive-only convention |
| `src-tauri/src/coordinator.rs` | Log ingestion pipeline (3k+ lines) |
| `src-tauri/src/survey/replay_tests.rs` | Slow accuracy tests vs ground truth |
| `src/App.vue` | Phase-driven shell (splash → wizard → ready), view keep-alive |
| `src/components/Dashboard/dashboardWidgets.ts` | Widget registry |
| `src-tauri/tauri.conf.json` | Version source of truth + updater + window config |
| `vite.config.ts` | Two HTML inputs (`index.html`, `dev-panel.html`), port 1420 strict |
| `.github/workflows/release.yml` | Tag-triggered release pipeline: test gate → signed Windows/macOS(aarch64)/Linux builds → GitHub release → updater-latest manifest (`workflow_dispatch` with `publish=false` = dry run, no release); calls `flatpak.yml` to attach `glogger.flatpak` |
| `.github/workflows/flatpak.yml` | Flatpak bundle build (gnome-50 builder container + offline cargo/npm source lists) driven from `flatpak/io.github.cprivitere.glogger.yml` |
|`HANDOFF.md`|Session log with durable gotchas — read before nontrivial work|
|`RELEASE_NOTES.md`|Written by `npm run release` from the commits since the last *published* release (or `--notes <file>`); committed with the bump and published as the GitHub Release body + `latest.json.notes`. The `validate` job fails a tag push whose notes don't cover its range|

## Runtime/Tooling Preferences

- **Node 24, npm only** (package-lock + `npm ci` in CI; no Bun, no yarn/pnpm). No `engines` pin in package.json.
- **Rust**: stable toolchain, edition 2021, crate `glogger_lib`.
- Tauri plugins (import `@tauri-apps/plugin-*`): dialog, opener, process, updater, window-state. Capabilities live in `src-tauri/capabilities/` (windows `main` + `dev-panel`).
- **No linter configured**; the type-check gate is `vue-tsc --noEmit` via `npm run build`.
- Version lives in **5 files** (`tauri.conf.json` version = source of truth, plus its dev window title; `tauri.release.conf.json` + `tauri.experimental.conf.json` window titles; `package.json`; `Cargo.toml` package version) — always bump via `npm run version:bump`, never hand-edit.

## Testing & QA

- **Rust only**; conventional in-file `#[cfg(test)] mod tests` blocks at file bottom, no external runner, no integration dir. Count per `cargo test --lib` run; all fast except the slow/path-dependent suites, which are `#[ignore]`-gated (survey replay + chat-log backfill).
- DB tests use `rusqlite::Connection::open_in_memory()` + `crate::db::migrations::run_migrations` via a local `setup()` helper per module; parser tests embed real log-line fixtures as string constants.
- Frontend: **no tests** — `vue-tsc --noEmit` is the only gate; the dev panel is for manual/live testing.
- Live smoke = `skill://glogger-smoke` recipe (verbatim owner, read it):
  - Launch: `export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222; npm run tauri dev` (same command — the export does not carry across calls).
  - Attach at `http://127.0.0.1:9222` after verifying the port LISTENs (stale `msedgewebview2.exe` renderers block it — kill + restart the app).
  - Drive `navigateToView({ view, subTab })` off `Symbol(view-navigation)` — never click labels.
  - `tab.evaluate` for DOM (`tab.run` has none); prefer MutationObserver over round trips. In evaluated strings avoid arrow-IIFEs and regex literals (CDP rejects them); use `function(){}` + `indexOf`.
  - Assert render → sort → drill, then `tab.errors()` empty; in-window only (external `localhost:1420` has no Tauri IPC). Native file dialogs and Tauri IPC are invisible from CDP eval — user picks the path, or use the dev-panel bundle context for `invoke`/`listen`.
- Replay fixtures live **outside the crate** (`test_data/`, `docs/CDN-full-examples/`), reached via relative paths from `src-tauri` — breaks if run from another CWD.
- The release workflow gates releases with `npm run build` + `cargo test --lib` before bundling; ignored replay tests never run in CI.

## Linting

Before committing, lint every file type you changed and fix what the linter reports — local feedback first, not CI. Commands below run from the repo root; mise-managed tools go through `mise x` (the shim needs no global default that way).

| File type | Linter | Run |
| --------- | ------ | --- |
| TS/Vue/JS (`.ts`, `.vue`, `.mjs`, `.cjs`) | vue-tsc (gated today); eslint (pending `npm i -D`) | `npm run build` |
| Rust (`.rs`) | rustfmt + clippy, both advisory (823 unformatted spots, 167 findings pre-existing — report, don't gate) | from `src-tauri/`: `cargo fmt -- --check`, `cargo clippy --all-targets` |
| Rust deps (`Cargo.lock`) | cargo-audit, gated in CI (critical/high only, CVSS ≥ 7.0; unrated advisories noted non-blocking) | from `src-tauri/`: `cargo audit --json` (gate logic lives in `lint.yml: cargo-audit` — inline CVSS vector first, OSV fallback; `--ignore <RUSTSEC-id>` only with justification) |
| Rust (clippy, local) | clippy stays advisory until the 167 pre-existing findings are burned down — run it, fix what you touched, never gate on it | from `src-tauri/`: `cargo clippy --all-targets` |
| Shell (`.sh`, `.githooks/*`) | shellcheck (warning+; info notes don't fail) | `shellcheck -S warning scripts/*.sh .githooks/pre-push .githooks/commit-msg` |
| YAML / Workflows (`.yml`) | yamllint + zizmor medium+ (config `.github/zizmor.yml`) | `mise x yamllint -- yamllint .github/` then `mise x zizmor -- zizmor --no-online-audits --config <repo>/.github/zizmor.yml --min-severity medium <repo>/.github/workflows/` (absolute paths: `mise x` resets CWD for the inner command; jactionlint excluded, its cache-poisoning rule can't ignore accepted drift) |
| Markdown (`.md`) | markdownlint-cli2 (opt-in, not gated: 6k+ pre-existing) | `mise x npm:markdownlint-cli2 -- markdownlint-cli2 --fix "**/*.md"` when ready |
| PowerShell (`.ps1`) | PSScriptAnalyzer (pending install, opt-in) | `powershell -NoProfile -Command "Invoke-ScriptAnalyzer -Path scripts -Recurse"` |

Optional extras installed but not gated: `pinact`, `gasa` (action pinning). Not installed, add only if wanted: stylelint (CSS), eslint-plugin-jsonc (JSON validity). Formatters are a separate job from lint: `cargo fmt`, `ruff format` (prettier is not configured).

## Gotchas

- **`acceptHMRUpdate` on every Pinia store**: without it, `tauri dev` HMR leaves stale store singletons causing silent no-op bugs. Mirror the pattern at the bottom of `buildPlannerStore.ts`:

  ```ts
  if (import.meta.hot) {
    import.meta.hot.accept(acceptHMRUpdate(useBuildPlannerStore, import.meta.hot))
  }
  ```

  Only stores already migrated have it — add it when touching a store. (The old claim that stores use `persist: true` is wrong: no `pinia-plugin-persistedstate` exists; durability goes through the backend settings file or per-character DB rows.)
  Changing a composable export + its consumer in one HMR pass can unmount the widget — verify export changes with a reload, not just HMR. Guard recurring timers with `globalThis` so HMR re-execution does not double-tick.
- **Time math**: never parse display-formatted time strings (`formatTimeFull` honors 12/24h user setting; 12h strings break `tsToSeconds` → NaN). Parse machine formats; format only at render; guard comparisons with `Number.isFinite`.
- **TS casts in templates:** inline type literals in a template expression fail `vue-tsc` `TS1005` — shape rows in `computed` instead; `as number` / `as string` inside slots is fine (same for synthetic column keys hidden behind `as unknown as`).
- **Shell quoting:** git-bash heredoc needs `<< 'EOF'` when the body has `$`; PS5.1 here-strings with `'` inside C# break `Add-Type` — use file-based scripts or Python ctypes instead.
- **Dev-DB wipe is move-never-delete, and only on request**: `taskkill /F /IM glogger.exe`, move `glogger.db` (+ `-wal`/`-shm`) from `%APPDATA%\glogger.Dev\` to a timestamped `db-backup-*` dir beside it; keep `settings.json` + `data/` (CDN cache). Full ritual lives in `skill://glogger-smoke`.
- `game-state-updated` payloads are domain lists — refresh only named domains.
- **Avoid force-pushes to shared branches** — an accidental force-push once wiped `main` (restored via PR). `main` is the release branch; cut a release with `npm run release <patch|minor|major|x.y.z>`, which bumps, generates `RELEASE_NOTES.md` from the commits since the last *published* release, tags `v<semver>` and pushes. A tag whose Release run failed never published, so the next notes must still cover its commits (v1.3.8 → v1.3.9 dropped them and had to be repaired by hand). Avoid force-pushes to `main`.
- **Branch-based development — never commit to `main` directly.** All work happens on `feature/<name>` branches cut from `main` (`git checkout -b feature/<name> main`); merge back via PR, or fast-forward `main` after a `/skill:frozen-review` pass closes clean. `main` stays releasable at all times: the `npm run release` gate runs `npm run build` + `cargo test --lib` before bundling, so a broken branch merged to `main` blocks everyone's release — keep branches green before merging. Release tags and version bumps land on `main` only (via `npm run release`), never on a feature branch.
- `scripts/bump-version.sh` rewrites the release window title with a version-agnostic sed.
- Commit prefixes: `feat:`, `fix:`, `impv:`, `docs:`, `test:`, `build:` (`.githooks/commit-msg` owns the list — `release:` and `Merge` commits are exempt).
- **Bare `gh` commands may hit the wrong repo**: with `origin` (canonical) plus other remotes present, `gh` defaults to whichever remote carries `gh-resolved` — a fresh clone may resolve elsewhere, so bare `gh release view` can read the wrong repo's releases. Check with `gh repo set-default --view`; pin with `gh repo set-default Glogger-Team/glogger` (per-clone, not committed). Scripts MUST NOT rely on the default: pass `-R/--repo` explicitly (CI uses `${{ github.repository }}`; `release.sh` derives the slug from `remote.origin.url`).
- **Release caches stay on, poison-narrowed**: `setup-node` `cache: npm` + `Swatinem/rust-cache` are kept (linux build is ~30 min without them). Safety comes from narrowing writers, not dropping caches: `rust-cache` gets `save-if: ${{ github.event_name == 'push' }}` so dispatched smoke tests restore but never save (only tag pushes write what releases read); no `pull_request` trigger exists so no untrusted run can poison either cache. `npm ci` (frozen lockfile) is the npm integrity anchor. Signing secrets stay repo-level (`TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`): environment-scoping them would require re-entering values nobody has on hand, and buys nothing here — a job that names an environment still sees repo-level secrets, so the split adds a second copy to rotate, not a narrower audience.
