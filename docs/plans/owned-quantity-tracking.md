# Owned Quantity Changes Over Time — Investigation

## Goal

Track how total owned quantities of items change over time, starting with currencies and expanding to other items. This enables trend analysis ("am I gaining or losing councils?"), historical views, and rate-of-change insights.

## What Already Exists

The codebase has a robust item tracking pipeline that gives us strong foundations:

### Parser Layer (`src-tauri/src/player_event_parser.rs`)

Three core item events are already emitted:

- **ItemAdded** — from `ProcessAddItem`. Registers instance in `instance_registry`, seeds stack_size=1 for new items (`slotIndex=-1, isNew=True`). Storage withdrawals (`slotIndex>=0`) defer seeding to `ProcessRemoveFromStorageVault`.
- **ItemStackChanged** — from `ProcessUpdateItemCode`. Decodes `((stackSize-1) << 16) | itemTypeId` (0-based encoding, +1 for actual count), computes signed delta against previous stack. Suppresses first update for existing items (baseline establishment).
- **ItemDeleted** — from `ProcessDeleteItem`. Uses 1-line lookahead to classify context: `StorageTransfer`, `VendorSale`, `Consumed`, or `Unknown`.

### State Tracking

- **`instance_registry`** — `HashMap<u64, InstanceInfo>` maps instance_id to item name + type_id.
- **`stack_sizes`** — `HashMap<u64, u32>` tracks last known stack per instance.
- **`pending_deletes`** — 1-line lookahead buffer for delete context resolution.

### Database Tables

- **`game_state_inventory`** — Current inventory state per character/server. Columns: instance_id, item_name, item_type_id, stack_size, slot_index, last_confirmed_at, source.
- **`game_state_storage`** — Same structure but per vault. Keyed by (character, server, vault_key, instance_id).
- **`item_transactions`** (v15 migration) — Append-only audit trail of every item gain/loss. Columns: timestamp, character_name, server_name, item_name, internal_name, item_type_id, quantity (signed), context (loot/vendor_sell/storage_deposit/etc.), source (player_log/chat_status), instance_id, vault_key.

### Chat Cross-Validation (`src-tauri/src/chat_status_parser.rs`)

- **ItemGained** — parsed from `"X added to inventory."` / `"X xN added to inventory."` chat lines.
- **`correct_stack_from_chat()`** — fixes Player.log's stack_size=1 artifact using the authoritative chat quantity.
- Chat-sourced transactions recorded separately with `source='chat_status'`.

### Frontend (`src/stores/gameStateStore.ts`)

- **`liveItemMap`** — in-memory per-session inventory state.
- **`liveEventLog`** — last 50 inventory events.
- **`inventoryItemCounts`** — computed property merging DB + live inventory into `Record<itemName, totalQuantity>` (excludes storage).
- **`heldItemCounts`** — items on the player (inventory + saddlebag when tracked).
- **`ownedItemCounts`** — total owned across inventory + storage.

### Storage Report Import (`src-tauri/src/db/inventory_commands.rs`)

- Export Storage as JSON (Settings → VIP) import seeds `game_state_inventory` and `game_state_storage`.
- Uses `character_item_snapshots` and `character_snapshot_items` tables.
- PG doesn't overwrite old inventory reports in the reports folder — historical backfill source.

## What's Missing

### No Time-Series Storage

Current tables track **current state** only. `item_transactions` has the raw events but no pre-aggregated "total owned at time T" series. Answering "how many councils did I have last Tuesday?" requires replaying all transactions from a known baseline — expensive and fragile without a starting Character/Storage report.

### No Historical Quantity Reports

There's no mechanism to periodically record "at this moment, you own X of item Y across inventory + storage." The `game_state_*` tables are overwritten as state changes, so historical values are lost.

### No Currency-Specific Tracking

Currencies (councils, gold, etc.) may appear in:
- Character JSON exports (overwritten on each export though)
- Inventory reports (not overwritten — historical backfill possible)
- Chat status messages
- The context bar (`ContextBar.vue`) reads currencies from game state

There's no dedicated currency tracking path — they're just items in the general system.

### No Aggregation or Rollup Queries

No backend commands exist for:
- Item transaction history by date range
- Net quantity change over a period
- Rate of gain/loss (items/hour, items/day)

### No Trend UI

No charts, graphs, or historical views for quantity data.

## Currency Data (Resolved)

Currencies are **not** inventory items. They live in a dedicated `Currencies` object in the character export JSON, completely separate from the item system:

```json
"Currencies": {
    "GOLD": 105912,
    "GUILDCREDITS": 30,
    "REDWINGTOKENS": 0,
    "DRUIDCREDITS": 0,
    "WARDENPOINTS": 0,
    "FAEENERGY": 0,
    "LIVEEVENTCREDITS": 24,
    "GLAMOUR_CREDITS": 37,
    "COMBAT_WISDOM": 25504,
    "BLOOD_OATHS": 0,
    "VIDARIA_RENOWN": 470,
    "STATEHELM_RENOWN": 640,
    "STATEHELM_DEMERITS": 0,
    "NORALA_TOKENS": 0
}
```

