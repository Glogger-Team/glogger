# Crafting — Brewery

## Overview

A brewing discovery journal and recipe explorer that helps players track their personal ingredient→effect mappings. Brewing in Project Gorgon assigns effects to ingredient combinations uniquely per player, so each player must experiment to find which combos produce which effects. The Brewery tab automates this record-keeping by extracting discoveries from inventory JSON exports, and provides tools for systematic exploration.

## Key Concepts

### Per-Player Effect Randomization

The same ingredient combination produces different effects for different players. CDN recipes define the universe of possible effect categories (e.g., "Party & Dance", "Endurance", "Racial Bonuses"), but the specific mapping from ingredient combo to effect is randomized per character. This is why personal tracking is essential — wikis can't help here.

### Three Drink Categories

- **Beers** — single-step craft. Fixed ingredients (grain + hops + yeast) plus variable slots. Glass and Keg variants share the same variable slot structure.
- **Wines** — single-step bottling. Fixed ingredients (bottle + cork + fruit) plus variable slots. Wines gain uses over time while aging in inventory.
- **Liquors** — two-step. First "Prepare" recipe creates an un-aged cask (no variable slots). Cask ages in a cave for ~1 hour. Then a "Finish" recipe adds variable ingredients and determines the effect. Liquors have unique `BrewingAnimalPart` slots (monster parts).

### Race Restrictions & Skill Requirements

Some brewing effects are restricted to specific races (e.g., "of Elfinity" = Elf only). Detected automatically from TSysPower names. Drinks also require specific skills to consume — not always Brewing. Can be Endurance, Gourmand, or Nature Appreciation at varying levels. Deployed kegs bypass skill requirements but not race restrictions.

## Data Sources

- **CDN `recipes.json`** — all Brewing-skill recipes parsed into structured `BrewingRecipe` objects with fixed/variable ingredients, `BrewItem(tier,skill,slots=pools)` effect definitions, and skill requirements
- **CDN `items.json`** — item names, `Brewing*` keywords that define which recipe slots each item can fill, drink metadata (`NumUses`, `AlcoholLevel`)
- **CDN `tsysclientinfo.json`** — resolved effect descriptions, prefix/suffix labels, required skills, tiers. Accessed via `get_tsys_power_info_batch` for bulk lookups
- **Item JSON exports** — per-item `IngredientItemTypeIds`, `TSysPowers`, `Crafter`, `UsesRemaining`. Primary source for building the discovery journal
- **Game state (inventory + storage)** — `ownedItemCounts` from `gameStateStore` for material availability indicators
- **DB `brewing_discoveries` table** — persisted discovery journal, deduplicated on `(character, recipe_id, ingredient_ids)`

## How It Works

### Discovery Extraction

