# Changelog

Per-release notes use the same grouping as scripts/generate-changelog.py
(feat: → New, fix: → Fixed, impv: → Improved) with one line per entry
(no type prefix, hash, PR ref, or commit count).
Releases that predate the commit-prefix convention list all commit subjects.

## Unreleased (since v1.12.0)

### Improved
- No visible changes — stability and maintenance update.


## v1.12.0 (2026-11-04)

### Improved
- Clearer names across the app; unused views removed.
- StallTracker recounts correctly after restocking.
- Clear unsure items instead of sold-out list.

## v1.11.0 (2026-11-01)

### Improved
- Stall stock stays accurate with less game data.

## v1.10.0 (2026-11-01)

### Improved
- Inventory items load faster.
- Portraits close with Esc and size better.

## v1.9.1 (2026-10-30)

### Fixed
- Recipes with no progress still show as known.

## v1.9.0 (2026-10-30)

### Improved
- Merged XP view is now called XP Rate.
- Tidier XP, stall prices, and gift views.

### Fixed
- Weather matches the right area.
- Status shows live weather; outdated badges removed.

## v1.8.0 (2026-10-30)

### Improved
- NPC list loads faster and stays current.
- Removed the Inventory Live tab.

### Fixed
- Character picker stays open when adding.
- Wealth no longer shows ??? entries.

## v1.7.0 (2026-10-29)

### Improved
- No visible changes — stability and maintenance update.

## v1.6.0 (2026-10-28)

### New
- Cross-character work orders.
- Survey Retry after load errors.

### Improved
- Clearer storage and exports.

### Fixed
- Surveys and work orders reload correctly.
- Items reload when switching servers with All Characters.
- Loading states no longer stick.

## v1.5.2 (2026-10-27)

### New
- Past incoming milkings now appear in ability history.
- Skills shows Prodigy sessions and level progress.

### Improved
- Survey loot is counted once and correctly.
- Log imports run faster and more reliably.
- Screens refresh in the right order.

### Fixed
- Roulette, self-milking times, and scans are steadier.
- Item stacks, search, and shortcuts work better.
- Currency, arena, and hidden tabs update correctly.
- Status respects time zone; inactive trackers stay hidden.
- Survey rewards attach to the right items.

## v1.5.1 (2026-10-26)

### New
- Skill level-ups from chat are now tracked.
- VIP reports import automatically.

### Fixed
- Level-up XP counts correctly in combat rate.
- Milking notices track again after chat fix.
- Version check works more reliably.

## v1.5.0 (2026-10-25)

### New
- More accurate deaths, treasure, and inventory counts.
- Notices when others milk you are now tracked.
- Vendor view grouped into categories with favorites.

### Improved
- Milking history matches across views.

### Fixed
- Live data resumes updating after pauses.
- Switching characters clears old data reliably.

## v1.4.0 (2026-10-23)

### Fixed
- Base release notes on last published release, repair v1.3.9 notes

## v1.3.9 (2026-10-22)

### Fixed
- Unique temp DB/dir names for parallel cargo tests
- Bind Ctrl+K quick-search alias, correct repository URL

## v1.3.8 (2026-10-22)

### New
- Wire snapshot abilities, VIP NPC fields, achievements catalog to UI

### Fixed
- Split chat import status into result/error refs, label Player.log error box
- Restore chat import result feedback, fix stuck importing flag
- Folder chat import nested-tx crash + progress indicator, CDP/wipe guidance
- Filter placeholder character reports (??? name / error race)
- Clear snapshotAbilities on character switch
- Guard snapshot timer fallback on live-row existence (review fix)
- Clear areas in CDN persist (same UNIQUE re-persist class)
- Clear item_uses in CDN persist (UNIQUE failure on v488 re-persist)
- Vendor councils estimates + CDN tsys clear + drop Vite devtools

### Improved
- Ingest VIP report fields + achievements from 10/21 update

## v1.3.7 (2026-10-21)

### Fixed
- Revert Map.getOrInsert in InventoryView grouping for older WebView2

### Improved
- Post-major-bump cleanup + opt-ins (TS 6, Vite 8, rusqlite 0.40, zip 9)

## v1.3.6 (2026-10-20)

