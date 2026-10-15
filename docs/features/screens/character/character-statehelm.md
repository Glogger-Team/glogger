# Statehelm Reputation Tracker

Tracks weekly gift progress for Statehelm NPCs. Each NPC accepts up to 5 gifts per week (weeks reset Monday 00:00 America/New_York — the game's own reset instant, 04:00/05:00 UTC). Shows NPC services (training, storage, vendor) and gift preferences at a glance.

## How It Works

Gift events are detected from `ProcessDeltaFavor` log lines where `is_gift` is `True`. These are already parsed by `PlayerEventParser` as `FavorChanged` events with the `is_gift` flag. When a gift is detected:

1. The favor delta is applied to `game_state_favor` (existing behavior)
2. A separate row is inserted into `game_state_gift_log` with the NPC key, timestamp, and favor delta
3. The frontend composable watches the favor activity feed for real-time updates

## NPC Selection

Statehelm NPCs are identified from CDN data by area:
- `area_name` is `AreaStatehelm` (the city) **or** `AreaStatehelmCaves` (the
  sewers — Hamilton, Smoke Tail, Fuzzlebun), and
- the NPC has at least one entry in `preferences` (they accept gifts)

The weekly cap is a Statehelm-region rule (the game's own note: "Statehelm
townsfolk are more sophisticated than villagers elsewhere…"), and it applies in
the sewers too. No CDN field marks the cap, so the area is the signal. Fully
data-driven — new Statehelm NPCs in future CDN updates appear automatically.

## Weekly Reset

The week boundary is Monday 00:00 America/New_York, computed by
`useStatehelmTracker.getCurrentWeekStart` (frontend) and
`game_state::current_week_start_utc` (backend, for the gift-log reconcile) —
**the two must stay identical**. Gift counts are derived by filtering
`game_state_gift_log` entries with `gifted_at >= weekStart`. No data is deleted
on reset — old gift logs are retained for history.

## Backfill from the game's logs

Gifts given while glogger was closed are recovered by re-reading the game's own
gift-cap notes from `Player.log`, `Player-prev.log` and the archive in
`<app data>/player-logs`, then reconciling each capped NPC's current-week row
count to the game's number. Runs at startup and from the **Rescan logs** button;
see [statehelm-gift-backfill.md](../../statehelm-gift-backfill.md) for the
invariants, the archive limits (Settings → General) and the dating rule.

## Manual Adjustment

Users can manually increment/decrement gift counts per NPC using +/- buttons on each card. Manual additions insert a row with `favor_delta = 0.0`; removals delete the most recent gift log entry for that NPC within the current week. A manual change is reconciled back to the game's own count the next time a cap note for that NPC is read (live, at startup, or via Rescan logs).

## NPC Card Info

Each card displays:
- **NPC name** (clickable via `NpcInline`)
- **Favor tier** badge (from `game_state_favor`)
- **Gift progress** — dot indicators and count (X / 5)
- **Training** — skills the NPC trains (via `SkillInline`)
- **Storage** — whether they offer storage and at which tiers space increases
- **Vendor** — buying caps per favor tier and item types
- **Gift preferences** — what the NPC likes/loves/hates with pref values

## Filters

- **Hide above tier** — dropdown to hide NPCs at or above a chosen favor tier (e.g., hide Soul Mates)
- **Trainers / Vendors / Storage** — checkbox filters (additive: show NPCs matching any checked service)
- **Hide maxed gifts** — hide NPCs that already received 5 gifts this week

## Data Sources

| Data | Source |
|------|--------|
| Gift events | `game_state_gift_log` (DB), populated from `FavorChanged` events where `is_gift = true` |
| NPC list & preferences | CDN via `gameDataStore.npcsByKey` |
| NPC services | CDN `services` array, parsed via `parseServices()` |
| Favor tiers | `game_state_favor` (DB) via `gameStateStore.favorByNpc` |

## Key Files

- Gift log persistence: `src-tauri/src/game_state.rs` (in `FavorChanged` handler)
- Database migration: `src-tauri/src/db/migrations.rs` (v17)
- Tauri commands: `src-tauri/src/db/game_state_commands.rs` (`get_gift_log`, `add_manual_gift`, `remove_last_gift`, shared `reconcile_gift_count`) and `src-tauri/src/db/statehelm_commands.rs` (`backfill_statehelm_gifts`)
- Frontend composable: `src/composables/useStatehelmTracker.ts`
- UI: `src/components/Character/StatehelmView.vue`
- Feature doc: [statehelm-gift-backfill.md](../../statehelm-gift-backfill.md)
