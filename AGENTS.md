# Repository Guidelines

Guidance for AI assistants working on **glogger** — a Tauri v2 desktop app that ingests Project Gorgon game logs (`Player.log`, `Chat.log`) in real time, parses them into a local SQLite database, and renders dashboards (inventory, crafting, surveys, economy, chat, combat stats) in a Vue 3 UI.

## Architecture & Data Flow

```
Project Gorgon logs (Player.log / Chat.log)
        │  filesystem watchers (500ms polling thread)
        ▼
DataIngestCoordinator (src-tauri/src/coordinator.rs)
        │  parsers/ (player_event, chat_status, chat_combat, arena, shop_log)
        ▼
SQLite (WAL) via r2d2 pool          ──►  Tauri events (game-state-updated, ...)
        ▲                                        │
        │  invoke('snake_case_cmd')              ▼
Vue 3 + Pinia frontend (src/)  ◄────────  lib.rs generate_handler!
```

- **Single Rust lib crate** (`glogger_lib` in `src-tauri/`); `main.rs` is a 5-line shim calling `run()`. `lib.rs` (~1k lines) is the wiring hub: numbered, commented startup sequence, `tauri::generate_handler![...]` with ~300 commands grouped by `// Domain` comment sections.
- **FE→BE**: `invoke('snake_case_cmd', { camelCaseArgs })` from `@tauri-apps/api/core`. Commands mostly sync `fn -> Result<T, String>`, registered in `lib.rs`.
- **BE→FE**: `app_handle.emit(...)` → `listen(...)` in stores. Key contract: `game-state-updated` payload is a **list of domain names** (e.g. `["arena"]`) — handlers must refresh only the named domains.
- **State split**: Rust `app.manage()` holds durable state (`DbPool`, `SettingsManager` JSON-file settings, `GameDataState` CDN data, `DataIngestCoordinator`, `StallOpsLock`). Pinia stores are session caches, hydrated via invoke and kept fresh by events — durability flows through backend settings/DB, NOT localStorage.
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
| Rust tests | `cd src-tauri && cargo test --lib`|702 tests, 57 files|
| TOS/CoC hard lines | `pwsh scripts/check-glogger-constraints.ps1` | Game-interaction limits gate: local pre-push hook (install per clone: `scripts/setup-hooks.sh`) + the release workflow's `validate` job |
| Full Rust tests | `cd src-tauri && cargo test` | No `src-tauri/tests/` dir exists; everything is in-lib |
| Survey replay | `npm run survey-test` | `--ignored` accuracy report, minutes-scale, needs `test_data/` + `docs/CDN-full-examples/` |
| Version bump | `npm run version:bump patch\|minor\|major\|x.y.z`|See version section|
| Release | `npm run release patch\|minor\|major\|x.y.z`|On `main`, clean tree: bumps, writes `RELEASE_NOTES.md`, commits, tags `v<semver>`, pushes; CI publishes the release with those notes|
| Changelog | `npm run changelog` | Git-log notes since the last tag; `npm run release` writes them to `RELEASE_NOTES.md` (publishable to the GitHub Release + `latest.json`). `--notes <file>` overrides |

## Code Conventions & Common Patterns

- **Commands**: snake_case, verb-first (`get_*`, `save_*`, `scan_*`, `backfill_*`). Adding one: define `#[tauri::command]` fn + **register in `lib.rs`'s `generate_handler!`** (single giant list, easy to forget).
- **Errors**: uniformly `Result<T, String>` with human-readable messages (`map_err(|e| format!(...))`); no custom error enums across the IPC boundary.
- **Async**: sync commands by default; `async fn` only for CDN/IO-heavy work. Background work: `tauri::async_runtime::spawn`; ingestion runs on threads with batched emits and yield guards.
- **Migrations**: append-only in `src-tauri/src/db/migrations.rs` (currently v70). Add `migration_vN_*` + `if current_version < N` block + `record_migration`. **Never edit a shipped migration.**
- **State management**: `defineStore` + composables for shared logic; no vue-router (views toggled via provide/inject navigation; `App.vue` hand-rolls keep-alive with `v-if="visited.has(view)"` + `v-show`). Every Pinia store **must** have an `acceptHMRUpdate` block (see gotchas) — most stores do; some already persist through settings instead.
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
- Version lives in **4 files** (`tauri.conf.json` = source of truth; `package.json`, `Cargo.toml` package version, plus the release/experimental window titles) — always bump via `npm run version:bump`, never hand-edit.

## Testing & QA