### Fixed
- Strict session-boundary gates for tagged chat lines
- Key catch-up staged gains by producing chat session
- Restore live cow-milk path; gate clear-on-adopt to login-line adoption
- Session-keyed catch-up stack correction + clear-on-adopt inventory dump

## v1.3.5 (2026-10-18)

### Fixed
- Seed live inventory from ItemAdded initial_quantity so multi-stack pickups count at once

## v1.3.4 (2026-10-17)

### Fixed
- Rename Statehelm favor filter to hide at/above tier

## v1.3.3 (2026-10-17)

### New
- Let /frozen take its target from the argument
- Add /frozen — this repo's one-word review call

### Fixed
- Give keyboard-focusable DataTable rows a visible focus indicator
- Correct the review-loop and gate claims; scope the exemption check to the rule

### Improved
- Retire the repo review command in favour of the shared skill

## v1.3.2 (2026-10-17)

### Fixed
- Correct the ledger's padding rows and scope the gate's exemption check
- Restore the sub-row indents the DataTable sweep flattened
- Gate DataTable row keydown to the row itself; stable MaterialTable keys
- Repair DataTable fallthrough + row keyboard access; third review pass
- Restore row keys, dense empty states and pane scrollers after the DataTable sweep
- Repair the sticky-header wrapper and the compact/size conflation in DataTable

### Improved
- Migrate every data grid to the shared DataTable

## v1.3.1 (2026-10-17)

### Fixed
- Address review findings on the Statehelm gift rules
- Count only real gifts in the Statehelm tracker

## v1.3.0 (2026-10-15)

### New
- Backfill Statehelm gift status from the game's own logs

### Fixed
- Attribute each Player.log session to its own character during catch-up
- Review pass over the Statehelm gift backfill

## v1.2.1 (2026-10-13)

### Fixed
- Show learned-but-uncooked recipes in Cook's Helper

### Improved
- Drop dead chat-retention settings

## v1.2.0 (2026-10-12)

### New
- chat: Open search results in the day's real log
- Track cow-curse abilities and make milking timestamps honest

### Fixed
- chat: Keep "Jump to Present" honest while newer rows exist above
- chat: Close the review findings on the day log
- chat: Bucket chat days in the viewer's display timezone

### Improved
- chat: Pin row-availability status to the list edges

## v1.1.1 (2026-10-11)

### Improved
- market: Label stall price capture as experimental

## v1.1.0 (2026-10-11)

### Fixed
- gourmand: Show every Gourmand-XP food and bucket unknown categories

## v1.0.3 (2026-10-06)

### Fixed
- chat: Repair search-context flow and context-query performance
- db: Make residual multi-statement writers atomic
- db: Scope single-instance guard to the install identity
- db: Route stall captures through the write pool; atomic writes + single-instance
- coordinator: Record stall purchases via the write pool

### Improved
- Remove leftover resuscitation debug logging
- crafting: Cook's Helper auto-imports the gourmand report and persists eaten state

## v1.0.2 (2026-10-05)

### Improved
- Credit fork maintainer in the status bar

## v1.0.1 (2026-10-05)

### Improved
- No visible changes — stability and maintenance update.

## v1.0.0 (2026-10-05)

### New
- books: Ingest the game's Books directory on startup
- chat: FTS deep search + day navigation across chat views
- Stall price capture for other players' stalls (capture panel + OCR)

### Fixed
- build: Repair the personal-title sed (it emitted '?' and broke the JSON) and the corrupted conf
- Disable updater artifacts on the personal profile
- stall: Price capture fixes from live testing
- db: Explicit cursor-reset path so MAX guard doesn't block rotation
- db,chat: Teleport binds on writer; count survives pagination
- books: Propagate first hoplology insert failure
- books: Propagate ingest errors; route writers through the write pool
- books: Prune removed files from mtime map + live-path year resolver for shop logs
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
- chat: Chips remove raw tokens; day-scoped count label
- chat: Keep date toolbar reachable when a day filter yields nothing
- rez: Dedup paired rez lines; keep Unknown out of name-based metrics
- rez: Parse the target-only "comes back to life" phrasing
- Pre-push hook failed silently on Windows

### Improved
- db: Route remaining CRUD/import writers through the write pool
- db: Single-writer pool architecture (app-side write queue)

## v0.12.18 (2026-06-26)

