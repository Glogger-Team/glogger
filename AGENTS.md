# Repository Guidelines

Guidance for AI assistants working on **glogger** — a Tauri v2 desktop app that ingests Project Gorgon game logs (`Player.log`, `Chat.log`) in real time, parses them into a local SQLite database, and renders dashboards (inventory, crafting, surveys, economy, chat, combat stats) in a Vue 3 UI.

## Architecture & Data Flow

```
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

- **Single Rust lib crate** (`glogger_lib` in `src-tauri/`); `main.rs` is a 5-line shim calling `run()`. `lib.rs` (~1k lines) is the wiring hub: numbered, commented startup sequence, `tauri::generate_handler![...]` with ~300 commands grouped by `// <area>` comment sections.
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
| `src/composables/` | ~30 `use*.ts` shared logic (navigation is provide/inject, no vue-router) |
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
| Dev server | `npm run tauri dev` | Vite HMR on :1420, auto-rebuilds Rust |
| Frontend build + typecheck | `npm run build` | `vue-tsc --noEmit && vite build` |
| Rust check | `cd src-tauri && cargo check` | Faster feedback |
| Rust tests | `cd src-tauri && cargo test --lib`|~715 tests, ~52 files|
|TOS/CoC hard lines|`pwsh scripts/check-glogger-constraints.ps1`|Game-interaction limits gate: local pre-push hook (install per clone: `scripts/setup-hooks.sh`) + the release workflow's `validate` job|
|Grid standards|`npm run check:grids`|Fails on a raw `<table>` outside the documented exemptions, a stale exemption, a `DataTable` census that disagrees with `docs/TODO.md`, or an exemption not named in `ux-standards.md`'s `- **Data tables:**` rule (`scripts/check-grid-standards.mjs`) — run it when touching a table or the census, and hand its output to a review round (see *Code review loop*)|
| Full Rust tests | `cd src-tauri && cargo test` | No `src-tauri/tests/` dir exists; everything is in-lib |
| Survey replay | `npm run survey-test` | `--ignored` accuracy report, minutes-scale, needs `test_data/` + `docs/CDN-full-examples/` |
| Version bump | `npm run version:bump patch\|minor\|major\|x.y.z`|See version section|
| Release | `npm run release patch\|minor\|major\|x.y.z`|On `main`, clean tree: bumps, writes `RELEASE_NOTES.md`, commits, tags `v<semver>`, pushes; CI publishes the release with those notes|
| Changelog | `npm run changelog` | Git-log notes since the last tag; `npm run release` writes them to `RELEASE_NOTES.md` (publishable to the GitHub Release + `latest.json`). `--notes <file>` overrides |

## Code review loop

**`/skill:frozen-review` is this repo's review call** — a user-level skill (`~/.agents/skills/frozen-review/SKILL.md`, the agents-standard root) that owns the whole loop: the freeze invariant, the target grammar, facts-once, the three intake categories, the report contract and the closing rule. It takes its target from the argument when it names a revision, range, or `worktree` (`/skill:frozen-review b77e4bd` freezes that one commit, `v1.3.2` the range starting after that tag); anything else is focus text and the target is otherwise **figured out**: the delta since its own last `review-pass*` marker tag, the working tree when the tree is dirty, and nothing at all when the marker already sits on HEAD — so a fix batch is reviewed by asking again with no argument. Skills are discovered at startup (`omp://skills.md`) — there is no file watcher for them either — so a freshly written or moved skill answers `Unknown skill` until a restart. That is why this section keeps glogger's *values*: the fact rows, the gate commands and the accepted-drift ledger below, which the skill reads from here (always in context, no second file to maintain). `.omp/commands/frozen.md` is gone — `/skill:frozen-review` resolves from the agents root, so this repo carries no review command any more and a bare `/frozen` is now just literal text (nothing shadows the skill).

`/review` cannot be re-pointed at those defaults: it is the harness's own command (a `session.customCommands` entry, dispatched before project files), so a project file named `review.md` would be shadowed and dead. `/review` itself does not review anything — it freezes a diff and dictates how the work is distributed. `agentCount` is derived from that snapshot — `< 100` changed lines or `<= 2` files → 1, `< 500` → `min(2, files)`, `< 2000` → `min(4, ceil(files/3))`, `< 5000` → `min(8, ceil(files/2))`, else `min(16, files)` — and `skipDiff` (raw diff over 50 000 chars, or over 20 files) replaces the inlined diff with `max(5, floor(100/files))` lines per file and tells reviewers to run `git diff`/`git show` for their own files instead. Modes, when a UI is available: base branch (PR-style, `mergeBase(base, HEAD)..HEAD`), uncommitted, one commit, a PR (`pr://owner/repo/N` as an argument skips the menu), custom instructions (offered only when no focus text was passed); with no UI it emits the headless form — exactly 1 reviewer over recent changes plus `Focus: <args>`. `/annotate code-review [focus]` runs the same diff pipeline to collect line notes first and hands them to `/review` as the operator focus.

