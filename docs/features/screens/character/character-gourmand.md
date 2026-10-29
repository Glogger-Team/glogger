# Character — Gourmand

## Overview

Tracks which foods a player has eaten for the Gourmand skill. Shows progress toward eating everything, highlights what's still needed, lets players compare food buff combinations, and supports exporting an uneaten foods list.

## Architecture

### Files

**Backend (Rust):**
- `src-tauri/src/db/gourmand_commands.rs` — report parsing, food queries, import/export
- `src-tauri/src/db/cdn_persistence.rs` — `foods` table population during CDN refresh
- `src-tauri/src/game_data/food_sources.rs` — source classification (crafted vs event vs …)

**Frontend (Vue/TS):**
- `src/stores/gourmandStore.ts` — Pinia store
- `src/components/Gourmand/GourmandView.vue` — main layout
- `src/components/Gourmand/FoodCategorySection.vue` — category with sorting/filtering
- `src/components/Gourmand/FoodCard.vue` — card view item
- `src/components/Gourmand/FoodListRow.vue` — compact list row
- `src/components/Gourmand/FoodItemWithTooltip.vue` — hover tooltip wrapper
- `src/components/Gourmand/FoodComparisonPanel.vue` — meal + snack buff comparison
- `src/components/Gourmand/GourmandProgressBar.vue` — reusable progress bar

## Data Flow

```
CDN items (food_desc != null) → foods table (built during CDN ingestion)
Game Gourmand report (.txt)   → gourmand_eaten_foods table → Vue frontend
```

1. During CDN refresh, items with a non-null `food_desc` are parsed into a `foods` table (606 rows at CDN 486, every one an item the game marks as giving Gourmand XP)
2. The category token is canonicalised by `canonical_food_category` (`Instant Snack` and `Instant-Snack` are the same category — CDN versions disagree on the separator, migration v70 repairs rows already stored)
3. Player uses the Gourmand skill's "Request Skill Report" ability in-game, producing `SkillReport_*.txt`
4. On view mount, the app auto-imports the latest gourmand report from Books folder, or user imports manually
5. Eaten foods are persisted in `gourmand_eaten_foods` so data survives across sessions

## Report Parsing

The gourmand report format:

```
Gourmand Report for PlayerName
...
Foods Consumed:
  Super Fishy Surprise (HAS MEAT) (HAS DAIRY): 28
  Weird Fruit Cocktail: 25
```

- Lines after `"Foods Consumed:"` are parsed as food entries
- `strip_food_tags()` removes parenthesized tags like `(HAS MEAT)`, `(HAS DAIRY)`
- On import, `gourmand_eaten_foods` is cleared and repopulated (report model)

## Source Classification

Each food is tagged during CDN ingestion with *where it comes from*, so the
screen can separate the ~443 foods you can cook from the ones that only turn up
during an event. Classification lives in `game_data/food_sources.rs` and is
driven by the item's entry in `sources_items.json`:

| Kind | Derived from |
|------|--------------|
| `unobtainable` | `Lint_NotObtainable` keyword |
| `crafted` | a `Recipe` source entry (the recipe's skill becomes `craft_skills`) |
| `event` | curated rule list — see below |
| `vendor` / `quest` / `npc-gift` / `hangout` / `barter` / `container` | the matching `sources_items.json` entry type |
| `other` | no source entry at all — drops, foraged ingredients, butchered flesh |

A food carries a *set* of kinds (Cheese is often both `crafted` and `vendor`),
emitted in the priority order above; `source_kinds[0]` is what the UI badges and
sorts on.

**The CDN has no event flag** — no keyword, no field, no source entry — so the
`event` kind comes from a hand-curated list in `is_event_food()` and needs a
line added whenever Project: Gorgon ships a new event food. It currently
matches the `HalloweenCandy` keyword, any `*CandyCane` / `SteamCake*` internal
name, the summer treat block (items 6480–6485), and the commemorative cakes
(Welcome, Apology, Thank You, Celebratory, Duke's). Deliberately excluded: the
pumpkin dishes are ordinary year-round Cooking recipes despite the theming, and
Jellybeans / Tiramisu of Delights have no evidence either way, so they fall
through to `other` rather than being guessed at.

## Gourmand Level Resolution

The store resolves the player's Gourmand level with a priority chain:
1. **Manual override** — user-entered value via header input
2. **Live session** — from skill store if a Gourmand XP event has been seen
3. **Character report** — from character store imported data

This level determines which foods the player can currently eat (foods with `gourmand_req > level` are marked unusable).

## Layout

Uses `PaneLayout` (screen-key `char-gourmand`) with left + conditional right panes:
- **Left pane** — progress bars (overall + per-category) and favorites (top 3 per category)
- **Center** — header bar, controls, and food category lists (scrollable)
- **Right pane** — food buff comparison panel (only appears when a meal or snack is selected)

## UI Features

- **Progress bars** — overall and per-category (Meals, Snacks, Instant-Snacks), shown in left pane
- **Favorites** — top 3 most-eaten foods per category, shown in left pane
- **Food Buff panel** — select a meal + snack to see combined buff stats in the right pane. Effect descriptions are resolved via the backend `resolve_effect_descs` command (same as item tooltips) to correctly display human-readable labels from CDN attribute tokens.
- **Card and list views** — toggle between grid cards and compact three-column lists
- **Sorting** — by gourmand level, food level, alphabetical, or source; ascending or descending
- **Filtering** — hide eaten foods, hide unusable foods (gourmand level too low)
- **Source filter** — dropdown narrowing to one source kind, plus a "Not crafted" option. Only kinds that actually match something are offered, each with its count. Category headers follow the filter (`12 / 32 eaten`); the hide toggles don't affect those counts.
- **Source badges** — card view badges every food with its craft skill (`Cooking`, `Cheesemaking`, `Sushi Preparation`) or source kind; list view badges only non-crafted foods, to keep the three-column layout dense. Event foods are amber, unobtainable ones struck through. Hovering shows every source the food has.
- **Attainable only** — recomputes the progress bars against just the foods you can still realistically get, dropping `event` and `unobtainable` from the denominators. Lists are unaffected.
- **Item tooltips** — hover any food for full item tooltip (description, effects, keywords, value)
- **Click to select** — click meals/snacks to populate the food buff comparison panel
- **Export uneaten** — save remaining uneaten foods to a text file. Respects the source filter and "attainable only", tags each line with its source, and names the active filters in the header so a narrowed export isn't mistaken for the full list.
- **Unmatched detection** — warns when report foods don't match CDN data (renamed/removed items)
- **Unknown-category bucket** — foods whose CDN category token isn't one of the three known ones (a spelling drift such as CDN 486's `Instant Snack`, or a genuinely new game category) render in their own "Unknown Categories" section instead of silently losing their section. The section header names the offending token(s) and points at `canonical_food_category()` (see migration v70); each card shows the raw token as stored.
- **Manual eaten marking** — click any food to toggle its eaten/uneaten status without importing a report. Manual marks are preserved during report imports (report data won't overwrite manual marks).
- **Color coding** — green (report-imported eaten), blue (manually marked eaten), red (uneaten), dimmed (can't eat), gold border (selected)

## Database Tables

**`foods`** — pre-parsed food data built during CDN ingestion:

| Column | Type | Description |
|--------|------|-------------|
| item_id | INTEGER PK | FK to items.id |
| name | TEXT | Food item name |
| icon_id | INTEGER | Icon reference |
| food_category | TEXT | `'Meal'`, `'Snack'`, or `'Instant-Snack'` — canonicalised on ingest (migration v70); unknown categories pass through verbatim |
| food_level | INTEGER | Parsed from `food_desc` |
| gourmand_req | INTEGER | From `raw_json.SkillReqs.Gourmand` (nullable) |
| effect_descs | TEXT | JSON array of effect strings |
| keywords | TEXT | JSON array of keyword strings |
| value | REAL | Item gold value |
| source_kinds | TEXT | JSON array of source kinds, most-significant first (migration v67) |
| craft_skills | TEXT | JSON array of crafting skill display names; empty unless crafted (migration v67) |

> Migration v67 clears `cdn_version` so the next startup re-persists the CDN —
> without that, `persist_cdn_data` short-circuits on an unchanged version and
> the two new columns stay at their `[]` default. The UI treats an empty
> `source_kinds` as "unknown" and simply omits the badge.

**`gourmand_eaten_foods`** — last-imported report:

| Column | Type | Description |
|--------|------|-------------|
| food_name | TEXT PK | Name as it appears in the report |
| times_eaten | INTEGER | How many times eaten |
| imported_at | TEXT | Timestamp of import |

## Tauri Commands

| Command | Purpose |
|---------|---------|
| `get_all_foods` | Query all rows from `foods` table |
| `import_gourmand_report` | Parse user-selected report file, persist to DB |
| `get_gourmand_eaten_foods` | Return last-imported eaten foods |
| `import_latest_gourmand_report` | Auto-import latest report from Books folder |
| `export_text_file` | Write uneaten foods list to a text file |
