# Changelog

Full release history for glogger, generated from git tags.
Per-release notes use the same grouping as scripts/generate-changelog.py
(feat: → Features, impv: → Improvements, fix: → Fixes) with humanized
entries (no type prefix, hash, PR ref, or commit count).
Releases that predate the commit-prefix convention list all commit subjects.

## v1.11.0 (2026-11-01)

### Features
- StallTracker: Inventory accuracy without LocalPlayer sections

## v1.10.0 (2026-11-01)

### Features
- Backlog batch-2 (dynamic-items N+1, portrait Esc/clamps, 4 review smalls)

### Fixes
- Version-sync PR uses SYNC_PAT, skips cleanly without it

## v1.9.1 (2026-10-30)

### Fixes
- Version-sync PR check counts open PRs, not string length
- Treat zero-completion recipes as known in recipe items widget

## v1.9.0 (2026-10-30)

### Features
- Rename merged XP widget display name to XP Rate
- Batch-1 UI cleanup (XP merge, stall-price strip, gift delete)

### Fixes
- Anchor AreaConfig weather match, doc truth follow-ups
- Audit Status widget, restore live weather, strip dead badges

## v1.8.0 (2026-10-30)

### Features
- NPC list goes CDN-first, drops boards and orphans

### Improvements
- Hide Inventory Live tab outright

### Fixes
- Stop Add click closing character picker dropdown
- Purge ??? placeholder orphan from wealth view
- Next-version reads squash-body bullets; PR titles carry prefixes
- Resolve all clippy warnings, gate -D warnings in lint
- Audit interactable exclusion, restore NPC_ orphan fallback
- Correct inverted orphan predicate (CodeRabbit C round)
- Re-point artipacked ignore after setup-python insert
- Apply CodeRabbit autofixes (CRLF bytes, setup-python, ASCII tags, push retry)

## v1.7.0 (2026-10-29)

### Features
- Tag-only releases via tauri-action direct mode

### Fixes
- Attach latest.json to draft before publishing (PR bot round 2)
- Portable bump-version.sh for BSD sed (PR bot D round)
- Draft-until-channel-publishes (PR bot D round)
- Flatpak version sync, doc recovery rules (coderabbit C round)
- Strict tag shape + channel sig-aware retry (review round 1)

## v1.6.0 (2026-10-28)

### Features
- Outage-safe todo burn-down (survey manual framing, storage report fields, cross-character work orders, blocked consolidation, export names, dark-widget hide)

### Fixes
- Self-audit round — log remaining discards, queue comment, mixed-buffer test
- Survey round-5 — pending-queue kind guard, stored-spelling retro, claim logging
- CodeRabbit round-4 — tooltip error ref, drain logging, doc staleness
- Portable review-patterns gate (no grep -P, loud search errors)
- Harden review-patterns gate to alias and multiline discards
- Make review-patterns gate executable; invoke via bash
- Correlate manual deferred drain by map; guard WO state on loaded+server
- Propagate v81 backfill errors; define report/snapshot mapping
- WO cache retry on same-server invalidation; close stale doc claims
- Wrap manual survey use insert + count in one transaction
- Await retry holds loading flag; retry re-reads without backfill
- WO invalidation on server import + retry same-server load after mid-flight invalidation
- Restore recordError display dropped by retry-button edit
- Reload cross-character items on server switch while All-Characters on
- Add Retry button to survey catalog load error
- Review fix-now — brace init bump, align WO catch comment
- Altitude review — ordinal parity, items cache scope, survey UX, header
- PR #54 round 3 — ordinal backfill matching, poll invalidation, failure refresh
- Review delta fix-now — WO server filter + stale clear, true conflicting-values test
- PR #54 round 2 — fill-only COALESCE order, WO null-server guard, recordError surface
- PR #54 review — v81 raw_json backfill, WO cache server-key, recordUse error surface, doc minors + regression guards

## v1.5.2 (2026-10-27)

### Features
- Prodigy session count on Skills + base-level advancement ticks
- Recover incoming milkings in ability-history backfill
- Gate PRs on frontend build, Rust tests, TOS hard lines

### Improvements
- Release coordinator lock around poem backfill + poison-safe commands
- Pool-is-the-lock — drop dead op locks, idle stub, skill-drop counter
- Backlog triage PR3 — frontend refresh ordering
- Backlog triage PR2 — survey single loot authority + bounds
- Backlog triage PR1 — ingest guards + fewer locks

### Fixes
- Exempt chat-sourced rows from hint boundary tick
- PR review loop — roulette local key, self-milk UTC, anchor v80, scan observability
- Verification follow-ups — internal-name stack match, Ilmari arm, live search tab id, keyboard resolver
- Review fix batch — scan writer scoping, currency live-gate, stack/arena follow-ups, dark-tab reachability
- Route scan-path Status dispatch through shared module, honor file tz, dark-state dead trackers
- Thread item identity into survey single-authority guards

## v1.5.1 (2026-10-26)

### Features
- Ingest chat skill level-ups and auto-import VIP reports

### Fixes
- Parse chat timestamps in timestampToSeconds, accrue level-up XP to combat rate
- Fetch version over HTTPS; drop no-op Mod replace
- Monotonic last_confirmed_at in apply_chat_level_up, log coordinator DB errors
- Restore chat MilkedBy arm, correct tnl-unknown and timestamp-format docs
- Review findings on chat level-up ingest — recency guard, session fold-in, Reports-dir confinement

## v1.5.0 (2026-10-25)

