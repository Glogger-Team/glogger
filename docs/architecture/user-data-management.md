# User Data Management

How the app scopes, stores, and queries data across multiple characters and servers.

---

## Data Scoping Model

All persistent data falls into one of three scopes:

### Character-Scoped (`character_name + server_name`)

Data belonging to a specific character on a specific server:

- **Game state** — skills, inventory, equipment, recipes, favor, currencies, attributes, effects, active skills, combat, mount (all `game_state_*` tables)
- **Character reports** — Export Character as JSON (Settings → VIP) exports
- **Storage reports** — point-in-time inventory records
- **Session tracking** — XP rates, skill deltas, live inventory (in-memory, resets on login)
- **Crafting projects** — work orders, ingredient tracking
- **Gourmand progress** — food tracking per character

### Server-Scoped (`server_name`)

Data shared across all characters on a server:

- **Market values** — user-specified player-to-player prices (or universal with `server_name = "*"`)
- **Vendor gold tracking** — vendor gold pools are server-wide
- **Survey data** — surveying sessions and results

### Global (no key / user-scoped)

- **App settings** — paths, startup behavior, UI preferences
- **CDN game data** — items, skills, abilities, recipes, NPCs, quests (same across all servers)

---

## Server Management

### Auto-Detection

Chat logs contain the server name on login:

```
26-03-22 18:12:49  **************************************** Logged In As Zenith. Server Dreva. Timezone Offset -07:00:00.
```

Parsing flow:
1. `ChatLogWatcher` detects login lines via `parse_chat_login_line()` in `src-tauri/src/chat_parser.rs`
2. Emits `LogEvent::ServerDetected { server_name, character_name }` **before** `LogEvent::CharacterLogin`
3. Coordinator handles `ServerDetected`: auto-creates server record in `servers` table, updates `active_server_name` in settings, emits `server-detected` event to frontend
4. Frontend `gameStateStore` listens for `server-detected` and updates `settingsStore.settings.activeServerName`

### Servers Table

