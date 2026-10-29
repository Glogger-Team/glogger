# Quest Tracking

Track active quests, completions, and repeatable quest cooldowns across characters.

## The Problem

Players juggle dozens of quests across zones, many with repeatable cooldowns (daily, weekly, 30-day). There's no way to see "what can I do right now?" across characters without manually checking each one in-game. Work orders have 30-day cooldowns. Statehelm quests have weekly resets. Players lose track.

## What Glogger Can Do

### Live Quest State from Player.log

`ProcessCompleteQuest`, `ProcessAddQuest`, and related quest events exist in the log format (confirmed in `docs/architecture/capture-analysis-results.md`) but are **not yet parsed** — no variants exist in the player event parser enum.

### Quest Data from CDN

CDN quest data is already loaded (`get_all_quests`, `resolve_quest` exist). Quests include `ReuseTime_*` fields for cooldown durations and renown reward values. `QuestInline` component already provides hover/click behavior.

### Quest State from Character Export

`character_snapshots.raw_json` already stores Character reports which contain `ActiveWorkOrders` and `CompletedWorkOrders` arrays. Work orders screen loads these today.

## Feature Areas

### 1. Quest Event Parsing

Add `ProcessCompleteQuest`, `ProcessAddQuest`, `ProcessUpdateQuest` to the player event parser. This is a prerequisite for everything below.

### 2. Repeatable Quest Cooldown Tracker

Track completions with timestamps, calculate cooldown expiry from CDN `ReuseTime_*` fields, show what's available now vs. on cooldown. Must support multiple characters.

### 3. Statehelm Quest Tracking

`StatehelmView` already has full gift tracking. Extend with quest completion tracking to show which repeatable Statehelm quests are available, plus track renown earned vs. possible (from CDN reward data). Depends on quest event parsing above.

### 4. Work Order Cooldown Tracking

Work orders have 30-day cooldowns. Currently no `ProcessCompleteWorkOrder` parsing exists and no database table tracks completion timestamps. Character export provides "available"/"unavailable" state but not historical completion data.

### 5. Active Quest Browser

Show active quests grouped by location with search. Less urgent than cooldown tracking (players can see active quests in-game) but valuable for cross-character views and quest planning.

## Technical Foundation

- **New parser events:** Quest accept/complete/update variants in `player_event_parser.rs`
- **New DB table:** `game_state_quest_completions` (character, server, quest_key, completed_at)
- **CDN data:** Already available — `ReuseTime_*` fields, renown rewards, quest requirements
- **Frontend:** `QuestInline` and `resolve_quest` already exist. Statehelm view is a natural home for the first iteration.

## Relationship to Existing TODO Items

This plan consolidates several TODO entries:
- "Statehelm repeatable quest tracking" (Larger Efforts)
- "Repeatable quest cooldown tracking (general)" (Larger Efforts)
- "Repeatable quests / work orders tracking" (Larger Efforts)
- "Work order cooldown tracking" (Larger Efforts)

## Open Questions

- What exact quest events does Player.log emit? Need to grep samples or capture new data.
- Do work order completions emit their own event type or just a generic quest complete?
- How do we handle quests completed before glogger was watching? Character export may help backfill.