14 currency types total. These do **not** appear in Storage export item files — those only contain actual items (~1,078 items across ~34 storage vaults in the sample). Character exports are overwritten on each export, so they can't be used for historical backfill.

This means currency tracking and item quantity tracking are two separate data paths:
- **Currencies:** Must come from character export imports (report-seeded `character_currencies`) or from parsing chat/log events (if currency change messages exist).
- **Items:** Come from the existing inventory pipeline (`game_state_inventory` + `game_state_storage`) and can be backfilled from old Storage export (`*_items_*.json`) reports.

## Storage Size Estimate

A flat `(timestamp, item_name, count)` table is simple and plenty efficient:

- ~1,078 items in a typical export. A report row is ~100 bytes.
- 4 hours/day play, reports every 30 min = 8 reports/day = ~8,600 rows/day.
- **1 year ≈ 3.1M rows ≈ 300 MB. 10 years ≈ 3 GB.**
- SQLite handles this without issue. No need for a two-table report/entries split — a single flat table is simpler and the data volume doesn't justify the complexity.

## Key Design Questions

### 1. What's the "owned quantity" boundary?

Does "owned" mean inventory only, or inventory + storage? Storage is persistent and under the player's control, so probably both. But what about items in shop stalls (not yet tracked) or items mailed to others?

**Recommendation:** Start with inventory + storage. It's what we have data for.

### 2. Report-based vs. transaction-replay?

**Option A: Periodic reports** — Record total owned counts at regular intervals (every N minutes while active, on session start/end). Simple to query ("what was my count at time T?"), but misses granularity between reports.

**Option B: Transaction replay from baseline** — Use `item_transactions` to reconstruct state at any point from a known baseline. Precise, but expensive for large histories and requires a reliable baseline.

**Option C: Hybrid** — Take periodic reports AND keep the transaction log. Query reports for rough trends, replay transactions for precise windows.

**Recommendation:** Option C (hybrid). Periodic reports (e.g., every 30 minutes while active + on session boundaries) give cheap trend queries. Transaction log fills in detail when needed.

### 3. Report granularity

Report everything — storage is cheap (see estimate above), and deciding what to track retroactively is painful. Display is the filtering layer.

## Potential Schema

A single flat table:

```sql
item_quantity_history (
    timestamp TEXT NOT NULL,
    character_name TEXT NOT NULL,
    server_name TEXT NOT NULL,
    item_name TEXT NOT NULL,
    item_type_id INTEGER,
    inventory_count INTEGER NOT NULL DEFAULT 0,
    storage_count INTEGER NOT NULL DEFAULT 0,
    total_count INTEGER NOT NULL DEFAULT 0,
    snapshot_type TEXT NOT NULL,  -- 'periodic', 'session_start', 'session_end', 'import'
    PRIMARY KEY (timestamp, character_name, server_name, item_name)
)
```

Currencies would use a similar table (or the same one with a flag), recording from character export imports:

```sql
currency_quantity_history (
    timestamp TEXT NOT NULL,
    character_name TEXT NOT NULL,
    server_name TEXT NOT NULL,
    currency_key TEXT NOT NULL,  -- 'GOLD', 'COMBAT_WISDOM', etc.
    amount INTEGER NOT NULL,
    source TEXT NOT NULL,  -- 'character_export', 'chat_event'
    PRIMARY KEY (timestamp, character_name, server_name, currency_key)
)
```

## Potential Implementation Phases

### Phase 1: Item Quantity Reports
- New migration for `item_quantity_history` table
- Backend logic to record current owned quantities from `game_state_inventory` + `game_state_storage`
- Trigger reports on session start, session end, and periodic timer
- Backend query commands for retrieval

### Phase 2: Currency Tracking
- New migration for `currency_quantity_history` table
- Parse currencies from character export imports
- Investigate whether chat/log events exist for currency changes (e.g., "You received 500 councils")

### Phase 3: Historical Backfill
- Import old Storage export (`*_items_*.json`) reports as historical item reports
- Import old character exports for historical currency reports (if any exist un-overwritten)

### Phase 4: Trend UI
- Simple trend view — table or basic chart showing quantity over time
- Filterable by item/currency
- Rate-of-change calculations from report deltas

### Phase 5: Dashboard Integration
- Dashboard card for critical resource trends
- Net gain/loss summaries per session

## Remaining Open Questions

1. **Are there chat/log messages for currency changes?** (e.g., "You received 500 councils") — would provide event-level currency tracking between character exports.
2. **Session boundary detection** — Is there a reliable session start/end event to trigger reports? The parser may already have this.
3. **Character export frequency** — How often do users run Export Character as JSON? If rare, currency tracking between exports will have big gaps unless we find chat/log events for currency changes. Enable Automatically Export on Logout (Settings → VIP) to keep exports fresh — it also re-exports on every zone change.
