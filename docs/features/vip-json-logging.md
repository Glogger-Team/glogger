# VIP JSON Logging — observed contract (frozen 2026-11-17)

> Addendum 2026-11-18: Gardening Almanac extraction shipped (Book row,
> wanted signals). "Frozen" dates the observations, not the doc.
>
> Addendum 2026-11-27 (#421): blood-target, casino-edge tip, and warden-report EntityReport readers shipped — the `EntityReport` row ("One line observed — pattern TBD") and the `Book` row (no `warden_report` domain) below predate them.

Source: live `Chat-26-11-1*.log` on this box, game with VIP + chat logs + non-combat logging on.
Fixtures: `test_data/vip_json/chat-<type>.log` (names sanitized `TwinkleofToes` → `TestPlayer`),
`character-report-after.json` (trimmed: exact key shapes, 2 skills, 3 recipes, 3 NPCs).

Line shape: `YY-MM-DD HH:MM:SS\t[JSON:Type] {single-line JSON}` — no sender prefix, `\n` escaped inside strings.

## Observed types

| Type | Example JSON | Twin? | Consumer (existing → reuse) |
|---|---|---|---|
| `AddItem` | `{"Name": "Toxic Sludge", "TypeId": 1137, "Stack": 1, "Flags": [], "FromName": "Olugax The Ever-Pudding", "FromEntityId": 257400}` (`From*` absent when not loot) | Yes — English `[Status] <Name> [xN] added to inventory.` immediately before, same second | `insert_item_transaction_guarded` + `staged_gains` + survey hook, domain `inventory` |
| `EarnedXP` | `{"Skill":"Teleportation", "XP": 53, "OldLevel": 46, "NewLevel": 46}` | Yes — English `[Status] You earned N XP in <Skill>.` same second | `XP` is an **awarded increment, not a total** (proven: Riding 250 → 350 → 300 across live lines — totals cannot fall). Both writers (`upsert_chat_xp`, `apply_chat_level_up` when `NewLevel > OldLevel`) delegate to `apply_chat_xp_award`: same-level awards **accumulate**; a level-up stores just the award and clears the snapshot anchor. Identity is the v101 `chat_xp_awards` ledger (character, server, skill, timestamp, xp, level) — replays and twins skip, while same-second and out-of-order awards with a distinct identity apply (same-timestamp same-xp same-level pairs still collapse: second-granularity limit). Same-level awards at or before `xp_snapshot_at` (space-normalized export stamp) drop as already inside the seeded absolute. Domain `skills` |
| `Book` | `{"Title":"Skill Info", "Body":"Foods Consumed:\n\n ..."}` | No — sole record | `persist_book_content` upsert on `game_state_books`; gourmand/hoplology/teleport/report-stats/gardening triggers run on the body. Gardening: `parse_garden_almanac_body` + `import_garden_almanac` fill `garden_almanac` (snapshot-only; the `garden_almanac_history` log was dropped in v102), domain `garden_almanac` (matches the dormant `GardenAlmanacWidget` listener); rescan-safe no-op |
| `DeltaFavor` | `{"Npc":"NPC_Elune", "Delta": 18.06948}` | Display-only English (`You gained 18.1 favor` — rounded, unparsed); JSON exact value is authoritative | One row per line in `favor_events` (`INSERT OR IGNORE` — rescans collapse on the key, no marker table) + tier/name touch-up on `game_state_favor` + conditional `game_state_gift_log` INSERT (giftable tastes + positive delta only — favor losses are never gifts; Statehelm capped NPCs accrue events but never gift rows: their weekly counts are manual-only via the Statehelm +/- UI, no report field feeds the gift log). "Session favor" is `SUM(delta)` since login, computed at read time — covers capped and uncapped alike. Two genuinely distinct same-second same-delta lines insert separately (per-line `source_line_id` since #358); rescans collapse onto the line's own row |
| `RecipeUsage` | `{"RecipeName":"Recall Heart's Home", "RecipeId": 9008, "Completions": 211}` | No English twin (only the sibling XP line) | Upsert `game_state_recipes` `completion_count`, domain `recipes`. `Completions` is an **absolute total** (matches snapshot `RecipeCompletions` shape) — set newer-wins, never increment. Proven: Distill 776 → 777 across two live uses, 2026-11-17 |
| `Pigeon` | `{"Title":"Pigeon From <friend>", "Body":"...<i>You received:\n<Item> xN</i>"}` (receive only — send emits nothing) | No twin | `player_messages` (message_type='pigeon', direction='in', sender + item parsed), domain `player_messages`. Rescan-safe. Fixture: `chat-pigeon.log` |
| `ParagonXP` | `{"Skill":"Werewolf", "XP": 279200, "OldLevel": 6, "NewLevel": 6}` | Yes — English `You earned N Prodigy XP in <Skill>.` same second | Typed, deliberately unwritten: per-kill prodigy flow has no DB home (the Status twin owns the live tally; writing it to skill XP would corrupt). Counted-known so `unknown_json` stays clean. Present in `chat-kill-slice.log` |
| `EntityReport` | `{"Title":"Eveline Rastin", "Body":"Here's a hot tip:…"}` (arena tip, single line, no twin) | No | `player_messages` (message_type='entity_report', other_player=title), domain `player_messages`. Rescan-safe. One line observed — pattern TBD |
| `Tip` (word discovery) | `{"Title":"You discovered a word of power!", "Body":"...<sel>WORD</sel>...Word of Power: NAME...</size></b>\nEFFECT..."}` (10 scroll uses, 2026-11-19; non-word `Tip` titles, e.g. dementia, observed but unwritten) | No — sole record | `words_of_power` (source `'chat'`, rescan-safe on character/server/word), domain `words_of_power`. Required: word + power name + effect sentence |
| `StartQuest` | `{"QuestId":25542}` (single field — verbatim monitor line 26-11-18 10:22:45; no other keys observed) | No — sole record (giver screen no longer names the quest since the 2026-11-16 game update) | `quest_accepts` ledger (character, server, timestamp, quest_id + CDN/fallback name at write time, `source_line_id`), domain `quests`. Dedupe key (character, server, accepted_at, quest_id, line id) — rescans converge. Missing/zero/wrong-type `QuestId` → malformed, never `Unknown` |

## Twin rule (idempotency across the pair)

English-twin identity key stays `(timestamp, character, server, item, qty, context, source='chat_status')`, narrowed by per-occurrence `file:offset` (`source_line_id`) line identity plus stamped-entity/`json_twin`/`claimed_by`/`english_claimed_by` traces.
The English guard never sees entities. The JSON guard is entity-aware: exact-entity `EXISTS` first,
then a single-row unstamped-ENGLISH-twin claim (`from_name`/`from_entity_id` enrichment, `MIN(rowid)`).
JSON-originated rows carry `{"json_twin":true}` in `source_details` and are never claimed — an unsourced
JSON row is not an English twin, so the claim requires the marker's absence (pre-marker rows have NULL
details and stay claimable):

- JSON first: JSON inserts with entity; English twin guard-hits → skip without nulling enrichment.
- Rescan: both twins re-fire → exact-entity guard-hit → no new rows, no stamping.
- Schema: V91 adds `item_transactions.from_name/from_entity_id` (looted-from entity), the `favor_events` per-line log (one row per line — the log IS the guard; the `cumulative_delta` integral is retired, session sums by query), and `character_npc_favor.gifts_this_week`.

## Rescan / replay / scan parity

Three paths feed the same `dispatch_chat_message`; each arm converges by construction:

| Path | Identity | Area | Domains | Events | Content triggers |
|---|---|---|---|---|---|
| Live drain | login session (active character/server) | live zone | on change only | on fresh write only | run (gourmand/hoplology/teleport/report-stats/garden) |
| Rescan (same lines re-read) | same | same | silent — every arm gates on changed rows (`favor_events` key, changed-row counts, `Option`/`bool` write results, book content compare) | silent — gated on the write result | re-run but each is a no-op on identical content |
| Full-file scan (`scan_chat_log_file`) | per-line file session (`Logged In As` opens, settings character fallback) | `None` — death/rez rows land with `NULL` area (accepted) | merged from every outcome (deduped) so backfills refresh the UI | forwarded identically (`enemy-killed`, `character-death`, …) | **never run** — backfilled books are historical; importing them as current would clobber live snapshots. Scan logs the skip per title instead |

Scan also never accrues currency (`count_currency: false` — no per-line accumulator memory, so re-scans would double-count) and never imports `ReportSaved` files (live-only; logged per path). Unknown skills, malformed JSON, and unobserved types are logged per line, never silent.

## VIP character report delta (this patch)

- `NPCs.<npc>` gains `"GiftsThisWeek": N` (observed: `NPC_Elune → 1`, `NPC_Sulafa → 5`; absent = key missing, not 0). Shipped: `#[serde(default)] gifts_this_week` on `NpcFavorData`, V91 `character_npc_favor.gifts_this_week` column, report import + `get_snapshot_npc_favor` round-trip it. Increment proven 2026-11-17: Corinth absent (Before, favor 3075.912) → still absent after talking → `5` after 5 gifts (favor 3286.512, +210.6 = the live `DeltaFavor`).
- Hangout: only signal in this export is the quest string `"HangoutWithJesinaTracker"` (player not on a hangout) — no distinct hangout field observed → optional defaulted field, log-and-skip until a live on-hangout export is captured.

## Unobserved (parser + log-and-skip until live lines captured)

Popups, reports (`[JSON:Report]`-as-chat-line), minor extras, hangout event lines.
No Type names or keys invented — code from fixtures only when they land.

## Prerequisites (game-side, not app settings)

VIP + chat logs on + "non-combat logging" checked (default on). Combat logging unchanged — old `[Combat]` parsing stays.

## Wanted signals

Unobserved lines that would unlock features (harvest-NPC almanac extraction,
hangout persistence) are tracked in
`docs/vip-json-signal-wishlist.md`, with in-game capture steps in
`docs/vip-json-observation-checklist.md`. Remove entries as fixtures land.
