# Search

Search provides two interfaces for finding anything in the app: a **quick search overlay** (Ctrl+F) and a **dedicated search page**. Both are powered by the unified search engine, which searches all 14 data categories with a single query and supports structured filter syntax.

## Components

| File | Purpose |
|------|---------|
| [QuickSearchOverlay.vue](../../../src/components/Search/QuickSearchOverlay.vue) | Command-palette modal, teleported to body. Keyboard navigable, grouped results with per-category cap (default 5) |
| [SearchView.vue](../../../src/components/Search/SearchView.vue) | Full search page with category filter toggles, collapsible result sections, cap=50 per category |
| [useUnifiedSearch.ts](../../../src/composables/useUnifiedSearch.ts) | Frontend composable shared by both components. Calls a single backend command for game data, searches player data client-side, merges and orders results |
| [unified_search.rs](../../../src-tauri/src/unified_search.rs) | Rust search engine — query parser, relevance scoring, searches all 11 game data entity types in one pass |

## Search Syntax

The search supports plain text and structured filters inspired by [Scryfall](https://scryfall.com/docs/syntax). Multiple filters AND together.

### Plain text
Just type to search across all fields (name, description, keywords, etc.):
```
sword
fire sword
```

### Quoted phrases
Wrap in quotes for exact phrase matching:
```
"fire sword"
"Serbule Hills"
```

### Structured filters

| Filter | Example | Effect |
|--------|---------|--------|
| `type:` | `type:item`, `type:npc` | Restrict to a single entity type |
| `skill:` | `skill:Sword`, `skill:cooking` | Recipes (skill), NPCs (trainers), abilities (skill) |
| `area:` | `area:Serbule` | NPCs and quests in a zone |
| `level:` | `level:30`, `level:30-50` | Items (crafting target level) and recipes (skill requirement) |
| `keyword:` | `keyword:Food` | Items with a specific keyword |
| `has:` | `has:recipe`, `has:description` | Items with recipes or descriptions |
| `slot:` | `slot:MainHand` | Items for an equipment slot |
| `name:` | `name:sword` | Items only — restricts text match to the name field |

Filters only apply to the types listed above. A positive filter outside a
type's scope returns no rows for that type (reject-or-empty) instead of
being silently ignored — e.g. `keyword:` on NPCs or `level:` on quests
yields nothing for that type. Negated filters on unsupported properties
pass vacuously (an NPC can't have an item-only keyword, so `-keyword:X`
excludes nothing there).

### Negation
Prefix any filter with `-` to exclude. Negation applies to type, skill,
area, level, keyword, has, slot, and name filters (e.g. `-skill:Cooking`
excludes recipes/NPCs/abilities of that skill; `-level:30-50` excludes
items/recipes in that range; `-area:Serbule` excludes NPCs/quests there;
`-name:sword` excludes items with "sword" in the name):
```
-keyword:NotObtainable sword       → swords without the NotObtainable keyword
-type:effect fire                  → everything mentioning "fire" except effects
```
### Combining filters
All filters AND together. Combine text with filters freely:
```
type:item keyword:Food level:30-50     → food items between level 30-50
type:recipe skill:Cooking soup         → cooking recipes matching "soup"
type:npc area:Serbule                  → NPCs in Serbule
```

### Colon handling
Only known filter keys (`type`, `skill`, `area`, `level`, `keyword`, `has`, `slot`, `name`) are treated as filters. Other colons are literal text, so ability names like `"Knife: Precision Slash"` work as expected.

## Entity Types Searched

The `type:` filter accepts these values:

| Type | Fields Searched | Detail Shown |
|------|----------------|--------------|
| `item` | name, description, keywords, effect descriptions, food description | Keywords or description |
| `recipe` | name, description, skill, ingredient names, result item names | Skill, level requirement |
| `npc` | name, description, area | Area name |
| `quest` | name, description, location, favor NPC | Displayed location |
| `skill` | name, description | Description |
| `ability` | base name, tier names, descriptions | Skill |
| `effect` | name, description | Description |
| `enemy` | key, comment, strategy | Comment |
| `area` | friendly name | Area key |
| `title` | title text, tooltip | Tooltip |
| `lorebook` | title, text, category | Category |

Player data (searched client-side, not via `type:` filter):

| Category | Data Source | Detail Shown |
|----------|------------|--------------|
| Your Items | Backpack, vault storage, aggregate inventory | Count, location, market value |
| Your Skills | Game state skills | Level, XP |
| Market Values | Market price store | Market price |

## Relevance Scoring

Results within each category are sorted by relevance score:

| Score | Match Type |
|-------|------------|
| 100 | Exact name match |
| 80 | Name starts with query |
| 60 | Name contains query |
| 40 | Secondary field match (description, keywords) |

When no text query is provided (filter-only searches like `type:item keyword:Food`), all matching entities receive a base score of 50 and sort alphabetically.

## Quick Search Overlay (Ctrl+F)

A command-palette-style modal that appears centered over the current view.

- Opens on **Ctrl+F** or **Ctrl+K** globally (listener in `App.vue`). Escape or clicking outside dismisses.
- Single text input with instant results grouped by category, capped at 5 per category.
- Results use shared inline components (`ItemInline`, `RecipeInline`, `NpcInline`, `QuestInline`, `SkillInline`, `AreaInline`, `EnemyInline`) for hover tooltips.
- Keyboard: arrow keys to move through results (flat index across categories), Enter to select.
- Syntax help shown in the empty state before the user types.

## Dedicated Search Page

The "Search" nav item in the menu bar. For broader, more detailed exploration.

- Search bar auto-focused on page load.
- Filter toggles (pill buttons) for each of 14 categories — all enabled by default. Shows match count per category.
- Results grouped by category in collapsible cards, up to 50 results per category.
- Results use shared inline components with icons for richer display.
- Syntax reference shown in the empty state.

## Your Items Search Priority

Items are deduplicated across three sources with this priority:

1. **Backpack / live inventory** — from `ownedItemCounts` (merged DB inventory + live tracking)
2. **Vault storage** — from `storageByVault`, with stacks aggregated per item and vault keys resolved to NPC friendly names
3. **Other characters** — from `aggregateStore.inventory`, showing which characters hold the item

## Navigation from Search Results

Results carry a `SearchNavigation` object with `view`, `subTab`, and optionally `entityType` + `entityId`.

- **Entity results** (Game Items, Recipes, NPCs, Quests, Skills, Abilities, Effects, Enemies, Areas, Titles, Lorebooks): routed through the entity navigation system — switches to Data Browser with the correct tab and passes a nav target.
- **Player data results** (Your Items, Your Skills, Market Values): navigates to the target view and activates the sub-tab directly.

## Architecture

```
QuickSearchOverlay / SearchView
    ↓
useUnifiedSearch composable
    ├─ Synchronous: player data from Pinia stores
    │   └─ gameStateStore, marketStore, aggregateStore
    │
    └─ Async: single Tauri command
        └─ invoke("unified_search", { query, limit })
            ↓
        unified_search.rs
            ├─ parse_query() → ParsedQuery (text terms, phrases, filters, negations)
            ├─ should_search() per entity type (respects type: filters)
            ├─ Per-type search with relevance scoring
            └─ Returns Vec<UnifiedSearchResult> (sorted by score within each type)
```

The backend acquires a single read lock on `GameDataState` and iterates all entity collections in one pass — no parallel Tauri commands needed.

## Data Browser Integration

The same search syntax also works in Data Browser search bars. Each browser uses the [useDataBrowserSearch](../../../src/composables/useDataBrowserSearch.ts) composable, which applies the [SearchParser](../../../src/utils/SearchParser.ts) client-side to the browser's preloaded data.

Browsers that have dropdown filters (area, skill, strategy, category) compose them as pre-filters — the dropdown narrows the dataset, then the unified text search filters within it. Structured filters like `skill:Sword` and `area:Serbule` also work in browser search bars for users who prefer typing over dropdowns.

**Migrated browsers:** SkillBrowser, TitleBrowser, LoreBrowser, EnemyBrowser, NpcBrowser, RecipeBrowser, QuestBrowser, AbilityBrowser, ItemSearch.

ItemSearch additionally parses `keyword:`, `level:`, `slot:`, `"quoted phrases"`, and `-keyword:` from the search bar and merges them with its dropdown filters.