### Fixed
- build-planner: Parse indirect-damage mod prose the CDN actually stores

## v0.12.17 (2026-07-30)

### Improved
- No visible changes — stability and maintenance update.

## v0.12.16 (2026-07-29)

### New
- build-planner: Allow up to 3 mods per skill and unlimited generic mods

## v0.12.15 (2026-07-28)

### New
- recipes: Surface currency costs (Combat Wisdom, Fae Energy) on recipes

## v0.12.14 (2026-07-28)

### New
- dashboard: Almanac of Corruption widget for daily harvesting foci
- farming-db: Clear both equipped-skill filters to Any

## v0.12.13 (2026-07-25)

### New
- build-planner: Type-to-search filter for long dropdowns app-wide

## v0.12.12 (2026-07-24)

### New
- combat-wisdom: Draggable split to resize monster cooldowns

## v0.12.11 (2026-07-21)

### New
- consolidate: Dedupe account transfer chests + enlarge/widen move labels

## v0.12.10 (2026-07-21)

### New
- arena: Track personal betting rate + P&L from Player.log

## v0.12.9 (2026-07-21)

### New
- arena: Add Casino Arena fight tracker + dashboard widget

## v0.12.8 (2026-07-20)

### Fixed
- milking: De-duplicate cow timers by keying identity per-cow

## v0.12.7 (2026-07-20)

### New
- consolidate: Pool alt storage with bank targeting and category/gear filters

## v0.12.6 (2026-07-19)

### Fixed
- consolidate: Account for target vault slot capacity

## v0.12.5 (2026-07-19)

### Fixed
- drop-tracker: Stop filing ground/gathered loot under monster kills

## v0.12.4 (2026-07-19)

### Fixed
- build-planner: Convert direct damage to Fire for parenthesized type-change mods

## v0.12.3 (2026-07-19)

### New
- build-planner: Add Racial jewelry equipment slot
- build-planner: Fold Fairy Magic abilities and mods into Mentalism

### Fixed
- ability-tooltip: Show cost, cooldown, range, and Special effect text

## v0.12.2 (2026-07-18)

### Fixed
- dashboard: Categorize vendors that buy equipment/armor and other gear

## v0.12.1 (2026-07-18)

### New
- dashboard: Right-click vendor row to set councils to 0

## v0.12.0 (2026-07-17)

### New
- crafting: Import/export projects via shareable codes

### Fixed
- crafting: Tidy leveling toolbar and project entry layout

## v0.11.37 (2026-06-26)

### New
- inventory: Add cross-character storage search toggle

## v0.11.36 (2026-07-13)

### Fixed
- database: Dedup Player-prev backfill kills against live rows regardless of server

## v0.11.35 (2026-07-12)

### New
- chat: Add Poems tab to Chat Logs

## v0.11.34 (2026-07-12)

### Fixed
- build-planner: Default mod/augment tier to target level deterministically

## v0.11.33 (2026-07-11)

### Fixed
- crafting: Leveling planner understated materials for multi-yield recipes
- release: Only an open PR should suppress the release PR

## v0.11.32 (2026-07-11)

### New
- brewery: Collapsible + resizable discoveries and untried lists

## v0.11.30 (2026-07-10)

### New
- words-of-power: Auto-remove words when spoken in-game

## v0.11.29 (2026-07-07)

### New
- build-planner: Add "Import Equipped" gear loadout picker

### Fixed
- build-planner: Apply skill-wide "<Skill> Base Damage +N%" prose mods

## v0.11.27 (2026-07-07)

### New
- quests: Tabbed quest screen, work-order backlog, and board tracking
- farming: Relabel session "Gold" stat to "Councils"

## v0.11.26 (2026-07-06)

### Fixed
- farming: Stop session history fragmenting for 12-hour-clock users

## v0.11.25 (2026-07-06)

### New
- crafting: Add Recipe Book tab — unlocked vs missing recipes per skill

## v0.11.24 (2026-07-05)

### New
- farming: Make the 2h session cap configurable in App Settings
- sessions: Auto-roll survey + farming sessions past 2h

### Fixed
- build-planner: Resolve ability names past monster-ability collisions

## v0.11.23 (2026-07-04)

### Fixed
- survey: Index survey-attributed transactions and harden session capture

## v0.11.22 (2026-06-26)

