# TODO

Small tasks and notes that don't belong in a dedicated plan.

- DONE: Mushroom Substrate recommendations — backend-owned reference table (`game_data/mushroom_farming.rs`, #113 Mortaferus fix).
- NPC widget refactor — **SUPERSEDED by GiftWatcher zone grouping (no tag field exists).** Favor feed is dark/hidden (`FavorChangesWidget.vue:1`); zone is the grouping proxy (`resolveNpcZone` keys on `area_friendly_name`; `NpcInfo` has no tags). Shipped: zone filter + grouping (#112 `6ed0be5`), bulk add-all in zone + giftable-quantity sort (#143 `f198166` — `GiftWatcherWidget.vue:270 addAllInZone`, `:403 groupedMatches` qty-desc sort).
- DONE: Better release notes — player-first pipeline shipped (`scripts/release_notes.py` classifier + `Release-Note:` trailer, `generate-changelog.py` humanizer, `release.py` triage editor + `--dry-run` + protected-main PR fallback; `release-notes/v1.13.0+.md` player-shaped).
- DONE: `package-lock.json` version stamp — `bump-version.py` syncs it via `npm version` primary + regex fallback (#122).
- DONE: Cooking search drill-in — FIXED 2026-11-07 (#142 `c2503fa`): Your-Skills result carries `skillName` through `App.vue pendingSkillSelection` into `SkillsScreen`, which clears hiding filters, expands the group, and scrolls the row into view.
- Release tooling hardening (from the v1.13.0 walk-through, 2026-11-05 — first `release.py` triage run hit every Windows trap at once):
  - DONE: CRLF writes in `release.py` — `newline="\n"` on both notes writes + triage temp (this branch).
  - DONE: triage temp `.md` → `.txt` + `shlex.split($EDITOR)` for multi-word editors (this branch).
  - DONE: `--dry-run` skips clean-tree + sync gates, preview-only warning off `main` (this branch).
  - DONE: branch-protection fallback — failed `main` push routes through `release/<tag>-notes` branch + PR (this branch).
  - DONE 2026-11-05: `release-notes/*.md` exempted from the markdown gate (`lint-markdown-changed.py in_scope`) — same class as `CHANGELOG.md` (generator-shaped `##` + `###` fragments CI consumes verbatim, not hand docs).

> **Signal contract (2026-11-03):** the 10/21 client removed verbose `LocalPlayer:` events from Player.log — that stream is dark and not coming back. Replacements for the dark stream arrive via richer character-report exports (Chat.log and game-written `Books/` files, e.g. `Books/PlayerShopLog_*.txt`, remain supported sources — the `hired` action below arrives that way). Entries below say *report-only* where they wait on a new report field, and *dropped* where the only source was the log. The 10/21-10/28 triage history (`Player.log game-event outage triage`, per-tracker casualty verdicts) is in git history, not repeated here.

## Deferred tuning (needs real-world data)

- [ ] rez-dedup 30s window calibration — see `docs/plans/deferred-tuning.md` (needs a captured rez pair; window still forward-only so backward-ordered pairs fail open; no fixture contains either phrasing; watch for duplicate `character_resuscitations` rows, cosmetic only).

---

## Product decisions (all four decided 2026-11-03 — no LocalPlayer: dependency)

- [x] ~~Rename-orphan history on Rename-Golem/server-merge~~ — **DECIDED + BUILT (option b, move): `rename_character` (`setup_commands.rs`, registered `lib.rs`) UPDATEs the shared `GAME_STATE_TABLES` + `SCOPED_TABLES` consts (+ snapshot parents, whose children follow) in one `BEGIN IMMEDIATE` tx. Refuses missing source and occupied target (UNIQUE guard — no silent merge); refuses when owner labels are ambiguous across servers, or when the target name has retained owner rows from a deleted character; active identity follows the rename. Delete reuses the same consts so the lists can't drift. FE lives in Settings → General → Characters (per-row Rename/move, same-name no-op, busy gates); session reload only when the renamed character is active. Caveat: game-written `Reports/Character_<Old>_*.json` files still name the old character — the next report scan re-imports them and recreates the old identity. Remove or re-export old files after renaming (surfaced in the rename success copy); the backend deliberately does not touch the game's report files.**
- [x] ~~"No stall rented" vs "rented, zero sales"~~ — **DECIDED + BUILT (option b, minimal): hire lines parse as `action='hired'` (`shop_log_parser.rs` `HIRED_RE`, owner-eligible hint). Flows through the Books-files path (`books_watcher.rs` reads game-written `Books/PlayerShopLog_*.txt` — no log line needed), surfaces in the Shop Log list, excluded from every sales/inventory aggregation. "Hired + zero bought" = rented-empty; zero rows = never rented.**
- [x] ~~No-Resell flag with no item-model counterpart~~ — **DECIDED: dropped.** The only observable source was `ProcessPlayerVendorScreen` listing data, which never carried listing contents — nothing to model. Static half already covered by the `NoPlayerShops` CDN keyword. `vendor_prices` SQL table (dead — sole repo hit was its own `CREATE TABLE`, zero readers/writers, vendor pricing lives in the in-memory `vendor_items_by_npc` index at `value × 1.5`) dropped in migration v85. Do not revive.
- [x] ~~Angling Perfect Catches as a new loot signal~~ — **DECIDED: dropped.** Nothing parses angling, no angling signal was ever captured, and the only transport was the log. Revisit only if a report field ever carries it.

## Character-scoped work (report-backed, actionable)

- [x] ~~Show work order completion state in tooltips for all characters~~ — **DONE: `QuestTooltip.vue:53` + `ItemTooltip.vue:172` render `Done:/Active:` via `characterStore.workOrderStateFor` (`:608`) backed by `get_all_character_work_orders` (`crafting_commands.rs:957`, `lib.rs:180`) over `character_active_quests` work-order categories.**

- [x] ~~Persist dropped report fields from character/inventory exports (added 2026-10-20; audited against `test_data/inventory/completeState/Character_TestPlayer_Dreva.json` + `TestPlayer_Dreva_items_2026-04-14-15-57-59Z.json`)~~ — **FIXED 2026-10-28: character half landed in v74; inventory half landed in v81 (4 of 5 keys) + deliberate `IngredientItemTypeIds` skip.**
  - **Character (Export Character as JSON in Settings → VIP): only `Skills.<Skill>.Abilities` was dropped — now persisted (migration v74, 2026-10-21).** Everything else top-level is stored and game-state-seeded (skills, recipe_completions, current_stats, currencies, NPCs→favor: `character_commands.rs:217-332,690-823`). Caveats, not drops: NPC entries carry only `FavorLevel` strings (384 in the fixture, no point totals — the seeder preserves `cumulative_delta` instead of overwriting, `:771-772`), and older exports predate `CompletedQuests` (key absent → `#[serde(default)]` → zero completed non-WO quests). Abilities land in `snapshot_skill_abilities(snapshot_id, skill_name, ability_key)` (`migrations.rs` v74) via the import write path (`character_commands.rs` §7b, deduped per skill, `None`/empty skipped), readable through `get_snapshot_skill_abilities(snapshot_id)` (`lib.rs`). Backfill decision: skipped — old reports simply have no rows; re-import the export to populate (`raw_json` already contains the arrays).
  - **Inventory (Export Storage as JSON in Settings → VIP): 4 of 5 dropped keys now persisted (migration v81, this branch).** `StorageItem` and `SnapshotItem` (`inventory_commands.rs:23-57,88-114`) carry `IsAugmented`, `DyeCount`, `Inscription`, `Inscriber`; v81 adds the columns and backfills them from stored `raw_json`, and the duplicate-import path refreshes them fill-only (`duplicate_import_backfill_never_clobbers_newer_values`). Only `IngredientItemTypeIds` stays unpersisted — deliberate (single-occurrence curiosity; brewing already scans it straight off `raw_json`, `brewing_commands.rs:405`). No UI owed.
  - **Unblocks:** the Skillbook autowatchwords prerequisite (Larger Efforts — its audit names the missing `snapshot_skill_abilities` table explicitly), an honest "Hide unlearned abilities" in `AbilityPickerDialog.vue:52` (currently a `level <= skill level` proxy, `:246-248`), and a true "My abilities" filter in `ModBrowser.vue:16` (currently scoped to the build's assigned bars).
  - **Effort: Trivial (inventory half landed; only deliberate `IngredientItemTypeIds` omission remains) | Impact: Low (closes the report audit)**

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

- [ ] UI/UX standardization: Table sweep done — **33 instances across 30 files** (`ux-standards.md` checklist owns it; was 35 — batch-1 removed the MarketView stall-observations table and the orphan LiveInventoryTab table; stall inventory rework removed the Recently Sold Out table; +1 Farming Sources report). Remaining: heading-class standardization (`typography-audit.md`), color Tier 3 cleanup (`color-standards.md`).
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
  - **Audit: re-scoped — four features across three states, two already shipped.** (a) **Work orders — shipped**, but from character export, not the log (`get_work_orders_from_snapshot`, `crafting_commands.rs:836`; `QuestsScreen.vue:167-214`). (b) **Active quests — shipped**, also from export (`character_active_quests`, `migrations.rs:2062-2070`; `QuestsScreen.vue:154`). (c) **Repeatables — partial**: a `isRepeatable()` flag from `ReuseTime_Minutes/_Days` (`QuestsScreen.vue:118-120`), no completion timestamps, so no cooldown state. (d) **Statehelm quests + report-based parsing — not started**: plan §1/§3 log-parsing steps dead; rewrite against the report when completion records land. Also stale: `docs/features/screens/character/character-quests.md` still describes the pre-snapshot two-panel screen.
  - **Effort: Large | Impact: High**
  - **Report-gated remainder:** (c) repeatable completion timestamps + (d) Statehelm quests wait on Character-export completion records (see "Blocked on VIP export fields"). Work orders + active quests shipped from exports. Plan §1/§3 log-parsing steps are dead — rewrite against the report when fields land.

- [ ] Interactive zone maps — see `docs/plans/interactive-maps.md`
  - kaeus is working on this i believe.
  - **Audit: verified as "no code exists" — drop the unverified Kaeus claim.** No map dependency (`package.json`), no map/zone-map component, no map assets; the plan is an unstarted spec with no phase markers. Zone-adjacent UI is survey analytics (`Surveying/AnalyticsZoneView.vue`, `CrossZoneComparison.vue`) and the Zone NPCs widget (`Dashboard/widgets/ZoneNpcsWidget.vue`) — not maps. Point at an external branch/PR if one exists.
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

- [ ] Skillbook autowatchwords
  - Automatically watch chat for item links of skill books the player could learn but doesn't own or know. Two modes: (1) books for currently-trained skills, (2) "future skill" mode where players select skills and we watch for any skillbooks they don't already own/know, even if skill level is too low to use them yet. **Investigation:** watchword system is fully operational (`watch_rules.rs`) with pattern matching and toast notifications. CDN has skillbook items with pattern `Skillbook_*`/`SkillBook_*` (e.g. `Skillbook_FoxInABox` with `AbilityRecipe` keyword). Implementation requires: (1) mapping skillbooks → skills via CDN items, (2) filtering against player's known skills, (3) auto-generating watch rules for desired skillbooks.
  - **Audit: verified — investigation accurate; the missing prerequisites are named.** Live: `watch_rules.rs:19 evaluate_rules`, `WatchRule`/`WatchCondition`/`WatchNotifyConfig` (`settings.rs:282-312`), toast+sound on `watch-rule-triggered` (`watchwordAlertStore.ts`), UI `WatchwordsView.vue`. CDN: `Skillbook_FoxInABox` (`items.json:19289`), `SkillBook_Headbutt` (`:62631`), 537 `BestowAbility` items. Landed 2026-10-21: `snapshot_skill_abilities` (migration v74) + `get_snapshot_skill_abilities` now cover plan Phase 1. Still absent: a `source`/auto field on `WatchRule`, so the plan's `source:"auto_skillbook"` tag is currently unsupported; no `get_skillbooks_for_skill`/`generate_skillbook_watch_rules`. Note: `refs/tags/archive/feat-books-watcher` holds a prior, unmerged attempt.
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

## Review backlog (from PR #54 frozen-review + CodeRabbit rounds; tracked, not punted)

- [x] ~~`loadAllCharacterItems` unkeyed cache~~ — **FIXED this branch: server key (`allCharacterItemsServer`) + generation guard (`fetchAllCharacterItems` re-reads on same-server invalidation, no rescan) + reset on imports + null-server guard (clears cache).**
- [x] ~~Ordinal-order divergence guard~~ — **DONE 2026-11-01: per-group file-vs-stored count guard in both ordinal paths (live `inventory_commands.rs`, one-shot `migrations.rs` v81) — divergent groups skipped with `eprintln`, surviving groups stay aligned; `duplicate_import_backfill_skips_divergent_group` regression test. No `debug_assert` (would trip in debug test builds); detection = skip + log. Full suite 791 green.**
  - **Effort: Small | Impact: Low (latent misattribution)**
- [x] ~~Multihit manual-use coverage~~ — **DONE 2026-11-01: `test_record_manual_use_multihit_queues_for_mining_adopt` (pending-use queue + node open + chain + `PendingLoot`); full suite 791 green.**
  - **Effort: Small | Impact: Low (untested path)**
- [x] ~~Stale-FK transparent retry~~ — **DONE 2026-11-01: `record_manual_use` loops the tx + `insert_use` once on FK failure (drop cache, re-resolve/auto-start fresh, re-insert); `test_record_manual_use_retries_after_session_deleted_midflight`. Full suite 791 green.**
  - **Effort: Small | Impact: Low (UX papercut)**
- [x] ~~`startSession`/`endSession` error surfacing~~ — **DONE 2026-11-01: dedicated `sessionError` ref (separate from record-use `recordError`) set in both catch arms, exported, rendered under Start button; `vue-tsc` clean.**
  - **Effort: Small | Impact: Low**
- [x] ~~Debug-capture + login-skill-list `snapshot` wording~~ — **CLOSED 2026-11-03: kept deliberately — debug-capture `GameStateSnapshot` (point-in-time DB dump at capture start/stop, `debug_capture.rs:41`) and login `SkillsLoaded`/`SkillSnapshot` (in-memory full skill dump seeding the running table, `player_event_parser.rs:55-58`) are transient live-state captures, not persisted VIP export reports (`character_snapshots` / `character_item_snapshots`). Reopen only if users confuse the terms.**
  - **Effort: Trivial | Impact: Nil (decision record)**
- [ ] Tier-2 identifier rename (`SnapshotItem`, `get_snapshot_items`, `source='snapshot'`, table names) — explicitly out of scope for the tier-1 pass. A breaking IPC/DB migration; needs its own plan if ever wanted.
  - **Effort: Large | Impact: Low (cosmetic, high blast radius)**
- [x] ~~Scoped Rust discard lint for migrations~~ — **FIXED: `scripts/check-review-patterns.py` gates `db/migrations.rs` bodies against `let _ =` on DB handles and `filter_map(|x| x.ok())` row reads; wired into `lint.yml` + `mise run lint`.** Rustfmt stays advisory. Sanctioned exceptions: `EXISTS` guards with `.unwrap_or(false)`, serde defaults, `continue` on malformed JSON.

- [x] ~~Clippy advisory baseline~~ — **FIXED this branch: all warnings resolved (110 lib + ~60 test-only), `cargo clippy --all-targets -- -D warnings` green; wired as a gate (`lint.yml:rust-tests` clippy step + `mise run lint` entry, portable `python -c` form). AGENTS.md Rust row updated (rustfmt stays advisory).**
  - **Effort: Medium (tombstone table + delete-path + backfill filter) | Impact: Low (narrow race window) — declined**

## Small fixes (verified open)

- [x] ~~Roulette number CHECK~~ — **FIXED 2026-10-26: migration v77 (table rebuild per v38 precedent, out-of-range rows purged, `CHECK (number BETWEEN 0 AND 36)`); `record_roulette_result` rejects >36, backfill skips bad rows.**
- [x] ~~NpcResolve case-insensitive match~~ — **FIXED 2026-10-26: `COLLATE NOCASE` + trim at both sites; mixed-case test.**
- [x] ~~Trip-router nits~~ — **FIXED 2026-10-26: case-insensitive `casino_portal`, unresolved-name `eprintln`, `AreaDesert2` friendly name, `AreaCaves1`→Eltibule-dungeon relabel, score-`1`/monster-skip docs, `trip-routing.md` refresh.**
- [x] ~~Stall aggregation validation~~ — **FIXED 2026-10-26: trigram empty-set identity, OCR/chrome/price gate docs, day-aware year wrap + single-wrap limit, aggregation items documented as by-design, `StallScanRow.quantity: None` by-design.**
- [x] ~~Shop-log docs + arena payout trigger~~ — **FIXED 2026-10-26: stacked-path comment narrowed, BOM symmetric, owner defaults to active character; result-line resolve documented as the payout trigger.**
- [x] ~~Chat-status parity~~ — **FIXED 2026-10-26: wisdom comma-grouping, timestamp digit gate, `try_level_up` empty-skill reject; steal-anchor + empty-arm guards verified already present.**
- [x] ~~setup/gst validation~~ — **FIXED 2026-10-26: blank name/server rejected with column named; GST 256MiB cap + integrity-limit docs.**
- [x] ~~ParseFixB read-error follow-up~~ — **DECIDED 2026-10-26: keep `continue` (one undecodable line ≠ dead file); recorded at site.**
- [x] ~~stall_ocr edge + year wrap~~ — **FIXED 2026-10-26 (see stall validation line).**
- [x] ~~game_state_commands audits~~ — **FIXED 2026-10-26: `get_currency_estimate_for` split + anchor/delta test; normalization + fetch + traversal audits as code comments.**
- [x] ~~Scan-path Status dispatch loss~~ — **FIXED 2026-10-26: shared `chat_status_dispatch` module (live drain + all three scan/tail paths run the same arms with per-session identity; re-scan inserts nothing). ReportSaved intentionally excluded on scans (path may not exist locally); survey rows stay untagged for retro-claim.**

## UI backlog

- [ ] Portrait-mode / narrow-window friendly (user-requested 2026-10-28): closable panels/dialogs can end up off-screen with no reachable close affordance when the app gets narrow. Scope: audit dialogs/overlays/drawers for min-width/overflow traps; keep a close control reachable at narrow widths (responsive layout, scrollable content, sticky/fallback close, Esc handling).
  - **Batch-1 DONE 2026-11-01 (close/Esc/clamps, `vue-tsc` clean):** Esc on `ModalDialog` confirm + 4 MarketView dialogs + `BreweryRecipeDetail` project dialog + `AbilityPickerDialog`/`ImportEquippedDialog` (window-level, focus-safe) + `QuickSearchOverlay` (window-level, footer promises it) + `CdnUpdateModal`→`dismiss`; viewport clamps on `DatePicker`/`SearchableSelect`/`StyledSelect`/`ItemMinicard` popups; `max-h-[85vh]` scroll guards on Market/Brewery/`CdnUpdateModal`; toast `max-w-[min(350px,calc(100vw-2rem))]`. **Batch-2 DONE 2026-11-07 (#144): `ItemPickerDialog` stacks portrait-narrow (preview capped 45%, Select pinned) + window-level Esc.** Remaining: `DataBrowserOverlay` header/sidebar collapse, `HelpOverlay` tab rail — layout reworks needing visual proof.**
- [x] ~~NPC gift calculator rework (user-requested 2026-10-28)~~ — **DONE 2026-10-30 (batch-1): option (a) — deleted `NpcGiftCalculatorSection.vue` + `NpcDetailPanel.vue` wiring; sibling sections untouched.**

- [x] ~~Combine XP Rate + Prodigy Tracker into one widget (user-requested 2026-10-28)~~ — **DONE 2026-10-30 (batch-1): dropped `xp-rate` registry entry + deleted `XpRateWidget.vue`; one-time `DashboardView.vue` prefs migration strips `xp-rate` from `cardOrder`/`hiddenCards`/`cardSpans` and unhides `prodigy-tracker` if the user would otherwise lose the widget. Shared `xpRateSession`/`xpRateOf`/`prodigyEta` store fns unchanged.**

- [x] ~~Stray "???" still rendering (user-requested 2026-10-28)~~ — **DONE 2026-11-01: no stray found in prod code — sole `???` was the intentional market-missing affordance (`ItemMinicard.vue:29-33`), relabeled to `set price` (+ `title`); wealth-view `???` names already purged backend-side (`b6d26d4`); `DebugCaptureTab.vue:199` is dev-panel-only. `LootOverviewPanel.vue` comment updated.**

- [x] ~~Stall-price capture back to backlog, remove experimental surface (user-requested 2026-10-28)~~ — **DONE 2026-10-30 (batch-1): deleted FE (`StallPriceCapturePanel.vue`, `stallPriceStore.ts`, `ocrStore.ts` sole-consumer, `stallPrices.ts` types), BE (`db/stall_commands.rs`, `stall_ocr.rs`, 8 handler registrations, coordinator auto-detect + `record_purchase_observation`), `ItemTooltip` hookup. Kept migration v68 table + rows. Note: OCR stack fully removed (no other consumers — zero `ocrStore`/`stall_ocr` refs remained); re-introduction needs the OCR path rebuilt too.**

- [x] ~~Dynamic Items tab slow initial load (user-requested 2026-10-28)~~ — **DONE 2026-11-01: `get_recipe_ingredient_keywords` now returns `item_count` per keyword (single-pass count over the catalog, same case-insensitive match; `cdn_commands.rs:440-496`); FE seeds `itemCountCache` from the payload, preload loop deleted (`DynamicItemsTab.vue:222-228`). 1 IPC instead of 1+N; item lists still lazy-load per selection. `cargo check` + `vue-tsc` clean, 35 cdn tests green.**

- [x] ~~Hide Live inventory tab outright (user-requested 2026-10-28)~~ — **DONE 2026-10-30 (batch-1 fix batch): tab wiring was already gone (`InventoryWrapper.vue` has only `storage/vaults/consolidate`, zero importers) — deleted the orphan `LiveInventoryTab.vue` component outright; `LiveInventoryItem` game-state map retained for re-enable. Inventory docs (`inventory.md`, `inventory-live.md`) updated.**

- [x] ~~NPC list from CDN, report as overlay (user-requested 2026-10-28)~~ — **DONE 2026-11-01 (verified in-tree: `f6a5538 NPC list goes CDN-first`; `NpcsScreen.vue:122` `cdnNpcKeys()` from `gameData.npcsByKey`, `NpcFilterPanel.vue:252` CDN-universe pass, orphans counted card-less).**