### Features
- Backlog ingest accuracy — symmetric rez dedup, directional treasure, inventory metrics/buffer/login-clear
- Track 'X has milked you' status messages as milked_by events
- Vendor council overhaul — 14 buckets, newest-wins aggregate, favorites
- Full CHANGELOG.md history across all 143 releases

### Improvements
- Milked_by backlog — shared normalizer, fixture tests, doc, frontend union

### Fixes
- Install Linux system deps in release test job; docs: stop-and-ask rule for CI startup_failure
- Move flatpak concurrency from top level into jobs (inputs unavailable at top level)
- Use local-path syntax for flatpak reusable workflow call
- Drop pinact-action from CI (aqua bootstrap broken) — pin check stays local-only
- Pinact validate-only mode; drop audit-script shebang (ruff EXE001)
- Frozen-review PR30 findings batch — login-batch latch, idle delete backstop, audit gate fail-closed
- Frozen-review findings — restore poll processing, newest-wins correctness, audit gate hardening
- Gate login clear on identity stability, retention-aware poll drain
- Frozen-review round 2 findings (critical + 13 highs + App.vue poll + stole anchor)
- Hand-fix markdown findings (blanks, fences, URLs), LF endings intact
- Use zizmor-action inputs (not args), exclude config globs from markdown run
- Split Ruff check/format steps, wire markdownlint config + scoped globs
- Satisfy ruff check + format on helper scripts
- Flatpak uses ref, narrow apt-cache writers
- Review fixes — script +x bits, backfill empty-set guard, tagless changelog

## v1.4.0 (2026-10-23)

### Fixes
- Base release notes on last published release, repair v1.3.9 notes

## v1.3.9 (2026-10-22)

### Fixes
- Unique temp DB/dir names for parallel cargo tests
- Bind Ctrl+K quick-search alias, correct repository URL

## v1.3.8 (2026-10-22)

### Features
- Wire snapshot abilities, VIP NPC fields, achievements catalog to UI

### Improvements
- Ingest VIP report fields + achievements from 10/21 update

### Fixes
- Split chat import status into result/error refs, label Player.log error box
- Restore chat import result feedback, fix stuck importing flag
- Folder chat import nested-tx crash + progress indicator, CDP/wipe guidance
- Filter placeholder character reports (??? name / error race)
- Clear snapshotAbilities on character switch
- Guard snapshot timer fallback on live-row existence (review fix)
- Clear areas in CDN persist (same UNIQUE re-persist class)
- Clear item_uses in CDN persist (UNIQUE failure on v488 re-persist)
- Vendor councils estimates + CDN tsys clear + drop Vite devtools
- Review fixes — picker per-skill fallback, Known skills label, docstring

## v1.3.7 (2026-10-21)

### Improvements
- Post-major-bump cleanup + opt-ins (TS 6, Vite 8, rusqlite 0.40, zip 9)

### Fixes
- Revert Map.getOrInsert in InventoryView grouping for older WebView2

## v1.3.6 (2026-10-20)

### Fixes
- Strict session-boundary gates for tagged chat lines
- Key catch-up staged gains by producing chat session
- Restore live cow-milk path; gate clear-on-adopt to login-line adoption
- Session-keyed catch-up stack correction + clear-on-adopt inventory dump

## v1.3.5 (2026-10-18)

### Fixes
- Seed live inventory from ItemAdded initial_quantity so multi-stack pickups count at once

## v1.3.4 (2026-10-17)

### Fixes
- Rename Statehelm favor filter to hide at/above tier

## v1.3.3 (2026-10-17)

### Features
- Let /frozen take its target from the argument
- Add /frozen — this repo's one-word review call

### Improvements
- Retire the repo review command in favour of the shared skill

### Fixes
- Give keyboard-focusable DataTable rows a visible focus indicator
- Correct the review-loop and gate claims; scope the exemption check to the rule

## v1.3.2 (2026-10-17)

### Improvements
- Migrate every data grid to the shared DataTable

### Fixes
- Correct the ledger's padding rows and scope the gate's exemption check
- Restore the sub-row indents the DataTable sweep flattened
- Gate DataTable row keydown to the row itself; stable MaterialTable keys
- Repair DataTable fallthrough + row keyboard access; third review pass
- Restore row keys, dense empty states and pane scrollers after the DataTable sweep
- Repair the sticky-header wrapper and the compact/size conflation in DataTable

## v1.3.1 (2026-10-17)

### Fixes
- Address review findings on the Statehelm gift rules
- Count only real gifts in the Statehelm tracker

## v1.3.0 (2026-10-15)

### Features
- Backfill Statehelm gift status from the game's own logs

### Fixes
- Attribute each Player.log session to its own character during catch-up
- Review pass over the Statehelm gift backfill

## v1.2.1 (2026-10-13)

### Improvements
- Drop dead chat-retention settings

### Fixes
- Show learned-but-uncooked recipes in Cook's Helper

## v1.2.0 (2026-10-12)

### Features
- chat: Open search results in the day's real log
- Track cow-curse abilities and make milking timestamps honest

### Improvements
- chat: Pin row-availability status to the list edges

### Fixes
- chat: Keep "Jump to Present" honest while newer rows exist above
- chat: Close the review findings on the day log
- chat: Bucket chat days in the viewer's display timezone

## v1.1.1 (2026-10-11)

### Improvements
- market: Label stall price capture as experimental

## v1.1.0 (2026-10-11)

### Fixes
- gourmand: Show every Gourmand-XP food and bucket unknown categories

## v1.0.3 (2026-10-06)

### Improvements
- Remove leftover resuscitation debug logging
- crafting: Cook's Helper auto-imports the gourmand report and persists eaten state

