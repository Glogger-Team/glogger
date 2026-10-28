# Character — Stats

## Overview

The Stats tab manages character report imports (Export Character as JSON in Settings → VIP) and provides browsable, comparable Character reports of a character's skills, stats, currencies, and recipe completions at various points in time.

## Architecture

### Files

**Backend (Rust):**
- `src-tauri/src/db/character_commands.rs` — import, report queries, comparison

**Frontend (Vue/TS):**
- Inline in `src/components/Character/CharacterView.vue` (stats tab section)
- `src/components/Character/SnapshotList.vue` — report selector
- `src/components/Character/SnapshotComparison.vue` — diff two reports
- `src/components/Character/SkillTable.vue` — skill levels from report
- `src/components/Character/StatsTable.vue` — combat/attribute stats
- `src/components/Character/CurrencyTable.vue` — currency holdings
- `src/components/Character/RecipeTable.vue` — known recipe completions
- `src/stores/characterStore.ts` — Character report management, import

## How It Works

### Import Flow

1. Player runs Export Character as JSON in-game (Settings → VIP), producing `Character_{Name}_{Server}.json`
2. User clicks "Import Report" (or auto-import detects the file)
3. Rust parses JSON, validates `Report == "CharacterSheet"`, inserts report + all child data in a transaction
4. Deduplication: `ON CONFLICT DO NOTHING` on `(character_name, server_name, snapshot_timestamp)`
5. Import also seeds `game_state_skills`, `game_state_favor`, and other game state tables

### Report Browsing

- Dropdown to select from all imported Character reports for the active character
- Each report shows: skill levels/XP, combat stats, currencies, recipe completions
- Report comparison: select two reports to see skill diffs (level changes, XP changes)

## Character Report Format

### Top-Level Fields

| Field | Type | Description |
|-------|------|-------------|
| `Character` | string | Character name |
| `ServerName` | string | Server name |
| `Timestamp` | string | ISO-ish: `"2026-03-08 14:56:31Z"` |
| `Report` | string | Always `"CharacterSheet"` |
| `Race` | string | Character race |
| `Skills` | object | Skill name → `{ Level, BonusLevels, XpTowardNextLevel, XpNeededForNextLevel, Abilities? }` |
| `RecipeCompletions` | object | Recipe internal name → completion count |
| `CurrentStats` | object | Stat key → numeric value |
| `Currencies` | object | Currency key → amount |
| `NPCs` | object | NPC key → `{ FavorLevel }` |
| `ActiveQuests` | string[] | Active quest internal names (stored in raw_json only) |

## Database Tables

- **`character_snapshots`** — one row per import (character_name, server_name, timestamp, race, raw_json). Unique on (character_name, server_name, snapshot_timestamp).
- **`character_skill_levels`** — per skill per report (level, bonus_levels, xp_toward_next, xp_needed_for_next)
- **`character_npc_favor`** — per NPC per report (npc_key, favor_level)
- **`character_recipe_completions`** — per recipe per report (recipe_key, completions)
- **`character_stats`** — per stat per report (stat_key, value as REAL)
- **`character_currencies`** — per currency per report (currency_key, amount)

## Tauri Commands

| Command | Purpose |
|---------|---------|
| `import_character_report` | Parse JSON file, insert report + all child data |
| `get_character_snapshots` | List snapshots for a character (ordered by timestamp DESC) |
| `get_snapshot_skills` | All skill levels for a snapshot |
| `get_snapshot_npc_favor` | All NPC favor for a snapshot |
| `get_snapshot_recipes` | All recipe completions for a snapshot |
| `get_snapshot_stats` | All stats for a snapshot |
| `get_snapshot_currencies` | All currencies for a snapshot |
| `get_characters` | List all known characters (distinct name+server pairs) |
| `compare_snapshots` | Compare skills between two snapshots → `Vec<SkillDiff>` |

## Report Stats (Live)

