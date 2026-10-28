# Crafting Screen

## Overview

A full-featured crafting assistant for Project Gorgon. Helps players plan crafting projects, calculate ingredient requirements, optimize XP leveling paths, track live crafting progress, process work orders, review crafting history, and find cookable uneaten foods for Gourmand progress — all powered by CDN recipe/item/skill data, Storage reports, and character exports.

## Architecture

### Files

**Backend (Rust):**
- `src-tauri/src/db/crafting_commands.rs` — all Tauri commands (project CRUD, material checks, work orders, skill levels)
- `src-tauri/src/db/migrations.rs` — `migration_v6_crafting` (2 tables), `migration_v20_project_pricing` (adds fee_config/customer_provides to projects)

**Frontend (Vue/TS):**
- `src/types/crafting.ts` — all TypeScript interfaces
- `src/stores/craftingStore.ts` — Pinia store with project management, ingredient resolution, leveling optimizer, live tracking, work orders
- `src/stores/cooksHelperStore.ts` — Pinia store for Cook's Helper (gourmand report import, recipe filtering, material checks)
- `src/components/Crafting/` — Vue components (see Component Hierarchy below)

### Database Tables

- **`crafting_projects`** — id, name, notes, created_at, updated_at
- **`crafting_project_entries`** — project_id (FK), recipe_id (FK→recipes), recipe_name, quantity, sort_order, expand_intermediates

Entries cascade on delete from `crafting_projects`. The projects table also includes `fee_config` (JSON) and `customer_provides` (JSON) columns for the integrated Price Helper.

### Component Hierarchy

```
CraftingView.vue                    — 7-tab container (Projects tab includes integrated Price Helper)
├── QuickCalcTab.vue                — single-recipe calculator
│   └── IngredientTreeNode.vue      — recursive ingredient display
├── ProjectsTab.vue                 — project list + active project detail
│   ├── IngredientTreeNode.vue      — per-entry ingredient tree
│   ├── MaterialSummary.vue         — aggregated needs vs available
│   ├── PickupList.vue              — vault-organized pickup list
│   ├── ShoppingList.vue            — vendor shopping with cost estimates
│   └── LiveCraftingPanel.vue       — real-time craft detection overlay
├── LevelingTab.vue                 — XP leveling optimizer
├── HistoryTab.vue                  — crafting stats from character exports
│   └── SkillCraftingProgress.vue   — per-skill completion bars
├── WorkOrdersTab.vue               — work order harvester + project builder
├── CooksHelperTab.vue              — gourmand-aware uneaten food recipe finder
│   └── CooksHelperRecipeRow.vue    — per-recipe row with material status
└── SkillsTab.vue                   — per-skill crafting summary with recipe list
```

### Ingredient Resolution

The core resolver (`resolveRecipeIngredients` in the store) handles:
- **Recursive expansion** — walks `recipe_ingredients` → checks if ingredient has a recipe via `getRecipesForItem()` → optionally expands
- **Chance-to-consume** — expected consumption = `stack_size × chance_to_consume × craft_count`
- **Batch awareness** — if a recipe produces `stack_size > 1` per craft, adjusts craft count accordingly
- **Probabilistic outputs** — recipes with `percent_chance < 100` calculate expected crafts needed
- **Circular dependency detection** — guards against recipe loops

## Per-Tab Documentation

- [crafting-quickcalc.md](crafting/crafting-quickcalc.md) — Quick Calculator
- [crafting-projects.md](crafting/crafting-projects.md) — Crafting Projects (includes Live Crafting Detection)
- [crafting-leveling.md](crafting/crafting-leveling.md) — XP Leveling Optimizer
- [crafting-history.md](crafting/crafting-history.md) — Crafting History
- [crafting-workorders.md](crafting/crafting-workorders.md) — Work Orders
- [crafting-cookshelper.md](crafting/crafting-cookshelper.md) — Cook's Helper
- [crafting-price-helper.md](crafting/crafting-price-helper.md) — Price Helper (integrated into Projects)
- [crafting-skills.md](crafting/crafting-skills.md) — Skills

## Tauri Commands

### Project CRUD
- `create_crafting_project(name, notes) → project_id`
- `get_crafting_projects() → Vec<CraftingProjectSummary>`
- `get_crafting_project(project_id) → CraftingProject`
- `update_crafting_project(project_id, name, notes)`
- `delete_crafting_project(project_id)`
- `duplicate_crafting_project(project_id) → project_id`

### Entry Management
- `add_project_entry(project_id, recipe_id, recipe_name, quantity) → entry_id`
- `update_project_entry(entry_id, quantity, expand_intermediates)`
- `remove_project_entry(entry_id)`
- `reorder_project_entries(project_id, entry_ids)`

### Data Queries
- `check_material_availability(character_name, server_name, item_type_ids) → Vec<MaterialAvailability>`
- `get_latest_recipe_completions(character_name, server_name) → Vec<RecipeCompletionEntry>`
- `get_latest_skill_level(character_name, server_name, skill_name) → Option<(level, xp, xp_needed)>`
- `get_work_orders_from_snapshot(character_name, server_name) → WorkOrderData`

### Cook's Helper
- `import_cooks_helper_file(file_path) → Vec<String>` — parse gourmand report, return eaten food names
- `get_all_foods() → Vec<FoodItem>` — all food items from CDN `foods` table

### CDN Commands
- `get_quest_by_internal_name(name) → Option<QuestInfo>` — resolves quest by InternalName (used for work order enrichment)
- `get_xp_table_for_skill(skill) → Vec<number>` — per-level XP amounts for leveling optimizer

## Key Design Decisions

- **`reward_skill` vs `skill`**: The leveling optimizer matches recipes by `reward_skill`, not `skill`. Many recipes grant XP in a different skill than the crafting skill used (e.g., a Blacksmithing recipe that rewards Armorsmithing XP).
- **XP tables are per-level**: CDN `XpAmounts` contains per-level XP amounts (not cumulative). The store converts these to cumulative values for range calculations.
- **Skill internal names**: CDN skills use internal names (e.g., `JewelryCrafting`) that differ from display names (`Jewelry Crafting`). The optimizer resolves display → internal name for proper recipe matching.
- **Work order key formats**: Character exports store work orders by quest `InternalName` (e.g., `Carpentry_QualityMeleeStaves`), while the CDN indexes quests by numeric keys (e.g., `quest_50344`). A `quest_internal_name_index` bridges this gap.
- **Inventory scroll detection**: Work order scrolls appear in inventory exports with either display names (`Work Order for ...`) or internal names (`Scroll_...`). The SQL query matches both patterns.
- **Ingredient resolution is frontend-side**: The recursive ingredient resolver runs in the store (TypeScript), not Rust, since it needs to call multiple CDN lookup functions and handle UI-driven expansion toggles.
- **Recipe known-status key format**: `knownRecipeKeys` uses `Recipe_{numeric_id}` format (e.g., `Recipe_12345`), matching the CDN recipe ID — not the recipe's `internal_name`. All code checking whether a player knows a recipe must use this format.