### Fixes
- chat: Repair search-context flow and context-query performance
- db: Make residual multi-statement writers atomic
- db: Scope single-instance guard to the install identity
- db: Route stall captures through the write pool; atomic writes + single-instance
- coordinator: Record stall purchases via the write pool

## v1.0.2 (2026-10-05)

### Improvements
- Credit fork maintainer in the status bar

## v1.0.1 (2026-10-05)

_Pre-convention history — all commits in range:_

- release: v1.0.1 (`e0373ae`)

## v1.0.0 (2026-10-05)

### Features
- books: Ingest the game's Books directory on startup
- chat: FTS deep search + day navigation across chat views
- Stall price capture for other players' stalls (capture panel + OCR)

### Improvements
- db: Route remaining CRUD/import writers through the write pool
- db: Single-writer pool architecture (app-side write queue)

### Fixes
- build: Repair the personal-title sed (it emitted '?' and broke the JSON) and the corrupted conf
- ocr: NSIS silent-install flags for the tesseract downloader; bump-version personal-title match
- Disable updater artifacts on the personal profile
- stall: Price capture fixes from live testing
- db,chat,ui: Review round fixes - accent-highlight index map, FTS fallback, watcher retry semantics
- db,books: Self-review fixes for d0e53ab
- db,chat: Seventh-round review fixes
- db: Explicit cursor-reset path so MAX guard doesn't block rotation
- db,chat: Sixth-round review fixes
- db,chat: Fifth-round review fixes (previously-missed notes)
- db,chat: Fourth-round review fixes
- db,chat: Teleport binds on writer; count survives pagination
- books: Propagate first hoplology insert failure
- db,chat: Third-round review fixes
- books: Propagate ingest errors; route writers through the write pool
- books: Prune removed files from mtime map + live-path year resolver for shop logs
- books: Apply code-review fixes to books watcher
- db: Scope the writer connection per file in startup backfills
- startup: Run log backfills sequentially to stop BEGIN IMMEDIATE starvation
- startup: Per-file backfill transactions so concurrent writers fit busy_timeout
- startup: Use BEGIN IMMEDIATE for all startup backfill transactions
- db: Set PRAGMA query_only on read-pool connections
- db: Route every remaining writer through the single write connection
- arena: Convert bets backfill to per-file IMMEDIATE transactions
- db: Route the last missed writers through the write pool + drop dead helper
- chat: Day-anchor only around_time; missing generation captures
- chat: Shared request-generation guard across day-jump and live loads
- chat: Second-round review fixes
- chat: Chips remove raw tokens; day-scoped count label
- chat: Review fixes — wildcard rules, FTS highlight tokens, filter-preserving day windows
- chat: Review fixes for date navigation
- chat: Keep date toolbar reachable when a day filter yields nothing
- rez: Dedup paired rez lines; keep Unknown out of name-based metrics
- rez: Parse the target-only "comes back to life" phrasing
- Pre-push hook failed silently on Windows

## v0.12.18 (2026-06-26)

### Fixes
- build-planner: Parse indirect-damage mod prose the CDN actually stores

## v0.12.17 (2026-07-30)

### Features
- gourmand: Classify foods by source (crafted vs event vs drop)

## v0.12.16 (2026-07-29)

### Features
- build-planner: Allow up to 3 mods per skill and unlimited generic mods

## v0.12.15 (2026-07-28)

### Features
- recipes: Surface currency costs (Combat Wisdom, Fae Energy) on recipes

## v0.12.14 (2026-07-28)

### Features
- dashboard: Almanac of Corruption widget for daily harvesting foci
- farming-db: Clear both equipped-skill filters to Any

## v0.12.13 (2026-07-25)

### Features
- build-planner: Type-to-search filter for long dropdowns app-wide

## v0.12.12 (2026-07-24)

### Features
- combat-wisdom: Draggable split to resize monster cooldowns

## v0.12.11 (2026-07-21)

### Features
- consolidate: Dedupe account transfer chests + enlarge/widen move labels

## v0.12.10 (2026-07-21)

### Features
- arena: Track personal betting rate + P&L from Player.log

## v0.12.9 (2026-07-21)

### Features
- arena: Add Casino Arena fight tracker + dashboard widget

## v0.12.8 (2026-07-20)

### Fixes
- milking: De-duplicate cow timers by keying identity per-cow

## v0.12.7 (2026-07-20)

### Features
- consolidate: Pool alt storage with bank targeting and category/gear filters

## v0.12.6 (2026-07-19)

### Fixes
- consolidate: Account for target vault slot capacity

## v0.12.5 (2026-07-19)

### Fixes
- drop-tracker: Stop filing ground/gathered loot under monster kills

## v0.12.4 (2026-07-19)

### Fixes
- build-planner: Convert direct damage to Fire for parenthesized type-change mods

## v0.12.3 (2026-07-19)

### Features
- build-planner: Add Racial jewelry equipment slot
- build-planner: Fold Fairy Magic abilities and mods into Mentalism

### Fixes
- ability-tooltip: Show cost, cooldown, range, and Special effect text

## v0.12.2 (2026-07-18)

### Fixes
- dashboard: Categorize vendors that buy equipment/armor and other gear

## v0.12.1 (2026-07-18)

### Features
- dashboard: Right-click vendor row to set councils to 0

## v0.12.0 (2026-07-17)

### Features
- crafting: Import/export projects via shareable codes

### Fixes
- crafting: Tidy leveling toolbar and project entry layout

## v0.11.37 (2026-06-26)

### Features
- inventory: Add cross-character storage search toggle

## v0.11.36 (2026-07-13)

### Fixes
- database: Dedup Player-prev backfill kills against live rows regardless of server