### New
- data-browser: Project needs + friendly locations on item detail

## v0.11.21 (2026-07-02)

### New
- surveying: Extend chat-loot identity to sulfur, saltpeter, base crystals
- surveying: Item-identity fallback for chat loot attribution

## v0.11.20 (2026-07-02)

### New
- surveying: Only track items actually collected from surveys

## v0.11.19 (2026-07-02)

### Improved
- No visible changes — stability and maintenance update.

## v0.11.18 (2026-07-02)

### New
- build-planner: Prose damage mods + damage-type conversions in effective stats

## v0.11.17 (2026-07-01)

### Improved
- No visible changes — stability and maintenance update.

## v0.11.16 (2026-07-01)

### New
- brewery: Track untried combos and build crafting projects from them

## v0.11.15 (2026-06-30)

### Improved
- No visible changes — stability and maintenance update.

## v0.11.14 (2026-06-30)

### New
- roulette: Last-10 rolls tracker + hottest-3 board highlight
- build-planner: Group equipped mods by skill with color stripes

## v0.11.13 (2026-06-30)

### New
- roulette: Table-felt number board + true casino colors

## v0.11.12 (2026-06-29)

### New
- roulette: Casino roulette outcome tracker widget

## v0.11.11 (2026-06-28)

### New
- db: User-data maintenance — purge big tables, compact, fix FTS triggers
- Add CSV import for words of power widget

## v0.11.10 (2026-06-26)

### Fixed
- crafting: Move consume-buffer arrows outside the input box

## v0.11.9 (2026-06-26)

### New
- crafting: Configurable consume-chance safety buffer on project materials

## v0.11.8 (2026-06-26)

### New
- crafting: Per-line clear button + live XP-buff recompute in leveling planner

## v0.11.7 (2026-06-26)

### Fixed
- build-planner: Apply Core/Nice/Epic attack & Signature Debuff damage to abilities

## v0.11.6 (2026-06-25)

### New
- farming: Add SQLite .db export/import for the drop-rate database

## v0.11.5 (2026-06-25)

### Fixed
- build-planner: Apply conditional mods, item bonuses, and DoT-add mods to ability stats

## v0.11.4 (2026-06-24)

### New
- farming: Auto-start session logging with status light + checkbox

### Fixed
- build-planner: Show health/armor-specific damage components

## v0.11.3 (2026-06-24)

### Fixed
- build-planner: Register direct-hit damage from HealthSpecificDamage
- build-planner: Apply indirect-damage rule to every DoT component
- build-planner: DoT damage uses indirect modifiers only

## v0.11.2 (2026-06-24)

### New
- build-planner: Effective ability stats on hover + slot deselect toggle
- dashboard: Live council-balance estimate anchored on last export

## v0.11.1 (2026-06-23)

### New
- build-planner: Ability mod tooltips, collapsible sections, global mod search + apply

## v0.11.0 (2026-06-23)

### New
- build-planner: Default combat-skill dropdowns in Set Defaults
- farming: Serialized export filenames (charname-NNNN.csv)
- farming: Periodic auto-save of active session for crash recovery

### Fixed
- dashboard: Widget resize reflows neighbors via grid-column span

## v0.10.1 (2026-06-22)

### Fixed
- dashboard: Correct widget resize max-width clamp and stray-click pin

## v0.10.0 (2026-06-22)

### New
- dashboard: Resizable widgets via right-edge X-axis drag

### Fixed
- stall-tracker: Deepen Trends chart palette for dark/HDR legibility

## v0.9.23 (2026-06-22)

### New
- stall-tracker: Add Trends tab with sales time-series chart

## v0.9.22 (2026-06-21)

### New
- database: Equipped combat-skill loadout filter + live selector

## v0.9.21 (2026-06-21)

### New
- data-browser: Add drop-rate % to enemy Harvest Stats
- database: Capture equipped combat-skill loadout per kill (migration v55)
- database: CSV import/export, friend-format raw-event import, zone-aware drop rates

### Fixed
- Point changleog page at oddio repo.

## v0.9.20 (2026-06-21)

### New
- database: List-first drop DB, permanent imports, harvest stats + app scale

## v0.9.19 (2026-06-21)

