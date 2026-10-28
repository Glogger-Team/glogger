# TODO

Small tasks and notes that don't belong in a dedicated plan.

## Deferred tuning (needs real-world data)

- [ ] rez-dedup 30s window calibration — see `docs/plans/deferred-tuning.md` (needs a captured rez pair; window still forward-only so backward-ordered pairs fail open; no fixture contains either phrasing; watch for duplicate `character_resuscitations` rows, cosmetic only).

---

## Log-signal items

- [ ] Gardening helper — see `docs/plans/gardening-helper.md`
  - Parser events exist (`EntityDescriptionUpdated`), plan written, garden almanac (Phase 0) complete. Missing is Phase 1+ code: no `EntityDescriptionUpdated` arm, no `garden_plots` table, no `get_garden_plots` command.
  - **Effort: Medium for Phase 1 (plot tracker) | Impact: High**
  - **Blocked portion:** whole Phase 1 sits behind the `LocalPlayer: Process` fast-path gate (`player_event_parser.rs:1257-1262`) — the current client emits zero such lines, so any `garden_plots` table would stay empty. No code this batch; un-blocks when lines recur.


- [ ] Product decisions (from 10/21 update-notes triage; each needs a yes/no, no code until decided): rename-orphan history on Rename-Golem/server-merge (character tables key on `(character, server)`, no rename path); "no stall rented" vs "rented, zero sales" indistinguishable (no stall-rental table); No-Resell flag has no item-model counterpart (`vendor_prices` dead); Angling Perfect Catches as a new loot signal (nothing parses angling).
  - **Blocked portion:** Angling Perfect Catches needs a new `LocalPlayer:` signal — no angling lines exist and the current client emits zero `LocalPlayer:` lines at all (fast-path gate `player_event_parser.rs:1257-1262`); un-blocks when lines recur. Rename-orphan, stall-rental, No-Resell stay actionable.
## Post-10/21 client: Player.log emits zero game events — LocalPlayer: pipeline blind (added 2026-10-26; needs scoping, then investigation)