## v0.11.35 (2026-07-12)

### Features
- chat: Add Poems tab to Chat Logs

## v0.11.34 (2026-07-12)

### Fixes
- build-planner: Default mod/augment tier to target level deterministically

## v0.11.33 (2026-07-11)

### Fixes
- crafting: Leveling planner understated materials for multi-yield recipes
- release: Only an open PR should suppress the release PR

## v0.11.32 (2026-07-11)

### Features
- brewery: Collapsible + resizable discoveries and untried lists

### Fixes
- flatpak: Invoke node generator as a module, unsilence pip (exit-127 on gnome-50 image)

## v0.11.30 (2026-07-10)

### Features
- words-of-power: Auto-remove words when spoken in-game

### Fixes
- flatpak: Bump runtime to GNOME 50 (47 is EOL)

## v0.11.29 (2026-07-07)

### Features
- build-planner: Add "Import Equipped" gear loadout picker

### Fixes
- build-planner: Apply skill-wide "<Skill> Base Damage +N%" prose mods

## v0.11.27 (2026-07-07)

### Features
- quests: Tabbed quest screen, work-order backlog, and board tracking
- farming: Relabel session "Gold" stat to "Councils"

## v0.11.26 (2026-07-06)

### Fixes
- farming: Stop session history fragmenting for 12-hour-clock users

## v0.11.25 (2026-07-06)

### Features
- crafting: Add Recipe Book tab — unlocked vs missing recipes per skill

## v0.11.24 (2026-07-05)

### Features
- farming: Make the 2h session cap configurable in App Settings
- sessions: Auto-roll survey + farming sessions past 2h

### Fixes
- build-planner: Resolve ability names past monster-ability collisions

## v0.11.23 (2026-07-04)

### Fixes
- survey: Index survey-attributed transactions and harden session capture

## v0.11.22 (2026-06-26)

### Features
- data-browser: Project needs + friendly locations on item detail

## v0.11.21 (2026-07-02)

### Features
- surveying: Extend chat-loot identity to sulfur, saltpeter, base crystals
- surveying: Item-identity fallback for chat loot attribution

## v0.11.20 (2026-07-02)

### Features
- surveying: Only track items actually collected from surveys

## v0.11.19 (2026-07-02)

_Pre-convention history — all commits in range:_

- Merge pull request #57 from crisp-oddio/release/v0.11.19 (`e833ee8`)
- release: v0.11.19 (`8cacac1`)

## v0.11.18 (2026-07-02)

### Features
- build-planner: Prose damage mods + damage-type conversions in effective stats

## v0.11.17 (2026-07-01)

_Pre-convention history — all commits in range:_

- Merge pull request #54 from crisp-oddio/release/v0.11.17 (`0894a55`)
- release: v0.11.17 (`457806d`)
- Merge pull request #53 from crisp-oddio/dev (`3120af7`)
- style(roulette): fit widget within fixed card height (`bd348bd`)

## v0.11.16 (2026-07-01)

### Features
- brewery: Track untried combos and build crafting projects from them

## v0.11.15 (2026-06-30)

_Pre-convention history — all commits in range:_

- Merge pull request #50 from crisp-oddio/release/v0.11.15 (`ad37fdc`)
- release: v0.11.15 (`ff5ad7e`)
- Merge pull request #49 from crisp-oddio/dev (`bb1d66d`)
- style(roulette): roomier board cells for legibility (`be1eb40`)

## v0.11.14 (2026-06-30)

### Features
- roulette: Last-10 rolls tracker + hottest-3 board highlight
- build-planner: Group equipped mods by skill with color stripes

## v0.11.13 (2026-06-30)

### Features
- roulette: Table-felt number board + true casino colors

## v0.11.12 (2026-06-29)

### Features
- roulette: Casino roulette outcome tracker widget

## v0.11.11 (2026-06-28)

### Features
- db: User-data maintenance — purge big tables, compact, fix FTS triggers
- Add CSV import for words of power widget

## v0.11.10 (2026-06-26)

### Fixes
- crafting: Move consume-buffer arrows outside the input box

## v0.11.9 (2026-06-26)

### Features
- crafting: Configurable consume-chance safety buffer on project materials

## v0.11.8 (2026-06-26)

### Features
- crafting: Per-line clear button + live XP-buff recompute in leveling planner

## v0.11.7 (2026-06-26)

### Fixes
- build-planner: Apply Core/Nice/Epic attack & Signature Debuff damage to abilities

## v0.11.6 (2026-06-25)

### Features
- farming: Add SQLite .db export/import for the drop-rate database

## v0.11.5 (2026-06-25)

### Fixes
- build-planner: Apply conditional mods, item bonuses, and DoT-add mods to ability stats

## v0.11.4 (2026-06-24)

### Features
- farming: Auto-start session logging with status light + checkbox

### Fixes
- build-planner: Show health/armor-specific damage components

## v0.11.3 (2026-06-24)

### Fixes
- build-planner: Register direct-hit damage from HealthSpecificDamage
- build-planner: Apply indirect-damage rule to every DoT component
- build-planner: DoT damage uses indirect modifiers only

## v0.11.2 (2026-06-24)

### Features
- build-planner: Effective ability stats on hover + slot deselect toggle
- dashboard: Live council-balance estimate anchored on last export

## v0.11.1 (2026-06-23)

### Features
- build-planner: Ability mod tooltips, collapsible sections, global mod search + apply

## v0.11.0 (2026-06-23)

### Features
- build-planner: Default combat-skill dropdowns in Set Defaults
- farming: Serialized export filenames (charname-NNNN.csv)
- farming: Periodic auto-save of active session for crash recovery

