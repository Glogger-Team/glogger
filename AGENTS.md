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
- **State split**: Rust `app.manage()` holds durable state (`DbPool`, `SettingsManager` JSON-file settings, `GameDataState` CDN data, `DataIngestCoordinator`). Pinia stores are session caches, hydrated via invoke and kept fresh by events — game-state durability flows through backend settings/DB, not `localStorage` (which holds only UI prefs: pinned NPCs, timers).
- **DB locking — the pool is the lock**: `DbPool` is a 1-write + N-read pool pair; all writers serialize at `get_write()` checkout (fair FIFO, 15s timeout). NEVER add an app-level `Mutex`/`RwLock` around writers — a second lock only adds deadlock shape (poll thread + command + watcher each holding one side). Multi-statement writes use `BEGIN IMMEDIATE`; never hold an app lock across a pool checkout (parse first, check out the writer around the transaction).

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
| Release | `npm run release vX.Y.Z`|On `main`, clean tree: creates + pushes annotated tag `v<semver>`, zero commits; CI syncs the tag version, generates notes since the last *published* release (`scripts/last-published-tag.sh`), builds + signs + publishes, refreshes the channel. The tag is the version record — committed version files are a dev fallback |
| Changelog | `npm run changelog` | Git-log notes since a base tag (default: nearest tag — pass the last published tag explicitly after a failed release); CI uses the same generator for the GitHub Release body + `latest.json` notes |

## Work isolation (herdr outer, omp inner)

- **One feature = one herdr worktree = one omp main session.** `herdr worktree create --branch feature/<name>` (existing branch → checkout, else created from `--base`/HEAD under `<worktrees.directory>/<repo>/<branch-slug>`), launch omp in that checkout, never in the `main` checkout. Close with `herdr worktree remove` (runs `git worktree remove`, never deletes the branch). Grouped worktrees are normal workspaces (own tabs/panes); closing the parent row closes the group but not the checkouts.
- **Inside the worktree, main plans then fans out file-disjoint implementation workers with `isolated: true`.** Shared checkout is the default — concurrent writers interleave — so say `isolated workspaces` explicitly and give each worker disjoint files; main integrates, resolves conflicts, verifies, commits. Overlapping patches still conflict at merge even when isolated.
- **Required omp settings (this box):** `task.isolation.enabled true`, `isolation.backend auto` (resolves to ProjFS here — `prjflt` running — else `rcopy`; `block-clone` is ReFS-only, dead on NTFS), `task.isolation.merge patch`, `task.isolation.apply true`. Needs a git repo; unavailable in plan mode; not a security sandbox (workers keep tools/network/creds, only checkout edits are separated). Isolated workers are non-revivable after teardown — keep assignments self-contained, follow up in main.
- **Review is `/skill:frozen-review`, report-only.** Parent measures the fact rows once (`npm run build`, `cargo test --lib` + count, `npm run check:grids`, `check-glogger-constraints.ps1`, freeze state) and pastes frozen hunks into each brief; reviewers run **non-isolated, read-only** (`scout`/`reviewer`), never re-run `git diff` or a fact row. Only the behaviour slice may live-smoke (CDP recipe). One fix batch next turn, then gates only, then a bare re-ask reviews the delta; marker `review-last` is local, never pushed.

## Code review loop

**`/skill:frozen-review` is this repo's review call** — a user-level skill (`~/.agents/skills/frozen-review/SKILL.md`, the agents-standard root) that owns the whole loop: the freeze invariant, the target grammar, facts-once, the three intake categories, the report contract and the closing rule. It takes its target from the argument when it names a revision, range, or `worktree` (`/skill:frozen-review b77e4bd` freezes that one commit, `v1.3.2` the range starting after that tag); anything else is focus text and the target is otherwise **figured out**: the delta since its own `review-last` marker, the working tree when the tree is dirty, and nothing at all when the marker already sits on HEAD — so a fix batch is reviewed by asking again with no argument. Skills are discovered at startup (`omp://skills.md`) — there is no file watcher for them either — so a freshly written or moved skill answers `Unknown skill` until a restart. That is why this section keeps glogger's *values*: the fact rows, the gate commands and the accepted-drift ledger below, which the skill reads from here (always in context, no second file to maintain). `.omp/commands/frozen.md` is gone — `/skill:frozen-review` resolves from the agents root, so this repo carries no review command any more and a bare `/frozen` is now just literal text (nothing shadows the skill).

