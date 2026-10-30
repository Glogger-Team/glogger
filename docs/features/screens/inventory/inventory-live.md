# Inventory — Live Inventory (removed, batch-1 2026-10-30)

> The `LiveInventoryTab.vue` component was deleted. The tab wiring is gone;
> the `LiveInventoryItem` game-state map is retained for re-enable.

## Overview (historical)

## How It Works

- Subscribes to `PlayerEvent` emissions from the coordinator (ItemAdded, ItemStackChanged, ItemDeleted)
- Displays current inventory as a table with slot index, item name (via `ItemInline`), and stack size
- Summary bar shows total item count and total quantity across all stacks
- Search filter for finding items by name
- Activity feed on the right side shows recent inventory changes with color-coded indicators (green=added, red=removed, yellow=changed)
- Green pulsing status dot indicates when player log tailing is active

## Data Source

All data is session-based and in-memory via `gameStateStore.liveItems`. Not persisted to database — resets on character switch or app restart. For persisted inventory data, see the Storage tab.
