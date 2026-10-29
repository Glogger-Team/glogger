# Inventory — Live Inventory

## Overview

Real-time inventory tracking that shows what the player currently has, updated as items are gained, lost, or changed during gameplay.

## How It Works

- Subscribes to `PlayerEvent` emissions from the coordinator (ItemAdded, ItemStackChanged, ItemDeleted)
- Displays current inventory as a table with slot index, item name (via `ItemInline`), and stack size
- Summary bar shows total item count and total quantity across all stacks
- Search filter for finding items by name
- Activity feed on the right side shows recent inventory changes with color-coded indicators (green=added, red=removed, yellow=changed)
- Green pulsing status dot indicates when player log tailing is active

## Data Source

All data is session-based and in-memory via `gameStateStore.liveItems`. Not persisted to database — resets on character switch or app restart. For persisted inventory data, see the Storage tab.
