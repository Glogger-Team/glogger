# VIP JSON Logging — observed contract (frozen 2026-11-17)

> Addendum 2026-11-18: Gardening Almanac extraction shipped (Book row,
> wanted signals). "Frozen" dates the observations, not the doc.

Source: live `Chat-26-11-1*.log` on this box, game with VIP + chat logs + non-combat logging on.
Fixtures: `test_data/vip_json/chat-<type>.log` (names sanitized `TwinkleofToes` → `TestPlayer`),
`character-report-after.json` (trimmed: exact key shapes, 2 skills, 3 recipes, 3 NPCs).

Line shape: `YY-MM-DD HH:MM:SS\t[JSON:Type] {single-line JSON}` — no sender prefix, `\n` escaped inside strings.

## Observed types

| Type | Example JSON | Twin? | Consumer (existing → reuse) |
|---|---|---|---|
| `AddItem` | `{"Name": "Toxic Sludge", "TypeId": 1137, "Stack": 1, "Flags": [], "FromName": "Olugax The Ever-Pudding", "FromEntityId": 257400}` (`From*` absent when not loot) | Yes — English `[Status] <Name> [xN] added to inventory.` immediately before, same second | `insert_item_transaction_guarded` + `staged_gains` + survey hook, domain `inventory` |
| `EarnedXP` | `{"Skill":"Teleportation", "XP": 53, "OldLevel": 46, "NewLevel": 46}` | Yes — English `[Status] You earned N XP in <Skill>.` same second | Skill XP upsert / `apply_chat_level_up` when `NewLevel > OldLevel`, domain `skills` |
| `Book` | `{"Title":"Skill Info", "Body":"Foods Consumed:\n\n ..."}` | No — sole record | `persist_book_content` upsert on `game_state_books`; gourmand/hoplology/teleport/report-stats/gardening triggers run on the body. Gardening: `parse_garden_almanac_body` + `import_garden_almanac` fill `garden_almanac` (snapshot) + `garden_almanac_history`, domain `gardening`; rescan-safe no-op |
| `DeltaFavor` | `{"Npc":"NPC_Elune", "Delta": 18.06948}` | Display-only English (`You gained 18.1 favor` — rounded, unparsed); JSON exact value is authoritative | Additive upsert on `game_state_favor` + conditional `game_state_gift_log` INSERT (giftable only), guarded by `json_favor_processed`, domain `favor`. Sole structured favor source after LocalPlayer deletion |
| `RecipeUsage` | `{"RecipeName":"Recall Heart's Home", "RecipeId": 9008, "Completions": 211}` | No English twin (only the sibling XP line) | Upsert `game_state_recipes` `completion_count`, domain `recipes`. `Completions` is an **absolute total** (matches snapshot `RecipeCompletions` shape) — set newer-wins, never increment. Proven: Distill 776 → 777 across two live uses, 2026-11-17 |
| `Pigeon` | `{"Title":"Pigeon From <friend>", "Body":"...<i>You received:\n<Item> xN</i>"}` (receive only — send emits nothing) | No twin | `player_messages` (message_type='pigeon', direction='in', sender + item parsed), domain `player_messages`. Rescan-safe. Fixture: `chat-pigeon.log` |
| `ParagonXP` | `{"Skill":"Werewolf", "XP": 279200, "OldLevel": 6, "NewLevel": 6}` | Yes — English `You earned N Prodigy XP in <Skill>.` same second | Typed, deliberately unwritten: per-kill prodigy flow has no DB home (the Status twin owns the live tally; writing it to skill XP would corrupt). Counted-known so `unknown_json` stays clean. Present in `chat-kill-slice.log` |
| `EntityReport` | `{"Title":"Eveline Rastin", "Body":"Here's a hot tip:…"}` (arena tip, single line, no twin) | No | `player_messages` (message_type='entity_report', other_player=title), domain `player_messages`. Rescan-safe. One line observed — pattern TBD |

## Twin rule (idempotency across the pair)

English line always precedes its JSON twin, but the rule is order-independent.
Identity key stays `(timestamp, character, server, item, qty, context, source='chat_status')`
— `from_name`/`from_entity_id` are enrichment, not identity:

- JSON second (usual): guard hits → `UPDATE` the row's `from_name`/`from_entity_id` (V91) instead of skipping. Exactly one row, with entity. Survey columns are never touched — enrichment is identity-adjacent, not attribution.
- JSON first: JSON inserts with entity; English twin guard-hits → skip without nulling enrichment.
- Rescan: both twins re-fire → guard-hit → no new rows (entity re-stamped, harmless).
- Schema: V91 adds `item_transactions.from_name/from_entity_id` (looted-from entity); V92 adds `json_favor_processed` rescan marker (favor accrue + gift inserts are additive, not idempotent).

## VIP character report delta (this patch)

- `NPCs.<npc>` gains `"GiftsThisWeek": N` (observed: `NPC_Elune → 1`, `NPC_Sulafa → 5`; absent = key missing, not 0). Shipped: `#[serde(default)] gifts_this_week` on `NpcFavorData`, V93 `character_npc_favor.gifts_this_week` column, report import + `get_snapshot_npc_favor` round-trip it. Increment proven 2026-11-17: Corinth absent (Before, favor 3075.912) → still absent after talking → `5` after 5 gifts (favor 3286.512, +210.6 = the live `DeltaFavor`).
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
