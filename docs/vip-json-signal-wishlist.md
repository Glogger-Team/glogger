# Wanted Signal Lines

Signals glogger could build on but has no live writer or fixture for. Entries
marked HAVE exist in logs or code but aren't wired to a feature yet. Remove
entries as fixtures land and recorders ship.

## Kill line (enemy killed) — HAVE the signal, HAVE the pairing, MISSING the recorder

- **Signal:** Combat-channel FATALITY lines, parsed by `try_enemy_kill`
  (`chat_combat_parser.rs`) with entity ID + killing ability. Fires for YOUR fatal blow.
- **Join key PROVEN paired (2026-11-17, was inference):** FATALITY → receipt
  entity match ×3 — City Slime #488966 (17:04:42 → 17:04:47), #488976
  (17:06:36 → 17:06:39), Clawstrider Shark #19868 (17:12:46 → 17:12:51).
  Slice frozen in `test_data/vip_json/chat-kill-slice.log` (kill #488966 with
  ParagonXP twins, anatomy XP, bury-corpse, Status twin, receipt).
- **Denominator PROVEN:** #488964 FATALITY with no receipt (kill without loot).
- **Veto case LIVE:** #488970 killed 17:08:21, player died 17:08:22 (Ratkin
  Mugger FATALITY on player), no receipt — the player-death-veto scenario.
- **Missing:** a recorder writing FATALITY events to `enemy_kills`, and the
  receipt↔kill join on entity ID + timestamp. That lights up
  `get_enemy_kill_stats` / `get_item_drop_sources` "mine" scope.
- **Denominator caveat:** unlooted kills need the player-death veto (signal
  persisted: `character_deaths` via `persist_death_event`, and Dying hit 30
  in this same run) + solo-only rule (group kills are invisible to us). Ship
  numerator first.

## Player death — HAVE the signal, NEED the veto wiring

- **Signal:** `PlayerDeath` parsed + persisted (`persist_death_event`,
  `character_deaths`, `deaths` domain, `character-death` event live + scan).
- **Missing:** consume it in kill inference — suppress unlooted-kill
  candidates for entities with combat activity near a recorded death.

## `JSON:Pigeon` (receive direction) — HAVE the signal, MISSING the typed arm

- **Observed 2026-11-17** (friend renamed, `test_data/vip_json/chat-pigeon.log`):
  empty pigeon + 2 with-item (`Mulberry Salad x10`, `Flatbread Chips x10`).
  Shape: `{"Title":"Pigeon From <friend>", "Body":"...<i>You received:\n<Item> xN</i>"}`.
  Currently parses as `Unknown`; a typed arm unlocks pigeon tracking.
- **Send direction confirmed-absent:** the 16:48–16:49 send window has zero
  pigeon lines. No send fixture will ever land — receive only.

## Popup / report event lines

- **Unlocks:** popup/report ingestion (Book is observed; these are not).
- **Capture:** open a popup, a book, a report; save the slice.

## Gardening Almanac `JSON:Book` — OBSERVED 2026-11-17, fixture frozen

- **Observed:** `{"Title":"Gardening Almanac"}` 16:47:31, 4th line of
  `test_data/vip_json/chat-book.log`. Current/upcoming season blocks with
  `<indent=20>` markup.
- **Unblocks:** harvest-table extraction design (same pattern as the gourmand
  trigger). Entry kept until the extractor ships.

## Hangout-active character export

- **Unlocks:** hangout persistence (step 4).
- **Capture:** export character *while on* a hangout, save the JSON.

## `JSON:ParagonXP` — HAVE the signal (Unknown arm), MISSING the typed arm

- **Observed 2026-11-17:** fires per kill per skill, same twin shape as
  `EarnedXP` (`{"Skill":"Werewolf", "XP": 279200, "OldLevel": 6, "NewLevel": 6}`,
  twin `You earned N Prodigy XP in <Skill>.`). Present in `chat-kill-slice.log`.
  This is per-kill prodigy flow, NOT cap-overflow — overflow is still open below.
- **Unlocks:** typed prodigy accounting (currently `Unknown`, counted but unwritten).

## `JSON:EntityReport` — HAVE one line, MISSING the pattern

- **Observed 2026-11-17:** `{"Title":"Eveline Rastin", "Body":"Here's a hot tip:…'}` 16:55:02
  (arena tip). Single line; no twin. Currently `Unknown`.
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
