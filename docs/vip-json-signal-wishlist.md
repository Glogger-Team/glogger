# Wanted Signal Lines

Signals glogger could build on but has no live writer or fixture for. Entries
marked HAVE exist in logs or code but aren't wired to a feature yet. Remove
entries as fixtures land and recorders ship.

## Kill line (enemy killed) — HAVE the signal, MISSING the recorder

- **Signal:** Combat-channel FATALITY lines, parsed by `try_enemy_kill`
  (`chat_combat_parser.rs`) with entity ID + killing ability. Fires only for
  YOUR fatal blow — group kills where another lands it emit nothing for you
  (Olugax 2026-11-17: combat entity 257400, receipts `FromEntityId` 257400,
  no FATALITY anywhere).
- **Join key verified:** receipt `FromEntityId` == combat entity ID (Olugax
  257400, exact match).
- **Missing:** a recorder writing FATALITY events to `enemy_kills`, and the
  receipt↔kill join on entity ID + timestamp. That lights up
  `get_enemy_kill_stats` / `get_item_drop_sources` "mine" scope.
- **Denominator caveat:** unlooted kills need the player-death veto (signal
  already persisted: `character_deaths` via `persist_death_event`) + solo-only
  rule (group kills are invisible to us). Ship numerator first.
- **Capture:** kill anything, note exact mob name + time, save surrounding
  Chat.log *and* combat-log lines.

## Player death — HAVE the signal, NEED the veto wiring

- **Signal:** `PlayerDeath` parsed + persisted (`persist_death_event`,
  `character_deaths`, `deaths` domain, `character-death` event live + scan).
- **Missing:** consume it in kill inference — suppress unlooted-kill
  candidates for entities with combat activity near a recorded death.

## `JSON:Pigeon` (either direction, with/without item)

- **Unlocks:** pigeon tracking.
- **Capture:** send and receive a pigeon, save the slice.

## Popup / report event lines

- **Unlocks:** popup/report ingestion (Book is observed; these are not).
- **Capture:** open a popup, a book, a report; save the slice.

## Gardening / Harvest Almanac `JSON:Book`

- **Unlocks:** auto farming sessions + harvest-table extraction (same pattern
  as the gourmand trigger).
- **Capture:** open the almanac books, save the slice.

## Hangout-active character export

- **Unlocks:** hangout persistence (step 4).
- **Capture:** export character *while on* a hangout, save the JSON.

## Prodigy-overflow `EarnedXP`

- **Unlocks:** correct prodigy accounting at cap.
- **Capture:** earn XP on a capped skill, save the slice.

## Level-up `EarnedXP` flag

- **Unlocks:** confirm the level-up shape (partially seen; needs the flag).
- **Capture:** force a level-up, save the slice.

## Confirmed-absent (validate the dead, do not expect lines)

- Arena bet `LocalPlayer:` lines (place a bet start to finish).
- Cow interaction `Process*` lines (interact with a cow).
- Word-of-power discovery lines.