### Fixes
- dashboard: Widget resize reflows neighbors via grid-column span

## v0.10.1 (2026-06-22)

### Fixes
- dashboard: Correct widget resize max-width clamp and stray-click pin

## v0.10.0 (2026-06-22)

### Features
- dashboard: Resizable widgets via right-edge X-axis drag

### Fixes
- stall-tracker: Deepen Trends chart palette for dark/HDR legibility

## v0.9.23 (2026-06-22)

### Features
- stall-tracker: Add Trends tab with sales time-series chart

## v0.9.22 (2026-06-21)

### Features
- database: Equipped combat-skill loadout filter + live selector

## v0.9.21 (2026-06-21)

### Features
- data-browser: Add drop-rate % to enemy Harvest Stats
- database: Capture equipped combat-skill loadout per kill (migration v55)
- database: CSV import/export, friend-format raw-event import, zone-aware drop rates

### Fixes
- Point changleog page at oddio repo.

## v0.9.20 (2026-06-21)

### Features
- database: List-first drop DB, permanent imports, harvest stats + app scale

## v0.9.19 (2026-06-21)

### Fixes
- linux: Disable WebKitGTK DMABUF renderer on all Linux, not just NVIDIA
- linux: Disable WebKitGTK DMABUF renderer on NVIDIA to stop native crash

## v0.9.18 (2026-06-21)

_Pre-convention history — all commits in range:_

- release: v0.9.18 (`8549d6f`)

## v0.9.17 (2026-06-21)

### Features
- vendors: Add historical council earnings tracker with two-tab widget
- dashboard: Combat Wisdom tracker widget

## v0.9.16 (2026-06-20)

### Features
- farming: Corpse-search drop-rate model, history drop-rate hover, butchering harvest detail

## v0.9.15 (2026-06-20)

### Features
- farming: Split gathered boxes, Active/History parity, drop-rate Database tab

## v0.9.14 (2026-06-20)

### Features
- dashboard: Drop high-favor Statehelm NPCs from gifting widget
- survey: Chat-authoritative loot summary from [Status] log

## v0.9.13 (2026-06-20)

### Features
- dashboard: Skill-driven Statehelm gifting widget + gift-count fixes

## v0.9.12 (2026-06-20)

### Features
- menubar: One-click live-tailing toggle for all logs
- gourmand: Track eaten foods live from Player.log

## v0.9.11 (2026-06-20)

### Features
- words-of-power: Category/level grouping + CSV export

## v0.9.10 (2026-06-19)

### Features
- farming: Looted-items + gathered columns with per-source drop rates

## v0.9.9 (2026-06-19)

### Features
- settings: Path auto-detect buttons + startup auto-detect

### Fixes
- Parse signed 32-bit item instance IDs; update handoff

## v0.9.8-exp (2026-06-19)

### Features
- Default file pickers to configured game data folders
- Add Start Tailing button to Inventory tab warning banner
- Make MenuBar status dots clickable to toggle tailing

### Fixes
- Pin @tauri-apps/api & plugin-dialog to match Rust crate minors
- Pin tauri-action to @v0, not @v1 (v1 tag does not exist)
- Prevent Cmd+Q from being caught by tab cycling handler
- MacOS Project Gorgon file path split — Player.log and game data are in separate locations

## v0.9.5 (2026-06-18)

### Features
- Add Prodigy Tracker widget with manual current-XP baseline
- Add XP Rate dashboard widget (combat + prodigy XP/hr & prodigy ETA)

### Fixes
- Correct crafting leveling XP model (flat 4x first craft + repeat-craft drop-off)

## v0.9.4 (2026-06-18)

### Features
- Enable auto-updater with fork signing key

### Fixes
- Apply static first-time bonus to leveling restore path (0.9.3.1)

## v0.9.3 (2026-06-18)

### Fixes
- Correct crafting leveling XP math (first-time bonus + synergy)

## v0.9.2 (2026-06-18)

### Features
- V0.9.1 — Flatpak packaging for Linux

### Fixes
- Flatpak production build renders correctly; release v0.9.2

## v0.9.0 (2026-06-18)

### Features
- Oddio fork v0.9.0 — font/size settings, widget opacity, crafting XP scaling fix

## v0.8.7 (2026-05-02)

### Fixes
- Bug with db migrations only had manual marking gourmand items working for fresh installs.

## v0.8.6 (2026-05-01)

### Features
- Devtools: can now do a simple, non-persisted playback of debug captures to validate loot attribution stuff

### Fixes
- Actually fixing the kill loot attribution bug i hope
- Synergies should work for multi-word skills now

## v0.8.5 (2026-05-01)

### Fixes
- Kill loot attribution and farming tracker missing new stacks

## v0.8.4 (2026-05-01)

### Improvements
- Words of power effects are collapsable now

## v0.8.3 (2026-05-01)

_Pre-convention history — all commits in range:_

- release: v0.8.3 (`ca813fd`)
- Merge branch 'main' of https://github.com/danielout/glogger (`620ec79`)
- chore: prepping for swapping to singular repo (`e145506`)

## v0.8.2 (2026-05-01)

### Features
- Enemy browser now uses player facing names from string table.
- Improved item info layout in data browser
- Area browser with monster and npc info
- Downloading string data as well now as part of CDN retrieval

### Improvements
- Consolidation workflow changes

## v0.8.1 (2026-05-01)

### Features
- Added drop data to item views
- Add kills and drop rate info to data browser
- Kill and loot tracking

### Fixes
- Making sure the widget and vendor details are communicating with each other.

## v0.8.0 (2026-05-01)

