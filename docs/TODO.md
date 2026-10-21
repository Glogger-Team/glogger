# TODO

Small tasks and notes that don't belong in a dedicated plan.

*Last reviewed: 2026-10-13* — **audited end-to-end the same day** against the tree at `e103595` (v1.2.0).
Every item carries an inline **Audit 2026-10-13** verdict with `path:line` evidence:

- **closed** — shipped, or the question is already answered elsewhere.
- **re-scoped** — partly shipped, or the item's own stated cause/blocker/counts were wrong (the item stays open only for the residual work named in the note).
- **verified** — the item is accurate as written and still unimplemented.

**Tally: 26 checkbox items open at review → 3 closed, 13 re-scoped, 10 verified**, plus one new item (the
narrowed Cook's-Helper defect that replaced the closed recipe-learning investigation — since fixed and closed) and one re-homed item
(the appearance-only equipment view, promoted out of the reference notes below), plus one item **added 2026-10-15** from the
character-attribution review (the catch-up guard's own accepted cost — see *Closed*; fixed the same day) — **24 open, 8 closed now**.
The five Investigation reference notes carry no `- [ ]` checkbox, so they were never in any of these counts;
three remain, all blocked outside the repo, after one closed and one was promoted to live work. The biggest
misses were the casino bet tracker (fully shipped; its plan doc previously said "not yet built"), the
macro/process-interaction question (already answered by the hard-lines research doc), and recipe learning
(the live path exists — the real defect is narrower). Everything closed — including the two older
resolutions — is collected in **Closed — kept for the record** at the end; everything above is either live
work or settled research blocked outside the repo.

---

## Deferred tuning (needs real-world data)

- [ ] One item left for real-log tuning — see `docs/plans/deferred-tuning.md`:
      rez-dedup 30s window calibration (needs a captured rez pair; the window is
      still forward-only, so backward-ordered pairs fail open). The other two are
      closed in that file: brewing bulk-scan memory **measured 1.79 MB total** on the
      dev DB (2026-10-13 — leave as-is), and the ChatMessageList day-jump scroll race
      was **superseded** by the Session 38 day-log rewrite.
  - **Audit 2026-10-13: verified.** `rez_within_dedup_window` is still `WINDOW = 30s`, forward-only
    (`coordinator.rs:3823-3833`), and **no fixture contains either rez phrasing**
    (`grep 'resuscitates\|comes back to life' test_data --include=Chat.log` → 0 rows), so the capture is
    genuinely outstanding. Symptom to watch: duplicate rows in `character_resuscitations` (no UNIQUE key,
    cosmetic only).

---

## Investigations — settled research, blocked outside the repo

Settled research kept for reference: **nothing in this section is actionable code work.** Each entry is blocked on something outside the repo — a game capture that no fixture contains, or a fundamental `Player.log` format limitation — and names that blocker inline. (An earlier occupant, the appearance-only equipment view, was never actually blocked and now lives under *Medium Effort*.)

- **Match-3 winnings tracker** — No log events exist for the Match-3 minigame. Zero references in parser, chat logs, or Player.log. No feasible path without game-side changes. Minigame niche.
  - **Blocked on: a casino capture (no fixture contains the minigame).**
  - **Audit: re-scoped — "zero references" is wrong.** Correct: no fixture contains the minigame (`grep -ri 'match.?3' test_data` → 0) and there is no parser/table/UI. Wrong: the CDN *does* carry it — `CasinoFreePlayTicketMatch3`/`CasinoFreePlayTicketMatch3B` (`docs/CDN-full-examples/items.json:18588-18609`) and the **Gaming** skill whose rewards read "New match-3 stat visible" (`skills.json:6132-6190`). Existing generic ingestion (item gains + `ProcessUpdateSkill`) may already yield a partial view — needs a casino capture to tell.
- **Monsters and Mantids winnings tracker** — Same as Match-3: no log events exist. No feasible path without game-side changes. Minigame niche.
  - **Blocked on: a casino capture (no fixture contains the minigame).**
  - **Audit: re-scoped.** Same capture requirement (0 fixture hits for `mantid`; the Chat.log `mantis` hits are unrelated NPC/combat text). But the CDN exposes a real **win counter**: quest_45084 `Event_Halloween_TheGamblersCurse` carries the objective `{"Target":"WonMonstersAndMantids","Type":"Special"}` (`quests.json:60635-60651`), and the parser already handles that vehicle — `ProcessCompleteDirectedGoals` → `DirectedGoalsLoaded` (`player_event_parser.rs:1379-1382,3245-3264`). Whether the counter's progress reaches Player.log is untestable on current fixtures.
- **Hot tips tracker** — No log events, chat channels, or CDN data references to "hot tips" found in current samples. Need to collect samples, or just find what Kaeus has...
  - **Blocked on: a sample or a link to an existing artifact (none in-repo).**
  - **Audit: verified — but it is a request, not a scoped item.** Repo-wide `grep -ri 'hot.?tip'` returns exactly one hit: this line. No in-repo Kaeus artifact exists (`gst_manager.rs` is GorgonSurveyTracker; no GorgonBetTracker anywhere), so there is nothing to scope until a sample or a link arrives.

---

## Log-signal items (parser-adjacent; nothing left here is capture-blocked)

The audit cleared both capture blockers in this bucket: recipe learning is shipped end-to-end (moved to
*Closed — kept for the record*), and the gardening blocker is wrong — the captures the item asked for already
exist. The narrow frontend defect this bucket produced is now fixed (see *Closed*); what remains is the unimplemented garden Phase 1.

- [ ] Gardening helper — see `docs/plans/gardening-helper.md`
  - Parser events exist (`EntityDescriptionUpdated`), plan written, garden almanac (Phase 0) complete.
  - **Audit: re-scoped — the "needs additional captures" blocker is wrong.** `docs/architecture/capture-analysis-results.md:16-34` already documents the whole observed state machine (Thirsty/Hungry/Growing/Ripe/Blooming, `Water`/`Fertilize`/`Check`/`Harvest`, varying `Scale=`) from existing captures. What is actually missing is Phase 1+ code: **no** `EntityDescriptionUpdated` arm in `game_state.rs`/`coordinator.rs` (0 hits), **no** `garden_plots` table (`migrations.rs` 0 hits), **no** `get_garden_plots` command. Phase 0 = the almanac widget/table only (`GardenAlmanacWidget.vue`, `dashboardWidgets.ts:221-224`).
  - **Effort: Medium for Phase 1 (plot tracker) | Impact: High**

---

## Medium Effort

- [ ] Bug: occasional inventory item miscounts
  - **Root causes identified:** (1) Chat-to-player event correlation uses a ±2 second window — mismatches fall back to stack_size=1. (2) Parser's 1-line lookahead buffer for deletions (`pending_deletes`) can fail if log lines reorder. (3) During catch-up replay, stack count corrections from chat may arrive before/after Player.log events depending on replay ordering. (4) Login doesn't treat inventory as a full state dump, so items from earlier sessions can pollute later ones. The widget itself warns "item tracking is not great right now." Fix requires reconciliation logic and smarter replay-mode handling.
  - **Audit: re-scoped — cause (4) is mostly fixed.** Live tailing *does* clear inventory (and equipment/combat/mount) on login: `game_state.rs:176-201`, reached once `live_mode` flips after catch-up (`coordinator.rs:622-628`); the "never cleared on login" reading only holds during catch-up, where it is deliberate. Causes (1)–(3) are confirmed live: `CHAT_GAIN_MATCH_WINDOW_SECS = 2` (`player_event_parser.rs:826`, matcher `:985-1011`, fallback `:1556-1560`), `pending_deletes` (`:632,1414-1506`), and the dual chat-correction path (`coordinator.rs:1265-1279,1306-1310`). The warning is in the **Critical Resources** widget (`CriticalResourcesWidget.vue:16`), not the Inventory tab.
  - **Effort: Medium-High (multiple interacting root causes) | Impact: Medium (data accuracy)**

- [ ] Smarter gamestate saving and initializing
  - **Investigation complete:** Three core problems: (1) `ProcessAddItem` always records stack_size=1 as defensive default; `correct_stack_from_chat()` patches this via status messages but timing is fragile during replay. (2) No detection of "login reload" full-state dumps vs. incremental changes — items are UPSERTed individually, never cleared on login during catch-up. (3) During catch-up replay (`live_mode=false`), inventory is not cleared on character login, so items from earlier replayed sessions pollute later ones. Fix requires: detecting login-phase full-state dumps, intelligently clearing transient state, coordinating stack-correction timing with chat log replay order.
  - **Audit: re-scoped.** (1) stands (`parse_add_item` seeds 1 or a matched buffered chat gain, patched later by `correct_stack_from_chat`, `game_state.rs:1138-1195`). (2) is half-solved: effects already do login-batch full-dump detection (`is_login_batch`, `player_event_parser.rs:3098`, clear at `game_state.rs:749-753`) and live mode clears the inventory family on login — **inventory alone has no dump flag**. (3) is intact and intentional. Residual: an inventory login-batch signal + replay-safe stack correction.
  - **Effort: Medium-High | Impact: Medium (data accuracy)**

- [ ] Item provenance downstream features
  - Now that provenance is in the transaction ledger (item-provenance plan phases 1-5 complete), new analytics become possible: mining node yield stats per node type, vendor purchase history with total spend, kill loot breakdown by mob type, crafting yield analysis per recipe, "unknown source" diagnostic reports for discovering new signal patterns.
  - **Audit: re-scoped — two of the five already shipped, just not on top of provenance.** Kill-loot-by-mob exists (`enemy_kills`/`enemy_kill_loot`, `migrations.rs:2900-2927`; `kill_tracking_commands.rs:156,312`; `EnemyBrowser.vue`); mining/survey yield ships **session-only** in Farming (`farmingStore.ts:814-840`, `FarmingSessionCard.vue:268-290`). Still absent: vendor-purchase total spend (0 production consumers of `source_kind='vendor_browsing'`), ledger-based craft yield (`HistoryTab.vue` uses `game_state_recipes.completion_count` instead), and the unknown/uncertain-source report (all production `source_kind` consumers are survey).
  - **Effort: Medium per feature | Impact: Medium-High (analytics depth)**

- [ ] Boss kill loot timers
  - Currently only player *deaths* are tracked (via `ChatCombatEvent::PlayerDeath` with killer detection). No reverse tracking exists (player kills boss). Would need to extend `chat_combat_parser` or `player_event_parser` for enemy kill events with boss identification. Loot timer logic would layer on top.
  - **Audit: re-scoped — the premise is wrong; reverse tracking already exists.** `chat_combat_parser.rs:9-30` defines `PlayerDeath`, `EnemyKilled` and `DamageOnPlayer`; `EnemyKilled` is emitted as `enemy-killed` (`coordinator.rs:1517-1525`) into `farmingStore.handleEnemyKilled`, and drop attribution comes from Player.log corpse searches (`coordinator.rs:1910`, migration v49, `kill_tracking_commands.rs`). Genuinely missing: **boss/named classification** (no log class signal — `combat_wisdom_commands.rs:8-11`) and the timer/lockout layer (0 hits for `loot_timer`). So this is a heuristic + UI item, not a parser extension.
  - **Effort: Medium (classification heuristic + UI) | Impact: Medium**

- [ ] Show work order completion state in tooltips for all characters
  - Work order tooltips should show completion/cooldown state across all tracked characters.
  - **Audit: verified (still unimplemented).** `QuestTooltip.vue` and `ItemTooltip.vue` render no state; WO completion exists only inside the Quests screen for the selected character (`QuestsScreen.vue:167-173`), and the only cross-character command (`get_all_character_items`, `inventory_commands.rs:566`) is inventory-items-only (used by `ConsolidateTab`).
  - **Effort: Medium | Impact: Medium (multi-character awareness)**

- [ ] Persist dropped snapshot fields from character/inventory exports (added 2026-10-20; audited against `test_data/inventory/completeState/Character_TestPlayer_Dreva.json` + `TestPlayer_Dreva_items_2026-04-14-15-57-59Z.json`)
  - **Character (`/outputcharacter`): only `Skills.<Skill>.Abilities` is dropped but recoverable.** Everything else top-level is stored and game-state-seeded (skills, recipe_completions, current_stats, currencies, NPCs→favor: `character_commands.rs:217-332,690-823`). Caveats, not drops: NPC entries carry only `FavorLevel` strings (384 in the fixture, no point totals — the seeder preserves `cumulative_delta` instead of overwriting, `:771-772`), and older exports predate `CompletedQuests` (key absent → `#[serde(default)]` → zero completed non-WO quests). Struct field exists (`SkillData.abilities`, `character_commands.rs:47`) but is `#[allow(dead_code)]`: parsed, then dropped, nothing stored. Every stored `character_snapshots.raw_json` already contains the data, so historical snapshots can be backfilled by re-import. Work: migration (`character_skill_abilities` / `snapshot_skill_abilities`: snapshot_id, skill_name, ability_key), import write path, backfill over existing `raw_json`, per-character known-ability query.
  - **Inventory (`/outputitems`): 5 keys parsed-and-dropped** (never enter `StorageItem` / `character_snapshot_items`): `IsAugmented` (8/1194 items in the fixture), `IngredientItemTypeIds` (1 — only survives as a brewing-scan read straight off `raw_json`, `brewing_commands.rs:405`), `DyeCount` (4), `Inscription` + `Inscriber` (1). Augmentation is the only one with a plausible consumer ("which gear is augmented" is currently unanswerable from the DB); the rest are single-digit-occurrence curiosities — persist for completeness alongside the migration, no UI owed.
  - **Unblocks:** the Skillbook autowatchwords prerequisite (Larger Efforts — its audit names the missing `snapshot_skill_abilities` table explicitly), an honest "Hide unlearned abilities" in `AbilityPickerDialog.vue:52` (currently a `level <= skill level` proxy, `:246-248`), and a true "My abilities" filter in `ModBrowser.vue:16` (currently scoped to the build's assigned bars).
  - **Effort: Medium (migrations + import write paths + raw_json backfill + known-ability query) | Impact: Medium-High (unblocks skillbook watch, fixes two filters; closes the snapshot audit)**
- [ ] Current gear display (blocked: no source knows what's worn)
  - The ask is a "what's my character wearing" tab — names, stats, details. Neither source answers it. The log path (`ProcessSetEquippedItems` → `EquipmentChanged`, stored as `slot`+`appearance_key` only) is live but nameless: appearance look-codes, never item names. The snapshot path has names/mods/TSys (`character_snapshot_items` via `get_equipped_gear_candidates`) but the report carries no "currently equipped" flag — worn gear and carried spares are indistinguishable, so the picker returns best-guess candidates and asks the user to confirm. An appearance-keys-only view was built and reverted: it rendered correctly but answered nothing the user asked.
  - **Unblocks when:** a source identifies worn gear — either a log event carrying item identity per slot, or a snapshot/VIP report flagging equipped items.
  - **Effort: unknown (needs the source first) | Impact: Medium**

- [ ] Per-attacker damage history on a specific monster (blocked: the log never shows other players' outgoing attacks)
  - The ask was a pane logging every hit any player lands on a named monster (e.g. Aktaari Cocks): attacker, damage, one-shot flag. A full implementation was built and reverted (2026-10-20): parser variant, `aktaari_cock_hits` table, live capture, Chat tab — it worked end to end, but against 5 months of logs it recorded 215 of the user's own hits, 1 pet hit, and 0 from any other player, because the game only writes `[Combat]` attack lines for fights the local client observes. Other players' hits on the same monster are simply absent (only their regen ticks, burials, and banter appear); other players' deaths likewise leave no `[Combat]` line, only `comes back to life!` aftermath in `[Action Emotes]`. Kill credit via corpse searches has the same ceiling: `enemy_kills` stores the searcher (always the local player), never the killer.
  - **Unblocks when:** a source attributes other players' damage — either a log/chat setting that echoes nearby players' outgoing attacks, or a game/client change emitting them.
  - **Effort: small (the reverted build is the template) | Impact: Medium (group-fight awareness)**

- [ ] Evaluate ingestion pipeline / coordinator architecture
  - **Partial investigation (see `docs/architecture/pipeline-structure.md`):** Coordinator is a manual dispatch hub (~2K lines). No formal event bus — each new feature adds match arms. No history/audit tables for most domains (only `item_transactions` is append-only). No shared schema contract for Tauri event payloads. Current design works at this scale but these are real gaps to watch as features grow.
  - **Partial investigation:** Two-tier architecture: `PlayerLogWatcher` → `PlayerEventParser` → `GameStateManager` (50-event batches, 20ms flush window). Chat parallel: `ChatLogWatcher` → bulk insert + per-event status parsing. Main concerns: (1) chat-to-player correlation window is tight at ±2 seconds, (2) no query-side indexing on `game_state_inventory` for per-character/per-server lookups, (3) pending chat gains buffer ages items after 10s with no metrics on correlation failure rate, (4) all inventory deletes require 1-line lookahead. Overall reasonable design — main focus should be correlation tuning and reconciliation.
  - **Audit: re-scoped — two of three headline claims are stale.** `coordinator.rs` is **3927 lines** (not ~2K), `PlayerEvent` has **40 variants**, and **17** `game-state-updated` emits live in-file (24 repo-wide). "Only `item_transactions` is append-only" is wrong: history tables now exist for milking, pooping, garden, hoplology, arena (`arena_matches`/`arena_bets`), roulette, combat-wisdom and books; only the `game_state_*` "last known value" family has no history. The one real remaining gap: **no shared schema contract for Tauri event payloads** (no specta/ts-rs/typeshare in `src-tauri/` or `package.json`).
  - **Effort: Medium | Impact: Medium (reliability)**

- [ ] Investigate dashboard refresh issues
  - Reported as "weird page refresh issues." No explicit refresh/reload logic in `DashboardView.vue`. Main reactive trigger is `orderedWidgets` computed property tied to `useViewPrefs` + settings store. `useViewPrefs` debounces writes (500ms) but mutations can fire during transitions. Likely caused by reactive store updates triggering unexpected re-renders on pane resizing or settings updates. Needs runtime debugging to reproduce.
  - **Audit: verified (unreproduced, unfixed).** `DashboardView.vue` still has no refresh path; every refresh-like behaviour is per-widget — 30 s tickers in `GardenAlmanacWidget:152`, `HarvestAlmanacWidget:273`, `HoplologyWidget:257,279`, `MilkingTimersWidget:338`, `WordsOfPowerWidget:424`; 500 ms debounced reload in `RecipeItemsWidget:144-151`; `watch` reload in `VendorCouncilWidget:405`. Still needs a live repro.
  - **Effort: Low-Medium (once reproduced) | Impact: Unknown**

- [ ] Color theme support
  - Investigate whether supporting user-selectable color themes makes sense. Low priority but high delight.
  - **Audit: verified (not started, no investigation artifact).** The only user-facing appearance settings are font family, font size, UI scale and widget opacity (`settingsStore.ts:5-47`, `AppSettingsTab.vue`); `src/assets/css/theme.css` is Tailwind's `@theme`, not user theming. Needs a design decision (what a theme owns — tokens only? chart palette too?) before an estimate.
  - **Effort: Medium (investigation + implementation) | Impact: Low (personalization/delight)**

- [ ] Continue UI/UX standardization across screens
  - Some screens still don't look like they fit within the app, or have their own paradigms. Sidebars that don't use standardized panels, inconsistent patterns, etc. Should write a consolidated UI/UX checklist for new frontend features and then do a pass on existing screens against it.
  - **Progress:** Color tokens, typography scale, and widget sizing are now standardized. DataTable/FilterBar/SkeletonLoader shared components are available. **Table sweep done 2026-10-17:** `DataTable` now covers **35 instances across 30 files** (was 6/5) — every grid that is not on the exemption list below is migrated, including `PlayerAttributesCard`, which the first pass mis-filed as a label/value panel and left on a raw `<table>` (the counts also stopped including `<DataTableSkeleton>`, a different component imported by `CooksHelperTab`, `HistoryTab` and `ProjectMaterialsPanel`). The standard (`docs/architecture/ux-standards.md` § Visual Consistency Checklist) now says which tables MUST use it. What remains is genuinely not a grid and was left alone on purpose: `ComputedStatsCard` (label/value with `colspan` section rows), `ReportStatsSection` (label/value, no header row — its group headings are `<h4>`s outside the table), `DeathsView` (its parent rows expand into a full-width `colspan` row; only the nested damage-source table is a sub-table), `BreweryRecipeDetail` (columns are the recipe's `variable_slots`), `AggregateView` skills-across-characters, `ArenaWidget` (runtime columns / pivot), `StallRevenueTab` (pivot + `tfoot`), plus the debug dumps (`Settings/GameStateDebug`, dev-panel `GameStateTab`). Next incremental steps: heading class standardization (see `docs/architecture/typography-audit.md`), color token Tier 3 cleanup (see `docs/architecture/color-standards.md`) — **the table-migration half of this item is finished**.
  - **Audit: re-scoped — the Progress line's counts were wrong (corrected here).** *Measured at `ebeb295` (v1.3.1), before the table sweep.* Shared `DataTable` was used in **6 instances across 5 files** (not 10). Raw `<table>` remained in **34 files** — 33 consumers plus `Shared/DataTable.vue` itself: Character 10, Crafting 8, Surveying 4, Dashboard 3, DataBrowser 2, Chat/Inventory/Market/Settings/StallTracker 1 each, dev-panel `GameStateTab` 1 — i.e. **14** in the three named areas (Crafting + Surveying + DataBrowser), not ~30. `FilterBar`: **1 file**. `SkeletonLoader`: **21 files**. Heading classes are defined (`components.css:3-13`) and adopted; color Tier 3 has a small literal-hex tail (5 files) plus the chart palette. The consolidated checklist the item asks for **exists**: `docs/architecture/ux-standards.md`. *(Superseded 2026-10-17 by the sweep in Progress above, which has the current counts.)*
  - **Effort: Medium (iterative) | Impact: Medium (consistency/polish)**

---

## Larger Efforts / Research Needed

- [ ] Quest tracking system (work orders, repeatables, Statehelm, active quests)
  - **See `docs/plans/quest-tracking.md`** — consolidated plan covering quest event parsing, repeatable cooldown tracking, work order cooldowns, Statehelm quest tracking, and active quest browsing. Prerequisite: add quest events to the player event parser.
  - **Audit: re-scoped — four features in four different states, and one already shipped.** (a) **Work orders — shipped**, but from character export, not the log (`get_work_orders_from_snapshot`, `crafting_commands.rs:836`; `QuestsScreen.vue:167-214`). (b) **Active quests — shipped**, also from export (`character_active_quests`, `migrations.rs:2062-2070`; `QuestsScreen.vue:154`). (c) **Repeatables — partial**: a `isRepeatable()` flag from `ReuseTime_Minutes/_Days` (`QuestsScreen.vue:118-120`), no completion timestamps, so no cooldown state. (d) **Statehelm quests + log-based parsing — not started**: 0 quest variants in `player_event_parser.rs`; plan §1/§3 unstarted. Fixtures do contain the events the plan wants (`ProcessLoadQuests` 29 lines across 9 files, `ProcessUpdateQuest` 22 lines across 5 files under `test_data/`). Also stale: `docs/features/screens/character/character-quests.md` still describes the pre-snapshot two-panel screen.
  - **Effort: Large | Impact: High**

- [ ] Interactive zone maps — see `docs/plans/interactive-maps.md`
  - kaeus is working on this i believe.
  - **Audit: verified as "no code exists" — drop the unverified Kaeus claim.** No map dependency (`package.json`), no map/zone-map component, no map assets; the plan is an unstarted spec with no phase markers. Zone-adjacent UI is survey analytics (`Surveying/AnalyticsZoneView.vue`, `CrossZoneComparison.vue`) and `CurrentZone.vue` — not maps. Point at an external branch/PR if one exists.
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
  - Start with currencies, expand to other items. Currencies may be in character JSON, inventory JSON, or both. PG doesn't overwrite inventory reports in the reports folder — could backfill historical data from old reports. Character JSONs are overwritten on export though. **Infrastructure gap:** database supports point-in-time snapshots only (`character_item_snapshots`). `sales_history` tracks vendor transactions but no time-series quantity tracking exists. Needs: new tables for quantity history, periodic delta recording (event-driven or snapshot-based), UI for time-range queries and charts.
  - **Audit: re-scoped — prose corrected.** The gap is real (0 hits for `quantity_history`), but "point-in-time snapshots only" is wrong: the ledger/series family already exists — `item_transactions`, `sales_history`, per-import `character_currencies` (`migrations.rs:2050-2058`), `character_item_snapshots`+`character_snapshot_items` (`import_date` indexed), plus the per-domain history tables. Also wrong: currencies are **character-export-only** (no inventory path) — the plan's own "Currency Data (Resolved)" section says so. `currency_estimate` (v56) is an anchor+delta estimate, not a history series. Both plan docs (`owned-quantity-tracking.md`, `quantity-tracking-over-time.md`) have no corresponding code.
  - **Effort: Large (new infrastructure) | Impact: High (trend data is very valuable)**

- [ ] Skillbook autowatchwords
  - Automatically watch chat for item links of skill books the player could learn but doesn't own or know. Two modes: (1) books for currently-trained skills, (2) "future skill" mode where players select skills and we watch for any skillbooks they don't already own/know, even if skill level is too low to use them yet. **Investigation:** watchword system is fully operational (`watch_rules.rs`) with pattern matching and toast notifications. CDN has skillbook items with pattern `Skillbook_*`/`SkillBook_*` (e.g. `Skillbook_FoxInABox` with `AbilityRecipe` keyword). Implementation requires: (1) mapping skillbooks → skills via CDN items, (2) filtering against player's known skills, (3) auto-generating watch rules for desired skillbooks.
  - **Audit: verified — investigation accurate; the missing prerequisites are named.** Live: `watch_rules.rs:19 evaluate_rules`, `WatchRule`/`WatchCondition`/`WatchNotifyConfig` (`settings.rs:282-312`), toast+sound on `watch-rule-triggered` (`watchwordAlertStore.ts`), UI `WatchwordsView.vue`. CDN: `Skillbook_FoxInABox` (`items.json:19289`), `SkillBook_Headbutt` (`:62631`), 537 `BestowAbility` items. Absent: a `snapshot_skill_abilities` table (plan Phase 1) and a `source`/auto field on `WatchRule`, so the plan's `source:"auto_skillbook"` tag is currently unsupported; no `get_skillbooks_for_skill`/`generate_skillbook_watch_rules`. Note: `refs/tags/archive/feat-books-watcher` holds a prior, unmerged attempt.
  - **Effort: Large | Impact: High (proactive skill progression)**

- [ ] Statehelm repeatable quest tracking — see `docs/plans/quest-tracking.md` §3
  - Sub-task: track statehelm renown possible vs earned.
  - **Audit: verified open — fold under the quest-tracking item (same plan, same dependency).** Renown exists only as a currency balance in the context bar (`contextBarPrefs.ts:10,30`); there is no possible-vs-earned computation, and no Statehelm quest-completion tracking (the tracker covers weekly gifts/favor only).
  - **Effort: High | Impact: Medium-High**

- [ ] Write 'how to use' docs for each screen
  - Even brief docs for each screen would help users understand features. Could be in-app help or docs folder.
  - Need to have some standard place in the panel layout for a button or something, perhaps, that explains how to use it. maybe top right of the panel view? we will need a way to turn it off/on (not all panel layout consumers will want it) and then how to define what it contains.
  - **Audit: re-scoped into two halves.** (a) *Docs folder*: **69** markdown files exist under `docs/features/screens/`, but they are developer/architecture references (0 hits for a usage/how-to heading), so the end-user half is untouched — decide whether that counts. (b) *In-app affordance*: specified in `docs/plans/in-app-help.md` (PaneLayout `helpKey` prop + `src/components/Shared/ScreenHelpPanel.vue` + `useScreenHelp.ts` + a `src/help/` registry) and **does not exist** (0 hits for `helpKey`/`ScreenHelp`; `PaneLayout.vue` takes only `screenKey/leftPane/rightPane`). App-level help does exist (`components/Help/*`, `?` button at `MenuBar.vue:86-90`).
  - **Effort: Large (breadth) | Impact: Medium (onboarding/discoverability)**

- [ ] Nightmare cave challenge door tracker
  - Need to look up all the challenges and see which ones we can track. Some are easy (1200 armor) and some are harder (have 4x 10-second premonition buffs). Could also track letters of authority as alternate path. **No existing code found** — no parser events, coordinator handlers, or database tables. Requires research into all challenge types + log event identification + new persistence layer.
  - **Audit: re-scoped — "no existing code" is right, but the ingredients are closer than stated.** Generic effect/attribute events already parse (`ProcessAddEffects`/`ProcessRemoveEffects`/`ProcessUpdateEffectName`, `player_event_parser.rs:1349-1362,3064-3107`) and fixture effect IDs map onto CDN effects (`docs/CDN-full-examples/effects.json` `effect_12602-12608` = the Spawn-Premonition family, including the 10-second tier), so "1200 armor" and "4× 10-sec premonition" are plausibly derivable from existing events + CDN joins; Letters of Authority already flow through inventory snapshot tracking. What is genuinely missing is any challenge-*door* identifier in fixtures or CDN → **capture first**, then the persistence layer.
  - **Effort: Large (research + implementation) | Impact: Medium (niche but useful)**

- [ ] Reevaluate test suite
  - Think about what tests make sense, what isn't giving value, and how to harden against future failures.
  - need to figure out if we can build some sample data or something that makes sense for tests.
  - **Audit: re-scoped — the sample-data ask is done; only hardening remains.** `test_data/` now holds **8** replay datasets under `surveyLogs/` (`Player.log` + `Chat.log`; 6 of them also carry a hand-recorded `results.txt`, and those 6 are the set the accuracy report iterates — `survey/replay_tests.rs:985-1034`) plus inventory/login-state fixtures, exercised by `survey::replay_tests`. Census: **709 `#[test]` across 57 files, 6 `#[ignore]`** (5 replay + 1 chat backfill); no integration dir, no JS tests. Slow suites: `survey::replay_tests`, `cdn_items_load_smoke`. (Counts match `AGENTS.md`; `cargo test --lib` reports 703 passed + 6 ignored.)
  - **Effort: Large | Impact: Medium (reliability/confidence)**

- [ ] Analyze what should move from frontend to Rust
  - Some frontend logic may be better served on the Rust backend. Needs analysis to identify candidates.
  - **Audit: verified open — but the candidate list is now identifiable.** Largest TS logic: `craftingStore.ts` (1746 lines — recursive ingredient tree + material aggregation with per-node IPC and hand-rolled caches), `gameStateStore.ts` (1301), `buildPlannerStore.ts` (951), `useStorageConsolidation.ts` (944 — cross-alt vault math), `useRecipeCost.ts` (per-ingredient recursion + IPC). The target pattern already exists in-tree: `aggregateStore.ts` (thin IPC shim) and `useAbilityBuildStats.ts` ("the math lives in the Rust `compute_ability_build_stats` command").
  - **Effort: Large (research) | Impact: Medium (performance/architecture)**

---

## Closed — kept for the record

- [x] ~~Catch-up guard drops a non-selected character's login snapshot~~ — **FIXED 2026-10-15**: the Player.log login line now *attributes* the session that follows it instead of gating on the selection.
  - **One owner for the ingest identity.** `DataIngestCoordinator::adopt_player_log_session` (`coordinator.rs`) writes only `GameStateManager` — the single owner — and every log-derived writer reads it, including `ingest_shop_log`, which used to read `settings.active_character_name` directly and was the reason per-session attribution was unsafe before. `settings` is the durable/display selection (picker; the Chat.log marker, which writes it in both modes; the live Player.log re-arm when `active_character` is `None`; and `poll()` right after the Player.log drain, syncing character *and* server to the ingest identity — the chat drain that follows may then re-set them from its own marker). `GameStateManager::clear_active_character` is deleted.
  - **One frontend line was needed anyway.** The plan assumed `startupStore`'s post-catch-up refresh re-read the backend settings; it does not (`settingsStore.initialize()` early-returns on `isLoaded`, set during the splash phase), so the UI stayed on the pre-catch-up character while ingest wrote the adopted one. `initialize(force)` + `await settingsStore.initialize(true)` there fixes it — verified over CDP (header `OtherAltServer — OtherAlt`, the adopted character's own skills loaded).
  - **`session_server_for` fallback.** A Player.log login line has no server, so it is resolved from `user_characters` (written by the chat-login marker): the server already in use wins, else the picker's active row, else the newest (`updated_at`, with `id` breaking its second-precision ties); a character **absent from `user_characters`** takes its own `Chat.log` login line instead, and only a character absent from *both* keeps the server in use — as does a lookup that could not run (`SessionServerLookup::Failed` — a read error answers `Failed`, never `NoRow`, so it never borrows the chat tier). **Those fallback-keyed rows then stay under that key for good** — a full-state dump only replaces its own `(character, server)` slice, so a later real login fixes the rows written after it, not those. Accepted: the fallback key is the one that character's other sessions use unless it plays on a different server than the current selection, and the alternative (leaving the server unset) drops the whole session — the cost this change exists to remove. Invisible only to the server-scoped UI, and named in the console.
  - **Evidence:** unit tests `coordinator::session_identity_tests::session_server` (all six resolution cases, including a dropped `user_characters` table for `Failed` and that an alt resolves through its own row rather than through the server in use) and `game_state::tests::identity_switch_keeps_each_sessions_skills` (a contract guard on the pair-scoped dump — two sessions' skill dumps stay separate; it passes on the pre-change code too, since the routing is the coordinator's `adopt_player_log_session`, which the smoke covers); full two-character catch-up smoke on a throwaway profile, both orders. Live switches and the `character-login` emit remain the Chat.log marker's job.

- [x] ~~Bug: instant-snack foods missing from gourmand report~~ — **RESOLVED 2026-10-11 (real cause: category-token drift; fixed in migration v70)**
  - The 2026-10-03 "stale import" diagnosis was wrong, and the same session's evidence pointed at the answer: the `foods` table holds 57 **`'Instant Snack'`** rows (CDN 486 builds the token from the `Food_Effect_Level_InstantSnack` string — `"Level {0} Instant Snack"`) while `gourmandStore` matches **`'Instant-Snack'`**. The Instant-Snacks section and its progress bar were therefore permanently empty — 57 of 606 foods invisible to the UI, no import or watcher could change that.
  - **Fixed:** `parse_food_desc` canonicalises the category via `canonical_food_category` (case- and separator-insensitive; unknown categories pass through) and migration v70 backfills rows already on disk in place — no CDN re-download. Verified on a copy of the dev DB: 606/606 foods match the UI's three category literals, idempotent, eaten joins intact.
  - Lesson: name-based joins (every eaten entry resolves to a `foods` row) cannot detect this class of bug — compare the *token* the UI filters on against what ingestion stored.
  - Note: `Flesh Lump` report lines carry race variants (`Flesh Lump (Elf)` etc.) — the tag stripper only removes `(HAS MEAT)`-class tags, so variant names match the CDN exactly. No fix needed (an earlier "variant matching gap" suspicion was a bug in the test script, not the app).

- [x] ~~Bug: rez counter not working~~ — **ROOT CAUSE FOUND 2026-10-03; FIX IMPLEMENTED**
  - The Action Emotes channel hypothesis was wrong: 22 `[Action Emotes]` lines reach the chat log today. The real gap: the game emits TWO rez phrasings — `<Caster> resuscitates <Target>` (parsed, populated the 78 historic rows) and `<Target> comes back to life!` (target-only, no caster named, UNPARSED). Sessions where the game uses the second phrasing left the counter silent.
  - **Fixed:** `chat_resuscitate_parser.rs` now parses the second shape with `caster_name = "Unknown"` (3 new tests incl. real-log lines and a spaced-target case). "Top rezzers" stays caster-based; "times rezzed" counting is now complete for newly tailed sessions (historic backfill of the second phrasing is not retroactive — backfilled chat logs don't re-parse old lines).
  - Still live in that area: the 30 s dedup window calibration (see *Deferred tuning* above).

- [x] ~~Investigate detecting recipe learning without character.json import~~ — **CLOSED 2026-10-13 (audit): the feature was already live**
  - `ProcessUpdateRecipe`/`ProcessLoadRecipes` parse into `RecipeUpdated` (`player_event_parser.rs:2878-2892,3021-3030`); the coordinator UPSERTs every update into `game_state_recipes` and emits the `recipes` domain (`game_state.rs:599-615`); Cook's Helper consumes it live through `learnedRecipeKeys` (`cooksHelperStore.ts:68`; `gameStateStore.ts:406-411`, refreshed on the `recipes` domain at `:1116`). The item's "STILL UNKNOWN — does it fire on a first learn (count 0)?" is answered **yes**: `test_data/surveyLogs/gazluk-motherlodes-vanilla/Player.log:27604` shows `ProcessUpdateRecipe(9116, 0)` for a recipe appearing nowhere else in the file.
  - The narrowed defect this item produced — "Cook's Helper hides recipes that were learned but never cooked" — was **fixed 2026-10-13** (see *Closed*, below).

- [x] ~~Bug: Cook's Helper hides recipes that were learned but never cooked~~ — **FIXED 2026-10-13**
  - Was the original report ("bought a recipe, helper didn't update"): `cooksHelperStore.ts` filtered the helper list on `knownRecipeKeys` (`completion_count > 0`), so a recipe whose learn event carried count 0 (`ProcessUpdateRecipe(id, 0)`) looked "not learned". Now it filters on `learnedRecipeKeys` (`gameStateStore.ts:406-411` — the presence of any `game_state_recipes` row), so learned-but-uncooked recipes are offered.

- [x] ~~Casino arena bet tracker~~ — **CLOSED 2026-10-13 (audit): shipped, as the arena widget rather than the plan's `casino_*` shape**
  - Bets parse from the log (`arena_parser.rs:286-369` — `Confirm Bet`, placement, payout gate; tests `:563-618`), outcomes from NPC Chatter (Kuzavek) chats (`arena_parser.rs:93-97`, wired `coordinator.rs:1192-1230`), persisted in `arena_matches` (v64) / `arena_bets` (v65, dedup index `idx_arena_bets_dedup`), exposed via `record_arena_bet`/`get_arena_stats` (`lib.rs:704`) and rendered by `ArenaWidget.vue`.
  - `docs/plans/casino-bet-tracker.md:13` previously listed the betting/P&L half as "Still TODO … not yet built"; corrected in this pass, because that is exactly what `ArenaBetTracker` + `arena_bets` + `record_arena_bet` now do (the plan's own Status section already documents the shipped fighter-prediction half). The plan's richer Economics→Casino tab (`:48,:73`) was never the shipped shape and is not part of this item.

- [x] ~~Macros or process interaction~~ — **CLOSED 2026-10-13 (audit): answered, and mechanically enforced**
  - `docs/research/glogger-game-interaction-limits.md` settles it: **targeting/sending to the game process is forbidden** — never open a handle to `WindowsPlayer.exe`, never inject, never read/write game memory, never synthesize input (§2, plus the TOS §5/§8 rows). Allowed: reading files the user's own client wrote (`Player.log`/`Chat.log`/glogger's DB), own-screen pixels via GDI `BitBlt`+OCR (passive, `stall_ocr.rs`), and CDN GETs.
  - `scripts/check-glogger-constraints.ps1` enforces the lines (`OpenProcess(`, `ReadProcessMemory`, `WriteProcessMemory`, `CreateRemoteThread`, `SendInput(`, `keybd_event(`, packet tooling, port 9002, …) on pre-push and in the release `validate` job. The old claim that "mouse/keyboard macros are okay" was wrong and is deleted.
  - Any future ask here starts from that doc, not from a feasibility study.

- [x] ~~Item mods/augments not shown in inventory~~ — **CLOSED 2026-10-13 (audit): log-format limitation, no feasible path — the data exists only in the VIP JSON export, which snapshot import already consumes**
  - Player.log does NOT include TSys mod/augment data. Only `ProcessAddItem` (name + instance_id) and `ProcessUpdateItemCode` (stack size + type ID) are available. TSys data is only in VIP Inventory JSON export (snapshot imports already store it). Fundamental log format limitation.
  - **Audit: verified.** 0 hits for `TSysPowers|IsAugmented|ProcessSetMods` in any `Player.log` fixture; the snapshot path does surface mods (`inventory_commands.rs:38-39,213-238` → `InventoryTable.vue:92-95`), and the user-visible note lives in `KnownIssuesTab.vue:115-119`. Log-format limitation stands as written.
