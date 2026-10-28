# Crafting - Skills

## Overview

Summary pages for all crafting skills. Uses `PaneLayout` with a collapsible/resizable skill list on the left and a detailed skill breakdown in the center pane.

## Contents

### Left Pane (collapsible, resizable via `PaneLayout`)

Scrollable list of all skills that have crafting recipes.
- Displays level next to each skill
- Selected skill highlighted with gold accent border
- Sortable by skill level, alphabetical, or total number of crafts completed for that skill
- Clicking a skill displays its details in the center pane
- Auto-selects first skill on load

### Center Pane

#### Hero Header
Bold, screenshot-worthy header card (`bg-surface-base` with border):
- Skill name in accent gold with icon via `SkillInline`
- Large level number (3xl font-black) with base + bonus breakdown
- XP progress bar (gold fill) with percentage, or "MAX LEVEL" badge
- 2x2 grid of key stats (right-aligned, large bold numbers):
  - **Recipes** — crafted / total
  - **Available** — learnable recipes (gold when > 0)
  - **Completion** — percentage of recipes crafted at least once
  - **Total Crafts** — lifetime count (uses compact format: K/M)

The header also includes lifetime value stats on the right side (when data is available):
- **Materials Consumed** — estimated total value + item count
- **Output Value** — estimated total value + items produced
- **Profit / Loss** — output minus input cost with percentage return. Green/red coloring.

Pricing uses the app-wide two-tier strategy: market price if set, otherwise vendor value x 1.5. Material quantities account for `chance_to_consume` on ingredients and `percent_chance` on outputs.

#### Charts
Side-by-side donut chart panels (~60% of center pane width):
- **Top Materials Used** — donut chart (`VueUiDonut`) + full `ItemInline` list of all ingredients by quantity
- **Top Items Crafted** — donut chart + full `ItemInline` list of all outputs by quantity
- Items under 2% of total are bucketed into an "Other" slice in the chart (the list below still shows every item)

#### Recipe List
- Full table of all recipes in the skill, independently scrollable (max 400px)
- Columns: recipe name (via `RecipeInline`), required level, craft count, XP per craft
- Uncrafted recipes shown at reduced opacity
- Default sort: most crafted first
- Sortable by name, level, or craft count
- Filters: hide unlearned recipes, hide recipes above current level
- Sticky table header

## Data Sources

- **Skill levels** — `craftingStore.getSkillLevel()` (game state from player log / Character report)
- **Recipe completions** — `gameStateStore.recipeCompletions` (from `ProcessUpdateRecipe` events)
- **Per-skill stats** — `craftingStore.getSkillCraftingStats()` (aggregated from CDN recipes + player completions)
- **Recipe data** — `gameDataStore.getRecipesForSkill()` (CDN)
- **Item pricing** — `marketStore` (player-set prices) with vendor value x 1.5 fallback
- **Item resolution** — `gameDataStore.resolveItemsBatch()` for bulk ingredient/output item lookups
- **Charts** — `vue-data-ui` (`VueUiDonut`) for donut chart visualizations

## Files

- `src/components/Crafting/SkillsTab.vue` — main component

## Dependencies

- `vue-data-ui` — charting library (CSS imported globally in `main.ts`). First usage in the project; available for other screens too.