### Fixed
- linux: Disable WebKitGTK DMABUF renderer on all Linux, not just NVIDIA
- linux: Disable WebKitGTK DMABUF renderer on NVIDIA to stop native crash

## v0.9.18 (2026-06-21)

### Improved
- No visible changes — stability and maintenance update.

## v0.9.17 (2026-06-21)

### New
- vendors: Add historical council earnings tracker with two-tab widget
- dashboard: Combat Wisdom tracker widget

## v0.9.16 (2026-06-20)

### New
- farming: Corpse-search drop-rate model, history drop-rate hover, butchering harvest detail

## v0.9.15 (2026-06-20)

### Improved
- No visible changes — stability and maintenance update.

## v0.9.14 (2026-06-20)

### New
- dashboard: Drop high-favor Statehelm NPCs from gifting widget
- survey: Chat-authoritative loot summary from [Status] log

## v0.9.13 (2026-06-20)

### New
- dashboard: Skill-driven Statehelm gifting widget + gift-count fixes

## v0.9.12 (2026-06-20)

### New
- menubar: One-click live-tailing toggle for all logs
- gourmand: Track eaten foods live from Player.log

## v0.9.11 (2026-06-20)

### New
- words-of-power: Category/level grouping + CSV export

## v0.9.10 (2026-06-19)

### New
- farming: Looted-items + gathered columns with per-source drop rates

## v0.9.9 (2026-06-19)

### New
- settings: Path auto-detect buttons + startup auto-detect

### Fixed
- Parse signed 32-bit item instance IDs; update handoff

## v0.9.8-exp (2026-06-19)

### New
- Default file pickers to configured game data folders
- Add Start Tailing button to Inventory tab warning banner
- Make MenuBar status dots clickable to toggle tailing

### Fixed
- Pin @tauri-apps/api & plugin-dialog to match Rust crate minors
- Prevent Cmd+Q from being caught by tab cycling handler
- MacOS Project Gorgon file path split — Player.log and game data are in separate locations

## v0.9.5 (2026-06-18)

### New
- Add Prodigy Tracker widget with manual current-XP baseline
- Add XP Rate dashboard widget (combat + prodigy XP/hr & prodigy ETA)

### Fixed
- Correct crafting leveling XP model (flat 4x first craft + repeat-craft drop-off)

## v0.9.4 (2026-06-18)

### New
- Enable auto-updater with fork signing key

### Fixed
- Apply static first-time bonus to leveling restore path (0.9.3.1)

## v0.9.3 (2026-06-18)

### Fixed
- Correct crafting leveling XP math (first-time bonus + synergy)

## v0.9.2 (2026-06-18)

### Improved
- No visible changes — stability and maintenance update.

## v0.9.0 (2026-06-18)

### New
- Oddio fork v0.9.0 — font/size settings, widget opacity, crafting XP scaling fix

## v0.8.7 (2026-05-02)

### Fixed
- Bug with db migrations only had manual marking gourmand items working for fresh installs.

## v0.8.6 (2026-05-01)

### New
- Devtools: can now do a simple, non-persisted playback of debug captures to validate loot attribution stuff

### Fixed
- Actually fixing the kill loot attribution bug i hope
- Synergies should work for multi-word skills now

## v0.8.5 (2026-05-01)

### Fixed
- Kill loot attribution and farming tracker missing new stacks

## v0.8.4 (2026-05-01)

### Improved
- Words of power effects are collapsable now

## v0.8.3 (2026-05-01)

### Improved
- No visible changes — stability and maintenance update.

## v0.8.2 (2026-05-01)

### New
- Enemy browser now uses player facing names from string table.
- Improved item info layout in data browser
- Area browser with monster and npc info
- Downloading string data as well now as part of CDN retrieval

### Improved
- Consolidation workflow changes

## v0.8.1 (2026-05-01)

### New
- Added drop data to item views
- Add kills and drop rate info to data browser
- Kill and loot tracking

### Fixed
- Making sure the widget and vendor details are communicating with each other.

## v0.8.0 (2026-05-01)

### New
- Vendor gold counter with multi-character aggregation support.
- Hoplology widget
- Auto-detect item moves and add next-zone navigation to wizard
- Add storage consolidation tool

### Fixed
- Time presets
- Item search was too sensitive about its cases
- Wizard tracks carry bag to prevent premature dropoff instructions
- Redesign consolidation UI — vault cards in responsive grid
- Handle portable vaults and missing zones in consolidation routing
- Consolidation route planner casinoPortal type mismatch

