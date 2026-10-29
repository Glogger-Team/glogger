# Brewery — Implementation Plan

A new tab under the Crafting view that helps brewers track their personal
ingredient→effect mappings, monitor aging progress, and plan their brewing
sessions. Brewing in Project Gorgon has per-player randomized effect mappings,
making personal record-keeping essential — and that's exactly what glogger
can automate.

See `docs/samples/player-log-samples/buppis-brew-logs/investigationNodes.md`
for the raw investigation data that informed this plan.

---

## Table of Contents

- [Brewery — Implementation Plan](#brewery--implementation-plan)
  - [Table of Contents](#table-of-contents)
  - [1. Goals \& non-goals](#1-goals--non-goals)
  - [2. Background — how brewing works](#2-background--how-brewing-works)
    - [Three drink categories](#three-drink-categories)
    - [The BrewItem system](#the-brewitem-system)
    - [Race restrictions \& skill requirements](#race-restrictions--skill-requirements)
    - [Data available per crafted drink (from item JSON)](#data-available-per-crafted-drink-from-item-json)
  - [3. Data sources](#3-data-sources)
  - [4. Build order](#4-build-order)
  - [5. Phase 1 — CDN brewing data extraction](#5-phase-1--cdn-brewing-data-extraction)
    - [Tasks](#tasks)
    - [Key data structures](#key-data-structures)
  - [6. Phase 2 — Discovery journal (item JSON import)](#6-phase-2--discovery-journal-item-json-import)
    - [Tasks](#tasks-1)
    - [Schema](#schema)
  - [7. Phase 3 — Frontend store \& Brewery tab shell](#7-phase-3--frontend-store--brewery-tab-shell)
    - [Tasks](#tasks-2)
  - [8. Phase 4 — Discovery view](#8-phase-4--discovery-view)
    - [Discovery Matrix](#discovery-matrix)
    - [Discovery List](#discovery-list)
    - [Effect Scaling](#effect-scaling)
    - [Considerations](#considerations)
  - [9. Phase 5 — Aging tracker](#9-phase-5--aging-tracker)
    - [Wine aging](#wine-aging)
    - [Liquor aging](#liquor-aging)
    - [Schema addition](#schema-addition)
    - [Considerations](#considerations-1)
  - [10. Phase 6 — Live brew tracking (Player.log)](#10-phase-6--live-brew-tracking-playerlog)
    - [Event sequence to detect](#event-sequence-to-detect)
    - [Tasks](#tasks-3)
    - [Considerations](#considerations-2)
  - [11. Phase 7 — Brew session summary](#11-phase-7--brew-session-summary)
    - [Display](#display)
  - [12. Phase 8 — Polish \& empty states](#12-phase-8--polish--empty-states)
  - [13. Future ideas](#13-future-ideas)
  - [14. Open questions](#14-open-questions)
  - [random notes](#random-notes)

---

## 1. Goals & non-goals

**Goals:**
- Help players discover and record their personal ingredient→effect mappings
  through a "Brewing Journal" that automatically extracts data from item JSON
  exports.
- Show a "What haven't I tried?" matrix for each recipe so players can
  systematically explore their combinatorial space.
- Track wine aging (uses gained over time) and liquor aging (un-aged→aged
  transition) across inventory exports.
- Provide a live session view when the player is actively brewing (XP rate,
  brews completed, new effects discovered).
- Use CDN recipe data to enumerate all valid ingredient combinations per recipe,
  so the discovery matrix is complete and accurate.

**Non-goals:**
- **No wiki-style "correct answers"** — the whole point is that mappings are
  per-player. We never tell the player what effect a combo "should" give.
- **No brewing automation** — we track and display, we don't tell the player
  what to brew. The discovery matrix shows what's untried, not what's optimal.
- **No cross-player data sharing** — each player's journal is their own.
- **No real-time aging simulation** — we observe aging from JSON reports, we
  don't guess rates or predict completion times (at least not in v1).

---

## 2. Background — how brewing works

### Three drink categories

1. **Beers** — single-step craft (`"Brewing..."` action). Fixed ingredients
   (grain + hops + yeast) plus 2-4 keyword-based variable slots. Effect
   determined at craft time by the variable ingredient choices. Served as
   single glasses or kegs (24 uses).

2. **Wines** — single-step bottling (`"Bottling..."` action). Fixed ingredients
   (bottle + cork + fruit) plus 1-4 variable slots. Wines **age in inventory,
   gaining uses over time** until first used. CDN base is 24 uses; observed
   values reach 100+.

3. **Liquors** — two-step process. First, a "Prepare" recipe produces an
   un-aged cask (no variable ingredients, no effects). The cask ages in a cave
   (typically 1 hour, offline OK). Then a "Finish" recipe takes the aged cask
   plus 4 variable keyword ingredients — this is where effects are determined.
   Liquors include unique `BrewingAnimalPart` slots (monster parts).

### The BrewItem system

CDN recipes define effects via `ResultEffects`:
```
BrewItem(tier, skillReq, slot1+slot2+...=effectPool1+effectPool2+...)
```
- **Left of `=`:** keyword slots that determine the effect (e.g.,
  `BrewingMushroomC4+BrewingGarnishC3`)
- **Right of `=`:** effect category pools — the universe of possible effects
  for this recipe (e.g., `Partying4+Gathering4+RacialBonuses48`)
- **Per-player randomization:** The mapping from specific ingredient combo to
  specific effect within the pool is unique per player.

### Race restrictions & skill requirements

Brewed drinks can have **race restrictions** — e.g., "elf beer" can only be
drunk by elves. This is extremely painful for non-matching races to discover
by accident (you waste ingredients). The `RacialBonuses48` effect pool is the
likely source of these. The discovery matrix should clearly flag race-restricted
effects.

Drinks also have **skill requirements to consume** that vary per effect. The
required skill is NOT always Brewing — it can be:
- **Endurance** (e.g., level 38)
- **Nature Appreciation** (e.g., level 50)
- **Gourmand** (e.g., level 50)

The same effect template scales with recipe tier — higher tiers give bigger
numbers but require higher skill levels. E.g., "Dance Appreciation Boost +12 /
Extra Effect Chance +16%" is the same template as a lower tier that gives +8.

**Keg bypass:** Anyone can drink from a **deployed keg** (tapped on the ground)
regardless of skill level requirements. Race restrictions still apply. This
means kegs are how brewers share high-level effects with lower-level players.

### Data available per crafted drink (from item JSON)

Each brewed item in the inventory JSON stores:
- `IngredientItemTypeIds` — array of the exact item TypeIDs used
- `TSysPowers` — the effect granted (`Power` name + `Tier`)
- `UsesRemaining` — servings left
- `Crafter` — who brewed it

This is the primary data source for building the personal discovery journal.

---

## 3. Data sources

| Source | What it provides | When available |
|--------|-----------------|----------------|
| CDN `recipes.json` | Recipe definitions, keyword slots, valid ingredient lists, effect pools, skill requirements | Always (loaded at startup) |
| CDN `items.json` | Item names, `Brewing*` keywords on ingredients, drink metadata (`NumUses`, `DestroyWhenUsedUp`, `AlcoholLevel`) | Always |
| Item JSON export | Per-item `IngredientItemTypeIds`, `TSysPowers`, `UsesRemaining`, `Crafter` | On user import (manual or auto-detect from Reports folder) |
| Player.log | `ProcessDoDelayLoop` ("Brewing..."/"Bottling..."), `ProcessUpdateRecipe`, `ProcessUpdateSkill`, `ProcessAddItem`, `ProcessUpdateItemCode`/`ProcessDeleteItem` (ingredient deltas) | Real-time during play |
| Character JSON | Brewing skill level, `WeaponAugmentBrewing` level, recipe completion counts | On user import |

---

## 4. Build order

```
Phase 1 ── CDN brewing data extraction                              [Rust]
Phase 2 ── Discovery journal (item JSON import)                     [Rust]
Phase 3 ── Frontend store & Brewery tab shell                       [Vue]
Phase 4 ── Discovery view (journal + matrix)                        [Vue]
Phase 5 ── Aging tracker                                            [Rust + Vue]
Phase 6 ── Live brew tracking (Player.log events)                   [Rust]
Phase 7 ── Brew session summary                                     [Vue]
Phase 8 ── Polish pass: empty states, onboarding, tooltips          [Vue]
```

Phases 1-4 form the MVP — a useful tool from day one via item JSON imports.
Phases 5-7 add live tracking and aging. Phase 8 is polish.

---

## 5. Phase 1 — CDN brewing data extraction

**Goal:** Parse CDN recipe and item data to build a structured brewing data
model that the frontend can query.

### Tasks

- Parse all recipes where `Skill == "Brewing"` from CDN `recipes.json`.
- For each brewing recipe, extract:
  - Fixed ingredients (items with `ItemCode`)
  - Variable ingredient slots (items with `ItemKeys` — the keyword-based slots)
  - The `ResultEffects` `BrewItem(...)` string — parse into structured fields:
    tier, skill requirement, slot keywords, effect pool categories
  - Result item code, XP, skill level requirement
  - `UsageDelayMessage` to classify: "Brewing...", "Bottling...", "Preparing...",
    "Tapping..."
- Parse all items with any `Brewing*` keyword from CDN `items.json`.
  Build an index: keyword → list of items that can fill that slot.
- Expose a Tauri command: `get_brewing_recipes` → returns structured recipe list
  with resolved ingredient slot options.
- Expose a Tauri command: `get_brewing_ingredients` → returns all items usable
  in brewing, grouped by keyword slot.

### Key data structures

```
BrewingRecipe {
    recipe_id: u32,
    internal_name: String,
    name: String,
    category: BrewingCategory,  // Beer, Wine, Liquor, Utility
    skill_level_req: u32,
    xp: u32,
    fixed_ingredients: Vec<FixedIngredient>,
    variable_slots: Vec<VariableSlot>,
    effect_pools: Vec<String>,  // e.g., ["Partying4", "RacialBonuses48"]
    result_item_id: u32,
    brew_tier: u32,
}

BrewingCategory { Beer, BeerKeg, Wine, LiquorUnaged, LiquorFinished, Utility }

VariableSlot {
    keyword: String,           // e.g., "BrewingMushroomC4"
    description: String,       // e.g., "Coral Mushroom Powder, Groxmax, ..."
    valid_items: Vec<u32>,     // item TypeIDs that have this keyword
}
```

---

## 6. Phase 2 — Discovery journal (item JSON import)

**Goal:** Import brewed drinks from the player's item JSON exports and extract
the ingredient→effect mappings into a persistent journal.

### Tasks

- When an item JSON is imported (existing import flow), scan for items where:
  - `IngredientItemTypeIds` is present AND
  - `TSysPowers` is present AND
  - The item's TypeID matches a known brewing result item (from Phase 1 CDN data)
- For each such item, record a `BrewingDiscovery`:
  - `recipe_id` — resolved by matching the result item TypeID to CDN recipe data
  - `ingredient_ids: Vec<u32>` — from `IngredientItemTypeIds`
  - `power: String` — from `TSysPowers[0].Power`
  - `tier: u32` — from `TSysPowers[0].Tier`
  - `effect_name: String` — the human-readable drink name suffix/prefix (e.g.,
    "Partier's", "of Elfinity") extracted from the item name vs base recipe name
  - `race_restricted: Option<String>` — if the power name contains a race
    identifier (e.g., `BrewingMaxHealthElf` → "Elf"), record it. This is
    critical for non-matching races to avoid wasting ingredients.
  - `discovered_at: timestamp` — from the JSON export timestamp
  - `character: String`
- Also capture from the item: any skill requirements to drink (if present in
  the item JSON — may need CDN item data for the result item to get `SkillReqs`)
- Deduplicate: same recipe + same ingredient combo = same discovery. Update
  timestamp if re-observed but don't create duplicates.
- DB migration: new `brewing_discoveries` table.
- Expose Tauri command: `get_brewing_discoveries(recipe_id?)` → returns all
  known mappings, optionally filtered by recipe.

### Schema

```sql
CREATE TABLE brewing_discoveries (
    id INTEGER PRIMARY KEY,
    character TEXT NOT NULL,
    recipe_id INTEGER NOT NULL,
    ingredient_ids TEXT NOT NULL,  -- JSON array of TypeIDs, sorted
    power TEXT NOT NULL,
    power_tier INTEGER NOT NULL,
    effect_label TEXT,            -- human name: "Partier's", "of Elfinity", etc.
    race_restriction TEXT,       -- NULL if none, or "Elf", "Rakshasa", etc.
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    UNIQUE(character, recipe_id, ingredient_ids)
);
```

---

## 7. Phase 3 — Frontend store & Brewery tab shell

**Goal:** Wire the Brewery tab into the Crafting view and create the Pinia store.

### Tasks

- Add `{ id: "brewery", label: "Brewery" }` to the crafting tabs in MenuBar.vue.
- Create `src/components/Crafting/BreweryTab.vue` using PaneLayout:
  - **Left pane:** Recipe list grouped by category (Beers, Wines, Liquors),
    filterable by skill level, searched by name.
  - **Center:** Detail panel — changes based on selected recipe.
- Create `src/stores/breweryStore.ts`:
  - Load brewing recipes and ingredients from backend (Phase 1 commands).
  - Load discoveries from backend (Phase 2 commands).
  - Computed: group recipes by category, filter/search, build discovery matrix
    per recipe.
- Add the BreweryTab to CraftingView.vue's tab switcher.

---

## 8. Phase 4 — Discovery view

**Goal:** The main feature — show what the player has discovered and what's
left to try for each recipe.

### Discovery Matrix

For the selected recipe, display a grid/table of all valid ingredient
combinations for the variable slots:

- **Axes:** One axis per variable slot. For a 2-slot recipe, it's a simple 2D
  grid. For 3-4 slot recipes, use nested grouping or a flat table with columns
  per slot.
- **Cell contents:**
  - **Discovered:** Show the effect label (e.g., "Partier's" or "of Elfinity").
    Color-coded green for usable effects.
  - **Race-restricted:** Discovered but restricted to a race the player can't
    use — color-coded red/orange with the race name. This is the most important
    warning in the matrix: "don't craft this again, it's elf-only and you're
    a rakshasa."
  - **Untried:** Gray/empty cell. Hovering shows the ingredient combo.
- **Stats:** "X of Y combinations discovered" progress bar per recipe.
  Separate count: "Z race-restricted (avoid)".
- **Effect search:** "I want BrewingDanceAppreciation — which combos might give
  it?" Show the effect pool for this recipe and highlight which effects have
  been found, which are still in play.

### Discovery List

Below or beside the matrix, a flat table of all discoveries for this recipe.
Columns inspired by what real brewers actually track (per Buppis's spreadsheet):

| Slot 1 | Slot 2 | Slot 3 | Slot 4 | Effect | Race? | Req Skill | Req Lvl |
|--------|--------|--------|--------|--------|-------|-----------|---------|
| Corn | Green Apple | Coral Mush. | Peppercorns | Dance Appreciation +12 | — | Endurance | 38 |
| Corn | Green Apple | Groxmax | Cinnamon | Max Health Elf | Elf | Gourmand | 50 |

- Sortable by effect name, discovery date, ingredients, race restriction
- Each row shows: ingredient names (resolved via CDN) → effect + skill req
- Click to highlight the cell in the matrix
- Filter: hide race-restricted effects for current player's race

### Effect Scaling

Effects follow templates where the numbers scale with recipe tier — e.g.,
"Dance Appreciation Boost +8 / Extra Effect Chance +12%" at one tier becomes
"+12 / +16%" at a higher tier. The discovery matrix should group these as the
same underlying effect across tiers when possible, showing only that the combo
maps to the "Dance Appreciation" template. The specific numbers come from the
tier of the recipe being brewed.

### Considerations

- For high-tier recipes with 4 variable slots and 4+ options each, the matrix
  could have 256+ cells. Consider pagination or collapsible sections.
- The matrix should indicate which effect pools are possible for this recipe
  (from CDN `ResultEffects`), so the player knows the search space.
- Some effect pools include `TBD` categories — flag these as "placeholder
  effects (not yet implemented by devs)".
- **Race restriction detection** from power names: powers containing race
  identifiers (`Elf`, `Rakshasa`, `Orc`, `Dwarf`, `Fae`, `Human`, `Lycanthrope`)
  should be flagged. Cross-reference with the player's race from character JSON.

---

## 9. Phase 5 — Aging tracker

**Goal:** Track wine aging (uses gained) and liquor aging (un-aged→aged
transitions) across item JSON reports.

### Wine aging

- When an item JSON is imported, record wine items with their `UsesRemaining`.
- Across multiple imports, calculate the delta in uses for the same wine item
  (matched by... item instance? TypeID + vault location? TBD — see open
  questions).
- Display: list of wines with current uses, base uses (from CDN), and
  observed gain over time.
- Flag wines that have been "opened" (uses have decreased) — these have
  stopped aging.

### Liquor aging

- Track items named "Un-Aged X" across imports.
- When the same storage slot shows "Aged X" (or "X") in a later import, mark
  the transition.
- Display: list of un-aged casks with time-in-storage estimate.

### Schema addition

```sql
CREATE TABLE brewing_aging_snapshots (
    id INTEGER PRIMARY KEY,
    character TEXT NOT NULL,
    snapshot_at TEXT NOT NULL,          -- JSON export timestamp
    item_name TEXT NOT NULL,
    item_type_id INTEGER NOT NULL,
    storage_vault TEXT,
    uses_remaining INTEGER,
    is_unaged BOOLEAN NOT NULL DEFAULT 0
);
```

### Considerations

- Item identity across exports is tricky — items don't have stable instance IDs
  in the JSON. We may need to match on TypeID + vault + position, or accept
  some ambiguity.
- Wine aging rate calculation needs at least 2 reports with the same wine.
  Show "insufficient data" until then.

---

## 10. Phase 6 — Live brew tracking (Player.log)

**Goal:** Detect brewing activity in real-time from Player.log events and
record what was brewed.

### Event sequence to detect

1. `ProcessDoDelayLoop(3, UseItem, "Brewing..."/"Bottling...", actionId, ...)`
   — marks the start of a brew.
2. `ProcessDeleteItem` / `ProcessUpdateItemCode` — ingredient consumption
   (stack deltas).
3. `ProcessAddItem(InternalName(instanceId), -1, True)` — the output drink.
4. `ProcessUpdateRecipe(recipeId, count)` — recipe completion.
5. `ProcessUpdateSkill({type=Brewing,...}, ...)` — XP gained.

All of these typically fire at the same timestamp.

### Tasks

- Add brewing event detection to the PlayerEventParser / coordinator.
  Emit a `BrewCompleted` event containing:
  - `recipe_id`, `xp_gained`, `output_item_name`, `timestamp`
  - Ingredient deltas (from UpdateItemCode stack changes) — best effort,
    since correlating which deltas belong to which brew is noisy.
- The frontend store subscribes to `BrewCompleted` events to update the
  live session view and increment brew counts.
- **Do NOT attempt to extract the TSysPower from logs** — the effect is only
  known from the item JSON. The log only tells us the item name (which encodes
  the effect name as a prefix/suffix, but this is fragile to parse).

### Considerations

- The ingredient delta tracking from logs is a "nice to have" — the item JSON
  is the authoritative source for ingredient→effect mapping. Log-based
  ingredient tracking is useful for session summaries ("you consumed 50 Barley
  and 20 Coral Mushroom Powder") but not reliable enough for discovery mapping.
- Distinguish "Brewing..." (beers/kegs, actionId 5745) from "Bottling..."
  (wines, actionId 6766) from "Preparing..." (liquor casks) and "Tapping..."
  (deploying kegs).

---

## 11. Phase 7 — Brew session summary

**Goal:** After a brewing session, show a summary of what was accomplished.

### Display

- **Session stats:** Total brews, XP earned, XP rate (per hour), level
  progress, time spent.
- **By recipe:** Breakdown of how many of each recipe were brewed.
- **New discoveries:** Any brews where the ingredient combo was previously
  untried (cross-reference with discovery journal). This requires a post-session
  item JSON import to learn the actual effects.
- **Ingredient consumption:** Total ingredients consumed during the session
  (from log deltas).
- **Prompt:** After a brewing session, prompt the user to export their item
  JSON so the discovery journal can be updated with the new brews' effects.

---

## 12. Phase 8 — Polish & empty states

- **Empty state:** "No brewing data yet. Import an item JSON export to get
  started, or start brewing with glogger running!"
- **Onboarding:** Brief explanation of per-player effect randomization and why
  the discovery journal matters.
- **Tooltips:** On ingredient items in the matrix, show the item's CDN
  description and Brewing keywords.
- **Effect pool legend:** For each recipe, show the available effect categories
  with descriptions of what kinds of effects they contain.
- **Inline components:** Use `ItemInline` for all ingredient references in the
  discovery list/matrix.

---

## 13. Future ideas

These are not in scope for the initial build but are worth considering:

- **Aging rate estimation** — after enough reports, calculate uses-per-hour
  for each wine type and show predicted maturity dates.
- **Brew planner** — "I want to make 10 Dwarven Stout Kegs targeting
  BrewingDanceAppreciation. Here's the shopping list." Integrates with the
  existing Projects tab.
- **Effect database** — resolve TSysPower names to actual buff descriptions
  (duration, magnitude). May require a CDN `tsyspowers` file or manual data
  entry.
- **Drink inventory** — show all brewed drinks across all storage vaults with
  uses remaining, effects, and aging status. A "cellar view."
- **Auto-detect item JSON exports** — watch the Reports folder for new exports
  and auto-import without user action.
- **Keg deployment tracker** — when a keg is tapped (TapAlcoholKeg recipe),
  track which effects were served to the community. Note: deployed kegs bypass
  skill requirements (anyone can drink regardless of Endurance/Gourmand/Nature
  Appreciation level), but race restrictions still apply.
- **Skill requirement resolver** — map TSysPower names to their consumption
  skill requirements (Endurance, Gourmand, Nature Appreciation) and levels.
  This data may be on the CDN result items or discoverable from item JSON
  exports. Would let the discovery matrix show "you can't drink this yet
  (needs Nature Appreciation 50)".

---

## 14. Open questions

1. **Wine identity across exports** — how do we match the same wine bottle
   across two JSON exports to track aging? Items don't have persistent instance
   IDs in the JSON. Options: match on (TypeID + vault + power + crafter), or
   accept that we're tracking types not instances.

2. **Effect pool contents** — CDN recipes give us pool names like `Partying4`
   and `RacialBonuses48`, but we don't know which specific TSysPowers are in
   each pool. Is there a CDN file that maps pool→powers? Or do we have to
   discover that empirically from player data?

3. **Liquor aging location** — the CDN says "stored in a cave." Does the item
   need to be in specific storage vaults? If so, which ones count as "caves"?
   This affects whether we can predict aging completion.

4. **Wine aging rules** — does aging rate depend on wine type? Storage
   location? Is it linear? Does it stop at a cap? We need more data points.

5. **"Placeholder Effects"** — some drinks get `TSysPowers` with names that
   map to placeholder/TBD pools. Should we hide these in the discovery matrix
   or flag them as "effect not yet implemented"?

6. **Keg vs glass recipes** — keg versions use the same variable slots as
   glass versions. Do they share the same per-player ingredient→effect mapping?
   If so, a discovery from a glass recipe would apply to the keg recipe too,
   cutting the search space in half.

7. **Drink skill requirements** — the required skill to drink (Endurance,
   Gourmand, or Nature Appreciation) and level vary per effect, not per recipe.
   Where does this data live? Is it on the result item in CDN? In the item JSON
   export? Or is it only visible in-game on the item tooltip? We need to find
   the source so we can show "needs Endurance 38" vs "needs Gourmand 50" in the
   discovery list.

8. **Race restriction detection** — we're inferring race from TSysPower names
   (e.g., `BrewingMaxHealthElf` → Elf). Is this reliable? Are there
   race-restricted effects whose power names don't contain the race? The CDN
   result item keywords include `AlcoholLevel` but may also contain race info.


## random notes
- what about a 'make this next' generator? we could do it globally and by specific recipe. finds a combo of ingredients you haven't used, but own. 