- **Freeze the target, and never mix revisions.** The diff handed to a reviewer and the files it reads must be the same revision. `/skill:frozen-review` derives it — do not re-derive here: `marker..HEAD` with a clean tree (marker is the newest `review-pass*` tag; with no marker yet the newest `v*` release tag is the fallback), or the working tree (`git diff HEAD`, reported as uncommitted) when the tree is dirty. What stays forbidden is reviewing a committed range while the checkout has moved past it: a reviewer reading unreviewed files is the failure the freeze exists to prevent — a peer repo's two high findings came from the layer that was being written while that pass ran.
- **Measure the facts once, in the parent, and paste them into every brief.** Rows: `npm run build`, `cd src-tauri && cargo test --lib` (the count), `npm run check:grids`, `pwsh scripts/check-glogger-constraints.ps1`, plus the freeze state (`git rev-parse --short <target>`, `git status --porcelain`, `git log --oneline BASE..TARGET | wc -l`). A red row is a fact to report once, not a per-file finding, and subagents never re-run a row. The cost is measured, not theoretical: re-deriving them was roughly a third of a peer repo's pass wall time and produced three "findings" that were facts — and Session 45's own delta pass re-derived a padding census the audit script already knew, and got a number wrong.
- **Intake is three categories.** (1) behaviour nobody intended, (2) a claim the code or its docs make that the code does not honour, (3) a gate whose documentation is stronger than its check. Anything else is out of scope for the round: say so in the request, and bulk-rule already-decided differences where they can be cited from the request — the accepted-drift ledger in `docs/architecture/ux-standards.md` is the worked example, including its "do not re-file them in a review" line.
- **Report to a fixed contract.** Per finding: severity (critical / high / medium / low / nit) · `file:line` · the defect · evidence (an exact quote or command output) · the fix · `verified` or `[INFERENCE]`. Then one verdict — **ship** / **ship-with-fixes** / **blocked** — an explicit fix-now vs backlog split (fix-now = a defect in code this range changed), and what could not be verified and why.
- **One fix batch, then gates only — and the pass closes when the range is clean.** Re-run the gates on the batch's tree and stop: pointing a reviewer back at its own fix re-files decided steps. A pass closes the review when it returns 0 critical, 0 high and 0 fix-now mediums in the changed code; everything else is backlog, i.e. ordinary work rather than another pass, and at most one further verification-only pass is allowed (verify the fixes, no new scope). If a batch does need eyes, review the *delta* range, never the series.
- **Scale the round to the delta, and tier the fan-out by risk class.** A high `agentCount` is the tool's heuristic over changed lines, not a risk assessment — 8 reviewers is what a 2 800-line series earns by arithmetic, and that is also the most a round here should run. Tier by what can go wrong, not by directory: changed behaviour (which may *run* the thing — the live smoke found what reading could not), wiring/docs, and the gates/CI themselves; one runner, the parent.

## Code Conventions & Common Patterns

- **Commands**: snake_case, verb-first (`get_*`, `save_*`, `scan_*`, `backfill_*`). Adding one: define `#[tauri::command]` fn + **register in `lib.rs`'s `generate_handler!`** (single giant list, easy to forget).
- **Errors**: uniformly `Result<T, String>` with human-readable messages (`map_err(|e| format!(...))`); no custom error enums across the IPC boundary.
- **Async**: sync commands by default; `async fn` only for CDN/IO-heavy work. Background work: `tauri::async_runtime::spawn`; ingestion runs on threads with batched emits and yield guards.
- **Migrations**: append-only in `src-tauri/src/db/migrations.rs` (currently v73). Add `migration_vN_*` + `if current_version < N` block + `record_migration`. **Never edit a shipped migration.**
- **State management**: `defineStore` + composables for shared logic; no vue-router (views toggled via provide/inject navigation; `App.vue` hand-rolls keep-alive with `v-if="visited.has(view)"` + `v-show`). Every Pinia store **must** have an `acceptHMRUpdate` block (see gotchas) — only 7/27 stores do today; add it when touching a store.
- **Dashboard widget**: create `src/components/Dashboard/widgets/<Name>Widget.vue` (+ optional `<Name>Config.vue`), then append to `DASHBOARD_WIDGETS` in `dashboardWidgets.ts`. Widgets are dumb consumers of Pinia stores, refreshed by `game-state-updated` domain payloads.
- **Tailwind v4**: CSS-first via `@tailwindcss/vite`; **no** `tailwind.config.js`/`postcss.config.js`. Classes applied directly in templates.
- **Component naming**: PascalCase with role suffixes (`*Widget.vue`, `*Tab.vue`, `*View.vue`, `*Panel.vue`, `*Overlay.vue`). Rust: `<domain>_commands.rs`, `<domain>_parser.rs`.
- **Rust↔TS contract**: Rust snake_case structs, hand-mirrored camelCase types in `src/types/`. When adding a setting: update both `toBackendSettings`/`fromBackendSettings` converters + defaults in `settingsStore.ts`.

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
|`RELEASE_NOTES.md`|Written by `npm run release` from the commits since the previous tag (or `--notes <file>`); committed with the bump and published as the GitHub Release body + `latest.json.notes`. The `validate` job fails a tag push whose notes don't cover its range|

