# Inventory Screen

## Overview

The inventory screen provides views into what the player owns: point-in-time Storage reports from Export Storage as JSON, a comprehensive storage vault database, and a cross-character consolidation planner. (The real-time live inventory tab was removed with the `LiveInventoryTab.vue` component, batch-1 2026-10-30 — the `LiveInventoryItem` game-state map is retained for re-enable; see `inventory-live.md`.)

## Architecture

### Files

**Backend (Rust):**
- `src-tauri/src/db/inventory_commands.rs` — Storage report import, item queries, summary stats
- `src-tauri/src/game_data/storage_vaults.rs` — CDN vault metadata parsing

**Frontend (Vue/TS):**
- `src/components/Inventory/InventoryWrapper.vue` — 3-tab container
- `src/components/Character/InventoryView.vue` — report-based inventory browser
- `src/components/Inventory/VaultDatabaseTab.vue` — storage vault browser
- `src/components/Inventory/ConsolidateTab.vue` — cross-character consolidation planner
- `src/components/Inventory/VaultAreaCard.vue` — area grouping card
- `src/components/Inventory/VaultRow.vue` — individual vault with items

**Stores:**
- `gameStateStore` — live inventory events, storage items, vault metadata, favor, attributes
- `characterStore` — Storage report management, import, report items

### Component Hierarchy

```
InventoryWrapper.vue                — 3-tab container
├── InventoryView.vue               — report-based browsing (from Storage exports)
│   ├── InventoryTable.vue          — detail list view
│   ├── InventorySmallGrid.vue      — small icon grid
│   ├── InventoryLargeGrid.vue      — large icon grid
│   └── InventoryItemPanel.vue      — card view
├── VaultDatabaseTab.vue            — all vaults with capacity tracking
│   ├── VaultAreaCard.vue           — area summary card
│   └── VaultRow.vue                — per-vault detail with items
└── ConsolidateTab.vue              — cross-character consolidation planner
```

## Per-Tab Documentation

- [inventory-live.md](inventory/inventory-live.md) — Live Inventory (removed tab; retained game-state map)
- [inventory-snapshots.md](inventory/inventory-snapshots.md) — Storage Reports
- [inventory-vaults.md](inventory/inventory-vaults.md) — Storage Vault Database
- [inventory-consolidate.md](inventory/inventory-consolidate.md) — Consolidation Planner

## Tauri Commands

### Storage Report Management
- `import_inventory_report(file_path) → InventoryImportResult` — import Storage export JSON, seeds game_state_storage and game_state_inventory
- `get_inventory_snapshots(character_name, server_name?) → Vec<InventorySnapshotSummary>`
- `get_snapshot_items(snapshot_id) → Vec<SnapshotItem>` — full item details (rarity, level, durability, etc.)
- `get_inventory_summary(snapshot_id) → InventorySummary` — aggregate stats

### Vault Data
- `get_storage_vault_metadata() → Vec<StorageVaultDetail>` — CDN vault definitions with capacity info

## Database Tables

- **`character_item_snapshots`** — Storage report headers (character, server, timestamp)
- **`character_snapshot_items`** — individual items within Storage reports
- **`game_state_inventory`** — current inventory items (seeded from latest Storage report)
- **`game_state_storage`** — current vault items (seeded from Storage report import)
- **`game_state_favor`** — NPC favor tiers (determines unlocked vault slots)
- **`game_state_attributes`** — player attributes (for attribute-scaled vaults)

## Key Design Decisions

- ~~**Dual inventory tracking** — the Live tab (real-time session state, in-memory) was removed batch-1; Storage reports (persisted point-in-time exports) and the vault database remain.~~
- **Storage report import seeds game state** — importing a Storage export report populates both `game_state_inventory` and `game_state_storage`, making vault data available immediately.
- **Vault capacity models** — vaults use multiple capacity systems (fixed slots, favor-tiered, attribute-scaled), all unified behind `getVaultUnlockedSlots()` / `getVaultMaxPossibleSlots()` helpers.
- **Entity resolution** — all item names use `ItemInline` and NPC names use `NpcInline` for consistent tooltip/navigation behavior.