`/review` cannot be re-pointed at those defaults: it is the harness's own command (a `session.customCommands` entry, dispatched before project files), so a project file named `review.md` would be shadowed and dead. `/review` itself does not review anything — it freezes a diff and dictates how the work is distributed. `agentCount` is derived from that snapshot — `< 100` changed lines or `<= 2` files → 1, `< 500` → `min(2, files)`, `< 2000` → `min(4, ceil(files/3))`, `< 5000` → `min(8, ceil(files/2))`, else `min(16, files)` — and `skipDiff` (raw diff over 50 000 chars, or over 20 files) replaces the inlined diff with `max(5, floor(100/files))` lines per file and tells reviewers to run `git diff`/`git show` for their own files instead. Modes, when a UI is available: base branch (PR-style, `mergeBase(base, HEAD)..HEAD`), uncommitted, one commit, a PR (`pr://owner/repo/N` as an argument skips the menu), custom instructions (offered only when no focus text was passed); with no UI it emits the headless form — exactly 1 reviewer over recent changes plus `Focus: <args>`. `/annotate code-review [focus]` runs the same diff pipeline to collect line notes first and hands them to `/review` as the operator focus.

- **Freeze the target, and never mix revisions.** The diff handed to a reviewer and the files it reads must be the same revision. `/skill:frozen-review` derives it — do not re-derive here: `marker..HEAD` with a clean tree (marker is `review-last`, else the newest `review-pass*`; with no marker yet the newest `v*` release tag is the fallback), or the working tree (`git diff HEAD`, reported as uncommitted) when the tree is dirty. What stays forbidden is reviewing a committed range while the checkout has moved past it: a reviewer reading unreviewed files is the failure the freeze exists to prevent — a peer repo's two high findings came from the layer that was being written while that pass ran.
- **Measure the facts once, in the parent, and paste them into every brief.** Rows: `npm run build`, `cd src-tauri && cargo test --lib` (the count), `npm run check:grids`, `pwsh scripts/check-glogger-constraints.ps1`, plus the freeze state (`git rev-parse --short <target>`, `git status --porcelain`, `git log --oneline BASE..TARGET | wc -l`). A red row is a fact to report once, not a per-file finding, and subagents never re-run a row. The cost is measured, not theoretical: re-deriving them was roughly a third of a peer repo's pass wall time and produced three "findings" that were facts — and Session 45's own delta pass re-derived a padding census the audit script already knew, and got a number wrong.
- **Intake is three categories.** (1) behaviour nobody intended, (2) a claim the code or its docs make that the code does not honour, (3) a gate whose documentation is stronger than its check. Anything else is out of scope for the round: say so in the request, and bulk-rule already-decided differences where they can be cited from the request — the accepted-drift ledger in `docs/architecture/ux-standards.md` is the worked example, including its "do not re-file them in a review" line.
- **Report to a fixed contract.** Per finding: severity (critical / high / medium / low / nit) · `file:line` · the defect · evidence (an exact quote or command output) · the fix · `verified` or `[INFERENCE]`. Then one verdict — **ship** / **ship-with-fixes** / **blocked** — an explicit fix-now vs backlog split (fix-now = a defect in code this range changed), and what could not be verified and why.
- **One fix batch, then gates only — and the pass closes when the range is clean.** Re-run the gates on the batch's tree and stop: pointing a reviewer back at its own fix re-files decided steps. A pass closes the review when it returns 0 critical, 0 high and 0 fix-now mediums in the changed code; everything else is backlog, i.e. ordinary work rather than another pass, and at most one further verification-only pass is allowed (verify the fixes, no new scope). If a batch does need eyes, review the *delta* range, never the series.
- **Scale the round to the delta, and tier the fan-out by risk class.** A high `agentCount` is the tool's heuristic over changed lines, not a risk assessment — 8 reviewers is what a 2 800-line series earns by arithmetic, and that is also the most a round here should run. Tier by what can go wrong, not by directory: changed behaviour (which may *run* the thing — the live smoke found what reading could not), wiring/docs, and the gates/CI themselves; one runner, the parent.

