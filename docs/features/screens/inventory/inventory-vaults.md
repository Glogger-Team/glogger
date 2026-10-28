# Inventory — Storage Vault Database

## Overview

A CDN-driven database of ALL storage vault locations in the game (~92 vaults), showing what the player has stored, vault capacity, and unlock progress. Combines CDN vault metadata with player-specific storage, favor, and attribute data. Tracks vault contents live via Player.log events and seeds from Storage reports.

## Architecture

### Files

**Backend (Rust):**
- `src-tauri/src/game_data/storage_vaults.rs` — CDN `storagevaults.json` parsing
- `src-tauri/src/player_event_parser.rs` — `vault_key` enrichment on StorageDeposit/Withdrawal
- `src-tauri/src/game_state.rs` — StorageDeposit → UPSERT, StorageWithdrawal → DELETE
- `src-tauri/src/db/game_state_commands.rs` — `get_game_state_storage` query
- `src-tauri/src/cdn_commands.rs` — `get_storage_vault_metadata` with area resolution

**Frontend (Vue/TS):**
- `src/components/Inventory/VaultDatabaseTab.vue` — main vault browser
- `src/components/Inventory/VaultAreaCard.vue` — area summary card
- `src/components/Inventory/VaultRow.vue` — individual vault with items
- `src/stores/gameStateStore.ts` — storage state, vault metadata, capacity helpers

## How It Works

Storage in Project Gorgon is NPC-based — players unlock vault slots by gaining favor with NPCs. All vaults within a zone are accessible from any storage NPC in that zone.

- **Area-based layout:** Vaults grouped into cards by geographic area
- **Search:** Filter by vault name, area name, or stored item names
- **Summary:** Total stored items and number of vaults with items
- Areas with stored items sort first, then alphabetical. Location-independent vaults (`"*"` area) sort last.

### Area Cards (VaultAreaCard)

Each area card shows:
- Area name and vault count
- Total stored items in that area
- Capacity bar (used / unlocked slots) with color progression: green → yellow → red

### Vault Detail (VaultRow)

Clicking an area card expands to show individual vaults:
- NPC name (via `NpcInline` with full tooltip) or vault display name
- Requirement description (if any)
- Favor tier badge (for NPC-gated vaults)
- Capacity bar — color-coded: green (<70%), yellow (70-90%), red (>90%), showing "used / unlocked (max)"
- Expandable item list (auto-expands if ≤10 items), items shown via `ItemInline`

## Vault Key Resolution

The critical mapping problem: `ProcessAddToStorageVault(14804, ...)` log lines contain a game **entity ID** that does NOT match `storagevaults.json` `ID` fields.

**Solution:** The parser uses `current_interaction.npc_name` — set by the `ProcessVendorScreen` event that always fires before storage operations — to get the CDN-compatible vault key (e.g., `"NPC_Joe"`). This `vault_key` is added to both `StorageDeposit` and `StorageWithdrawal` events at parse time.

## Vault Capacity Models

Vaults use different systems for determining available slots:

| Model | How It Works |
|-------|-------------|
| **Fixed** | Always has `num_slots` available |
| **Favor-tiered** | Slots unlock at NPC favor thresholds (e.g., Neutral=8, Friends=16) |
| **Attribute-scaled** | Slots scale with a player attribute value |
| **Event-based** | Slots tied to event participation levels |

### Capacity Calculation

**`getVaultMaxPossibleSlots(vault)`** — theoretical maximum if player had max favor:
1. `num_slots` (skips 0 — NPC vaults have `num_slots: 0` in CDN)
2. `slot_attribute` → falls back to `num_slots_script_atomic_max`
3. `levels` → highest slot count from any favor tier
4. `num_slots_script_atomic_max`

**`getVaultUnlockedSlots(vault)`** — currently unlocked based on player state:
1. `num_slots` (static, skips 0)
2. `slot_attribute` → current attribute value from `attributesByName`
3. `levels` → player's favor tier for the NPC via `vault.key`, finds highest qualifying tier using `FAVOR_TIER_ORDER`
4. `num_slots_script_atomic_max` as fallback

**Important:** NPC vaults in CDN have `num_slots: 0` (not null). Both functions skip `num_slots` when it's 0 to avoid short-circuiting before the `levels` check.

## Data Flow

### Live Events

```
Player.log → LogWatcher → PlayerEventParser (adds vault_key)
                                    ↓
                          DataIngestCoordinator
                               ↓              ↓
                    GameStateManager     accumulate into batch
                    (UPSERT/DELETE)
                           ↓
               flush → emit("player-events-batch")
                      + emit("game-state-updated", ["storage"])
                           ↓
                    gameStateStore.refreshDomain("storage")
                           ↓
                    VaultDatabaseTab (reactive update)
```

### Report Seeding

```
Export Storage as JSON → character_snapshot_items (DB)
                        ↓
            seed_game_state_from_snapshot()
                        ↓
            DELETE all storage for character → INSERT from Storage report
```

Live log events gradually replace report data with real instance IDs as the player interacts with storage.

## Database Schema

```sql
CREATE TABLE game_state_storage (
    character_name TEXT NOT NULL,
    server_name TEXT NOT NULL,
    vault_key TEXT NOT NULL,
    instance_id INTEGER NOT NULL,
    item_name TEXT NOT NULL,
    item_type_id INTEGER,
    stack_size INTEGER NOT NULL DEFAULT 1,
    slot_index INTEGER NOT NULL DEFAULT -1,
    last_confirmed_at TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'log',
    PRIMARY KEY (character_name, server_name, vault_key, instance_id)
);
```

- **`vault_key`** — maps to `storagevaults.json` keys (e.g., `"NPC_Joe"`)
- **`source`** — `'log'` (from live events) or `'snapshot'` (from a Storage export)
- **`instance_id`** — real IDs from log events; synthetic negative IDs (`-(i+1)`) from reports

## Storage Persistence

Unlike inventory (which clears on login), storage persists across sessions. The `GameStateManager` does NOT clear storage on `CharacterLogin`. Items are only removed by explicit `StorageWithdrawal` events or full report replacement.

## Event Handling

| PlayerEvent | DB Operation | Notes |
|-------------|-------------|-------|
| `StorageDeposit` | UPSERT by (character, server, vault_key, instance_id) | Updates item_name, stack_size, slot_index |
| `StorageWithdrawal` | DELETE by (character, server, vault_key, instance_id) | Removes item from storage |
| `CharacterLogin` | *(no-op for storage)* | Storage persists across sessions |

## Data Sources

| Data | Source |
|------|--------|
| Vault definitions (~92 vaults) | CDN `storagevaults.json` via `get_storage_vault_metadata()` |
| Stored items | `game_state_storage` table (seeded by import, updated by live events) |
| Favor tiers | `game_state_favor` table (from character reports and log events) |
| Attributes | `game_state_attributes` table |
