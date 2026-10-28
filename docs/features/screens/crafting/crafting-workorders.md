# Crafting — Work Orders

## Overview

Loads the player's memorized work orders from character export data and optionally includes un-memorized work order scroll items from inventory/storage.

## Data Flow

1. **Active work orders** — extracted from `ActiveWorkOrders` array in the Character report `raw_json`
2. **Completed work orders** — from `CompletedWorkOrders` array in the same report
3. **Inventory scrolls** (optional toggle) — queries `character_snapshot_items` for items matching `"Work Order for %"` or `"Scroll_%"` name patterns, then resolves their `BestowQuest` field through CDN items → quest internal name

Each work order is enriched with CDN quest data:
- Quest name, crafting skill (`WorkOrderSkill`), objective item and quantity
- Matching recipe (looked up via `getRecipesForItem` on the objective item)
- Industry XP reward and gold reward from quest rewards
- Industry level requirement

## UI

- Skill filter pills for quick filtering
- Status badges: ACTIVE (green), SCROLL (blue, for inventory items), done (muted)
- Checkbox multi-select with "Select All"
- "Create Project" button to batch-create a crafting project from selected work orders
- Summary bar showing total work orders, Industry XP, and gold