## Review stage order (cheapest gate first)

Four stages, always in this order — each is cheaper than the next, and skipping ahead burns the expensive reviewer on findings a free gate catches. CodeRabbit's own docs prescribe the same shape: verify-with-a-second-pass after fixes, loop limits with severity-scoped exit criteria, and CLI-vs-PR reviews as complementary (CLI = immediate feedback, PR = collaboration context) rather than redundant.

- **A. Local gates (free).** `mise run lint` (build, grids, TOS, review-patterns) + the pre-commit self-review checklist below. Repeat until clean — this is where ~70% of findings should die.
- **B. Frozen review (free, deep).** `/skill:frozen-review` on the pushed range. One fix batch, gates on the batch tree, then stop. At most one delta-only verification pass. B closes *every* fix batch, including fixes from C and D — never point a reviewer back at its own fix.
- **C. CodeRabbit local (paid in credits/time).** `coderabbit review --agent`, wait for `complete`, triage by severity. After fixing, go back to B (frozen delta pass), not straight to C — re-running the reviewer on its own fix re-files decided steps.
- **D. PR review (async, free).** Push to the PR; the bot reviews in ~5 min. Don't trigger a local C on the same push — the bot covers it. Fix bot threads via `skill://autofix` (unresolved + bot-authored + not outdated, per-fix approval, one consolidated commit), then back to B.

## Pre-commit self-review (move findings left)

Frozen-review + CodeRabbit rounds on the `todo-burndown` series (22 commits, ~half fix-loops) showed ~70% of findings were author-catchable in one pass — they were found because a reviewer ran, not because the fix was hard. Each loop costs a full review round (4–9 min wall time). Run this checklist against your own diff before every commit; a finding caught here never costs a round:

- **Swallowed errors.** `grep -n "let _ ="` on touched Rust files: every discard needs a one-line justification (best-effort delete/emit) or `?`. Inside `db/migrations.rs` bodies the gate (`scripts/check-review-patterns.sh`) forbids two shapes — `let _ =` on DB handles (`conn`/`tx`/`stmt`) and `filter_map(|x| x.ok())` row reads — because the single-transaction run turns those silent skips into a recorded version lie (v81 lesson). Sanctioned exceptions the gate deliberately allows: `EXISTS` guards with `.unwrap_or(false)` (never error on absent tables), serde defaults, and `continue` on malformed JSON (documented fill-only skips).
- **Cache reads.** Every `workOrderStateFor`-shaped reader (rows + `loaded` + server key) must check `loaded && server matches` before serving rows; every invalidation must either clear rows or bump the generation the reader guards on.
- **Bulk UPDATEs.** Any `UPDATE ... WHERE` without a scoping predicate on identity (map/use/character) mistags kept rows in a mixed set — scope it or prove the set can't be mixed.
- **Doc claims.** If the diff changes behaviour, `grep` the docs for the old claim (`unkeyed`, `5 keys`, `never`, `always`) and update or close the entry in the same commit.
- **New scripts need the exec bit.** Files created via tooling land mode 644 — CI invokes `./scripts/...` directly, so run `git update-index --chmod=+x` on any new script (and prefer `bash ./scripts/...` invocation in CI as backup). A lost bit fails the job with exit 126, as the review-patterns gate proved on its first run.
- **Fix the class, not the instance.** When fixing a defect of class X in function F, grep F for every other X before calling it done — the round-5 major (map-filtered drain, unfiltered `pending_uses` push two inches above it) was the same misattribution through a sibling path. `grep -n` the function for the shape, not just the reported line.
- **Re-read the function after applying a suggested fix.** Reviewer diffs land as hunks; the dropped-flush bug shipped because nobody re-read the enclosing function afterward. Read the whole function top to bottom, then `cargo check`.
- **Surgical edits.** One edit per hunk; `cargo check --lib` after each Rust edit before the next; never batch two edits to the same file in one turn. The round-2/3 churn (dropped flush, mangled docs, duplicated tails) came from big PUT ranges eating neighboring lines — tight ranges + immediate compile check prevent the whole class.
- **Test orderings, not just the reported ordering.** When the defect is a race between paths (buffered-gain × manual-use × log-delete), the test should enumerate the orderings — the isolation test proved the drain but never walked the queue path, and the reviewer did.

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
| `src-tauri/tauri.conf.json` | Local-build version source of truth + updater + window config (the pushed tag is the release version record; CI syncs workspaces in-build) |
| `vite.config.ts` | Two HTML inputs (`index.html`, `dev-panel.html`), port 1420 strict |
| `.github/workflows/release.yml` | Tag-only release pipeline: version from the tag → test gate → tauri-action direct-mode signed Windows/macOS(aarch64)/Linux builds → channel job attaches `latest.json` + refreshes `updater-latest` (`workflow_dispatch` with `publish=false` = dry run, no release); calls `flatpak.yml` to attach `glogger.flatpak` |
| `.github/workflows/flatpak.yml` | Flatpak bundle build (gnome-50 builder container + offline cargo/npm source lists) driven from `flatpak/io.github.cprivitere.glogger.yml` |
|`HANDOFF.md`|Session log with durable gotchas — read before nontrivial work|

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