### Features
- Vendor gold counter with multi-character aggregation support.
- Hoplology widget
- Auto-detect item moves and add next-zone navigation to wizard
- Add storage consolidation tool

### Improvements
- Dashboard card scrolling improvement. temp improvement, need to do a better job with how we think about this.
- Consolidation workflow changes
- Consolidation workflow changes

### Fixes
- Time presets
- Item search was too sensitive about its cases
- Wizard tracks carry bag to prevent premature dropoff instructions
- Redesign consolidation UI — vault cards in responsive grid
- Handle portable vaults and missing zones in consolidation routing
- Consolidation route planner casinoPortal type mismatch

## v0.7.14-exp (2026-04-24)

### Fixes
- Correcting max card height

## v0.7.13-exp (2026-04-24)

### Fixes
- Experimental build now experimental

## v0.7.12-exp (2026-04-24)

### Features
- Add backend persistence for timer widget
- Add bulk price setting for market values
- Add seedling/plant/milling product chain linkage
- Add garden almanac widget
- Improve market price add UX with autocomplete and validation
- Switch report detection from folder polling to chat log events
- Add recurrent event timer dashboard widget
- Add hoplology equipment study tracker
- Persist screen and sub-tab state across navigation
- Add Show Totals mode and persist view settings in inventory
- Manual brewery recipe/discovery adding
- Add "Show Totals" mode to storage view
- Add general-purpose timer dashboard widget
- Manual food eaten/not-eaten marking for gourmand
- Implement audio alerts for watchword matches
- Collapsible recipe list and split raw/intermediate materials
- Add enemy database to data browser
- Make critical resources widget configurable
- Add home zone setting for route planner
- Add area tooltips with NPC cross-reference
- Add skeleton loading states to crafting project materials
- Add shared DataTable, FilterBar, and Skeleton components
- Trip planner widget (and initial trip routing work)

### Fixes
- Status channel messages with colons parsed as player chat
- Resolve all 19 Rust compiler warnings
- Correct survey session start/end timestamps
- Disable hoplology widget until book/report parsing is implemented
- Changelog downloads section formatting and auto-collapse
- Register toggle_food_eaten_status command in Tauri handler
- Add debug logging for rez counter and stop swallowing emit errors
- Hide stack size in item tooltips for non-stackable items
- Enforce uniform dashboard widget height and fix scroll patterns
- Improve login inventory handling

## v0.7.11 (2026-04-21)

### Features
- Parse ProcessUpdateDescription for entity state changes
- Show learned/unlearned status in recipe tooltips
- Show last import timestamps on character name hover

### Fixes
- Survey analytics time calculation was inflated by use count
- Food tooltip parsing on gourmand comparison panel
- NPCs without an area now appear in search results
- Crafting planner restores first-time XP when entry is removed
- Crafting projects not properly calculating all intermediate crafts

## v0.7.10 (2026-04-21)

### Improvements
- Update notif moved to about

## v0.7.9 (2026-04-21)

### Improvements
- Adding "self-milked" counter. sighing heavily.

## v0.7.8 (2026-04-20)

### Improvements
- About page tweaks

## v0.7.7 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.7 (`460cc97`)
- build: pipe pipe pipe (`92911a1`)

## v0.7.6 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.6 (`ee7a435`)
- Merge branch 'main' of https://github.com/danielout/glogger (`6b044e7`)
- build: fix the release script maybne (`40dbb25`)

## v0.7.5 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.5 (`3e56d6c`)

## v0.7.4 (2026-04-20)

### Features
- Version comparison tool in data browser

### Fixes
- Mac updater fix

## v0.7.3 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.3 (`7c878e4`)
- Merge branch 'main' of https://github.com/danielout/glogger (`ccd6e06`)
- build: fixing release script (`42273dd`)

## v0.7.2 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.2 (`451e3dd`)
- Merge branch 'main' of https://github.com/danielout/glogger (`ad6f2bf`)
- fear: updater and new build process (`e0709d0`)

## v0.7.1 (2026-04-20)

### Features
- Wop tracker
- GST launch and download button, and link to info

### Improvements
- Hovering the moon gives you good moon info!

### Fixes
- Attribution

## v0.7.0 (2026-04-20)

### Features
- Adding milking widget and stat tracker widget to dashboard
- Character stats page now tracks all attributes, /age, and /behaviorreport - also initial batch of computed stats.

### Improvements
- Update gourmand report just on log view, saving not required
- Better log parsing, updating documentation, additional game state tracking

### Fixes
- Fixing dashboard flashing, hopefully

## v0.6.6 (2026-04-20)

### Features
- Devtools capture lets you build a single file limited capture, stripped of player chat messages

### Improvements
- Devmode log captures are a little thinner, and truncate lines less.
- Crafting project performance improvements.

### Fixes
- Fixed polling issue with chat logs that caused it to potentially sleep the js thread on windows.
- Fixed bug with not all intermediate crafts resolving their materials correctly
- Build cleanup, snapshot favor fix, and crafting UX improvements

## v0.6.5 (2026-04-18)

### Improvements
- Dedicated chat log search screen now with "from:USER" and "in:CHANNEL"

### Fixes
- Survey data calculating incorrect session durations
- Survey data overflowing its rightful space

## v0.6.4 (2026-04-18)

### Improvements
- Making collapsed side panels much more noticable and clear they expand, i hope

### Fixes
- Fixing where i broke ingesting skill updates

## v0.6.3 (2026-04-18)

