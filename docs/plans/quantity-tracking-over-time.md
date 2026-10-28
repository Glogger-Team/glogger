# Quantity Tracking Over Time

Time-series tracking of currencies and item quantities with charts and trend analysis.

## Data Sources

1. **Character reports** (`character_currencies`): 14 currency types, overwritten on each export (no historical backfill from files)
2. **Storage reports** (`character_snapshot_items`): PG doesn't overwrite — historical backfill possible
3. **Item transactions** (`item_transactions`): append-only ledger with provenance (items only, not currencies)
4. **Chat currency events**: `CouncilsChanged`, `CoinsLooted` — parsed but NOT persisted currently (critical gap)
5. **Game state tables**: current values only, no history

## Data Model

### `currency_quantity_history` (Phase 1)
Columns: timestamp, character, server, currency_key, amount (absolute), delta, source (snapshot_import/chat_event/periodic/session_boundary). Indexed on (char, server, currency_key, timestamp).

### `item_quantity_history` (Phase 2)
Columns: timestamp, character, server, item_name, item_type_id, inventory_count, storage_count, total_count, snapshot_type. Indexed on (char, server, item_name, timestamp).

## Recording Strategy: Hybrid

**Currencies:**
1. Event-driven: persist CouncilsChanged/CoinsLooted from chat to currency_quantity_history
2. Report-based: write absolute values on Character report import
3. Periodic: record `game_state_currencies` every 30 minutes while active

Event-driven alone is insufficient — only Councils changes detected from chat, other currencies have no chat events.

**Items:**
Periodic reports of `game_state_inventory` + `game_state_storage` aggregated by item_name. Item_transactions provides event-level detail; quantity history adds point-in-time totals.

## Backfill

- **Inventory reports**: scan all historical `*_items_*.json` files (not overwritten by PG)
- **Existing reports**: mine `character_snapshots` + `character_currencies` already in DB
- One-time migration backfill on first run

## Query Patterns

- "How much X on date Y?" — `ORDER BY timestamp DESC LIMIT 1` with compound index
- "Graph gold over time" — range query returning time-series for direct chart rendering
- "Net change this week" — two point-in-time queries with subtraction

## UI

Charts via `vue-data-ui` (VueUiXy line/area charts). Lives in Economics view:
- **CurrencyTrendsView**: currency picker using existing CURRENCY_DISPLAY_ORDER, date range selector, line chart
- **ItemTrendsView**: searchable item picker, date range, chart
- Time range presets: 24h, 7d, 30d, all time + custom

## Phases

### Phase 1: Currency history infrastructure
- Migration: `currency_quantity_history` table
- Tauri commands for insert/query
- Write to history on character report import
- Backfill from existing `character_snapshots`

### Phase 2: Live currency event persistence
- Coordinator persists CouncilsChanged/CoinsLooted to history
- Periodic 30-min report of `game_state_currencies`

### Phase 3: Currency trends UI
- CurrencyTrendsView with VueUiXy chart, currency picker, date range

### Phase 4: Item quantity history
- Migration: `item_quantity_history` table
- Periodic reports of inventory+storage
- Backfill from historical inventory reports

### Phase 5: Item trends UI
- ItemTrendsView with item picker + chart
- Dashboard widget for pinned item trends

## Key Files

- [character_commands.rs](../../src-tauri/src/db/character_commands.rs) — currency import flow
- [inventory_commands.rs](../../src-tauri/src/db/inventory_commands.rs) — inventory import
- [chat_status_parser.rs](../../src-tauri/src/chat_status_parser.rs) — CouncilsChanged events
- [coordinator.rs](../../src-tauri/src/coordinator.rs) — event persistence point
- [LootDonutChart.vue](../../src/components/Surveying/LootDonutChart.vue) — vue-data-ui chart pattern
- [owned-quantity-tracking.md](owned-quantity-tracking.md) — earlier investigation notes