In addition to report-based stats from character exports, the Stats tab displays **live report stats** parsed from in-game reports. When a player opens their behavior report or age report in-game, the content flows through `ProcessBook` → coordinator → `report_stats::parse_*` → `character_report_stats` table automatically.

### Data Sources

| Report | book_type | Stats Extracted |
|--------|-----------|-----------------|
| Age Report | `PlayerAge` | Created on, time played, deaths, attacks, kills, damage dealt/taken, VIP days |
| Behavior Report | `HelpScreen` | Challenge restrictions (with counts), behavior badges, food stats, killing stats by species, misc stats (time online, friends, burials, logins) |

### Storage

Stats are stored in `character_report_stats` as key-value pairs:

| Column | Type | Description |
|--------|------|-------------|
| `character_name` | TEXT | Character name |
| `server_name` | TEXT | Server name |
| `category` | TEXT | Group: `age`, `challenges`, `badges`, `food_stats`, `killing_stats`, `misc_stats` |
| `stat_name` | TEXT | Snake_case key, e.g. `killed_foes_any`, `time_spent_online` |
| `stat_value` | TEXT | Cleaned value: numbers stripped of commas, durations as-is, badges as `"true"` |
| `updated_at` | TEXT | When the report was last processed |

Upserts on `(character_name, server_name, category, stat_name)` — opening the report again updates values.

### UI

`ReportStatsSection.vue` displays below the report tables on the Stats tab:
- Groups stats by category with section headers
- Filterable search across names, categories, and values
- Numbers formatted with locale separators, badges shown as checkmarks
- Shows "Last updated" timestamp
- Auto-refreshes when `report_stats` domain updates

### Key Files

- `src-tauri/src/report_stats.rs` — Parsers for PlayerAge and Behavior Report content
- `src-tauri/src/db/game_state_commands.rs` — `get_character_report_stats` query command
- `src/components/Character/ReportStatsSection.vue` — Frontend component

## Pooping (Dungcrafting) Group

The same card also renders a synthetic **Dungcrafting** group: the lifetime poop count plus the skill's level.

Pooping is the cow-curse ability whose skill the game displays as **Dungcrafting** (CDN `skills.name = 'Dungcrafting'` with `xp_table = 'Pooping'`; Player.log reports it as `ProcessLoadSkills({type=Pooping,...})`). Its only per-event trace is a Combat-channel chat line:

```text
26-10-11 19:59:15	[Combat] TwinkleofToes: Poop on TwinkleofToes!
```

`Combat` is in `default_excluded_channels`, so those lines never reach `chat_messages` — the count is fed from the raw chat stream instead:

| Source | Path |
|---|---|
| Live tail | `DataIngestCoordinator::process_chat_events` → `chat_combat_parser::parse_self_poop_line` → `record_poop` (Combat branch, right beside the self-milking check) |
| Chat.log history | `backfill_ability_history` command — reads every `Chat-*.log` from byte 0 (position-based scans had already reached EOF before this existed); the same scan also imports self-milks for the Milking Timers widget |

Storage: `player_pooping_log` (migration v71), one row per event, `UNIQUE(character_name, server_name, pooped_at, target_name)`. `pooped_at` is the chat log's **local wall-clock** timestamp so live and backfill rows dedup against each other; the insert targets that key (`INSERT … ON CONFLICT DO NOTHING`), so re-running the backfill counts nothing twice and a real write error still surfaces. Only *your* poops count — the sender must be the active character.

The card backfills itself once per app session (shared with the Milking Timers widget), and only for characters that actually have the Dungcrafting skill, then stops touching the archives.

### Rows

| Row | Source |
|---|---|
| Level | `game_state_skills` (`Dungcrafting`), shown as `level / max_level` |
| Total Poops | `COUNT(*)` from `player_pooping_log` |
| Days Pooped | `COUNT(DISTINCT substr(pooped_at, 1, 10))` |
| First Poop / Last Poop | `MIN`/`MAX(pooped_at)` (date only) |

