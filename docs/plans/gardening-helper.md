# Gardening Helper

Real-time garden plot tracking with inventory integration, complementary to the almanac widget.

## Data Source

`EntityDescriptionUpdated` events from [player_event_parser.rs](../../src-tauri/src/player_event_parser.rs) provide per-plot state:

- **entity_id**: persistent per plot, enables lifecycle tracking
- **name**: state prefix + crop (e.g., "Ripe Onion", "Thirsty Barley")
- **description**: human-readable state ("This barley needs water!")
- **action**: interaction verb ("Water Barley", "Harvest Potato")
- **appearance**: crop visual + Scale factor (correlates with growth progress)

State machine: Thirsty → Growing → Hungry → Growing → Ripe

## Coordinator Handler

Add `EntityDescriptionUpdated` match arm in [game_state.rs](../../src-tauri/src/game_state.rs). Parse crop name from action field, classify state from description, extract scale from appearance. Upsert into `garden_plots` table.

## Data Model

`garden_plots` table (migration): character, server, entity_id (PK), crop_name, state, action, scale, area_name, first_seen_at, last_updated_at.

## Inventory Integration

Query `game_state_inventory` for gardening items (seeds, fertilizer, water) filtered by CDN keywords or name patterns. Cross-reference: thirsty plots → water count, hungry plots → fertilizer count.

## UI: Dashboard Widget

Medium-size widget with:

- Summary bar: "3 need water | 1 needs fertilizer | 2 growing | 4 ready"
- Plot list grouped by area, state-colored badges
- Inventory sidebar showing relevant supply counts
- Almanac cross-reference: gold badge when crop+zone has active bonus

## Relationship to Almanac Widget

- **Almanac**: "What should I plant?" (server-wide bonus events)
- **Garden Plots**: "What do my plants need right now?" (player's actual plots)
- Cross-reference: highlight planted crops with active almanac bonuses
- Both live under the single "Gardening" dashboard widget, as tabs or sections

## Phases

### Phase 0: Almanac widget + history (DONE)

- `get_garden_almanac` Tauri command (history command removed with the dead history table in v102)
- Migration v41: `garden_almanac` + `garden_almanac_history` tables
- Coordinator replaces the snapshot on each almanac ingest (rescan-safe no-op; history writes removed in v102)
- `GardenAlmanacWidget.vue` registered as "Gardening" in dashboardWidgets.ts
- Shows active bonuses with countdown timers, upcoming events, and empty-state prompt

### Phase 1: Backend event handling + persistence

- EntityDescriptionUpdated handler in game_state.rs
- Garden state classification function
- Migration for garden_plots table
- `get_garden_plots` Tauri command

### Phase 2: Basic widget

- Add garden plots tab/section to Gardening widget
- Plot list grouped by area, state-colored badges

### Phase 3: Inventory integration

- `get_garden_inventory` command
- Supply count display, cross-reference with plot states

### Phase 4: Almanac cross-reference

- Query garden_almanac for active bonuses, show badges on matching plots

### Phase 5: History + analytics (future)

- garden_plot_history table, average grow times, yield tracking
- Season rotation patterns (needs a new history design — v102 dropped the unconsumed `garden_almanac_history` log)

## Key Files

- [game_state.rs](../../src-tauri/src/game_state.rs) — add EntityDescriptionUpdated handler
- [migrations.rs](../../src-tauri/src/db/migrations.rs) — garden_plots table
- [game_state_commands.rs](../../src-tauri/src/db/game_state_commands.rs) — new commands
- [capture-analysis-results.md](../../docs/architecture/capture-analysis-results.md) — state machine documentation
