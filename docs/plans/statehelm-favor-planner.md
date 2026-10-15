# Statehelm Favor Planner

Cross-reference stored items with NPC gift preferences, generate pickup/delivery routes.

## What Already Exists

- **NPC preferences**: CDN data with `desire` (Love/Like/Hate), `keywords`, `pref` weight per NPC
- **Reverse indices**: `npc_favor_by_item_name` and `npc_favor_by_keyword` HashMaps in game_data
- **Existing command**: `get_npcs_wanting_item` already combines both lookup types
- **Gift tracking**: `useStatehelmTracker` composable tracks gifts-this-week per NPC (max 5/week, Monday 00:00 America/New_York reset)
- **Storage data**: `gameState.storage` + `storageVaultsByKey` with area keys for routing
- **Route planner**: [trip_router.rs](../../src-tauri/src/trip_router.rs) handles multi-stop route optimization
- **Statehelm UI**: [StatehelmView.vue](../../src/components/Character/StatehelmView.vue) with NPC cards and gift tracking

## Design

### Backend: `plan_statehelm_gifts` command
Takes Statehelm NPC keys needing gifts → iterates storage items → resolves item keywords → cross-references preferences → returns `GiftMatch` list (item + vault + NPC + desire + pref).

### Frontend: `useStatehelmPlanner` composable
- Greedy assignment: Love > Like, higher pref first
- Respect remaining gift counts per NPC
- Deduplicate items across NPCs (assign to highest-benefit NPC)
- Produce gift plan: items to pick up, from where, deliver to whom

### Route Generation
- Pickup stops per vault area → TurnIn stop at Statehelm
- Uses existing `plan_trip` command
- Travel config reused from trip planner widget

### UI: "Gift Planner" section in StatehelmView
- Gift match table grouped by vault location
- NPC exclusion checkboxes
- Route display (reusing TripPlannerWidget pattern)
- Dashboard widget link when gifts are plannable

## Dependencies

No hard dependency on quest tracking. Uses existing gift count tracking, storage data, and route planner.

## Phases

### Phase 1: Backend matching command
- `plan_statehelm_gifts` in [cdn_commands.rs](../../src-tauri/src/cdn_commands.rs)

### Phase 2: Frontend assignment algorithm
- `useStatehelmPlanner.ts` composable consuming `useStatehelmTracker` + `gameStateStore`

### Phase 3: Planner UI
- Gift match table in StatehelmView, NPC exclusion controls

### Phase 4: Route integration
- RouteStop generation, `plan_trip` call, route display

### Phase 5: Polish
- Dashboard widget link, empty states, edge cases (keyword restrictions, favor level gating)

## Open Questions

- Include inventory items (carried) in addition to vault items?
- Factor in item value to avoid gifting expensive items when cheaper alternatives exist?
- Support "max N vault stops" constraint for route complexity limits?