### Features
- Brewery: Merge brewing investigation and helper tools
- Brewery: Add "Try Next" suggestions for untried ingredient combos
- Brewery: Add effect search panel with material availability
- Brewery: CSV import for brewing discoveries
- Brewery: Add discovery journal from inventory snapshot scanning
- Brewery: Add Brewery tab with CDN brewing data extraction

### Improvements
- Brewery: Support effect_desc column in CSV import for tooltip text
- Brewery: Discovery table with fixed columns, owned counts, and delete
- Brewery: UI polish, flexible CSV import, export, and format help
- Brewery: Randomize Try Next suggestions within availability tiers
- Brewery: Remove duplicate text descriptions, add owned counts to ingredients
- Brewery: Rework effect panel with resolved descriptions and center panel results
- Brewery: Human-readable effect pool labels with descriptions
- Brewery: Resolve TSys power info for human-readable effect descriptions

### Fixes
- Fix up some minor warnings
- Brewery: Filter by Crafter field + auto-scan on inventory import

## v0.6.2 (2026-04-18)

### Features
- Add mushroom farming dashboard widget
- Add teleport machine codes dashboard widget
- Item parser 2.0 + surveys 2.0

### Improvements
- Better batching and sleep controls for dual-log replays for development
- Show next moon phase instead of only full moon in mushroom widget
- Polish teleport codes and mushroom farming widgets
- Cap zone NPCs widget height to prevent it dominating dashboard
- Recipes widget now checks both inventory and storage
- Improve text contrast and bump base font size
- Auto-expand NPC details panel when selecting an NPC

### Fixes
- Forgot one piece of unhooking the tracker
- Recipe widget properly looks up recipe requirements
- Right sidebar in the survey tracker starts expanded
- Projects now respect the sort rules of that list
- Removing the 'start tracking' button since it is broke as hell. hardly a fix, but added a todo to investigate and reimplement later
- Stampede correctly shows with proper ranks in build planner and browser fix: we now rebuild derived tables on version change.
- Farming session timer handles midnight rollover
- Pricing calculator now uses market values with value*2 fallback
- Ability families with mixed monster/player tiers no longer misclassified
- Build planner name dropdown now gets its own line
- Tooltips now flip above/below to stay within viewport

## v0.6.1 (2026-04-14)

### Features
- '?' button now has useful information in it. moved and updated about, too.
- Npc pages and widget v2.0 - now actually useful

### Improvements
- Semi-improvement. surveying tracker honesty about its current status now.

### Fixes
- Small improvements for inventory tracking

## v0.6.0 (2026-04-14)

### Features
- StallTracker: Phase 12 — polish
- StallTracker: Phase 11 — store-level race protection
- StallTracker: Phase 10 — Import / Export / Clear
- StallTracker: Custom DatePicker, replaces native date inputs
- StallTracker: Phase 9 — Shop Log modal
- StallTracker: Phase 8 — Inventory tab with last-known-price
- StallTracker: Phase 7 — Revenue pivot tab
- StallTracker: Phase 6 — Sales tab
- StallTracker: Phase 5 — Pinia store + parent view
- StallTracker: Phase 4 — Revenue + Inventory aggregations
- StallTracker: Phase 3 — Tauri CRUD commands
- StallTracker: Phase 2 — live ingest via coordinator
- StallTracker: Phase 1 — parser, year resolver, migration
- Dev tools screen for testing
- Lorebook databrowser

### Fixes
- DatePicker: Sync visible month when modelValue changes externally
- StallTracker: Clear filterTimer on tab unmount
- StallTracker: Clear stale error on no-owner reload across all tabs
- StallTracker: Reset actionInProgress on parent view unmount
- StallTracker: Wrap handleImport body in try/finally
- DataBrowser: Replace nested buttons in sidebar list items
- StallTracker: Drop unused backfill_year helper
- StallTracker: Make Recently Sold Out table sortable
- StallTracker: Render Revenue pivot items via ItemInline
- StallTracker: Make Shop Log table sortable
- StallTracker: Make Inventory In Stock table sortable
- StallTracker: Reverse Revenue period columns to newest-first
- StallTracker: Render items via ItemInline for tooltips and navigation
- StallTracker: Extract xN quantity from stacked bought listings
- StallTracker: Blur date inputs on outside click
- Fixed multi output recipes moving from level planner to projects fix: slightly improved item tracking feat: known issues listin ? menu
- Statehelm gift tracker doesn't require you to be on the page fix: better item name matching for gear fix: some tooltip clipping issues

## v0.5.1 (2026-04-13)

### Features
- Add string-based import/export for build planner sharing
- Add ability to clone a build in the build planner
- Add sort order toggle for chat logs (newest/oldest first)
- Add 'Jump to Present' button in chat logs

### Fixes
- Removing the half-complete test that obviously doesn't pass
- Updating survey tests - they are still bad, but better bad
- Stupid arrows and their float value. blame archers
- Yet another attempt to correct survey reward logging
- Prevent survey session double-counting on zone change
- If you have no builds, now you can create one.

## v0.5.0 (2026-04-11)

### Features
- Pricing Calculator for Projects

### Improvements
- Improved build planner  functionality with CPs and belts, item selection
- Build planner layout rework
- Clicking a watchword alert on the dashboard takes you to the watchwords page

### Fixes
- Cleaned up some rust warnings
- Unbreaking survey data logging due to ring bonuses

## v0.4.8 (2026-04-09)

### Improvements
- Small dashboard layout tweaks
- Status widget more useful with more info. better layout, too
- Max height for dashboard cards before they get an internal scroll
- Character->Skills view now lists what bonus levels you have achieved, has a better layout, and provides more info
- Better patch notes organization

