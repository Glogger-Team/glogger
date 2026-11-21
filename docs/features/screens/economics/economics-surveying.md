# Economics — Surveying

Survey tracker built on the provenance pipeline. Three sub-tabs (Session, Session History, Analytics), all read-only views over `survey_sessions` + `survey_uses` + `item_transactions`. Backend owns session state; the frontend is a thin reader.

For the full design rationale and history of the rewrite, see:

- [item-provenance-overhaul.md](../../../archive/plans/item-provenance-overhaul.md) — parent plan (completed)
- [survey-tracker-rewrite.md](../../../archive/plans/survey-tracker-rewrite.md) — Phase 5 plan (completed)
- [survey-mechanics.md](../../../architecture/survey-mechanics.md) — Basic / Motherlode / Multihit behavior

## Architecture

### Files

**Backend (`src-tauri/src/survey/`):**

- `aggregator.rs` — `SurveySessionAggregator`; attributes chat loot gains to survey uses (`attribute_chat_gain`, identity + recency window), records manual uses, manages sessions
- `persistence.rs` — CRUD for `survey_sessions` and `survey_uses`
- `multihit_state.rs` — DB-backed open multihit nodes (survives restart)
- `commands.rs` — all `survey_tracker_*` Tauri commands (status, start/end, detail, historical, analytics, notes, delete)
- `types.rs` — `SurveySession`, `SurveyUse`, `SurveyUseKind`, `SurveyUseStatus`
- `replay_tests.rs` — integration tests against paired log datasets

**Frontend:**

- `src/stores/surveyTrackerStore.ts` — reads/caches backend state; subscribes to events
- `src/components/Economics/EconomicsSurveyView.vue` — three-tab container (Session / Session History / Analytics)
- `src/components/Surveying/SurveyTrackerView.vue` — Session tab (PaneLayout: recent sessions / live detail / selected session)
- `src/components/Surveying/HistoricalTab.vue` — expandable session list with notes + delete; each row shows cost / revenue / profit inline
- `src/components/Surveying/AnalyticsTab.vue` — PaneLayout: `AnalyticsNav` on the left, view components in the center, `ItemCostCalculator` on the right
- `src/components/Surveying/SessionSummary.vue` — shared detail block rendered by both the active-session view and any historical session expansion
- `src/components/Surveying/StatCard.vue`, `SurveyLootGrid.vue`, `ZoneRewardsCard.vue`, `CrossZoneComparison.vue` — reusable pieces used across the three tabs
- `src/components/Surveying/AnalyticsOverviewView.vue`, `AnalyticsZoneView.vue`, `AnalyticsTypeView.vue` — Analytics center-pane views

### Data flow

```text
Chat.log ──► ChatStatusParser ──► chat gains ──┐
                                              ├──► coordinator ──► SurveySessionAggregator
Manual use ──► record_manual_use ──────────────┘
                              │
                              ▼
             survey_sessions, survey_uses,
             open_multihit_nodes, item_transactions
                              │
                              ▼
               Tauri commands ──► frontend (read-only)
```

Live updates emit as `survey-tracker-session-started/ended` and `survey-tracker-use-recorded/completed`. The frontend subscribes and refreshes affected views. (No multihit open/close event exists — open nodes are read via the `survey_tracker_status` command.)

### Attribution (A3 stitching)

Every gain transaction carries a `source_kind` + `source_details` JSON. When the aggregator classifies a survey use, subsequent gains that fall in its window are tagged with `survey_use_id` in `source_details`. The Analytics and Historical queries join back through this key — no separate loot tables needed.

Window semantics per kind:

| Kind | Window | Notes |
|---|---|---|
| Basic | Same tick as consumption | Direct `SurveyMapUse` provenance |
| Motherlode | One Mining cycle within 60s of consumption | Chains consumption → mining context |
| Multihit | Until the player mines a different `entity_id` OR 30 minutes pass | `open_multihit_nodes` tracks the active node |

A tie-breaker in `compute_provenance` picks `Mining` over passive overlapping contexts (CorpseSearch, VendorBrowsing, StorageBrowsing) with `Probable` confidence — this is what lifted Multihit attribution from ~60% to ~99% on the povus validation set.

## Database schema

Migration **v26** introduced:

- `survey_sessions` — session header (started_at, ended_at, start_trigger, crafted_count, consumed_count, notes)
- `survey_uses` — one row per map use (session_id FK, kind, area, status, loot_qty, used_at, map_internal_name, map_display_name)
- `open_multihit_nodes` — per-character open multihit attributions keyed by `node_entity_id`

Migration **v27** dropped the legacy `survey_loot_items` / `survey_events` / `survey_session_stats` / `survey_imports` tables. The `survey_types` CDN reference table is retained (wiped and reloaded on CDN update).

`item_transactions.source_details` carries the `survey_use_id` field for gains attributed to a survey — this is the join key for all downstream queries.

