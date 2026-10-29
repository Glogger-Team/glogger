# Inventory — Storage Reports

## Overview

Browse and analyze point-in-time Storage reports created from the Export Storage as JSON button (Settings → VIP). Each report captures every item the character owns across all storage locations.

## Architecture

### Files

**Backend (Rust):**
- `src-tauri/src/db/inventory_commands.rs` — import, query, summary commands
- `src-tauri/src/cdn_commands.rs` — `resolve_effect_descs`, `get_tsys_power_info` for CDN enrichment

**Frontend (Vue/TS):**
- `src/components/Character/InventoryView.vue` — report selector, import, summary stats
- `src/components/Character/InventoryTable.vue` — filterable/sortable item table
- `src/components/Character/InventorySmallGrid.vue` — compact icon grid
- `src/components/Character/InventoryLargeGrid.vue` — large icon grid
- `src/components/Character/InventoryItemPanel.vue` — card view
- `src/components/Character/ItemPowerList.vue` — TSys power display
- `src/components/Character/ModPowerInline.vue` — individual TSys mod tooltip
- `src/stores/characterStore.ts` — report management, import

## Data Flow

```
Export Storage as JSON → JSON file in Reports/ → Rust parser → SQLite → Vue frontend
                                                 ↓
                                          CDN enrichment
                                    (items, attributes, TSys)
```

1. Player runs Export Storage as JSON in-game (Settings → VIP), producing `{CharName}_items_{timestamp}.json` in `Reports/`
2. Rust reads JSON, validates `Report == "Storage"`, inserts into `character_item_snapshots` + `character_snapshot_items`
3. Frontend queries reports and items, enriching display with CDN data on hover
4. Import also seeds `game_state_storage` (vault items) and `game_state_inventory` (inventory items)

## Auto-Import & Polling

- On character activation, `characterStore.initInventoryForActiveCharacter()` imports the latest `*_items_*.json` from Reports
- `pollForNewReports()` checks for new inventory files on each poll cycle
- Deduplication via `ON CONFLICT DO NOTHING` on `(character_id, snapshot_date)` unique constraint

## How It Works

- **Import:** File picker dialog or auto-import from Reports folder. Duplicate detection warns if already imported.
- **Report selector:** Dropdown to browse all imported Storage reports for the active character.
- **Summary stats:** Total items, stacks, unique items, total estimated value, and location count.

## View Modes

Four view modes for browsing items:
- **Detail** — full table with all item metadata, filterable/sortable columns
- **Small Grid** — compact icon grid
- **Large Grid** — larger icon grid
- **Panel** — card-based item display

## Grouping

- **None** — flat list
- **By Storage** — grouped by vault/storage location (Inventory, Saddlebag, NPC vaults, etc.)
- **By Zone** — grouped by geographic area (merges Inventory + Saddlebag together)

Groups are collapsible. Inventory, Saddlebag, and combined Inventory+Saddlebag groups default to expanded. Vault names are formatted for display (`"CouncilVault"` → `"Council Vault"`, `"NPC_Ashk"` → `"Ashk's Storage"`).

## Sorting

- By slot index
- Alphabetical by name
- By stack count
- By estimated value

## Item Data

Each `SnapshotItem` includes: name, stack size, rarity, level, durability (float), crafted status, crafter name, value, storage location, craft_points, uses_remaining, transmute_count, attuned_to, TSys powers.

## CDN Enrichment

### Item Tooltips
Items use `ItemInline` with the `item-id` prop so the tooltip shows CDN base item data even when the displayed name is an enchanted variant (e.g., "Pierce-Resistant Thorian Coat" → tooltip shows base "Thorian Coat" stats).

### Effect Description Resolution
Raw effect placeholders like `{MAX_ARMOR}{197}` are resolved to readable text like `Max Armor +197`:
1. `ItemTooltip` passes `effect_descs` to `resolve_effect_descs` backend command
2. Backend looks up each `{TOKEN}` in attributes.json for `Label` and `DisplayType`
3. `DisplayType` determines formatting: `AsPercent` → `+X%`, `AsBuffDelta` → `+X`, etc.

### TSys Mod Powers
`ItemPowerList` parses the `tsys_powers` JSON string into `ModPowerInline` components. On hover, each calls `get_tsys_power_info` to show skill, prefix/suffix, and resolved tier effects.

## Database Schema

- **`character_item_snapshots`** — one row per import (character_id, snapshot_date, unique for dedup)
- **`character_snapshot_items`** — one row per item stack with all fields from the game export: type_id, storage_vault, stack_size, value, item_name, rarity, slot, level, is_crafted, crafter, durability (REAL), craft_points, uses_remaining, transmute_count, attuned_to, tsys_powers (JSON string), tsys_imbue_power, tsys_imbue_power_tier, pet_husbandry_state

## Deserialization Notes

The game JSON uses non-standard casing:
- Structs use `#[serde(rename_all = "PascalCase")]` for most fields
- `TypeID` needs explicit `#[serde(rename = "TypeID")]` (PascalCase would produce `TypeId`)
- `Durability` is `Option<f64>` — the game outputs fractional values like `0.44006`

## Tauri Commands

| Command | Purpose |
|---------|---------|
| `import_inventory_report` | Parse JSON file and insert report + items |
| `get_inventory_snapshots` | List all Storage reports for a character |
| `get_snapshot_items` | Return all items for a given report |
| `get_inventory_summary` | Aggregate stats: total items, value, breakdowns by vault/rarity |
| `import_latest_inventory_for_character` | Scan Reports folder, find latest, import |
| `resolve_effect_descs` | Convert effect placeholders to readable text |
| `get_tsys_power_info` | Look up TSys power by name + tier |
