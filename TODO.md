# TODO

Small tasks and notes that don't belong in a dedicated plan.

Completed slices (fanout 2026-11-08 — PRs #152/#153/#154, all MERGED to main): WatchRule nullable `source` tag for auto_skillbook rules (`settings.rs`, `WatchwordsView.vue`; #152); character-quests screen doc refreshed to the 5-tab snapshot model (`character-quests.md`; #153); vendor context-menu Esc routed via shared overlay stack, poems Esc stays intentionally local (`VendorCouncilWidget.vue`, `PoemsView.vue`; #154).
Completed slices (fanout 2026-11-09 — PRs #159/#160/#161, all MERGED to main): tooltip/shared micro text sizes consolidated to type scale (`SkillCard.vue`, `SourcesPanel.vue`, `ItemTooltip.vue`, `RecipeTooltip.vue`, `QuestTooltip.vue`, `ShelfChip.vue`, `ItemCostCalculator.vue`; #159); chart palette + theme reader deduped into shared `useChartTheme.ts` (`SkillsTab.vue`, `LootDonutChart.vue`, `StallTrendsTab.vue`; #160); poem detail arrow keys gated on overlay Esc stack (`PoemsView.vue`; #161).
Completed slices (2026-11-09 — PRs #163/#164, all MERGED to main): rez dedup narrowed to cross-phrasing pairs (same-phrasing repeats always record; `last_rez_dedup` is now `(timestamp, caster)`; #163, grounded in 122-file Chat.log census: 150 resuscitate targets, 0 cross-phrasing pairs); `Sent N coins to <player>` parsed as wallet loss with negative-amount rejection (closes the last frequent GOLD line shape; #164).
Completed slices (fanout 2026-11-09 — PRs #166/#167/#168/#169, all MERGED to main): chart tooltip chrome read from theme tokens at runtime (`useChartTheme.ts` themeTooltipBg/Border, `SkillsTab.vue`, `LootDonutChart.vue`, `StallTrendsTab.vue`; #167); surface hexes migrated to theme tokens in 6 components (`HistoricalTab.vue`, `ItemTooltip.vue`, `SkillBrowser.vue`, `SkillDetailPanel.vue`, `ProjectRecipePanel.vue`, `FarmingSessionCard.vue`; #168); `ChatMessage.vue` sender + unknown-channel fallback migrated to theme tokens, fallback lifted to `text-secondary` for hover contrast (#166 + contrast fix); `get_skillbooks_for_skill` query over new `skillbooks_by_skill` CDN index with Unknown-bucket default for unresolvable skills (`game_data/mod.rs`, `cdn_commands.rs`, `lib.rs`; #169 — plan Phase 2 first half; still absent: `get_missing_skillbooks`/`generate_skillbook_watch_rules`).

> **Signal contract (2026-11-03):** the 10/21 client removed verbose `LocalPlayer:` events from Player.log — that stream is dark and not coming back. Replacements for the dark stream arrive via richer character-report exports (Chat.log and game-written `Books/` files, e.g. `Books/PlayerShopLog_*.txt`, remain supported sources — the `hired` action below arrives that way). Entries below say *report-only* where they wait on a new report field, and *dropped* where the only source was the log. The 10/21-10/28 triage history (`Player.log game-event outage triage`, per-tracker casualty verdicts) is in git history, not repeated here.

## Deferred tuning (needs real-world data)

- [ ] rez-dedup 30s window calibration — see `docs/plans/deferred-tuning.md` (bidirectional `abs()<=30s` window shipped in `rez_within_dedup_window`; still needs a captured rez pair to calibrate 30s vs 60s; no fixture contains either phrasing; watch for duplicate `character_resuscitations` rows, cosmetic only).

---

## Product decisions (decided 2026-11-03 — in git history, pruned 2026-11-08)

Rename-orphan move, stall-hired surfacing, No-Resell drop, angling drop: all decided + built in history. Live caveat: rename leaves game-written `Reports/Character_<Old>_*.json` files in place; re-export or remove old files after renaming (surfaced in the rename success copy).

## Character-scoped work (report-backed — landed, pruned 2026-11-08)

WO-completion tooltips + dropped-field persistence (v74/v81) shipped in history. Live caveats: `IngredientItemTypeIds` stays unpersisted (deliberate — brewing scans `raw_json`); "Hide unlearned abilities" (`AbilityPickerDialog.vue:52`) is a `level <= skill level` proxy; "My abilities" (`ModBrowser.vue:16`) is scoped to assigned bars.

## Blocked on VIP export fields (game writes the export, we import it)

- [ ] Current gear display — **report-gated, not log-gated.** The `ProcessUpdateItemCode` appearance-key path is dead, but the replacement is an export field, not a parser: per-slot equipped item identity in the Character export. Report already carries full inventory; only worn-vs-carried is missing. Appearance-only view built and reverted.
  - **Unblocks when:** a per-slot equipped-item field lands in the Character export. **Effort: unknown (game-side) + Small (import + view) | Impact: Medium**
- [ ] Quest tracking: repeatable-cooldown timestamps + Statehelm quests + renown possible-vs-earned — **report-gated.** Work orders + active quests shipped from exports (`get_work_orders_from_snapshot`, `crafting_commands.rs:836`; `character_active_quests`, `migrations.rs:2062-2070`); repeatables carry only `isRepeatable()` from `ReuseTime_Minutes/_Days`. Renown is a context-bar currency balance only. The log-parsing steps of `docs/plans/quest-tracking.md` §1/§3 are dead; rewrite against the report when fields land.
  - **Unblocks when:** the Character export gains quest-completion records (completion timestamps per repeatable; Statehelm quest completions; renown-possible schedule). **Effort: Large | Impact: High**
- [ ] Gardening helper Phase 1 (plot tracker) — see `docs/plans/gardening-helper.md:52-56`. `EntityDescriptionUpdated` log path is dead, but the feature is plot *state*, which an export field can carry. Plan written, garden almanac (Phase 0) complete. Missing: a `garden_plots` source, `get_garden_plots` command, widget tab.
  - **Unblocks when:** the Character export (or another report) gains garden-plot state (area, crop, growth stage per plot). Phase 0 almanac stays. **Effort: Medium | Impact: High**
- [ ] Vendor purchase receipts — see Analytical features below. The ledger path is dead with the log; the replacement is an export purchase log, not a parser. Tracked as an analytical feature, not here.
- [ ] Nightmare cave challenge door tracker — **report-gated.** The `ProcessAddEffects`/`ProcessRemoveEffects`/`ProcessUpdateEffectName` log path is dead, but the replacement is a static identifier, not a stream: a challenge-door ID in fixtures, CDN, or a report. Effect/CDN fixture research stays actionable (`effect_12602-12608` Spawn-Premonition family, `effects.json`); Letters of Authority via inventory reports.
  - **Unblocks when:** a challenge-door identifier appears in fixtures, CDN, or a report. **Effort: Large (research + implementation) | Impact: Medium (niche but useful)**

## Dropped with the log (no report path — dead unless the game re-emits)

- Per-attacker damage on a specific monster (other players' outgoing attacks were never logged: 215 own / 1 pet / 0 other). Reverted build is the template if a source ever appears.
- Angling Perfect Catches (decided above — no signal ever captured).
- Log-based quest parsing, log-based completion signals, live door-identifier capture (folded into the report-only entries above).
- Session mining-yield stats (needs `ProcessStartInteraction` + `ProcessDoDelayLoop` mining labels + `ProcessAddItem`, all `LocalPlayer:`; node identity is positional, no export field could carry it). Session survey yield survives via chat-backed attribution (`survey_chat` rows from Status `ItemGained` lines).
- Kill-loot denominators for new kills (corpse-search rows are `LocalPlayer: ProcessTalkScreen` lines). Existing rows stand; chat `EnemyKilled` keeps only the live session UI.
- Inventory miscount causes (1)–(3) (chat-correlation ±2s window, `pending_deletes` reorder, catch-up stack-correction ordering) and gamestate login-batch residual (inventory dump flag): fixture-replay-only value, no live effect. Cause (4) already fixed (live tailing clears the inventory family on login, `game_state.rs:176-201`). Keep the Critical Resources warning text as-is.

## Cross-cutting

- [ ] Ingestion architecture: no shared schema contract for Tauri event payloads (no specta/ts-rs/typeshare). Coordinator is a manual dispatch hub; history tables exist per-domain except `game_state_*`. Future re-source designs belong here.
  - **Effort: Medium | Impact: Medium (reliability)**

- [ ] UI/UX standardization: Table sweep done — **33 instances across 30 files** (`ux-standards.md` checklist owns it; was 35 — batch-1 removed the MarketView stall-observations table and the orphan LiveInventoryTab table; stall inventory rework removed the Recently Sold Out table; +1 Farming Sources report). Remaining: heading-class standardization (`typography-audit.md`); color Tier 3 leftovers DONE 2026-11-09 except the `text-[10px]` scale decision — chart tooltip chrome reads theme tokens at runtime (`useChartTheme.ts` themeTooltipBg/Border; #167), surface hexes migrated to tokens in 6 components (#168), `ChatMessage.vue` sender + fallback on theme tokens (#166; timestamp/empty `text-gray-500` left intentionally, no matching token). Done 2026-11-09: tooltip/shared/surveying micro sizes consolidated to type scale (#159); chart palette + theme reader deduped into `useChartTheme.ts` (#160). Open backlog from #159 review: fixed `text-[10px]` captions don't scale with Interface Size — needs a scale-owner decision (reconcile audit scale with root-font scaling, migrate all ~400 sites together).
  - **Effort: Medium (iterative) | Impact: Medium**

---

## Analytical features (report/chat-backed)

- [ ] Item provenance downstream features
  - **Audit: re-scoped — ledger halves are LocalPlayer:-gated, two features survive.** Mining/survey session yield dies with the log (`ItemAdded`/`ItemStackChanged` in `farmingStore.ts:814-840` are `LocalPlayer:` events). What survives: kill-loot-by-mob is already shipped **outside** provenance (`enemy_kills`/`enemy_kill_loot` denominators come from corpse-search rows — themselves `LocalPlayer:`-gated `ProcessTalkScreen` lines, but the shipped feature stands on existing rows; the chat `EnemyKilled` arm at `chat_status_dispatch.rs:265-268` feeds only the live farming-session UI). Ledger-based craft yield survives via the report (`HistoryTab` reads `game_state_recipes.completion_count`, seeded from the Character export `character_recipe_completions` → `character_commands.rs:855`, `source='snapshot'`; only the `ProcessUpdateRecipe` live-delta path is gated). Unknown/uncertain-source report partially survives (chat-backed `ItemGained` Status lines still land via `insert_item_transaction_guarded`, `:516`; Player.log-pipeline `UnknownSource`/`uncertain` rows stop).
  - **Effort: Medium per feature | Impact: Medium-High (analytics depth)**
- [ ] Vendor purchase receipts — **report-gated.** The `source_kind='vendor_browsing'` ledger path is dead with the log (stamp needs `ProcessVendorScreen` + `ProcessAddItem`, both `LocalPlayer:`). Revives if the Character export gains a purchase log (item, qty, price, timestamp per buy). Until then the surviving signal is the per-NPC `councils_spent_at_vendor` snapshot anchor (`character_npc_favor`, `character_commands.rs:308`) — a point-in-time balance, already shipped.
  - **Effort: Medium | Impact: Medium**

- [ ] Boss kill loot timers
  - Reverse tracking exists (`chat_combat_parser.rs:9-30` `PlayerDeath`/`EnemyKilled`/`DamageOnPlayer`; `EnemyKilled` → `enemy-killed` → `farmingStore.handleEnemyKilled`; corpse-search drop attribution via `coordinator.rs:1910`, migration v49). Combat Wisdom cooldowns shipped (`CombatWisdomWidget.vue:132-176`, `combat_wisdom_commands.rs:8-11`). Genuinely missing: **boss/named classification** (no log class signal) and per-boss timer/lockout UI. A heuristic + UI item, not a parser extension.

---

## Larger Efforts / Research Needed

- [ ] Quest tracking system (work orders, repeatables, Statehelm, active quests)
  - **See `docs/plans/quest-tracking.md`** — consolidated plan covering quest event parsing, repeatable cooldown tracking, work order cooldowns, Statehelm quest tracking, and active quest browsing. Prerequisite (report-era): Character-export quest-completion records — the plan's log-parsing steps (§1/§3) are dead.
  - **Audit: re-scoped — four features across three states, two already shipped.** (a) **Work orders — shipped**, but from character export, not the log (`get_work_orders_from_snapshot`, `crafting_commands.rs:836`; `QuestsScreen.vue:167-214`). (b) **Active quests — shipped**, also from export (`character_active_quests`, `migrations.rs:2062-2070`; `QuestsScreen.vue:154`). (c) **Repeatables — partial**: a `isRepeatable()` flag from `ReuseTime_Minutes/_Days` (`QuestsScreen.vue:118-120`), no completion timestamps, so no cooldown state. (d) **Statehelm quests + report-based parsing — not started**: plan §1/§3 log-parsing steps dead; rewrite against the report when completion records land. Screen doc refreshed to the 5-tab snapshot model 2026-11-08 (PR #153).
  - **Effort: Large | Impact: High**
  - **Report-gated remainder:** (c) repeatable completion timestamps + (d) Statehelm quests wait on Character-export completion records (see "Blocked on VIP export fields"). Work orders + active quests shipped from exports. Plan §1/§3 log-parsing steps are dead — rewrite against the report when fields land.

- [ ] Interactive zone maps — see `docs/plans/interactive-maps.md`
  - No code exists yet (no map dependency, component, or assets); the plan is an unstarted spec. Point at an external branch/PR if one exists.
  - **Effort: Large (mostly frontend) | Impact: High**

- [ ] Statehelm favor planner
  - Looks at storage vaults, finds appropriate gifts for Statehelm NPCs, creates a route to pick everything up before delivering. Respects remaining gift count needed per NPC that week. Combines storage data + gift preferences + route planner.
  - **Audit: verified open — and a full design already exists at `docs/plans/statehelm-favor-planner.md`.** All three inputs are built: vault occupancy (`gameStateStore.ts:481-511`), gift preferences (`npc_favor_by_item_name`/`_keyword`, `game_data/mod.rs:1069-1090`; `get_npcs_wanting_item`, `lib.rs:118`), routing (`trip_router::plan_trip`). Nothing implements the planner itself (0 hits for `plan_statehelm_gifts`, `useStatehelmPlanner`, `GiftPlanner`).
  - **Effort: Large | Impact: High (cross-system planning)**

- [ ] Work order route/crafting planner
  - Looks at all available work orders, shows where to go/craft/turn-in in optimal order using route planner. Depends on quest tracking system for work order state.
  - **Audit: re-scoped — the crafting half shipped; only route ordering is missing.** Uncompleted WOs → `craftingStore.createProjectFromWorkOrders` (`craftingStore.ts:1593`) already produces a project with material/pickup/shopping lists. The router knows the purposes (`trip_router.rs:34-36`, `Craft`/`TurnIn`) but nothing builds work-order stops — `plan_trip` callers are `TripPlannerWidget.vue:317` and `ConsolidateTab.vue:283`; `docs/features/trip-routing.md` lists WO fulfilment as unwired Phase 3.
  - **Effort: Large | Impact: High (cross-system planning)**

- [ ] Quest turn-in helper
  - Looks at storage/inventory and active quests to find completable quests. Suggests pickup and turn-in routes using route planner. Depends on quest tracking system.
  - **Audit: verified open.** Nothing correlates quest objectives against storage/inventory, and no caller supplies quest stops (`StopPurpose::TurnIn` exists unused at `trip_router.rs:35`). Only the Active/Uncompleted lists (`QuestsScreen.vue:154,216`) are in place.
  - **Effort: Large | Impact: High (cross-system planning)**

- [ ] Track total owned quantity changes over time
  - Start with currencies, expand to other items. Currencies may be in character JSON, inventory JSON, or both. PG doesn't overwrite inventory reports in the reports folder — could backfill historical data from old reports. Character JSONs are overwritten on export though. **Infrastructure gap:** database supports point-in-time reports only (`character_item_snapshots`). `sales_history` tracks vendor transactions but no time-series quantity tracking exists. Needs: new tables for quantity history, periodic delta recording (event-driven or report-based), UI for time-range queries and charts.
  - **Audit: re-scoped — prose corrected.** The gap is real (0 hits for `quantity_history`), but "point-in-time reports only" is wrong: the ledger/series family already exists — `item_transactions`, `sales_history`, per-import `character_currencies` (`migrations.rs:2050-2058`), `character_item_snapshots`+`character_snapshot_items` (`import_date` indexed), plus the per-domain history tables. Also wrong: currencies are **character-export-only** (no inventory path) — the plan's own "Currency Data (Resolved)" section says so. `currency_estimate` (v56) is an anchor+delta estimate, not a history series. Both plan docs (`owned-quantity-tracking.md`, `quantity-tracking-over-time.md`) have no corresponding code.
  - **Effort: Large (new infrastructure) | Impact: High (trend data is very valuable)**
  - **Data collections 2026-11-09 (user-confirmed):** (1) all frequent GOLD chat shapes parse (`try_councils_misc` + `try_councils_changed` + `try_coins_looted`; only Live Event Credits unparsed — non-GOLD, no feature needs it); (2) auto-export ON (character-only, fires on zone change + logout) + 1–2 manual exports/session (both files) — currency anchors dense, chat accrual only bridges minutes-to-hours; (3) Reports holds 7 `*_items_*.json` (Nov 4→today, append-only — item backfill viable) + 2 overwritten `Character_*.json`; note `Character__Dreva.json` (empty character name, 2026-11-09) — possible unnamed-anchor edge if a gap ever appears.

- [ ] Skillbook autowatchwords
  - Automatically watch chat for item links of skill books the player could learn but doesn't own or know. Two modes: (1) books for currently-trained skills, (2) "future skill" mode where players select skills and we watch for any skillbooks they don't already own/know, even if skill level is too low to use them yet. **Investigation:** watchword system is fully operational (`watch_rules.rs`) with pattern matching and toast notifications. CDN has skillbook items with pattern `Skillbook_*`/`SkillBook_*` (e.g. `Skillbook_FoxInABox` with `AbilityRecipe` keyword). Implementation requires: (1) mapping skillbooks → skills via CDN items, (2) filtering against player's known skills, (3) auto-generating watch rules for desired skillbooks.
  - **Audit: verified — investigation accurate; the missing prerequisites are named.** Live: `watch_rules.rs:19 evaluate_rules`, `WatchRule`/`WatchCondition`/`WatchNotifyConfig` (`settings.rs:282-312`), toast+sound on `watch-rule-triggered` (`watchwordAlertStore.ts`), UI `WatchwordsView.vue`. CDN: `Skillbook_FoxInABox` (`items.json:19289`), `SkillBook_Headbutt` (`:62631`), 537 `BestowAbility` items. Landed 2026-10-21: `snapshot_skill_abilities` (migration v74) + `get_snapshot_skill_abilities` now cover plan Phase 1. Landed 2026-11-08 (PR #152): nullable `source` field on `WatchRule` (serde default), so `source:"auto_skillbook"` is now supported as metadata. Landed 2026-11-09 (PR #169): `get_skillbooks_for_skill` over new `skillbooks_by_skill` CDN index (Phase 2 first half; unresolvable skills default to the Unknown bucket). Still absent: `get_missing_skillbooks`/`generate_skillbook_watch_rules`. Note: `refs/tags/archive/feat-books-watcher` holds a prior, unmerged attempt.
  - **Effort: Large | Impact: High (proactive skill progression)**

- [ ] Write 'how to use' docs for each screen
  - Even brief docs for each screen would help users understand features. Could be in-app help or docs folder.
  - Need to have some standard place in the panel layout for a button or something, perhaps, that explains how to use it. maybe top right of the panel view? we will need a way to turn it off/on (not all panel layout consumers will want it) and then how to define what it contains.
  - **Audit: re-scoped into two halves.** (a) *Docs folder*: **69** markdown files exist under `docs/features/screens/`, but they are developer/architecture references (0 hits for a usage/how-to heading), so the end-user half is untouched — decide whether that counts. (b) *In-app affordance*: specified in `docs/plans/in-app-help.md` (PaneLayout `helpKey` prop + `src/components/Shared/ScreenHelpPanel.vue` + `useScreenHelp.ts` + a `src/help/` registry) and **does not exist** (0 hits for `helpKey`/`ScreenHelp`; `PaneLayout.vue` takes only `screenKey/leftPane/rightPane`). App-level help does exist (`components/Help/*`, `?` button at `MenuBar.vue:86-90`).
  - **Effort: Large (breadth) | Impact: Medium (onboarding/discoverability)**

- [ ] Reevaluate test suite
  - Think about what tests make sense, what isn't giving value, and how to harden against future failures.
  - need to figure out if we can build some sample data or something that makes sense for tests.
  - **Audit: re-scoped — the sample-data ask is done; only hardening remains.** `test_data/` now holds **8** replay datasets under `surveyLogs/` (`Player.log` + `Chat.log`; 6 of them also carry a hand-recorded `results.txt`, and those 6 are the set the accuracy report iterates — `survey/replay_tests.rs:985-1034`) plus inventory/login-state fixtures, exercised by `survey::replay_tests`. Census: **709 `#[test]` across 57 files, 6 `#[ignore]`** (5 replay + 1 chat backfill); no integration dir, no JS tests. Slow suites: `survey::replay_tests`, `cdn_items_load_smoke`. (Counts match `AGENTS.md`; `cargo test --lib` reports 703 passed + 6 ignored.)
  - **Effort: Large | Impact: Medium (reliability/confidence)**

- [ ] Analyze what should move from frontend to Rust
  - Some frontend logic may be better served on the Rust backend. Needs analysis to identify candidates.
  - **Audit: verified open — but the candidate list is now identifiable.** Largest TS logic: `craftingStore.ts` (1746 lines — recursive ingredient tree + material aggregation with per-node IPC and hand-rolled caches), `gameStateStore.ts` (1301), `buildPlannerStore.ts` (951), `useStorageConsolidation.ts` (944 — cross-alt vault math), `useRecipeCost.ts` (per-ingredient recursion + IPC). The target pattern already exists in-tree: `aggregateStore.ts` (thin IPC shim) and `useAbilityBuildStats.ts` ("the math lives in the Rust `compute_ability_build_stats` command").
  - **Effort: Large (research) | Impact: Medium (performance/architecture)**

## Review backlog (PR #54 / CodeRabbit rounds — closed, pruned 2026-11-08)

Cache guards, ordinal divergence guard, multihit/Stale-FK/error-surfacing fixes, discard + clippy gates: all landed in history.

## Small fixes (Oct batch — landed, pruned 2026-11-08)

Roulette CHECK, NpcResolve NOCASE, trip-router nits, stall aggregation, shop-log/arena, chat-status parity, setup/GST validation, ParseFixB, scan-path dispatch: all landed in history.

## UI backlog

- [ ] Portrait-mode / narrow-window friendly (user-requested 2026-10-28): closable panels/dialogs can end up off-screen with no reachable close affordance when the app gets narrow. Scope: audit dialogs/overlays/drawers for min-width/overflow traps; keep a close control reachable at narrow widths (responsive layout, scrollable content, sticky/fallback close, Esc handling).
  - **Batches 1–3 DONE (in history, pruned 2026-11-08).** 390px visual proof pass DONE 2026-11-09 (verify-only slice, no commit — full screen tour at 390px via CDP, every closable keeps a reachable close; DataBrowser header `overflow-x-auto` + scrollIntoView + Esc + backdrop = reachable, not a trap; no redesign). Esc-stack scoping for the vendor context menu DONE 2026-11-08 (PR #154 — menu routes via shared stack; poem detail Esc intentionally local). Poem arrow-key stack-gating DONE 2026-11-09 (PR #161 — arrows yield when an overlay sits above; Esc stays local).

## Backlog audits (2026-11-09)

- [ ] Help Known Issues list — audit every entry against current behavior; remove fixed, reword stale.
- [ ] Settings text and entries — audit for stale copy and already-implemented items; remove or update.
- [ ] Hopology widget "to study" list — users report studied pieces still listed; verify studied-state tracking and fix.
- [ ] LocalPlayer collection routes — remove old vestiges; VIP reports are the only future source for that data.