Before committing, lint every file type you changed and fix what the linter reports — local feedback first, not CI. Tools live in `mise.toml` (`mise install` fetches them); run from the repo root:

- `mise run lint` — the blocking set (CI `lint.yml` gates runnable locally, plus `npm run build` + `check:grids` + `check-glogger-constraints.ps1`).
- `mise run lint:all` — `lint` plus advisory (rustfmt, clippy, markdown).

| File type | Linter | Covered by |
| --------- | ------ | ---------- |
| TS/Vue/JS (`.ts`, `.vue`, `.mjs`, `.cjs`) | vue-tsc (gated today); eslint (pending `npm i -D`) | `lint` → `npm run build` |
| Grids | grid standards census (gated) | `lint` → `npm run check:grids` |
| TOS/CoC | game-interaction limits gate (gated, pre-push hook) | `lint` → `scripts/check-glogger-constraints.ps1` |
| Review patterns | decided defect shapes from review rounds (gated) | `lint` → `bash scripts/check-review-patterns.sh` (migration discard check today; cache-guard + UPDATE-scope stay on the pre-commit checklist — need multi-line context grep can't do) |
| Python (`.py`) | ruff check + format --check (gated in CI) | `lint` → `ruff check scripts/`, `ruff format --check scripts/` |
| Shell (`.sh`, `.githooks/*`) | shellcheck (warning+; info notes don't fail) | `lint` → `python -c` wrapper that globs `scripts/*.sh` plus the two hooks into `shellcheck -S warning` (globbing in-process: portable, since mise array entries run under `cmd.exe`, which passes `*.sh` through literally) |
| YAML / Workflows (`.yml`) | yamllint + zizmor medium+ (config `.github/zizmor.yml`) + pinact (SHA pins + version comments) | `lint` → `yamllint .github/`, `zizmor --no-online-audits --config .github/zizmor.yml --min-severity medium .github/workflows/`, `pinact run --check` (CI runs yamllint + zizmor only — `pinact-action` is uninstallable on stock runners (aqua bootstrap `command is not found`, v3.0.1, 2026-10-25), so the pin check is local-only via mise; use `jactionlint` ad-hoc wherever actionlint would be called — never actionlint; its cache-poisoning rule can't ignore accepted drift) |
| Rust (`.rs`) | rustfmt + clippy, both advisory (823 unformatted spots, 167 findings pre-existing — report, don't gate) | `lint:all` → from `src-tauri/`: `cargo fmt -- --check`, `cargo clippy --all-targets` (fix what you touched, never gate on it) |
 | Rust deps (`Cargo.lock`) | cargo-audit, gated (critical/high only, CVSS ≥ 7.0; unscored advisories block unless OSV confirms unrated-for-all-locked-versions) | `lint` → `python scripts/cargo-audit-gate.py` (needs `cargo install cargo-audit --locked`; single source of truth shared with CI `lint.yml: cargo-audit` — inline CVSS vector first, OSV max-score fallback per locked version; partial OSV failure with no blocking score fails closed; `--ignore <RUSTSEC-id>` only with justification, documented in the script) |
| Markdown (`.md`) | markdownlint-cli2 (opt-in, not gated: 6k+ pre-existing) | `lint:all` → `markdownlint-cli2 --config .markdownlint-cli2.json` on hand-maintained docs (same four files CI checks); `--fix "**/*.md"` when ready |
| PowerShell (`.ps1`) | PSScriptAnalyzer (pending install, opt-in) | not in mise — `Install-Module PSScriptAnalyzer`, then `Invoke-ScriptAnalyzer -Path scripts -Recurse` |

Not installed, add only if wanted: stylelint (CSS), eslint-plugin-jsonc (JSON validity), `gasa` (no mise package). Formatters are a separate job from lint: `cargo fmt`, `ruff format` (prettier is not configured).

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
- **Avoid force-pushes to shared branches** — an accidental force-push once wiped `main` (restored via PR). `main` is the release branch; cut a release with `npm run release vX.Y.Z`, which creates + pushes the annotated tag (zero commits — CI syncs the tag version and generates notes since the last *published* release). A tag whose Release run failed never published, so the next notes must still cover its commits (v1.3.8 → v1.3.9 dropped them and had to be repaired by hand). Avoid force-pushes to `main`.
- **Branch-based development — never commit to `main` directly.** All work happens on `feature/<name>` branches cut from `main` (`git checkout -b feature/<name> main`); merge back via PR, or fast-forward `main` after a `/skill:frozen-review` pass closes clean. `main` stays releasable at all times: the release pipeline runs `npm run build` + `cargo test --lib` before bundling, so a broken branch merged to `main` blocks everyone's release — keep branches green before merging. Release tags point at `main` (cut after merge), never on a feature branch.
- `scripts/bump-version.py` rewrites the release window title with a version-agnostic regex.
- Commit prefixes: `feat:`, `fix:`, `impv:`, `docs:`, `test:`, `build:` (`.githooks/commit-msg` owns the list — `release:` and `Merge` commits are exempt).
- **Bare `gh` commands may hit the wrong repo**: `gh` follows the per-clone default, which a fresh clone may resolve elsewhere — so bare `gh release view` can read the wrong repo's releases. Check with `gh repo set-default --view`; pin with `gh repo set-default Glogger-Team/glogger` (per-clone, not committed). Scripts MUST NOT rely on the default: pass `-R/--repo` explicitly (CI uses `${{ github.repository }}`).
- **CI `startup_failure` with 0 jobs: stop and ask for the error text.** When a workflow run shows `startup_failure` with no jobs and no retrievable logs, the error text lives only in the Actions UI banner (the check-suite API confirms the conclusion but exposes `total_count: 0` check-runs; `gh run view --log` returns "log not found"; the `skill://gh` reference has no path to it). Do NOT bisect, toggle repo/org Actions settings, or push debug branches — that burns runs and risks real config for an unknown cause. Stop, report the run URL, and ask the user to paste the banner text from the run page.
- **Release caches stay on, poison-narrowed**: `setup-node` `cache: npm` + `Swatinem/rust-cache` are kept (linux build is ~30 min without them). Safety comes from narrowing writers, not dropping caches: `rust-cache` gets `save-if: ${{ github.event_name == 'push' }}` so dispatched smoke tests restore but never save (only tag pushes write what releases read); no `pull_request` trigger exists so no untrusted run can poison either cache. `npm ci` (frozen lockfile) is the npm integrity anchor. Signing secrets stay repo-level (`TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`): environment-scoping them would require re-entering values nobody has on hand, and buys nothing here — a job that names an environment still sees repo-level secrets, so the split adds a second copy to rotate, not a narrower audience.
