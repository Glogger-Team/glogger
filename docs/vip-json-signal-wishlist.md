# Wanted Signal Lines

Signals glogger could build on but has no live writer or fixture for. Entries
marked HAVE exist in logs or code but aren't wired to a feature yet. Remove
entries as fixtures land and recorders ship.

## Kill line (enemy killed) — SHIPPED 2026-11-17, veto display follow-up open

- **Recorder:** `record_enemy_kill` writes every FATALITY to `enemy_kills`
  (rescan-safe on character/server/entity/killed_at), hooked in the shared
  dispatch arm — live + scan + replay. `enemy-killed` event still emitted.
- **Loot link:** `link_kill_loot` runs in `insert_json_receipt_guarded` (both
  twin orders converge there): latest kill for the receipt entity within
  30 min → `enemy_kill_loot` row, rescan-safe. Unlooted kills link nothing
  (no inference). This lights up `get_enemy_kill_stats` /
  `get_item_drop_sources` "mine" scope.
- **Join key proof:** FATALITY → receipt entity match ×3 — City Slime #488966
  (17:04:42 → 17:04:47), #488976 (17:06:36 → 17:06:39), Clawstrider Shark
  #19868 (17:12:46 → 17:12:51). Denominator: #488964 FATALITY, no receipt.
  Veto case: #488970 killed 17:08:21, player died 17:08:22, no receipt.
- **Open:** player-death veto in kill *inference* (suppress unlooted-kill
  candidates near a recorded death) — display-side, not recording-side.

## Player death — HAVE the signal, NEED the veto wiring

- **Signal:** `PlayerDeath` parsed + persisted (`persist_death_event`,
  `character_deaths`, `deaths` domain, `character-death` event live + scan).
- **Missing:** consume it in kill inference — suppress unlooted-kill
  candidates for entities with combat activity near a recorded death.

## `JSON:Pigeon` (receive direction) — SHIPPED 2026-11-17

- **Observed:** empty pigeon + 2 with-item, `chat-pigeon.log` (friend renamed).
  Dispatch writes `player_messages` (message_type='pigeon', sender + item
  parsed), rescan-safe, domain `player_messages` (MessagesWidget refreshes).
- **Send direction confirmed-absent:** the 16:48–16:49 send window has zero
  pigeon lines. No send fixture will ever land — receive only.

## Popup / report event lines

- **Unlocks:** popup/report ingestion (Book is observed; these are not).
- **Capture:** open a popup, a book, a report; save the slice.

## Gardening Almanac `JSON:Book` — SHIPPED 2026-11-17

- **Observed:** `{"Title":"Gardening Almanac"}` 16:47:31, 4th line of
  `test_data/vip_json/chat-book.log`. Current/upcoming season blocks with
  `<indent=20>` markup.
- **Shipped:** `parse_garden_almanac_body` + `import_garden_almanac` fill
  `garden_almanac` (snapshot replace) + `garden_almanac_history`, via a
  coordinator trigger on the live book (rescan-safe no-op). Same
  parse/import + snapshot/history shape as gourmand; the trigger gate is
  title-only (the title classifier leaves it outside `SkillReport`).
  Harvest-NPC extraction still needs its own signal.

## Hangout-active character export

- **Unlocks:** hangout persistence (step 4).
- **Capture:** export character *while on* a hangout, save the JSON.

## `JSON:ParagonXP` — typed 2026-11-17, deliberately unwritten

- **Observed:** fires per kill per skill, same twin shape as `EarnedXP`
  (`{"Skill":"Werewolf", "XP": 279200, "OldLevel": 6, "NewLevel": 6}`, twin
  `You earned N Prodigy XP in <Skill>.`). Present in `chat-kill-slice.log`.
  This is per-kill prodigy flow, NOT cap-overflow (still open below).
- **Decision:** no DB home — the Status twin owns the live tally and writing
  it to skill XP would corrupt. Typed + counted-known. Revisit if a prodigy
  ledger is designed.

## `JSON:EntityReport` — SHIPPED 2026-11-17 (single line, pattern TBD)

- **Observed:** `{"Title":"Eveline Rastin", "Body":"Here's a hot tip:…'}` 16:55:02
  (arena tip). Dispatch writes `player_messages`
  (message_type='entity_report'), rescan-safe — read path exists.
- **Capture:** more `EntityReport` titles/bodies to learn whether this is
  arena-tips-only or a general entity channel.

## Prodigy-overflow `EarnedXP`

- **Unlocks:** correct prodigy accounting at cap.
- **Capture:** earn XP on a capped skill, save the slice. (`ParagonXP` above is
  adjacent but distinct — per-kill flow, not overflow.)

## Confirmed-absent (validate the dead, do not expect lines)

- Arena bet `LocalPlayer:` lines — PROVEN DEAD 2026-11-17 (Vizlark bet start to
  payout, zero `LocalPlayer` lines file-wide; payout via plain Status).
- Pigeon-send lines — PROVEN ABSENT 2026-11-17 (send window has zero lines).
- Cow interaction `Process*` lines (interact with a cow).
- Word-of-power discovery lines.