- **Rust only**; conventional in-file `#[cfg(test)] mod tests` blocks at file bottom, no external runner, no integration dir. ~702 tests across 57 files, all fast except the slow/path-dependent suites, which are `#[ignore]`-gated (5 survey replay + 1 chat-log backfill).
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
  Many stores are missing it today — add it when touching a store. (The old claim that stores use `persist: true` is wrong: no `pinia-plugin-persistedstate` exists; durability goes through the backend settings file or per-character DB rows.)
- **Time math**: never parse display-formatted time strings (`formatTimeFull` honors 12/24h user setting; 12h strings break `tsToSeconds` → NaN). Parse machine formats; format only at render; guard comparisons with `Number.isFinite`.
- **Player.log lines carry only `[HH:MM:SS]`, in UTC.** Dates come from `parsers::LogDateResolver`, shared by `GameStateManager` and the survey aggregator, in this precedence: an explicit base date (`set_base_date` — replay / old-log reparse), else the session date declared by the log's own login line (`parsers::parse_player_log_login_line` → `PlayerEvent::LogSessionAnchored` → `anchor_session`, rolled forward whenever the time of day goes backwards), else the clock with a one-day rollback when the line's time of day is still ahead of it. Never re-implement this: a login line read with the first line of a session is the only thing that survives arbitrarily long read lag, and the login line's bracket time equals its `Time UTC=` time. Resolving advances the session's midnight-rollover baseline, so it must happen exactly once per line, in log order: a side channel that reads a line before its batch is replayed (the coordinator's milking stamps) MUST use `GameStateManager::peek_utc`, not `to_utc` — resolving a later line first makes the in-order replay of an earlier one look like a midnight crossing and dates every following row a day ahead.
- **A `Player.log` login line is a character *guard* during catch-up, not a live identity change.** `log_watchers::match_character_login` matches both the current format (`Logged in as character X. Time UTC=…`) and the older bracketed `[X]` form, and the coordinator's Player.log `CharacterLogin` arm acts only while `!is_live()`: it ignores (and drops the events of) any character other than the selected one. Every game-state table keys on `character_name` and most are full-state dumps, so an unguarded catch-up over a log holding two characters silently replaces the selected character's skills/inventory with the other's. During live tailing, identity (and the frontend reload it triggers) is the Chat.log marker's job — `process_chat_events` — so the Player.log arm deliberately does not emit `character-login`. Two re-arm paths exist because that marker can be missed (its watcher tails from a saved position) and `process_events_batch` drops *every* event while `active_character` is `None`: at catch-up end, if the guard left it cleared, `poll()` re-arms from the Player.log watcher's last login line (the session actually being tailed); and a live login line re-arms when `active_character` is `None`. Both write `settings.active_character_name` *and* the game state, no emit. **Do not "fix" the guard by attributing each session to its own login line instead of dropping:** `settings.active_character_name` is read directly by other writers (`ingest_shop_log`, stall ops), so a game-state-only identity switch split-brains the DB (rows under the log's character, other rows under the selected one). The cost of dropping is bounded and accepted: when the log's *newest* session belongs to another character, that character's login snapshot dumps (`SkillsLoaded`, recipes, equipment) are dropped during catch-up and only return at their next login.
- **`src-tauri/Cargo.toml` phantom CRLF diff**: permanent git-status modification; leave uncommitted.
- **PowerShell 5.1** mangles `git commit -m` here-strings with double quotes — use `git commit -F <file>`.
- **Two Vite entrypoints**: new root-level pages must be added to `vite.config.ts` inputs; dev panel is a separate app, not a route.
- `game-state-updated` payloads are domain lists — refresh only named domains.
- `scripts/bump-version.sh` rewrites the release window title with a version-agnostic sed.
- **Avoid force-pushes to shared branches** — an accidental force-push once wiped `main` (restored via PR). `main` is the release branch; cut a release with `npm run release <patch|minor|major|x.y.z>`, which bumps, generates `RELEASE_NOTES.md` from the commits since the previous tag, tags `v<semver>` and pushes. Avoid force-pushes to `main`.
- Commit prefixes: `feat:`, `fix:`, `impv:`, `docs:`, `test:`, `build:` (hooks warn; install via `git config core.hooksPath .githooks` or `scripts/setup-hooks.sh`).
- Hooks are **opt-in per clone** (`core.hooksPath`) — a fresh clone has none until `scripts/setup-hooks.sh` runs, and `pre-push` is where the TOS/CoC hard-line scanner (`scripts/check-glogger-constraints.ps1`) blocks a violating push (`GLOGGER_SKIP_CONSTRAINTS=1` skips it, loudly). The release workflow re-runs the same scanner in `validate`, so an uninstalled hook cannot ship a violation.
- `db/price_helper_commands.rs` and `db/survey_commands.rs` are `#[allow(dead_code)]` placeholders kept intentionally.