```sql
CREATE TABLE servers (
    server_name TEXT PRIMARY KEY,
    display_name TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Servers are auto-created when first detected from chat logs. A `"Default"` server is available as fallback for users who haven't tailed a chat log yet.

### Manual Fallback

The character picker allows manually adding characters with a server name. Known servers populate a datalist for autocomplete.

---

## Character Management

### Character Switching

Character switches happen automatically when `"You are now logged in as X"` appears in Player.log. The coordinator:

1. Registers the character in `user_characters`
2. Updates `active_character_name` in settings
3. Sets the active character in `GameStateManager`
4. Emits `character-login` to frontend

The frontend resets session state and reloads all game state data on character login.

### Character Picker

`src/components/CharacterPicker.vue` — dropdown in the MenuBar header providing:

- List of all known characters (from `user_characters` table) with server names displayed
- Switch characters (calls `set_active_character`, reloads game state)
- Add new characters manually (with server input + datalist of known servers)
- Delete characters with confirmation dialog

### Character Deletion

`delete_character` command in `src-tauri/src/setup_commands.rs` cascade-deletes across all character-scoped tables:

- All 12 `game_state_*` tables
- `character_snapshots` + child tables (`character_snapshot_skills`, `character_snapshot_recipes`)
- `inventory_snapshots` + child tables (`inventory_snapshot_items`)
- `user_characters`

Uses a transaction. Clears active character from settings if the deleted character was active.

---

## Game State Tables — Server Isolation

All `game_state_*` tables include `server_name` in their primary key to prevent data collision when the same character name exists on different servers.

Example composite PKs:

| Table | Primary Key |
|---|---|
| `game_state_skills` | `(character_name, server_name, skill_name)` |
| `game_state_inventory` | `(character_name, server_name, instance_id)` |
| `game_state_equipment` | `(character_name, server_name, slot)` |
| `game_state_combat` | `(character_name, server_name)` |
| `game_state_mount` | `(character_name, server_name)` |
| `game_state_active_skills` | `(character_name, server_name)` |
| `game_state_attributes` | `(character_name, server_name, attribute_name)` |
| `game_state_recipes` | `(character_name, server_name, recipe_id)` |
| `game_state_favor` | `(character_name, server_name, npc_name)` |
| `game_state_currencies` | `(character_name, server_name, currency_name)` |
| `game_state_effects` | `(character_name, server_name, effect_instance_id)` |

`GameStateManager` tracks `active_character` and `active_server`, passing both to all DB writes and queries. See [game-state.md](game-state.md) for the full game state architecture.

---

## Market Values

User-specified prices for items — "how much this sells for to other players."

### Schema

```sql
CREATE TABLE market_values (
    server_name TEXT NOT NULL,        -- "*" for universal mode
    item_type_id INTEGER NOT NULL,
    item_name TEXT NOT NULL,          -- denormalized for display
    market_value INTEGER NOT NULL,    -- in councils
    notes TEXT,                       -- optional user notes
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (server_name, item_type_id)
);
```

### Universal vs Per-Server Mode

Controlled by the `market_price_mode` setting (`"universal"` default, or `"per_server"`).

- **Universal mode**: All rows use `server_name = "*"`. One price per item globally.
- **Per-server mode**: Rows keyed by actual server name. Different servers can have different economies.

Switching modes preserves data — universal prices remain and can serve as defaults.

The `resolve_server()` helper in `src-tauri/src/db/market_commands.rs` transparently maps the mode to the correct `server_name` value across all 6 market commands.

### Backend Commands

Six Tauri commands in `src-tauri/src/db/market_commands.rs`:

| Command | Purpose |
|---|---|
| `get_market_values` | List all market values (filtered by resolved server) |
| `get_market_value` | Single item lookup by `item_type_id` |
| `set_market_value` | Create or update a market value |
| `delete_market_value` | Remove a market value |
| `export_market_values` | Export all values as JSON |
| `import_market_values` | Import JSON with conflict resolution |

### Import/Export

Export produces JSON for sharing with other users. Import supports 3 conflict resolution strategies:

- **`newest`** — compare `updated_at` timestamps, keep whichever is newer
- **`overwrite`** — imported values always win
- **`keep_existing`** — existing values always win

### Frontend

- **`src/stores/marketStore.ts`** — Pinia store with `values`, `valuesByItemId`, `valuesByName` computed maps and CRUD actions
- **`src/components/Shared/Item/ItemTooltip.vue`** — Shows market value alongside vendor value with staleness indicator (e.g., "3 days ago"). Inline edit/remove controls.
- **`src/components/Market/MarketView.vue`** — Full page for bulk management: searchable/sortable table, inline editing, add form, mode toggle, clipboard export, JSON import with strategy picker

---

## Item Valuation Mode

Controls which value source is used when calculating item worth (aggregate wealth, inventory value). Configured via the `item_valuation_mode` setting, selectable on the Market page.

### Modes

| Mode | Calculation | Description |
|---|---|---|
| `highest_market_vendor` (default) | `max(market, vendor)` | Uses whichever is higher between market and vendor value |
| `highest_market_buy_used` | `max(market, vendor * 2)` | Uses whichever is higher between market and buy-used value |
| `vendor_only` | `vendor` | CDN vendor value only |
| `buy_used_only` | `vendor * 2` | Buy-used price (2x vendor value) |
| `market_only` | `market` | User-set market value only |

### Behavior

- **Tooltips always show all values** — vendor, buy-used, and market (when set) are displayed regardless of mode. The "Effective" line shows which value is actually used in calculations.
- **Wealth calculations** use the mode to resolve per-item value in `get_aggregate_wealth` (joins CDN `items` table for vendor value alongside `market_values`).
- The `resolve_item_value()` helper in `src-tauri/src/settings.rs` implements the mode logic and is shared across backend calculations.

### Value Terminology

- **Vendor value**: The CDN `Value` field on items — what an NPC vendor pays for the item
- **Buy-used value**: 2x vendor value — the typical NPC price to buy a used item
- **Market value**: User-specified player-to-player price stored in `market_values` table

---

## Aggregate Views

Cross-character queries for a server, accessible via the "All Characters on Server" toggle on the Dashboard.

### Backend Commands

Three Tauri commands in `src-tauri/src/db/aggregate_commands.rs`:

| Command | Returns |
|---|---|
| `get_aggregate_inventory` | All items across characters, grouped by item name with per-character stack size breakdown |
| `get_aggregate_wealth` | Total currencies + inventory value (using item valuation mode), with per-character wealth breakdown |
| `get_aggregate_skills` | All skills across characters for side-by-side comparison |

Wealth computation joins `game_state_inventory` with both `market_values` and CDN `items` (for vendor value), then applies `resolve_item_value()` per item based on the active item valuation mode.

### Frontend

- **`src/stores/aggregateStore.ts`** — Pinia store loading aggregate inventory, wealth, and skills for the active server
- **`src/components/Dashboard/AggregateView.vue`** — Wealth summary cards, per-character wealth breakdown table, searchable combined inventory table, collapsible cross-character skill comparison grid
- **`src/components/Dashboard/DashboardView.vue`** — "Active Character" / "All Characters on Server" toggle; aggregate view loads on switch

---

## Key Files

| Layer | File | Role |
|---|---|---|
| Schema | `src-tauri/src/db/migrations.rs` | All table definitions including `servers`, `game_state_*`, `market_values` |
| Server detection | `src-tauri/src/chat_parser.rs` | `parse_chat_login_line()` extracts character + server from chat log |
| Server detection | `src-tauri/src/log_watchers.rs` | `ServerDetected` LogEvent emitted from ChatLogWatcher |
| Coordinator | `src-tauri/src/coordinator.rs` | Handles `ServerDetected` and `CharacterLogin` events |
| Game state | `src-tauri/src/game_state.rs` | `GameStateManager` with server-scoped writes |
| Game state queries | `src-tauri/src/db/game_state_commands.rs` | All query commands accept `server_name` |
| Character management | `src-tauri/src/setup_commands.rs` | `delete_character` cascade delete |
| Market values | `src-tauri/src/db/market_commands.rs` | CRUD + import/export with conflict resolution |
| Aggregate queries | `src-tauri/src/db/aggregate_commands.rs` | Cross-character inventory, wealth, skills |
| Frontend game state | `src/stores/gameStateStore.ts` | Server/character event listeners, scoped queries |
| Frontend market | `src/stores/marketStore.ts` | Market value CRUD store |
| Frontend aggregate | `src/stores/aggregateStore.ts` | Aggregate data store |
| Character picker | `src/components/CharacterPicker.vue` | Character switching, add, delete UI |
| Market UI | `src/components/Market/MarketView.vue` | Bulk market value management |
| Aggregate UI | `src/components/Dashboard/AggregateView.vue` | Cross-character dashboard |

---

## Open Questions

- **Timezone from chat log:** The login line includes timezone offset. Should we store this per-character for timestamp normalization?
- **Market value categories:** Should we support tagging items by category (e.g., "materials", "food", "equipment") for easier bulk management?