1. When an inventory JSON is imported (manual, auto-poll, or startup), the system auto-scans the raw JSON for brewed items
2. Items must have `IngredientItemTypeIds`, `TSysPowers`, AND a `Crafter` field matching the active character (skips other players' brews)
3. Each qualifying item's TypeID is matched to a CDN brewing recipe via `result_item_id`
4. The ingredient combo + TSysPower mapping is stored as a `BrewingDiscovery`
5. Deduplication: same `(character, recipe_id, sorted_ingredient_ids)` = one discovery, `last_seen_at` updated on re-observation
6. Effect labels extracted by comparing item name to CDN base name (e.g., "Partier's Dwarven Stout" vs "Dwarven Stout" → "Partier's")
7. Race restrictions detected from power names containing race identifiers

### CSV Import

Players with existing spreadsheets can import discoveries via CSV. The format is flexible — uses header-based column detection, so columns can be in any order.

**Required columns:** `recipe_name` + at least one `ingredient` column (ingredient1..ingredient4).

**Effect columns** (at least one recommended):
- `effect_desc` — the actual buff text, e.g., "Orcs gain +38 Max Power" or "Archery Base Damage % +20%". Numbers are stripped for matching, so the exact values don't need to be precise. Multiple effects separated by " / " are supported. This is the most natural format for players who recorded tooltip text.
- `effect_name` — the drink's prefix/suffix, e.g., "Partier's" or "of Elfinity". Resolved to TSysPower via CDN prefix/suffix lookup.

**Advanced columns** (optional): `power`, `power_tier`, `type_id`, `item_name`.

Ingredient names are resolved case-insensitively against CDN item data. Recipe names are matched against CDN recipe names, internal names, and with parenthetical suffixes stripped (e.g., "Dwarven Stout" matches "Dwarven Stout (One Glass)"). If effect cannot be resolved, it's stored as `power="unknown"` — the combo is still recorded.

```
recipe_name,ingredient1,ingredient2,ingredient3,ingredient4,effect_desc
Dwarven Stout,Corn,Green Apple,Groxmax Powder,Cinnamon,Rakshasa gain +38 Max Power
Dwarven Stout,Corn,Pear,Groxmax Powder,Cinnamon,Rakshasa earn +11.8% Combat XP
Rice Wine,Rattus Root,Tomato,Walnuts,Pansy,Chance to Forage Extra Mushrooms +25%
```

## UI Layout

Three-pane PaneLayout (`screen-key: "crafting-brewery"`):

### Left Pane — Recipe List

- **Search** — text filter on recipe name/internal name
- **Category filter pills** — All, Beers (Glass), Beers (Keg), Wines, Liquors (Un-Aged), Liquors (Finished), Utility. Only non-empty categories shown.
- **Grouped recipe list** — recipes grouped by category with sticky headers, sorted by skill level within each group
- **Discovery badges** — green count next to recipes that have discoveries
- **Footer** — recipe count, total discoveries, Scan/Import CSV buttons

### Center Pane — Detail / Results

Shows one of three views depending on selection state:

**Recipe Detail** (when a recipe is selected in the left pane):
- Header with name, level, XP, action type
- Fixed ingredients with owned counts
- Variable ingredient slots with keyword tag, valid items (via `ItemInline`), and owned counts `(×N)` in green
- Possible effect categories with human-readable labels, descriptions on hover, race-locked warning
- Discoveries table: ingredients → effect label + resolved descriptions + required skill + race badge
- "Try Next" suggestions: untried ingredient combos prioritized by availability, session-randomized within tiers

**Effect Results** (when an effect is selected in the right pane):
- Effect header with resolved buff descriptions and required skill
- All recipe/ingredient combos that produce this effect, sorted by availability then tier
- Each card shows ingredients with availability dots + owned counts, plus fixed ingredients
- Click recipe name to navigate to recipe detail view

**Empty State** — prompts to select a recipe or search for an effect

### Right Pane — Effect Search

- **Search** — matches against resolved effect descriptions, labels, power names, and skill names (e.g., searching "wood" finds Lumberjack's, "armor" finds mitigation effects)
- **Effect list** — all discovered effects with human-readable descriptions and required skill. Race-restricted effects tinted red and sorted to bottom.
- **Discovery count** per effect
- Selecting an effect shows results in the center pane

## Backend

### Rust Modules

- [`src-tauri/src/game_data/brewing.rs`](../../../../src-tauri/src/game_data/brewing.rs) — `BrewItem` parser, recipe classifier, `build_brewing_data()` that derives brewing views from existing recipe + item CDN data
- [`src-tauri/src/db/brewing_commands.rs`](../../../../src-tauri/src/db/brewing_commands.rs) — discovery scanning, querying, and CSV import commands

### Tauri Commands

| Command | Purpose |
|---------|---------|
| `get_brewing_recipes` | All brewing recipes with parsed variable slots and effect data |
| `get_brewing_ingredients` | All items with `Brewing*` keywords |
| `get_brewing_discoveries` | Query discoveries for a character, optionally filtered by recipe |
| `scan_snapshot_for_brewing_discoveries` | Scan a single inventory snapshot for new discoveries |
| `scan_all_snapshots_for_brewing` | Bulk-scan all snapshots for a character |
| `import_brewing_discoveries_csv` | Import discoveries from a CSV file |
| `get_tsys_power_info_batch` | Bulk-resolve TSysPower names to human-readable descriptions |

### Database Schema

```sql
CREATE TABLE brewing_discoveries (
    id INTEGER PRIMARY KEY,
    character TEXT NOT NULL,
    recipe_id INTEGER NOT NULL,
    ingredient_ids TEXT NOT NULL,  -- JSON array of TypeIDs, sorted
    power TEXT NOT NULL,
    power_tier INTEGER NOT NULL,
    effect_label TEXT,
    race_restriction TEXT,
    item_name TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    UNIQUE(character, recipe_id, ingredient_ids)
);
```

## Frontend

### Store

[`src/stores/breweryStore.ts`](../../../../src/stores/breweryStore.ts):

- **State:** recipes, ingredients, discoveries, powerInfoMap, selection state, search/filter state
- **Computed:** recipe/ingredient lookups, category grouping, discovery grouping, effect entries with resolved descriptions, filtered views
- **Actions:** `loadBrewingData`, `loadDiscoveries`, `scanAllSnapshots`, `importCsv`, `onInventoryImported` (auto-scan hook)

### Components

| Component | Purpose |
|-----------|---------|
| [`BreweryTab.vue`](../../../../src/components/Crafting/BreweryTab.vue) | Root tab, PaneLayout with three panes |
| [`BreweryRecipeDetail.vue`](../../../../src/components/Crafting/BreweryRecipeDetail.vue) | Recipe detail with ingredients, discoveries, and Try Next suggestions |
| [`BreweryEffectPanel.vue`](../../../../src/components/Crafting/BreweryEffectPanel.vue) | Right pane effect search list |
| [`BreweryEffectResults.vue`](../../../../src/components/Crafting/BreweryEffectResults.vue) | Center pane effect search results |

### Auto-Scan Integration

The brewery store exposes `onInventoryImported(character)` which is called from `characterStore` after any non-duplicate inventory import (manual, polling, or startup). Uses dynamic `import()` so the brewery store module is only loaded if the user has previously visited the Brewery tab. Scan runs in the background and silently updates discoveries.

## Not Yet Implemented

Remaining phases from the brewing plan (`docs/plans/brewing-helper.md`):

- **Aging tracker** (Phase 5) — track wine use-count aging and liquor aged/un-aged transitions across Storage reports over time
- **Live brew tracking** (Phase 6) — detect active brewing sessions from Player.log events (XP gains, recipe completions)
- **Brew session summary** (Phase 7) — session view showing brews completed, XP earned, new discoveries during a session
