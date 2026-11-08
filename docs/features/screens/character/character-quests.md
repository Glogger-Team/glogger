# Character Quests Screen

## Overview

The Quests screen is a snapshot-driven quest reference that cross-references CDN quest data against the player's latest character report import. It answers "What quests do I have active?", "Which work orders are done or still todo?", and "What requirements or rewards does this quest have?"

Character state comes from the selected snapshot; quest details come from the CDN quest list. Requirement checks and work-order skill scoping read live game state (`gameStateStore`), so those results can change without a new snapshot while tab membership stays pinned to the snapshot.

## Layout

```text
┌─────────────────────────────────────────────────────────────────┐
│  CharacterName · ServerName                                      │
│  [Active (12)] [Work Orders (8)] [Uncompleted WOs (214)]        │
│  [Uncompleted (3,102)] [Completed (45)]                          │
├──────────────────────────────────┬──────────────────────────────┤
│  QUEST LIST (w-80)               │  QUEST DETAIL PANEL          │
│  Filter: [________]              │                              │
│  [By Area ▾] [Name ▾]  142 shown │  Kill Aberrant Spiders       │
│                                  │  [Eligible]                  │
│  ▼ Serbule (28)                  │  KillAberrantSpiders · Lv 30 │
│    Kill Rats     Lv 5   [Active]  │  Eltibule                    │
│    Bring Bones   Lv 8   [Done]    │                              │
│    Find Herbs    Lv 10  [Repeat.] │  "These spiders have been..."│
│    ...                           │                              │
│                                  │  ── Quest Info ──            │
│  ▼ Eltibule (15)                 │  Favor NPC: Joeh             │
│    Kill Spiders  Lv 30  [Active]  │  Cancellable: Yes            │
│    ...                           │  Reuse Time: 1h              │
└──────────────────────────────────┴──────────────────────────────┘
```

- Header shows the active character and server, plus a tab bar with per-tab counts.
- Empty state when no snapshot is selected: import a character report to see quests.
- Loading state while the CDN quest list is still empty for a selected snapshot.
- Four tabs share a two-panel layout (quest list + detail). The Uncompleted WOs tab replaces it with its own backlog layout.
- Hint banner on the Completed and Uncompleted tabs when `completedKeys` is empty (older game clients omit the completed-quest arrays — re-export in-game via Export Character as JSON in Settings → VIP; a newer export whose character genuinely completed nothing shows the same banner and needs no re-export).

## Data Sources

| Source | When Available | What It Provides |
|--------|---------------|-------------------|
| `characterStore.selectedSnapshot` + `activeQuests` (`get_snapshot_active_quests`) | After a character report import and snapshot selection | Quest keys grouped by snapshot category (see below) |
| `gameDataStore.allQuestsCache` (CDN, via `loadAllQuests` once status is `ready`) | After CDN load | Full quest details plus the pool the Uncompleted tabs are derived from |
| `gameStateStore.skillsByName` (live/report) | After login/full skill list or Character report import | Player skill levels for requirement checking and work-order skill scoping |
| `gameStateStore.favorByNpc` (live/report) | After login/favor events or Character report import | NPC favor tiers for requirement checking |
| `gameDataStore.getQuestSources()` (CDN) | On quest selection | Items that bestow the selected quest |

**Requirement evaluation** (detail panel only):

- `MinSkillLevel` — compares player skill level (including bonus levels) against the required level.
- `MinFavorLevel` — compares NPC favor tier via `isTierAtOrAbove()` from `useFavorTiers`.
- `ActiveCombatSkill` — checks whether the player has the skill at any level.
- All other requirement types (`QuestCompleted`, `Race`, `TimeOfDay`, etc.) — shown as unknown; the data to evaluate them is unavailable.

## Snapshot Categories

Per-snapshot rows live in `character_active_quests` (`snapshot_id`, `quest_key`, `category`), written at import from the `/outputcharacter` arrays (`ActiveQuests`, `ActiveWorkOrders`, `CompletedWorkOrders`, `CompletedQuests`). The TypeScript mirror is `SnapshotActiveQuest` (`src/types/database.ts`):

| Category | Source array | Meaning |
|----------|-------------|---------|
| `active` | `ActiveQuests` | Quest is currently active |
| `work_order` | `ActiveWorkOrders` | Work order currently accepted |
| `completed_work_order` | `CompletedWorkOrders` | Work order turned in before |
| `completed` | `CompletedQuests` | Non-work-order quest completed; only newer VIP exports include it |