Commands: `get_pooping_stats`, `backfill_ability_history`. Live poop events emit `game-state-updated: ["report_stats"]` so the card refreshes.

`useAbilityHistoryBackfill` (composable) owns the one-scan-per-session rule shared with the Milking Timers widget. Surfaces ask for the scan on mount rather than when their counter reads zero — a single live event before the first scan must not lock the archive out. Both surfaces also render the scan's failure (`lastError`) instead of an unexplained empty counter, and a failed attempt gets one retry on the next trigger before the session gives up (2 attempts per character/server — a persistent cause can't be fixed by re-reading the archive). Verified 2026-10-13: a missing `game_data_path` makes the app run its setup wizard and re-derive the path, so an unconfigured directory is not a realistic failure — the net is for file/DB errors.

## Computed Stats

The Stats tab includes a **Computed Stats** card that calculates derived statistics by joining game state data with CDN recipe metadata. Auto-refreshes on `skills`, `recipes`, or `report_stats` domain updates, and also loads on mount.

### Stats Computed

**Skill Totals:**

| Stat | Source |
|------|--------|
| Total Level (base + bonus + synergy) | `game_state_skills` SUM |
| Total Base Levels | `game_state_skills` SUM(level) |
| Total Bonus Levels | `game_state_skills` SUM(bonus_levels) |
| Skills Known | `game_state_skills` COUNT |
| Total XP Earned | `game_state_skills` SUM(xp) |

**Rate Stats** (requires `/age` report opened in-game):

| Stat | Source |
|------|--------|
| Time Played | `character_report_stats` age/time_played, parsed from "X days Y hours" |
| XP / Hour | total_xp_earned ÷ hours_played |
| Kills / Hour | age/kills ÷ hours_played |
| Deaths / Hour | age/deaths ÷ hours_played |

**Crafting Activity:**

| Stat | Source |
|------|--------|
| Items Crafted | `game_state_recipes` JOIN `recipes` — excludes Lore, Teleportation, Augmentation, Transmutation |
| Items Distilled | Transmutation recipes excluding Repair |
| Items Deconstructed | Augmentation Decompose/Distill recipes |
| Times Teleported | Teleportation recipes excluding Bind |
| Dye Crafts | Dye Making skill or DyeRecipe keyword |
| Time Watching Bars Fill | SUM(completion_count × usage_delay) across all recipes |

**Crafting By Skill:** Per-skill breakdown showing total crafts and time spent, sorted by most crafted. Each row shows the skill name, craft count, and bar-fill duration.

**Attribute Ranges:** Tracks the min/max/current value of every attribute that has changed. Attributes that never changed (min == max) are filtered out. Displayed as color-coded min / current / max values. Requires migration v35 (`min_value`/`max_value` columns on `game_state_attributes`). Updated live as `ProcessSetAttributes` events arrive.

### Key Files

- `src-tauri/src/db/game_state_commands.rs` — `get_computed_stats` command, `ComputedStats` + `SkillCraftingBreakdown` + `AttributeExtreme` structs
- `src/components/Character/ComputedStatsCard.vue` — Frontend component

## Layout

The Stats tab uses a **3-column card layout** with internal scrolling per card:

| Column 1 | Column 2 | Column 3 |
|----------|----------|----------|
| Combat Stats (flex) | Skills (flex) | Computed Stats (flex) |
| Player Attributes (flex) | Currencies (30% max) | Character Report Stats (flex) |

## Edge Cases

| Case | Handling |
|------|----------|
| Duplicate import | Skip via UNIQUE constraint, `was_duplicate: true` |
| `XpNeededForNextLevel: -1` | Stored as-is (means max level) |
| Wrong `Report` type | Error returned |
| Empty `Skills`/`NPCs` | Valid — import with 0 entries |
| Recipe with 0 completions | Stored — tracks which recipes are known |
| Stat values with decimals | Stored as REAL |