### Improved
- Dashboard card scrolling improvement. temp improvement, need to do a better job with how we think about this.
- Consolidation workflow changes

## v0.7.14-exp (2026-04-24)

### Fixed
- Correcting max card height

## v0.7.13-exp (2026-04-24)

### Fixed
- Experimental build now experimental

## v0.7.12-exp (2026-04-24)

### New
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

### Fixed
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

### New
- Parse ProcessUpdateDescription for entity state changes
- Show learned/unlearned status in recipe tooltips
- Show last import timestamps on character name hover

### Fixed
- Survey analytics time calculation was inflated by use count
- Food tooltip parsing on gourmand comparison panel
- NPCs without an area now appear in search results
- Crafting planner restores first-time XP when entry is removed
- Crafting projects not properly calculating all intermediate crafts

## v0.7.10 (2026-04-21)

### Improved
- Update notif moved to about

## v0.7.9 (2026-04-21)

### Improved
- Adding "self-milked" counter. sighing heavily.

## v0.7.8 (2026-04-20)

### Improved
- About page tweaks

## v0.7.7 (2026-04-20)

### Improved
- No visible changes — stability and maintenance update.

## v0.7.6 (2026-04-20)

### Improved
- No visible changes — stability and maintenance update.

## v0.7.5 (2026-04-20)

### Improved
- No visible changes — stability and maintenance update.

## v0.7.4 (2026-04-20)

### New
- Version comparison tool in data browser

### Fixed
- Mac updater fix

## v0.7.3 (2026-04-20)

### Improved
- No visible changes — stability and maintenance update.

## v0.7.2 (2026-04-20)

### Improved
- No visible changes — stability and maintenance update.

## v0.7.1 (2026-04-20)

### New
- Wop tracker
- GST launch and download button, and link to info

### Fixed
- Attribution

### Improved
- Hovering the moon gives you good moon info!

## v0.7.0 (2026-04-20)

### New
- Adding milking widget and stat tracker widget to dashboard
- Character stats page now tracks all attributes, /age, and /behaviorreport - also initial batch of computed stats.

### Fixed
- Fixing dashboard flashing, hopefully

### Improved
- Update gourmand report just on log view, saving not required
- Better log parsing, updating documentation, additional game state tracking

## v0.6.6 (2026-04-20)

### New
- Devtools capture lets you build a single file limited capture, stripped of player chat messages

### Fixed
- Fixed polling issue with chat logs that caused it to potentially sleep the js thread on windows.
- Fixed bug with not all intermediate crafts resolving their materials correctly
- Build cleanup, snapshot favor fix, and crafting UX improvements

### Improved
- Devmode log captures are a little thinner, and truncate lines less.
- Crafting project performance improvements.

## v0.6.5 (2026-04-18)

### Fixed
- Survey data calculating incorrect session durations
- Survey data overflowing its rightful space

### Improved
- Dedicated chat log search screen now with "from:USER" and "in:CHANNEL"

## v0.6.4 (2026-04-18)

### Fixed
- Fixing where i broke ingesting skill updates

### Improved
- Making collapsed side panels much more noticable and clear they expand, i hope

## v0.6.3 (2026-04-18)

### New
- Brewery: Add "Try Next" suggestions for untried ingredient combos
- Brewery: Add effect search panel with material availability
- Brewery: CSV import for brewing discoveries
- Brewery: Add discovery journal from inventory snapshot scanning
- Brewery: Add Brewery tab with CDN brewing data extraction

### Fixed
- Fix up some minor warnings
- Brewery: Filter by Crafter field + auto-scan on inventory import

### Improved
- Brewery: Support effect_desc column in CSV import for tooltip text
- Brewery: Discovery table with fixed columns, owned counts, and delete
- Brewery: UI polish, flexible CSV import, export, and format help
- Brewery: Randomize Try Next suggestions within availability tiers
- Brewery: Remove duplicate text descriptions, add owned counts to ingredients
- Brewery: Rework effect panel with resolved descriptions and center panel results
- Brewery: Human-readable effect pool labels with descriptions
- Brewery: Resolve TSys power info for human-readable effect descriptions

