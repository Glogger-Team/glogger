# VIP JSON Logging — In-Game Observation Checklist

Prereq for everything: VIP on, chat logs on, **non-combat logging checked**.
Game version/patch at time of run: _______________

## Log-line gaps (copy the `Chat-YY-MM-DD.log` slice covering each)

- [ ] **Send a pigeon with an item attached**, then one without. Validates: is
  there a `JSON:Pigeon`-shaped line at all, and does it carry the item?
- [ ] **Receive a pigeon** (with and without item). Same question, other direction.
- [ ] **Open a popup, a book, and a report** (e.g. skill report, Help screen).
  `Book` is observed; popup/report event lines are not — check what (if anything)
  they emit.
- [ ] **Open the Gardening Almanac / Harvest Almanac book**. Unobserved
  `JSON:Book` shape — needed to re-enable auto farming-session + harvest-table
  extraction (manual farming sessions were never disabled and still work).
- [ ] **Use a recipe** (any craft). `RecipeUsage` observed, but confirm
  `Completions` keeps climbing across two uses (absolute-total proof).
- [ ] **Solo-kill a mob, note its exact name + time, then loot it**. Captures
  YOUR FATALITY line shape + entity ID to confirm the receipt join key beyond
  the Olugax group case (his kill emitted no FATALITY — someone else landed it).
- [ ] **Solo-kill 3+ of the same mob, loot only some**. Drop rates need
  kills-without-loot as the denominator — confirms FATALITY fires per kill.
- [ ] **Earn XP to force a level-up** (combat + a trade skill). Confirms the
  level-up flag shape on `EarnedXP`.
- [ ] **Earn prodigy-overflow XP** (cap a skill, keep earning). Unobserved shape.
- [ ] **Give gifts to one NPC up to the weekly cap**, then one more past it.
  Confirms `DeltaFavor` keeps firing and the capped-exclusion logic (no gift row
  past cap).
- [ ] **Give a tasteless gift** (hated item). Confirms tasteless-exclusion path.
- [ ] **Place an arena bet start to finish** (confirm → success → payout or loss).
  Expectation: *no* `LocalPlayer:` lines at all — proves the bet pipeline is
  truly unrecoverable, not just quiet.
- [ ] **Milk a cow / get milked by a player / collect milk**. Expect chat
  `Collect Milk` / `MilkedBy` lines (live path) and zero `Process*` interaction
  lines.

## Character-export gaps (run export at the exact moment, save the JSON)

- [ ] **Export while ON a hangout** (hangout active with an NPC). The only
  hangout signal seen so far is a quest string when *not* on one — this is the
  missing field.
- [ ] **Export right after giving Statehelm weekly gifts** to 2+ NPCs. Confirms
  `GiftsThisWeek` keys/values per NPC (seen `Elune → 1`, `Sulafa → 5`; need the
  increment proof).
- [ ] **Export with zero gifts given that week**. Confirms absent = 0 vs.
  key-missing.

## Survey validation (chat-attribution path)

- [ ] **Consume a Basic survey map, then immediately export the log slice**.
  Confirms chat loot lands same-tick and attributes to the manual use with no
  `Player.log` feed.