## Tabs

### Session (unified view)

`SurveyTrackerView.vue` — `PaneLayout` with three panels that serve both active and historical sessions in one view (the previous "Session History" tab has been merged in):

- **Left pane**: "Start New Session" button (disabled when active exists) + "Auto-start sessions" checkbox (persisted in `AppSettings.auto_start_survey_sessions`). Below: the active session card pinned at top with a green border, then filter/sort controls (`SearchableSelect` for zone, sort by date/profit/duration/surveys via `useViewPrefs`), then historical session cards. Each `SessionCard.vue` shows: editable name, zone(s) via `AreaInline`, survey count, reactive profit/hr, date + duration.
- **Center pane**: rich detail for the selected session, laid out as:
  - Top row (grid 2/5 + 2/5 + 1/5, fixed 320px height with internal scroll): `LootOverviewPanel.vue` (item table with `ItemMinicard` + bonus split + inline market editing), `LootDonutChart.vue` (`VueUiDonut` of loot distribution, items <3% bucketed into "Other"), `TimeBreakdownPanel.vue` (start/end times respecting user overrides with inline editing for ended sessions, total/craft/prep/survey durations, avg per survey — live-ticking for active sessions; shows date+time when start/end span multiple calendar days).
  - Second row: `LootByTypePanel.vue` (accordion per survey type showing use count, avg loot, expandable per-item primary + bonus breakdown with `ItemInline`).
  - Open multihit nodes shown below for active sessions.
- **Right pane**: editable session name + notes, craft material cost breakdown (`CraftMaterialRow` from `recipe_ingredients` joined through `survey_types.recipe_id`, each ingredient shown via `ItemInline` with quantity and cost), revenue + revenue/hr, profit + profit/hr (both total-session-time and survey-only-time rates, color-coded), and delete button. All economics reactive via `useLiveValuation`.

Selecting a session in the left list loads its `SurveySessionDetail` into both the center and right panels. The active session auto-selects on load.

New backend columns (migration v29) support the richer session data: `name`, `user_started_at`/`user_ended_at` (user-adjustable time overrides), `first_craft_at`/`last_craft_at` (craft timing), `first_loot_at`/`last_loot_at` (loot timing). New commands: `survey_tracker_update_session_name`, `survey_tracker_update_session_times`.

### Analytics

`AnalyticsTab.vue` — `PaneLayout` with three slots:

- **Left pane** (`AnalyticsNav.vue`): buttons for Overview, each zone, each survey type. Counts are shown inline; the currently-selected view is marked with a gold left-border.
- **Center pane**: one of three views depending on selection.
  - `AnalyticsOverviewView.vue` — top stat cards (total surveys, speed-bonus rate, bonus items, zones), cross-zone comparison table (`CrossZoneComparison.vue`, sortable), and an all-survey-types table. Both tables are clickable to drill into the zone or type view.
  - `AnalyticsZoneView.vue` — zone-scope stat cards, one `ZoneRewardsCard` per survey type used in the zone, plus a zone-wide items rollup.
  - `AnalyticsTypeView.vue` — per-type stat cards, the type's item breakdown, and matching same-map entries in other zones when present.
- **Right pane** (`ItemCostCalculator.vue`): picks an item, desired quantity, sell price, and a sort mode (Cost / Time / Profit/hr); ranks the survey types that drop the item accordingly. The best row is highlighted with a gold border + "BEST" badge. Footer lines show the primary/bonus breakdown per row.

Data sources:

- Center-pane views read from `survey_tracker_analytics()`, which already carries per-zone and per-type item breakdowns.
- The calculator reads from `survey_tracker_item_cost_analysis()`, a dedicated command that returns one row per (item, survey type, zone) with primary/bonus splits and the per-type average survey duration.

All metrics derive from the `item_transactions` ↔ `survey_uses` join. Speed-bonus rates and bonus-item totals would come from the `is_speed_bonus` flag in `source_details` — but no production writer sets that flag today (only test fixtures do), so bonus metrics read zero on real data. The designed producer is a `(speed bonus!)` ScreenText handler in `chat_status_dispatch` — not yet implemented.

### Speed-bonus attribution

Basic surveys can grant bonus drops; the game signals this with a ScreenText line like `"Blue Spinel collected! Also found Rubywall Crystal x2 (speed bonus!)"` that arrives after the gain rows are staged. The designed consumer is a ScreenText handler in `chat_status_dispatch` that would look up the most recent Basic `survey_use_id` and patch `item_transactions.source_details` for each bonus-item row to set `is_speed_bonus: true` — that handler does not exist yet, so the flag is never set in production and the analytics below read zero on real data. `mark_speed_bonus` in `survey/commands.rs` is a test helper, not a producer.

## Economics

Each session carries a precomputed `SessionEconomics` rollup that both the Session tab and the History expansion render via the shared `SessionSummary.vue` component:

- **Cost** — sum of `survey_types.crafting_cost` for each consumed map. The CDN ingestion step populates `crafting_cost` using nominal vendor-buy pricing of fully-consumed recipe ingredients (see `cdn_persistence::insert_survey_types`).
- **Revenue** — `Σ(item_transactions.quantity × market_values.market_value)` over this session's gains, left-joined so unpriced items contribute zero and the UI can flag the gap.
- **Bonus revenue** — same sum but restricted to rows where `source_details.is_speed_bonus = true`. Currently zero on real data (no producer sets the flag); it will show how much of the profit came from bonus drops once the ScreenText handler lands.
- **Profit** = revenue − cost.
- **Priced / unpriced counts** — surfaced in the UI so users know how much of the revenue estimate is tentative.

The UI derives **profit-per-hour** and **ETA** on the frontend from the economics + live elapsed time — no backend work per tick.

For the History rollup, `survey_tracker_historical_sessions` runs the same economics helper per session (one loot-summary + one cost query each, both indexed) so each row can show cost/revenue/profit inline without needing a detail fetch.

## GorgonSurveyTracker Integration

The left pane includes a `GstLauncher.vue` widget that manages [GorgonSurveyTracker](https://github.com/kaeus/GorgonSurveyTracker) — a third-party survey overlay tool.

**Backend** (`src-tauri/src/gst_manager.rs`):

- `gst_check_status` — reports the current platform, checks whether `GorgonSurveyTracker.exe` exists in the app data `gst/` subfolder (Windows only), reads the locally stored version tag, and fetches the latest release from the GitHub API to detect updates.
- `gst_download` — (Windows only) downloads the latest release asset to `<app_data>/gst/GorgonSurveyTracker.exe` (via a temp file + rename) and writes a `gst_version.txt` sidecar for version tracking.
- `gst_launch` — (Windows only) spawns the exe as a detached process.

**Frontend** (`src/components/Surveying/GstLauncher.vue`):

- **Windows**: shows Download/Launch/Update buttons with version info.
- **macOS/Linux**: shows a note that Python 3.8+ is required with a pointer to the GST page for platform-specific setup steps (PyQt5, pynput, plus pyobjc-framework-Cocoa on macOS or python-xlib on Linux X11).
- **All platforms**: shows a "GST Homepage & Instructions" link that opens the GitHub repo page in the user's browser.

## Known follow-ups

- **Cost model preference** — current cost uses nominal vendor-buy pricing. A future enhancement could offer "use market value of ingredients" as an alternative, toggleable like the Crafting view's valuation modes.
- **Remaining off-by-1 misses** — the povus multihit dataset still has 3 items off by 1 (Gold Nugget 14/15, Expert-Quality Metal Slab 0/1, Amazing Metal Slab 0/1) — likely `UpdateItemCode`-as-baseline edge cases for rare single-instance drops. All 5 basic survey datasets now achieve **100.0% accuracy**. Overall accuracy is 99.9% across all 6 datasets (101/104 items exact).

## Time handling

Follows the app-wide conventions in [time.md](../../../architecture/time.md). All timestamps in `survey_sessions` and `survey_uses` are UTC strings (`YYYY-MM-DD HH:MM:SS`). Player.log timestamps are already UTC; chat timestamps pass through `chat_local_to_utc()` using the detected timezone offset before any survey logic runs.

### Session bounds are computed from events, not wall-clock

`started_at` and `ended_at` on `survey_sessions` are not sampled from `Utc::now()` at start/end time. Instead, when a session closes (manual end, auto-end on crafting-count match, or the dual-log replay equivalent), the aggregator calls `recompute_session_bounds_and_end()` — this re-derives both bounds from the first and last event timestamps actually attributed to the session via `survey_uses.used_at` and `item_transactions.timestamp` (joined by `source_details->>'survey_use_id'`). The wall-clock moment of the end event is only used as a fallback when a session had no uses at all.

This matters for two cases:

- **Live**: bounds are slightly tighter — they reflect when real activity started/stopped rather than when the user clicked the button.
- **Replay / old-log reparse**: `Utc::now()` is meaningless, so the recompute is what produces correct historical bounds. The replay path additionally sets a `base_date` override on `GameStateManager` and `SurveySessionAggregator` so `HH:MM:SS` Player.log times get stamped with the replayed date during event ingestion. The auto-end path now passes the current event's timestamp (derived from the log via `to_utc`) as the fallback instead of wall-clock `Utc::now()`, preventing multi-day gaps when replaying old logs.

Users can manually override start/end times via inline editing in `TimeBreakdownPanel.vue` (pencil icon). Overrides are stored in `user_started_at`/`user_ended_at` and take precedence over the auto-derived times in all duration calculations (both frontend display and `historical_session_rows` `duration_seconds`).