## Runtime/Tooling Preferences

- **Node 24, npm only** (package-lock + `npm ci` in CI; no Bun, no yarn/pnpm). No `engines` pin in package.json.
- **Rust**: stable toolchain, edition 2021, crate `glogger_lib`.
- Tauri plugins (import `@tauri-apps/plugin-*`): dialog, opener, process, updater, window-state. Capabilities live in `src-tauri/capabilities/` (windows `main` + `dev-panel`).
- **No linter configured**; the type-check gate is `vue-tsc --noEmit` via `npm run build`.
- Version lives in **5 files** (`tauri.conf.json` version = source of truth, plus its dev window title; `tauri.release.conf.json` + `tauri.experimental.conf.json` window titles; `package.json`; `Cargo.toml` package version) — always bump via `npm run version:bump`, never hand-edit.

## Testing & QA

- **Rust only**; conventional in-file `#[cfg(test)] mod tests` blocks at file bottom, no external runner, no integration dir. ~715 tests across ~52 files, all fast except the slow/path-dependent suites, which are `#[ignore]`-gated (5 survey replay + 1 chat-log backfill).
- DB tests use `rusqlite::Connection::open_in_memory()` + `crate::db::migrations::run_migrations` via a local `setup()` helper per module; parser tests embed real log-line fixtures as string constants.
- Frontend: **no tests** — `vue-tsc --noEmit` is the only gate; the dev panel is for manual/live testing.
- Replay fixtures live **outside the crate** (`test_data/`, `docs/CDN-full-examples/`), reached via relative paths from `src-tauri` — breaks if run from another CWD.
- The release workflow gates releases with `npm run build` + `cargo test --lib` before bundling; ignored replay tests never run in CI.

## Gotchas

- **`acceptHMRUpdate` on every Pinia store**: without it, `tauri dev` HMR leaves stale store singletons causing silent no-op bugs. Mirror the pattern at the bottom of `buildPlannerStore.ts`:
  ```ts
  if (import.meta.hot) {
    import.meta.hot.accept(acceptHMRUpdate(useBuildPlannerStore, import.meta.hot))
  }
  ```
  Only 7/27 stores have it today — add it when touching a store. (The old claim that stores use `persist: true` is wrong: no `pinia-plugin-persistedstate` exists; durability goes through the backend settings file or per-character DB rows.)