A repeatable work order can appear in both the active and completed export arrays. `QuestsScreen` dedupes by key and prefers the `work_order` state so the order shows as available rather than done.

## Tabs

| Tab | Key | Contents |
|-----|-----|----------|
| Active | `active` | Snapshot rows with category `active`, resolved to CDN details |
| Work Orders | `work_orders` | Snapshot work-order rows (`work_order` + `completed_work_order`, deduped); per-row badge keeps its own category (`Work Order` vs `Done`) |
| Uncompleted WOs | `work_orders_todo` | Computed backlog rendered by `WorkOrdersTodoPanel` (no quest list/detail panels) |
| Uncompleted | `uncompleted` | Every CDN non-work-order quest that is not active, minus permanently completed quests (see `isRepeatable`) |
| Completed | `completed` | Snapshot rows with category `completed`, resolved to CDN details |

Per-tab category map (`currentTabQuests` / `currentTabCategories`):

- Active maps each key to `active`; Work Orders maps each key to its snapshot category; Completed maps each key to `completed`.
- Uncompleted maps each quest to `repeatable` when `isRepeatable` is true, else `available`.
- Switching tabs clears the selected quest when the selection is absent from the new tab.

### `isRepeatable`

```text
isRepeatable(q) = !!(q.raw?.ReuseTime_Minutes || q.raw?.ReuseTime_Days)
```

Any quest with a `ReuseTime_Minutes` or `ReuseTime_Days` CDN field counts as repeatable. It drives two behaviors: completed repeatable quests reappear in the Uncompleted tab (permanently completed non-repeatables are hidden), and the Uncompleted tab badges them `Repeatable` instead of `Available`. `NpcQuestsSection` uses the same fields (explicit `> 0` checks) for its repeat marker.

## Components

### QuestsScreen (`src/components/Character/QuestsScreen.vue`)

Top-level screen. Gates on `hasSnapshot` (selected snapshot) and the CDN quest cache, builds the five tab lists above, and renders either the `WorkOrdersTodoPanel` (Uncompleted WOs tab) or the quest list + detail panels (all other tabs).

### QuestListPanel (`src/components/Character/QuestListPanel.vue`)

Left sidebar (w-80) showing the current tab's quests with a category badge per row.

Grouping options (default By Area):

- **By Type** — snapshot/derived category in `CATEGORY_ORDER`: Active, Work Order, Done (`completed_work_order`), Completed, Available, Repeatable.
- **By Area** — groups by `DisplayedLocation`.
- **By NPC** — groups by the quest's `FavorNpc`.
- **By Level** — 10-level buckets (1-10, 11-20, ..., 71-80, 80+).
- **By Keyword** — groups by first keyword.
- **No Grouping** — flat list.

Sorting options (default Name):

- **Name** (alphabetical).
- **Level** (ascending, nulls last).

Text filter matches quest name, area, favor NPC, first keyword, internal name, or category label. Each row shows the display name, a level badge when the quest has a level, the area (small, truncated), and the category badge. Groups are collapsible with sticky headers showing group name and count. `skillsByName` / `favorByNpc` are accepted for requirement context; row badges are category-driven, and eligibility evaluation lives in the detail panel.

### WorkOrdersTodoPanel (`src/components/Character/WorkOrdersTodoPanel.vue`)

Backlog layout for the Uncompleted WOs tab. Props: `orders: WorkOrderTodo[]` with `quest_key`, `name`, `craft_skill`, `item_internal_name`, `quantity`, `industry_xp`, `gold_reward`, `is_active`, `board_npc`, and `board_location`.

Backlog construction (`workOrderTodos` in `QuestsScreen`):

- Every CDN quest with a `WorkOrderSkill` outside `Unknown` (Fitz-the-Boatman gathering turn-ins are excluded) for a crafting skill the player has at level 1+.
- Minus quests already in the `completed_work_order` set.
- `is_active` marks keys also present in the `work_order` set.
- Industry XP comes from `SkillXp` / `Industry` rewards; gold from `WorkOrderCurrency` / `Gold` rewards.
- Turn-in board (1 of 9 fixed boards) is parsed per quest: the NPC from the Scripted "Deliver to X" objective and the location from the flavor Description ("…to X in/on/at \<Location\>").

Panel features:

- Text filter (item, name, board), board dropdown, and per-skill filter pills.
- Sort: active first, then by name.
- Checkbox multi-select with select-all/clear, plus `Create Project` (disabled with nothing selected) which calls `craftingStore.createProjectFromWorkOrders` and reports added/skipped counts.
- Rows show an ACTIVE marker when active, the item (rich `ItemInline` with icon + tooltip when 250 or fewer rows are shown, otherwise a plain label to keep the ~1,000-row backlog snappy) with quantity, board location, craft skill, Industry XP, and gold.
- Totals footer with shown count, summed Industry XP, summed gold, and active count.

### QuestDetailPanel (`src/components/Character/QuestDetailPanel.vue`)

Right panel composing the header and three section sub-components. Sections displayed:

- **Header** — Quest name, eligibility badge (shown when the quest has requirements), internal name, level, area (via `AreaInline`).
- **Description** — Italic quest description text.
- **Quest Giver Dialog** — Preface text in a styled quote block.
- **Quest Info** — Grid with favor NPC (via `NpcInline`), cancellable, reuse time, work order skill (via `SkillInline`).
- **Requirements** — `QuestRequirementsSection`.
- **Objectives** — `QuestObjectivesSection`.
- **Rewards** — `QuestRewardsSection`.
- **Completion Dialog** — Success text in a styled quote block.
- **Sources** — `SourcesPanel` showing items that bestow the quest.
- **Keywords** — Tag pills.

### QuestRequirementsSection (`src/components/Character/QuestDetailSections/QuestRequirementsSection.vue`)

Shows each requirement with a met/unmet/unknown status icon:

- **Met** (green checkmark) — Player meets this requirement.
- **Unmet** (red cross) — Player does not meet this requirement, with specifics (e.g., "Sword 32/50").
- **Unknown** (gray ?) — Cannot evaluate (quest completion, race, time-of-day, etc.).

Uses inline components: `SkillInline` for skill requirements, `NpcInline` for favor requirements, `QuestInline` for quest prerequisites.

### QuestObjectivesSection (`src/components/Character/QuestDetailSections/QuestObjectivesSection.vue`)

Lists quest objectives with type label, description, and count. Uses `ItemInline` for objectives that reference items.

### QuestRewardsSection (`src/components/Character/QuestDetailSections/QuestRewardsSection.vue`)

Shows all quest rewards with inline components:

- Favor rewards with `NpcInline` for the favor NPC.
- Skill XP rewards with `SkillInline`.
- Item rewards with `ItemInline`.
- Currency amounts and loot profile references.

## Shared Utilities

### Quest Display Helpers (`src/utils/questDisplay.ts`)

Pure functions for quest data formatting, shared between the Character Quests screen and the Data Browser Quest Browser:

- `getQuestDisplayName()`, `getQuestLevel()`, `getQuestArea()` — basic field extraction.
- `getObjectiveTypeDisplay()` — maps objective type codes to readable labels.
- `getRewardTypeDisplay()` — formats reward entries as readable strings.
- `getRequirementDisplay()` — plain-text requirement description.
- `formatReuseTime()` — formats reuse time from minutes/days.
- `extractNpcKeyFromFavorPath()` — parses `"AreaName/NPC_Foo"` to CDN key `"NPC_Foo"`.
- `extractNpcDisplayFromFavorPath()` — parses to display name `"Foo"`.

### Quest Requirements Composable (`src/composables/useQuestRequirements.ts`)

Core personalization logic for evaluating quest requirements against player data:

- `evaluateRequirement(req, skillsByName, favorByNpc)` — evaluates a single requirement.
- `evaluateQuestEligibility(quest, skillsByName, favorByNpc)` — aggregates all requirements into an overall eligibility status.
- `eligibilityLabel()`, `eligibilityClasses()` — display helpers for eligibility badges.
- `eligibilitySort()` — sort comparator for eligibility ordering.
- `requirementStatusIcon()`, `requirementStatusColor()` — per-requirement status display.

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

- **Quest objective progress** — Requires per-objective progress tracking, which snapshots don't record.
- **Quest chains** — While prerequisite quests are listed, we can't show a visual chain without quest completion tracking.
- **Exact favor requirements** — `MinFavorLevel` uses tier names ("Friends", "CloseFriends") but doesn't specify exact favor points within a tier.
- **Many requirement types** — Race, TimeOfDay, InteractionFlags, HangOutCompleted, and dozens of other requirement types can't be evaluated against current player data.
- **Old exports lack completed quests** — `CompletedQuests` is only recorded by newer game clients; older exports show an empty Completed tab and an over-full Uncompleted tab until the character is re-exported.

These will become available as the player event parser is extended and new game state domains are implemented.
