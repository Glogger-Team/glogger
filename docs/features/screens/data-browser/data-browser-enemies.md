# Data Browser — Enemies

## Overview

Browse enemies/monsters from the game. The primary data source is the translation strings (`strings_requested.json` from `Translation.zip`), which provides display names and area assignments for ~1,150 unique monsters. When available, AI template data from `ai.json` enriches entries with combat properties (strategy, mobility, abilities).

## Search & Filter

- Area filter dropdown (filters by spawn location)
- Text search by name, key, comment, area name, or ability name
- Result count displayed

## Detail View

- Monster display name (from translation strings)
- Internal key
- Area link (clickable `AreaInline` if area-specific)
- **Comment** (if AI data available)
- **Properties** (if AI data available) — strategy (Melee/Ranged), mobility type, swimming, uncontrolled pet
- **Kill Stats** — total kills, loot table with drop rates (from player kill tracking — historical + imported only, no new loot rows on the current client)
- **Raw JSON** (opt-in via settings)

## Data Sources

- **Primary:** Translation strings (`Translation.zip` -> `strings_requested.json`) — monster keys follow the pattern `monster_{identifier}_Name` where identifier can be `Area{Name}/{MonsterId}` or just `{MonsterId}`
- **Enrichment:** AI templates (`ai.json`) — matched by stripping trailing numbers from the monster identifier
- **Kill stats:** SQLite `enemy_kills` table, matched by display name
- Backend commands: `get_all_monsters()` (translation-based list), `get_all_enemies()` (AI-only, legacy)
- Frontend: `src/components/DataBrowser/EnemyBrowser.vue`