- **Time math**: never parse display-formatted time strings (`formatTimeFull` honors 12/24h user setting; 12h strings break `tsToSeconds` → NaN). Parse machine formats; format only at render; guard comparisons with `Number.isFinite`.
- **Player.log lines carry only `[HH:MM:SS]`, in UTC.** Dates come from `parsers::LogDateResolver`, shared by `GameStateManager` and the survey aggregator, in this precedence: an explicit base date (`set_base_date` — replay / old-log reparse), else the session date declared by the log's own login line (`parsers::parse_player_log_login_line` → `PlayerEvent::LogSessionAnchored` → `anchor_session`, rolled forward whenever the time of day goes backwards), else the clock with a one-day rollback when the line's time of day is still ahead of it. Never re-implement this: a login line read with the first line of a session is the only thing that survives arbitrarily long read lag, and the login line's bracket time equals its `Time UTC=` time. Resolving advances the session's midnight-rollover baseline, so it must happen exactly once per line, in log order: a side channel that reads a line before its batch is replayed (the coordinator's milking stamps) MUST use `GameStateManager::peek_utc`, not `to_utc` — resolving a later line first makes the in-order replay of an earlier one look like a midnight crossing and dates every following row a day ahead.
- **A `Player.log` login line *attributes* the session that follows it; `GameStateManager` is the single owner of the ingest identity.** `log_watchers::match_character_login` matches both the current format (`Logged in as character X. Time UTC=…`) and the older bracketed `[X]` form. While `!is_live()`, the coordinator's Player.log `CharacterLogin` arm calls `adopt_player_log_session` — the Player.log is a sequence of per-character sessions, so each login line says whose rows everything after it produces. Every game-state table keys on `(character_name, server_name)` and most are full-state dumps, so this is what keeps one session from replacing another's: before the Session-41 guard existed, a catch-up over a two-character log left **`TestPlayer / Combat = 99`** (the other alt's `ProcessLoadSkills` replacing the real 7 — the login matcher was dead, so no arm ran at all); the guard fixed that by *dropping* the other character's events, so its cost was the alt's own rows. The durable fix is *attribution*, not dropping: dropping was only ever needed while the identity had two owners (the guard compared `settings`, while persistence read the game state). `settings.active_character_name`/`active_server_name` are the durable/display selection, written by the picker, by the Chat.log marker (in *both* modes — that arm has no `is_live` gate — carrying the `character-login` emit and the frontend reload), and by `poll()` right after the Player.log drain of the catch-up poll: it syncs **both** fields to the ingest identity, and the chat drain that follows may then re-set them from its own marker — either way the frontend reads the result of the whole poll, which is what it loads — `startupStore`'s post-poll refresh MUST stay `settingsStore.initialize(true)`, because `initialize()` early-returns on `isLoaded` (set during the splash phase) and an unforced call leaves the UI on the pre-catch-up character while ingest writes the adopted one. The server is not in the Player.log — `session_server_for` resolves it from `user_characters` (written by the chat-login marker, which does read Chat.log), preferring the server already in use, then the picker's active row, then the newest (`updated_at`, `id` breaking second-precision ties); when that table has no row for the character, `apply_catch_up_login` falls back to the character's own `Chat.log` login line (`chat_session_servers`, every `Chat-*.log` scanned oldest-name-first, once per catch-up window, before the Player.log drain); only a character absent from both keeps the server in use, and so does a lookup that could not run (`SessionServerLookup::Failed` — a read error is not an unknown character, so it never reaches the chat tier). A session keyed to that last fallback stays under its `(character, server)` for good — a dump only replaces its own pair slice, so a later real login fixes the rows after it, not those (accepted: leaving the server unset would drop the session instead). Live login lines for another character are still ignored — live switches (and the reload) stay the marker's job — except that a live login line re-arms when `active_character` is `None` (writes settings *and* game state, no emit), and a catch-up that never saw a login line re-arms from the watcher's restored position. Non-log writers (`record_stall_prices`, `books_watcher`) read `settings` deliberately: they stamp user-side artifacts, not log-derived rows.
- **`src-tauri/Cargo.toml` phantom CRLF diff**: this file has shown a phantom `git status` modification in the past; if it shows as modified with no edits, leave it uncommitted.
- **PowerShell 5.1** mangles `git commit -m` here-strings with double quotes — use `git commit -F <file>`.
- **Two Vite entrypoints**: new root-level pages must be added to `vite.config.ts` inputs; dev panel is a separate app, not a route.
- `game-state-updated` payloads are domain lists — refresh only named domains.
- `scripts/bump-version.sh` rewrites the release window title with a version-agnostic sed.
- **Avoid force-pushes to shared branches** — an accidental force-push once wiped `main` (restored via PR). `main` is the release branch; cut a release with `npm run release <patch|minor|major|x.y.z>`, which bumps, generates `RELEASE_NOTES.md` from the commits since the previous tag, tags `v<semver>` and pushes. Avoid force-pushes to `main`.
- Commit prefixes: `feat:`, `fix:`, `impv:`, `docs:`, `test:`, `build:` (hooks warn — `release:` and `Merge` commits are exempt; install via `git config core.hooksPath .githooks` or `scripts/setup-hooks.sh`).
- Hooks are **opt-in per clone** (`core.hooksPath`) — a fresh clone has none until `scripts/setup-hooks.sh` runs, and `pre-push` is where the TOS/CoC hard-line scanner (`scripts/check-glogger-constraints.ps1`) blocks a violating push (`GLOGGER_SKIP_CONSTRAINTS=1` skips it, loudly). The release workflow re-runs the same scanner in `validate`, so an uninstalled hook cannot ship a violation.
- `db/price_helper_commands.rs` (8 commands) and `db/survey_commands.rs` (1 query) are unregistered/no-consumer modules (`#[allow(dead_code)]` on the `pub mod` lines in `db/mod.rs`), kept intentionally until their consumers land — not command placeholders.