- [x] ~~Player.log game-event outage triage~~ — **TRIAGED 2026-10-26: verdicts above; survey manual-framing split to next item; dark-state UI + cheap polling remain.**
  - Verified 2026-10-26 against the live game dir: live `Player.log` (24MB) and `Player-prev.log` (13.7MB, 68,708 lines) contain **zero** `LocalPlayer:` lines — no `ProcessAddItem/DeleteItem/UpdateItemCode/ScreenText/TalkScreen`, nothing. Remaining `Process*` hits are engine noise (`ServerCmdProcessor`, `ProcessCommand Create/Update`, Unity input, `AreaCmdProcessor` animation/attack). Only the `Logged in as character … Time UTC=` login line survives. The 10/21 update's "remove excessive client logging to Player.log" removed the verbose player events, not just noise. `player_event_parser.rs:1257-1262` fast-path-skips every line without the prefix, so the entire `PlayerEvent` stream is dark; only `LogSessionAnchored` (login line, handled above the fast path) still fires. Chat.log is healthy (999 `[Status]` + 9,510 `[Combat]` lines today, live loot lines in the scored format).
  - Casualties — triaged 2026-10-26 (zero `LocalPlayer:` lines confirmed; verdict per tracker):
    - Surveys — **manual framing, chat loot survives.** Loot *data* flows (`ItemGained` → `survey_chat` rows, idempotent, 97.5% on fixtures); loot *framing* (uses, sessions, nodes, bonus) scores 0 — all `DeleteItem`/`DelayLoop`/`ScreenText`-derived (`aggregator.rs:535-704`). Short-term: hand-framed sessions + a new `survey_tracker_record_use {map, kind}` command reuses the chat-loot pipeline untouched. Treasure pings (`TreasureDistance`, parsed but unwritten) carry no kind/map/area — unclassified counter at best, not use detection. Nodes + speed bonus go **dark**.
    - Skills — **chat fallback (degraded).** `apply_chat_level_up` keeps level current; base/bonus/tnl/max frozen (progress bars 100%/`<1min` = unknown). Character-report re-import re-anchors.
    - Arena bets — **dark** (bets); matches **unaffected** via Kuzavek chat narration. No Confirm-Bet source exists.
    - Poems — **dark** (new recitals lost; old-log backfill only). No chat/Reports source.
    - Gifts — **dark** (tiers unaffected via Character report; weekly-count UI hidden with the strip — `add_manual_gift`/`remove_last_gift` exist in `useStatehelmTracker.ts` but have no visible caller; rewire tracked under dark-state UI item (2) below).
    - Kill loot — **dark** (chat `EnemyKilled` covers counts/UI only; items, extracts, drop linkage lost).
    - Inventory live — **dark / manual** (stale until login, then empty; Storage export re-import reseeds Storage report rows).
    - Vendor live + other-stall browse — **dark / manual**; own-stall sales **unaffected** (shop-log books poll).
    - Recipes (Cook's Helper) — **dark / manual** (report re-anchors). Effects/equipment/mount/weather/moon/guild/goals/strings/attributes/live-loadout — **dark** (report abilities + guild-motd text survive via books/reports).
  - Open work: (1) `survey_tracker_record_use` command + manual-framing UI — **next build** (unblocks survey on current client); (2) per-tracker dark-state UI (hide vs explain — no silent stale); (3) watcher keeps polling cheaply (game may restore events). Player.log tailing stays — the login line still anchors sessions.
- [ ] Survey manual use framing (`survey_tracker_record_use {map_internal_name, kind}` + session buttons) — unblocks survey tracking on the zero-`LocalPlayer:` client: hand-created uses re-attach the live chat-loot pipeline (`claim_unlinked_chat_loot_near` + `survey_chat` attribution need a use row; scans already leave rows untagged for it).
  - **Effort: Medium (command + UI) | Impact: High (survey works again on current client)**

  - Historical note (fixture-era, pre-10/21): on April fixtures with live Player.log events, the full pipeline captures 1500/2400 expected pieces (62.5%, all deltas negative, use detection 50/50 + 100/100 — attribution lossy, measured 2026-10-26 `npm run survey-test`):
    | 50x-povus-marvelous (multihit) | 486 | 169 | 34.8% |
    | 100x-eltmetal (basic, ring+pick) | 299 | 241 | 80.6% |
    | 100x-serbcrystal (basic, with ring) | 406 | 269 | 66.3% |
    | 100x-serbcrystal (basic, vanilla) | 407 | 213 | 52.3% |
    | 100x-eltblue (basic, ring+pick) | 405 | 275 | 67.9% |
    | 100x-eltcrystal (basic, vanilla) | 397 | 333 | 83.9% |
    Item-level: 15 exact, 23 close, 65 off (>2) of 103. The plan-doc 99.9% claim (`docs/archive/plans/survey-tracker-rewrite.md`, written in the same commit as the harness, `7edd681`) is not reproducible from history — that commit needs an uncommitted CDN snapshot and chat files from a later commit, and it trimmed povus ground truth to survey-attributable-only. Chat-only replay on the same fixtures: 97.5% (2340/2400, all 5 basic datasets 100.0%; multihit 87.7% with extras the hand count excluded). The Oct-2026 item-identity tie-break is an overcount guard, byte-identical on replay — never going to move the undercount. Per-dataset miss attribution (which lines drop at which gate) is the right investigation *after* use detection is re-sourced, not before.

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

- [ ] Persist dropped report fields from character/inventory exports (added 2026-10-20; audited against `test_data/inventory/completeState/Character_TestPlayer_Dreva.json` + `TestPlayer_Dreva_items_2026-04-14-15-57-59Z.json`)
  - **Character (Export Character as JSON in Settings → VIP): only `Skills.<Skill>.Abilities` was dropped — now persisted (migration v74, 2026-10-21).** Everything else top-level is stored and game-state-seeded (skills, recipe_completions, current_stats, currencies, NPCs→favor: `character_commands.rs:217-332,690-823`). Caveats, not drops: NPC entries carry only `FavorLevel` strings (384 in the fixture, no point totals — the seeder preserves `cumulative_delta` instead of overwriting, `:771-772`), and older exports predate `CompletedQuests` (key absent → `#[serde(default)]` → zero completed non-WO quests). Abilities land in `snapshot_skill_abilities(snapshot_id, skill_name, ability_key)` (`migrations.rs` v74) via the import write path (`character_commands.rs` §7b, deduped per skill, `None`/empty skipped), readable through `get_snapshot_skill_abilities(snapshot_id)` (`lib.rs`). Backfill decision: skipped — old reports simply have no rows; re-import the export to populate (`raw_json` already contains the arrays). Remaining work on this item is the inventory half (5 keys below).
  - **Inventory (Export Storage as JSON in Settings → VIP): 5 keys parsed-and-dropped** (never enter `StorageItem` / `character_snapshot_items`): `IsAugmented` (8/1194 items in the fixture), `IngredientItemTypeIds` (1 — only survives as a brewing-scan read straight off `raw_json`, `brewing_commands.rs:405`), `DyeCount` (4), `Inscription` + `Inscriber` (1). Augmentation is the only one with a plausible consumer ("which gear is augmented" is currently unanswerable from the DB); the rest are single-digit-occurrence curiosities — persist for completeness alongside the migration, no UI owed.
  - **Unblocks:** the Skillbook autowatchwords prerequisite (Larger Efforts — its audit names the missing `snapshot_skill_abilities` table explicitly), an honest "Hide unlearned abilities" in `AbilityPickerDialog.vue:52` (currently a `level <= skill level` proxy, `:246-248`), and a true "My abilities" filter in `ModBrowser.vue:16` (currently scoped to the build's assigned bars).
  - **Effort: Medium (migrations + import write paths + raw_json backfill + known-ability query) | Impact: Medium-High (unblocks skillbook watch, fixes two filters; closes the report audit)**
- [ ] Current gear display (blocked: no source flags what's worn — log path is nameless appearance keys, Character report path can't distinguish worn from carried; appearance-only view built and reverted).
  - **Unblocks when:** a log event carrying item identity per slot, or a report/VIP flag for equipped items. **Effort: unknown | Impact: Medium**
  - **Blocked portion:** fully blocked — the worn-vs-carried distinction needs a `LocalPlayer:` signal (fast-path gate `player_event_parser.rs:1257-1262`, current client emits zero). Un-blocks when lines recur.

- [ ] Per-attacker damage history on a specific monster (blocked: the log never shows other players' outgoing attacks — 5 months of logs: 215 own hits, 1 pet, 0 others).
  - **Unblocks when:** a source attributes other players' damage (log/chat echo setting, or game emitting it). Reverted build is the template. **Effort: small | Impact: Medium (group-fight awareness)**
  - **Blocked portion:** fully blocked — other-players' outgoing attacks have no `LocalPlayer:` signal today and the current client emits zero `LocalPlayer:` lines at all (gate `player_event_parser.rs:1257-1262`). Un-blocks when lines recur.

- [ ] Ingestion architecture: no shared schema contract for Tauri event payloads (no specta/ts-rs/typeshare). Coordinator is a 3927-line manual dispatch hub; history tables exist per-domain except `game_state_*`. The outage re-source design belongs here.
  - **Effort: Medium | Impact: Medium (reliability)**
+
- [ ] UI/UX standardization: Table sweep done — **35 instances across 30 files** (`ux-standards.md` checklist owns it). Remaining: heading-class standardization (`typography-audit.md`), color Tier 3 cleanup (`color-standards.md`).
  - **Effort: Medium (iterative) | Impact: Medium**

---

## Larger Efforts / Research Needed

- [ ] Quest tracking system (work orders, repeatables, Statehelm, active quests)
  - **See `docs/plans/quest-tracking.md`** — consolidated plan covering quest event parsing, repeatable cooldown tracking, work order cooldowns, Statehelm quest tracking, and active quest browsing. Prerequisite: add quest events to the player event parser.
  - **Audit: re-scoped — four features in four different states, and one already shipped.** (a) **Work orders — shipped**, but from character export, not the log (`get_work_orders_from_snapshot`, `crafting_commands.rs:836`; `QuestsScreen.vue:167-214`). (b) **Active quests — shipped**, also from export (`character_active_quests`, `migrations.rs:2062-2070`; `QuestsScreen.vue:154`). (c) **Repeatables — partial**: a `isRepeatable()` flag from `ReuseTime_Minutes/_Days` (`QuestsScreen.vue:118-120`), no completion timestamps, so no cooldown state. (d) **Statehelm quests + log-based parsing — not started**: 0 quest variants in `player_event_parser.rs`; plan §1/§3 unstarted. Fixtures do contain the events the plan wants (`ProcessLoadQuests` 29 lines across 9 files, `ProcessUpdateQuest` 22 lines across 5 files under `test_data/`). Also stale: `docs/features/screens/character/character-quests.md` still describes the pre-snapshot two-panel screen.
  - **Effort: Large | Impact: High**
  - **Blocked portion:** (c) repeatable completion timestamps need log-based completion signals (dark — the log never reports completions today); (d) Statehelm quests + log-based quest parsing are not started (0 quest variants in `player_event_parser.rs`, fast-path gate `player_event_parser.rs:1257-1262`, current client emits zero). Work orders + active quests stay actionable (shipped from exports). Un-block condition: `LocalPlayer:` lines recur.

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
  - Start with currencies, expand to other items. Currencies may be in character JSON, inventory JSON, or both. PG doesn't overwrite inventory reports in the reports folder — could backfill historical data from old reports. Character JSONs are overwritten on export though. **Infrastructure gap:** database supports point-in-time reports only (`character_item_snapshots`). `sales_history` tracks vendor transactions but no time-series quantity tracking exists. Needs: new tables for quantity history, periodic delta recording (event-driven or report-based), UI for time-range queries and charts.
  - **Audit: re-scoped — prose corrected.** The gap is real (0 hits for `quantity_history`), but "point-in-time reports only" is wrong: the ledger/series family already exists — `item_transactions`, `sales_history`, per-import `character_currencies` (`migrations.rs:2050-2058`), `character_item_snapshots`+`character_snapshot_items` (`import_date` indexed), plus the per-domain history tables. Also wrong: currencies are **character-export-only** (no inventory path) — the plan's own "Currency Data (Resolved)" section says so. `currency_estimate` (v56) is an anchor+delta estimate, not a history series. Both plan docs (`owned-quantity-tracking.md`, `quantity-tracking-over-time.md`) have no corresponding code.
  - **Effort: Large (new infrastructure) | Impact: High (trend data is very valuable)**

- [ ] Skillbook autowatchwords
  - Automatically watch chat for item links of skill books the player could learn but doesn't own or know. Two modes: (1) books for currently-trained skills, (2) "future skill" mode where players select skills and we watch for any skillbooks they don't already own/know, even if skill level is too low to use them yet. **Investigation:** watchword system is fully operational (`watch_rules.rs`) with pattern matching and toast notifications. CDN has skillbook items with pattern `Skillbook_*`/`SkillBook_*` (e.g. `Skillbook_FoxInABox` with `AbilityRecipe` keyword). Implementation requires: (1) mapping skillbooks → skills via CDN items, (2) filtering against player's known skills, (3) auto-generating watch rules for desired skillbooks.
  - **Audit: verified — investigation accurate; the missing prerequisites are named.** Live: `watch_rules.rs:19 evaluate_rules`, `WatchRule`/`WatchCondition`/`WatchNotifyConfig` (`settings.rs:282-312`), toast+sound on `watch-rule-triggered` (`watchwordAlertStore.ts`), UI `WatchwordsView.vue`. CDN: `Skillbook_FoxInABox` (`items.json:19289`), `SkillBook_Headbutt` (`:62631`), 537 `BestowAbility` items. Landed 2026-10-21: `snapshot_skill_abilities` (migration v74) + `get_snapshot_skill_abilities` now cover plan Phase 1. Still absent: a `source`/auto field on `WatchRule`, so the plan's `source:"auto_skillbook"` tag is currently unsupported; no `get_skillbooks_for_skill`/`generate_skillbook_watch_rules`. Note: `refs/tags/archive/feat-books-watcher` holds a prior, unmerged attempt.
  - **Effort: Large | Impact: High (proactive skill progression)**

- [ ] Statehelm repeatable quest tracking — see `docs/plans/quest-tracking.md` §3
  - Sub-task: track statehelm renown possible vs earned.
  - **Audit: verified open — fold under the quest-tracking item (same plan, same dependency).** Renown exists only as a currency balance in the context bar (`contextBarPrefs.ts:10,30`); there is no possible-vs-earned computation, and no Statehelm quest-completion tracking (the tracker covers weekly gifts/favor only).
  - **Effort: High | Impact: Medium-High**
  - **Blocked portion:** fully blocked — renown possible-vs-earned needs Statehelm quest-completion tracking, which needs a `LocalPlayer:` quest signal (gate `player_event_parser.rs:1257-1262`, current client emits zero). Currency balance in the context bar stays actionable. Un-blocks when lines recur.

- [ ] Write 'how to use' docs for each screen
  - Even brief docs for each screen would help users understand features. Could be in-app help or docs folder.
  - Need to have some standard place in the panel layout for a button or something, perhaps, that explains how to use it. maybe top right of the panel view? we will need a way to turn it off/on (not all panel layout consumers will want it) and then how to define what it contains.
  - **Audit: re-scoped into two halves.** (a) *Docs folder*: **69** markdown files exist under `docs/features/screens/`, but they are developer/architecture references (0 hits for a usage/how-to heading), so the end-user half is untouched — decide whether that counts. (b) *In-app affordance*: specified in `docs/plans/in-app-help.md` (PaneLayout `helpKey` prop + `src/components/Shared/ScreenHelpPanel.vue` + `useScreenHelp.ts` + a `src/help/` registry) and **does not exist** (0 hits for `helpKey`/`ScreenHelp`; `PaneLayout.vue` takes only `screenKey/leftPane/rightPane`). App-level help does exist (`components/Help/*`, `?` button at `MenuBar.vue:86-90`).
  - **Effort: Large (breadth) | Impact: Medium (onboarding/discoverability)**

- [ ] Nightmare cave challenge door tracker
  - Need to look up all the challenges and see which ones we can track. Some are easy (1200 armor) and some are harder (have 4x 10-second premonition buffs). Could also track letters of authority as alternate path. **No existing code found** — no parser events, coordinator handlers, or database tables. Requires research into all challenge types + log event identification + new persistence layer.
  - **Audit: re-scoped — "no existing code" is right, but the ingredients are closer than stated.** Generic effect/attribute events already parse (`ProcessAddEffects`/`ProcessRemoveEffects`/`ProcessUpdateEffectName`, `player_event_parser.rs:1349-1362,3064-3107`) and fixture effect IDs map onto CDN effects (`docs/CDN-full-examples/effects.json` `effect_12602-12608` = the Spawn-Premonition family, including the 10-second tier), so "1200 armor" and "4× 10-sec premonition" are plausibly derivable from existing events + CDN joins; Letters of Authority already flow through inventory report tracking. What is genuinely missing is any challenge-*door* identifier in fixtures or CDN → **capture first**, then the persistence layer.
  - **Effort: Large (research + implementation) | Impact: Medium (niche but useful)**
  - **Blocked portion:** the live door-identifier capture is blocked — no challenge-door identifier exists in fixtures or CDN, and live capture would need a `LocalPlayer:` signal (gate `player_event_parser.rs:1257-1262`, current client emits zero). Effect/CDN research on fixtures stays actionable. Un-blocks when lines recur.

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

- [ ] `loadAllCharacterItems` unkeyed cache — same stale-server shape the WO cache had (`characterStore.ts:444-473`: `loaded` flag, no server key, no invalidation on import/switch). Fix like the WO cache: server key + reset on imports + null-server guard. Left alone in #54 as "exceeds proportionate rigor" — do not copy the shape for new caches.
  - **Effort: Small | Impact: Low-Medium (wrong-server storage search)**
- [ ] Ordinal-order divergence guard — ordinal backfill assumes file order == stored `id` order (true today: importer writes one row per entry in order). Nothing enforces it. Add a debug assertion or a test with shuffled stored ids proving mismatch is detected, not silently misattributed.
  - **Effort: Small | Impact: Low (latent misattribution)**
- [ ] Multihit manual-use coverage — `record_manual_use` has no Multihit-specific test (only Basic claim + Motherlode area-None). Add a happy-path Multihit test exercising the non-Basic pending-use + mining-adopt path.
  - **Effort: Small | Impact: Low (untested path)**
- [ ] Stale-FK transparent retry — manual `record_use` errors "try again" when the session was deleted mid-flight; the log path re-resolves silently. Add one transparent retry (drop cache, re-resolve, re-insert once) instead of surfacing the retry to the user.
  - **Effort: Small | Impact: Low (UX papercut)**
- [ ] `startSession`/`endSession` error surfacing — predate the `recordError` pattern; failures log to console with `null` return and no UI. Adopt `recordError`-style refs if these paths gain user-visible failure modes.
  - **Effort: Small | Impact: Low**
- [ ] Debug-capture + login-skill-list `snapshot` wording — kept deliberately (different sense from export reports). Revisit only if users confuse the terms; no action owed.
  - **Effort: Trivial | Impact: Nil (decision record)**
- [ ] Tier-2 identifier rename (`SnapshotItem`, `get_snapshot_items`, `source='snapshot'`, table names) — explicitly out of scope for the tier-1 pass. A breaking IPC/DB migration; needs its own plan if ever wanted.
  - **Effort: Large | Impact: Low (cosmetic, high blast radius)**

- [ ] Backfill restores deleted characters — DECIDED 2026-10-28: no tombstones. Delete clears DB rows only; report files survive on disk, so any backfill (`import_all_inventory_for_server`, `import_reports_for_server`) reimports a deleted character's reports. The invalidation retry avoids it (re-reads only, `characterStore.ts:520-538`), but every other backfill caller still restores. Accepted as pre-existing behavior; revisit only if users report ghost restores.
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
- [x] ~~Scan-path Status dispatch loss~~ — **FIXED 2026-10-26: shared `chat_status_dispatch` module (live drain + all three scan/tail paths run the same arms with per-file session identity; re-scan inserts nothing). ReportSaved intentionally excluded on scans (path may not exist locally); survey rows stay untagged for retro-claim.**

## Blocked — waiting on LocalPlayer: restoration (added 2026-10-28; no work until lines recur)

Missing event in most entries: a `LocalPlayer:` line (fast-path gate `player_event_parser.rs:1257-1262` — the current client emits zero such lines, so the whole `PlayerEvent` stream is dark). Default un-block condition: `LocalPlayer:` lines recur in Player.log. Two entries need more: Angling Perfect Catches needs a never-yet-seen angling signal, and per-attacker damage needs an other-players'-damage source never logged — lines recurring is necessary but not sufficient there. Moved here verbatim from their actionable sections above; the parents keep a `Blocked portion:` sub-line.

- [ ] Gardening helper Phase 1 (plot tracker) — see `docs/plans/gardening-helper.md:52-56`. Parser events exist (`EntityDescriptionUpdated`), plan written, garden almanac (Phase 0) complete. Missing Phase 1+ code: no `EntityDescriptionUpdated` arm, no `garden_plots` table, no `get_garden_plots` command.
- [ ] Current gear display — no source flags what's worn (log path is nameless appearance keys, Character report path can't distinguish worn from carried; appearance-only view built and reverted). Unblocks when: a log event carrying item identity per slot, or a report/VIP flag for equipped items.
- [ ] Per-attacker damage history on a specific monster — the log never shows other players' outgoing attacks (5 months of logs: 215 own hits, 1 pet, 0 others). Unblocks when: a source attributes other players' damage (log/chat echo setting, or game emitting it). Reverted build is the template.
- [ ] Angling Perfect Catches (blocked portion of Product decisions) — needs a new `LocalPlayer:` angling signal; nothing parses angling today. Rename-orphan, stall-rental, No-Resell stay actionable above.
- [ ] Quest tracking: log-based quest parsing + Statehelm quests + repeatable-cooldown timestamps — 0 quest variants in `player_event_parser.rs`; the log never reports completions. Work orders + active quests stay actionable above (shipped from exports).
- [ ] Statehelm repeatable quest tracking: renown possible-vs-earned + completion tracking — renown exists only as a currency balance in the context bar; no possible-vs-earned computation, no completion tracking. Currency balance stays actionable above.
- [ ] Nightmare cave challenge: live door-identifier capture — no challenge-door identifier in fixtures or CDN; effect/CDN research on fixtures stays actionable above.

