# VIP JSON Logging — In-Game Observation Checklist

Prereq for everything: VIP on, chat logs on, **non-combat logging checked**.
Game version/patch at time of run: _______________

## Log-line gaps (copy the `Chat-YY-MM-DD.log` slice covering each)

- [x] **Send a pigeon with an item attached**, then one without. Validates: is
  there a `JSON:Pigeon`-shaped line at all, and does it carry the item?
  - VERDICT 2026-11-17: send direction emits **nothing** (16:48–16:49 window has
    zero pigeon lines). The 16:50:03 tell shows the test pivoted to receive —
    see next box. Send = confirmed-absent.
- [x] **Receive a pigeon** (with and without item). Same question, other direction.
  - VERDICT 2026-11-17: **OBSERVED both** — `chat-pigeon.log` (friend renamed):
    empty pigeon 16:55:59, Mulberry Salad x10 16:55:59, Flatbread Chips x10
    17:06:21. `{"Title":"Pigeon From <friend>", "Body":"...<i>You received:\n<Item> xN</i>"}`.
    Receive = done; send needs no further work.
- [ ] **Open a popup, a book, and a report** (e.g. skill report, Help screen).
  `Book` is observed; popup/report event lines are not — check what (if anything)
  they emit.
  - NOTE 2026-11-17: no `JSON:Book` fired 16:48–16:49 in-log (nearest are the
    16:45 age/report re-opens and the 16:47:31 almanac); popup/report lines
    still unobserved. Keep open.
- [x] **Open the Gardening Almanac / Harvest Almanac book**. Unobserved
  `JSON:Book` shape — needed to re-enable auto farming-session + harvest-table
  extraction (manual farming sessions were never disabled and still work).
  - VERDICT 2026-11-17: **OBSERVED** — `{"Title":"Gardening Almanac"}` at
    16:47:31, appended to `chat-book.log`. Current/upcoming season blocks with
    `<indent=20>` markup. Unblocks harvest-table extraction design.
- [x] **Use a recipe** (any craft). `RecipeUsage` observed, but confirm
  `Completions` keeps climbing across two uses (absolute-total proof).
  - VERDICT 2026-11-17: **PROVEN absolute-total** — Distill Rare Magical Item
    776 → 777 across the two 16:50:49/16:50:55 uses (log shows both steps).
- [x] **Solo-kill a mob, note its exact name + time, then loot it**. Captures
  YOUR FATALITY line shape + entity ID to confirm the receipt join key beyond
  the Olugax group case (his kill emitted no FATALITY — someone else landed it).
  - VERDICT 2026-11-17: **PAIRED FATALITY+receipt ×3** — City Slime #488966
    (FATALITY 17:04:42 → receipt 17:04:47), #488976 (17:06:36 → 17:06:39),
    Clawstrider Shark #19868 (17:12:46 → receipts 17:12:51). Slice frozen in
    `chat-kill-slice.log`. The receipt↔kill join key is confirmed, not inferred.
- [x] **Solo-kill 3+ of the same mob, loot only some**. Drop rates need
  kills-without-loot as the denominator — confirms FATALITY fires per kill.
  - VERDICT 2026-11-17: **BOTH SIDES CONFIRMED** — FATALITY fired for all 4
    slime kills; #488964 (17:05) left **no receipt** = the kill-without-loot
    denominator is real. #488970 (killed 17:08:21) has no receipt because the
    player died 1s later — a live player-death-veto case for the recorder.
- [x] **Earn XP to force a level-up** (combat + a trade skill). Confirms the
  level-up flag shape on `EarnedXP`.
  - VERDICT 2026-11-17: **PROVEN** — `EarnedXP {"Skill":"Dying", XP 5,
    OldLevel 29 → NewLevel 30}` at 17:08:21 with twin `reached level 30` line.
- [ ] **Earn prodigy-overflow XP** (cap a skill, keep earning). Unobserved shape.
- [ ] **Give gifts to one NPC up to the weekly cap**, then one more past it.
  Confirms `DeltaFavor` keeps firing and the capped-exclusion logic (no gift row
  past cap).
  - VERDICT 2026-11-17: **UNTESTABLE — confirmed-absent shape.** The game
    refuses gifts at cap, so no log line can exist. Capped/tasteless exclusion
    stays code-covered only (existing dispatch tests).
- [ ] **Give a tasteless gift** (hated item). Confirms tasteless-exclusion path.
- [x] **Place an arena bet start to finish** (confirm → success → payout or loss).
  Expectation: *no* `LocalPlayer:` lines at all — proves the bet pipeline is
  truly unrecoverable, not just quiet.
  - VERDICT 2026-11-17: **PROVEN DEAD.** Zero `LocalPlayer` lines in the full
    day log; payout arrived as plain `[Status] You received 190 Councils.`
    (16:57:22) with no wager/bet lines. Only NPC Chatter fight color.
- [ ] **Milk a cow / get milked by a player / collect milk**. Expect chat
  `Collect Milk` / `MilkedBy` lines (live path) and zero `Process*` interaction
  lines.

## Character-export gaps (run export at the exact moment, save the JSON)

- [ ] **Export while ON a hangout** (hangout active with an NPC). The only
  hangout signal seen so far is a quest string when *not* on one — this is the
  missing field.
- [x] **Export right after giving Statehelm weekly gifts** to 2+ NPCs. Confirms
  `GiftsThisWeek` keys/values per NPC (seen `Elune → 1`, `Sulafa → 5`; need the
  increment proof).
- [ ] **Export with zero gifts given that week**. Confirms absent = 0 vs.
  key-missing.

## Survey validation (chat-attribution path)

- [ ] **Consume a Basic survey map, then immediately export the log slice**.
  Confirms chat loot lands same-tick and attributes to the manual use with no
  `Player.log` feed.