### Fixes
- Dashboard widget drag and drop
- In specific circumstances we were double-counting bonus levels
- Date lines on old chat messages don't cause ugly extra spacing anymore.
- Deletion confirmation not waiting for confirmation (thanks rey!)

## v0.4.7 (2026-04-07)

### Fixes
- Market price list didn't scroll

## v0.4.6 (2026-04-07)

### Fixes
- Test fix to bypass false failure

## v0.4.5 (2026-04-06)

_Pre-convention history — all commits in range:_

- release: v0.4.5 (`e0ccb70`)
- bad text on npc browser (`bcfc6a3`)
- gourmand screen scroll fix (`9ef266a`)
- multi-character fix (`b9fcca7`)

## v0.4.4 (2026-04-06)

_Pre-convention history — all commits in range:_

- release: v0.4.4 (`33d02ba`)
- statehelm rep tracker (`fcc3570`)
- survey analytics improvements (`bfec7fd`)
- crafting views now properly support dynamic slots, and valid daynamic slot items can be set by players (`c0e7e86`)
- Merge branch 'main' of https://github.com/danielout/glogger (`18404fe`)
- survey and analytics fixes (`d263a9d`)

## v0.4.3 (2026-04-03)

_Pre-convention history — all commits in range:_

- release: v0.4.3 (`e8ed4f0`)
- fixing survey summary historical data (`0552339`)
- documenting our NPC vendor stuff (`37e36db`)
- market prices made the app think it was vendor buyable due to legacy code. aka last week me is dumb (`624053c`)
- Properly displaying recipes with proto items in the crafting skills display. (`2d914a8`)

## v0.4.2 (2026-04-03)

_Pre-convention history — all commits in range:_

- release: v0.4.2 (`4bbcb88`)
- fixing silly build error (`a2060fe`)
- more workflow updates. i'm dumb and can't read (`a543909`)

## v0.4.1 (2026-04-03)

_Pre-convention history — all commits in range:_

- release: v0.4.1 (`ddb4ed4`)
- Updating workflow from v4 to v5 (`8bcdc62`)
- Tackling assorted TODO list items (`5726f1a`)
- surveying analytics improvements (`ddd00bb`)
- Merge branch 'main' of https://github.com/danielout/glogger (`07902f1`)
- timestamp standardization (`5fdfb9d`)

## v0.4.0 (2026-04-02)

_Pre-convention history — all commits in range:_

- release: v0.4.0 (`5af850c`)
- Layout updates (`1bb2367`)
- NEW FEATURE: Crafting Skill Reports (`98ad97f`)
- Layout/UX improvements (`d27b38f`)
- first step towards cross-referenced data browsing (`dab6a58`)
- data browser and layout bugfixing (`8701a75`)
- projtects tab improvements. groups, intermediates. (`b88669b`)
- startup order refinement (`ace780b`)
- titlebar version updating not automagical (`1d0c4d3`)

## v0.3.1 (2026-03-31)

_Pre-convention history — all commits in range:_

- release: v0.3.1 (`80a4087`)
- build planner bugs. joy. (`b189594`)

## v0.3.0 (2026-03-30)

_Pre-convention history — all commits in range:_

- release: v0.3.0 (`985bc73`)
- NEW FEATURE: Build Planner (`f799992`)
- Fixing a bug where monsters that ate flesh lumps couldn't import their gourmand report, WOGAN. (`3033f36`)
- layout improvement to crafting levelling planning (`31b653d`)
- fixing up our release pipeline so i don't have to nuke my real data for testing (`4d0dcd6`)
- build planner initial work (`53b6f35`)
- dev/realease directory breakout and window -state plugin (`d2c00fe`)
- fixing bug with first craft xp being modified (`463e603`)
- documentation consolidation and organization (`52b4bea`)
- small fix to inventory tracking - updating gamestate from json (`dfebd7f`)
- Merge branch 'main' of https://github.com/danielout/glogger (`1173ebe`)
- fixing the crafting level planner to use the right xp (base level vs total) standardizing how handle level across the app (`62befc4`)
- better organizing and breaking out docs for my sanity. (`8e98b1e`)
- haha let's stop murdering people's databases (`69b431f`)

## v0.2.2 (2026-03-29)

_Pre-convention history — all commits in range:_

- release: v0.2.2 (`2ec7401`)
- fixing a bug with Cook's Help Brought To You By ReyDash (`8d3a3c9`)
- moving gourmand doc slight ui tweaks for the crafting levelling tab. (`192413e`)
- Merge branch 'main' of https://github.com/danielout/glogger (`2dd7d25`)
- NPC and Quest screens for character (`abe86f7`)

## v0.2.1 (2026-03-28)

_Pre-convention history — all commits in range:_

- release: v0.2.1 (`e681e90`)
- new skills page that hopefully is more useful! (`e647a38`)
- fixed an issue with file position watchers and reactive components (`09bc459`)

## v0.2.0 (2026-03-27)

_Pre-convention history — all commits in range:_

- release: v0.2.0 (`efcb3d1`)
- prep for alpha v0.2.0 (`e501377`)
- prototype-v0.1.1 (`f22ef7a`)
- doc shuffle (`9c19c38`)
- prototype-v0.1.0 (`a633b4f`)
- adding player export samples (`add51ed`)
- adding data browser (`1b04bfe`)
- JSON sample files for easier browsing than full files (`74022a1`)
- Breaking out our CDN importing in to specialty files. (`09c0a00`)
- basic survey setup (`d3358ab`)
- breaking out things in to smaller components, refined skill view. (`05bf679`)
- barebones testing functional (`917d75c`)
- initial commit (`98f6efb`)
- Initial Project Scaffold (`1f76811`)

