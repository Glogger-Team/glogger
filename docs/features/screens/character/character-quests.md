# Character Quests Screen

## Overview

The Quests screen provides a personalized quest reference that cross-references CDN quest data against the player's current game state. It answers questions like "What quests can I do right now?", "What skills do I need to level to unlock this quest?", and "What rewards does this quest give?" Since we don't yet track active/completed quest state from the game log, the screen focuses on requirement eligibility — showing which quest prerequisites the player meets or doesn't meet based on their skill levels and NPC favor tiers.

## Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  CharacterName · ServerName         Quests: 3,412  Eligible: 847│
├──────────────────────────────────┬──────────────────────────────┤
│  Filter: [________]              │                              │
│  [By Area ▾] [Level ▾]          │  QUEST DETAIL PANEL          │
│  ☐ Eligible only                 │                              │
│  ☐ Work orders                   │  Kill Aberrant Spiders       │
│                       142 quests │  [Eligible]                  │
│                                  │  KillAberrantSpiders · Lv 30 │
│  ▼ Serbule (28)                  │  Eltibule                    │
│    Kill Rats     Lv 5   Eligible │                              │
│    Bring Bones   Lv 8   Partial  │  "These spiders have been..."│
│    Find Herbs    Lv 10  Not Met  │                              │
│    ...                           │  ── Quest Info ──            │
│                                  │  Favor NPC: Joeh             │
│  ▼ Eltibule (15)                 │  Cancellable: Yes            │
│    Kill Spiders  Lv 30  Eligible │  Reuse Time: 1h              │
│    ...                           │                              │
│                                  │  ── Requirements ──          │
│  ▼ Sun Vale (9)                  │  ✔ Sword 45/30               │
│    ...                           │  ✖ Joeh: Comfortable (need   │
│                                  │     Close Friends)           │
│  ▼ Unknown Area (34)             │                              │
│    ...                           │  ── Objectives ──            │
│                                  │  Kill   Kill 10 spiders (10) │
│                                  │                              │
│                                  │  ── Rewards ──               │
│                                  │  +50 Favor  Joeh             │
│                                  │  ✦ Sword: 200 XP             │
│                                  │  ✦ Spider Fang × 3           │
│                                  │                              │
│                                  │  ── Keywords ──              │
│                                  │  [COMBAT]  [REPEATABLE]      │
└──────────────────────────────────┴──────────────────────────────┘
```

## Data Sources

The screen works purely from CDN quest data enriched with player state:

| Source | When Available | What It Provides |
|--------|---------------|-------------------|
| `gameDataStore.getAllQuests()` (CDN) | After CDN load | All quests with objectives, rewards, requirements, keywords |
| `gameStateStore.skillsByName` (live/report) | After login/full skill list or Character report import | Player skill levels for requirement checking |
| `gameStateStore.favorByNpc` (live/report) | After login/favor events or Character report import (seeds `game_state_favor` with `source='snapshot'`) | NPC favor tiers for requirement checking |
| `gameDataStore.getQuestSources()` (CDN) | On quest selection | Items that bestow the selected quest |

**Requirement evaluation:**
- `MinSkillLevel` — Compares player's skill level (including bonus levels) against the required level
- `MinFavorLevel` — Compares player's NPC favor tier using `isTierAtOrAbove()` from `useFavorTiers`
- `ActiveCombatSkill` — Checks if the player has the skill at any level
- All other requirement types (`QuestCompleted`, `Race`, `TimeOfDay`, etc.) — shown as "unknown" since we lack the data to evaluate them

## Components

### QuestsScreen (`src/components/Character/QuestsScreen.vue`)

Top-level screen component. Loads all quests on mount, manages selected quest state, and provides the summary bar + two-panel layout. Summary shows total quest count and eligible quest count (when player data is available).

### QuestListPanel (`src/components/Character/QuestListPanel.vue`)

Left sidebar (w-80) with the full quest list.

**Grouping options:**
- **By Area** (default) — groups by `DisplayedLocation`
- **By Eligibility** — groups into Eligible / Partial / Unknown / Not Met
- **By NPC** — groups by the quest's `FavorNpc`
- **By Level** — groups into 10-level buckets (1-10, 11-20, ..., 71-80, 80+)
- **By Keyword** — groups by first keyword
- **None** — flat list

**Sorting options:**
- **Level** (default, ascending) — lowest level first, nulls last
- **Name** (alphabetical)
- **Eligibility** — eligible first, then partial, unknown, ineligible

**Filter toggles:**
- Text filter on quest name, area, NPC, keyword, or internal name
- Eligible only — shows only quests where all checkable requirements are met
- Work orders — shows only work order quests

**Each row shows:**
- Quest display name
- Level badge (if quest has a level)
- Area (small, truncated)
- Work order indicator dot (blue)
- Eligibility badge (colored: green/yellow/gray/red) — only shown for quests with requirements

Groups are collapsible with sticky headers showing group name and count.

### QuestDetailPanel (`src/components/Character/QuestDetailPanel.vue`)

Right panel composing the header and three section sub-components. Sections displayed:
- **Header** — Quest name, eligibility badge, internal name, level, area (via `AreaInline`)
- **Description** — Italic quest description text
- **Quest Giver Dialog** — Preface text in a styled quote block
- **Quest Info** — Grid with favor NPC (via `NpcInline`), cancellable, reuse time, work order skill (via `SkillInline`)
- **Requirements** — `QuestRequirementsSection`
- **Objectives** — `QuestObjectivesSection`
- **Rewards** — `QuestRewardsSection`
- **Completion Dialog** — Success text in a styled quote block
- **Sources** — `SourcesPanel` showing items that bestow the quest
- **Keywords** — Tag pills

### QuestRequirementsSection (`src/components/Character/QuestDetailSections/QuestRequirementsSection.vue`)

Shows each requirement with a met/unmet/unknown status icon:
- **Met** (green checkmark) — Player meets this requirement
- **Unmet** (red cross) — Player does not meet this requirement, with specifics (e.g., "Sword 32/50")
- **Unknown** (gray ?) — Cannot evaluate (quest completion, race, time-of-day, etc.)

Uses inline components: `SkillInline` for skill requirements, `NpcInline` for favor requirements, `QuestInline` for quest prerequisites.

### QuestObjectivesSection (`src/components/Character/QuestDetailSections/QuestObjectivesSection.vue`)

Lists quest objectives with type label, description, and count. Uses `ItemInline` for objectives that reference items.

### QuestRewardsSection (`src/components/Character/QuestDetailSections/QuestRewardsSection.vue`)

Shows all quest rewards with inline components:
- Favor rewards with `NpcInline` for the favor NPC
- Skill XP rewards with `SkillInline`
- Item rewards with `ItemInline`
- Currency amounts and loot profile references

## Shared Utilities

### Quest Display Helpers (`src/utils/questDisplay.ts`)

Pure functions for quest data formatting, shared between the Character Quests screen and the Data Browser Quest Browser:
- `getQuestDisplayName()`, `getQuestLevel()`, `getQuestArea()` — basic field extraction
- `getObjectiveTypeDisplay()` — maps objective type codes to readable labels
- `getRewardTypeDisplay()` — formats reward entries as readable strings
- `getRequirementDisplay()` — plain-text requirement description
- `formatReuseTime()` — formats reuse time from minutes/days
- `extractNpcKeyFromFavorPath()` — parses `"AreaName/NPC_Foo"` to CDN key `"NPC_Foo"`
- `extractNpcDisplayFromFavorPath()` — parses to display name `"Foo"`

### Quest Requirements Composable (`src/composables/useQuestRequirements.ts`)

Core personalization logic for evaluating quest requirements against player data:
- `evaluateRequirement(req, skillsByName, favorByNpc)` — evaluates a single requirement
- `evaluateQuestEligibility(quest, skillsByName, favorByNpc)` — aggregates all requirements into an overall eligibility status
- `eligibilityLabel()`, `eligibilityClasses()` — display helpers for eligibility badges
- `eligibilitySort()` — sort comparator for eligibility ordering
- `requirementStatusIcon()`, `requirementStatusColor()` — per-requirement status display

All functions are pure (no reactive state) — callers pass in store values, keeping the logic testable and reusable.

## Eligibility Logic

A quest's overall eligibility is computed from its individual requirements:

| Requirements | Result |
|-------------|--------|
| No requirements | `eligible` |
| All met | `eligible` |
| Some met, some unmet | `partial` |
| None met, some unmet | `ineligible` |
| All met + some unknown | `partial` |
| All unknown | `unknown` |

## Data We Cannot Show (Yet)

Some player questions can't be answered with currently available data:

- **Active/completed quests** — The player event parser doesn't yet handle `ProcessLoadQuests`, `ProcessAddQuest`, `ProcessUpdateQuest`, `ProcessCompleteQuest`. Once implemented, we could show which quests are active, completed, or available.
- **Quest objective progress** — Requires `ProcessUpdateQuest` parsing
- **Quest chains** — While prerequisite quests are listed, we can't show a visual chain without quest completion tracking
- **Exact favor requirements** — `MinFavorLevel` uses tier names ("Friends", "CloseFriends") but doesn't specify exact favor points within a tier
- **Many requirement types** — Race, TimeOfDay, InteractionFlags, HangOutCompleted, and dozens of other requirement types can't be evaluated against current player data

These will become available as the player event parser is extended and new game state domains are implemented.