## v0.6.2 (2026-04-18)

### New
- Add mushroom farming dashboard widget
- Add teleport machine codes dashboard widget
- Item parser 2.0 + surveys 2.0

### Fixed
- Forgot one piece of unhooking the tracker
- Recipe widget properly looks up recipe requirements
- Right sidebar in the survey tracker starts expanded
- Projects now respect the sort rules of that list
- Removing the 'start tracking' button since it is broke as hell. hardly a fix, but added a todo to investigate and reimplement later
- Stampede correctly shows with proper ranks in build planner and browser
- Farming session timer handles midnight rollover
- Pricing calculator now uses market values with value*2 fallback
- Build planner name dropdown now gets its own line
- Tooltips now flip above/below to stay within viewport

### Improved
- Better batching and sleep controls for dual-log replays for development
- Show next moon phase instead of only full moon in mushroom widget
- Polish teleport codes and mushroom farming widgets
- Cap zone NPCs widget height to prevent it dominating dashboard
- Recipes widget now checks both inventory and storage
- Improve text contrast and bump base font size
- Auto-expand NPC details panel when selecting an NPC

## v0.6.1 (2026-04-14)

### New
- '?' button now has useful information in it. moved and updated about, too.
- Npc pages and widget v2.0 - now actually useful

### Fixed
- Small improvements for inventory tracking

### Improved
- Semi-improvement. surveying tracker honesty about its current status now.

## v0.6.0 (2026-04-14)

### New
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

### Fixed
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
- Fixed multi output recipes moving from level planner to projects
- Statehelm gift tracker doesn't require you to be on the page

## v0.5.1 (2026-04-13)

### New
- Add string-based import/export for build planner sharing
- Add ability to clone a build in the build planner
- Add sort order toggle for chat logs (newest/oldest first)
- Add 'Jump to Present' button in chat logs

### Fixed
- Removing the half-complete test that obviously doesn't pass
- Updating survey tests - they are still bad, but better bad
- Stupid arrows and their float value. blame archers
- Yet another attempt to correct survey reward logging
- Prevent survey session double-counting on zone change
- If you have no builds, now you can create one.

## v0.5.0 (2026-04-11)

### New
- Pricing Calculator for Projects

### Fixed
- Cleaned up some rust warnings
- Unbreaking survey data logging due to ring bonuses

### Improved
- Improved build planner  functionality with CPs and belts, item selection
- Build planner layout rework
- Clicking a watchword alert on the dashboard takes you to the watchwords page

## v0.4.8 (2026-04-09)

### Fixed
- Dashboard widget drag and drop
- In specific circumstances we were double-counting bonus levels
- Date lines on old chat messages don't cause ugly extra spacing anymore.
- Deletion confirmation not waiting for confirmation (thanks rey!)

### Improved
- Small dashboard layout tweaks
- Status widget more useful with more info. better layout, too
- Max height for dashboard cards before they get an internal scroll
- Character->Skills view now lists what bonus levels you have achieved, has a better layout, and provides more info
- Better patch notes organization

## v0.4.7 (2026-04-07)

### Fixed
- Market price list didn't scroll

## v0.4.6 (2026-04-07)

### Fixed
- Test fix to bypass false failure

## v0.4.5 (2026-04-06)

### Improved
- No visible changes — stability and maintenance update.

## v0.4.4 (2026-04-06)

### Improved
- No visible changes — stability and maintenance update.

## v0.4.3 (2026-04-03)

### Improved
- No visible changes — stability and maintenance update.

## v0.4.2 (2026-04-03)

### Improved
- No visible changes — stability and maintenance update.

## v0.4.1 (2026-04-03)

### Improved
- No visible changes — stability and maintenance update.

## v0.4.0 (2026-04-02)

### Improved
- No visible changes — stability and maintenance update.

## v0.3.1 (2026-03-31)

### Improved
- No visible changes — stability and maintenance update.

## v0.3.0 (2026-03-30)

### Improved
- No visible changes — stability and maintenance update.

## v0.2.2 (2026-03-29)

### Improved
- No visible changes — stability and maintenance update.

## v0.2.1 (2026-03-28)

### Improved
- No visible changes — stability and maintenance update.

## v0.2.0 (2026-03-27)

### Improved
- No visible changes — stability and maintenance update.

