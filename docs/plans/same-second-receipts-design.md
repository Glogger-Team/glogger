# Same-second same-corpse receipts: ledger-guard collapse — design

## Problem

Two genuine loot receipts for the same item + quantity off the same corpse
in the same second collapse to one `item_transactions` row, so
`get_enemy_kill_stats` undercounts (AoE mass-kill + same-second multi-loot
is normal play, not a corner; autoloot makes same-second the norm).

Concretely: two stacks off one corpse in one second keep one ledger row.
`SUM(quantity)` undercounts by the lost stack. (`times_dropped` is
`COUNT(DISTINCT kill_id)` — kills, not stacks — so the one-corpse case costs
one kill either way; the systematic loss is in quantities and per-stack
visibility. See §5.)

Root cause: log timestamps are second-granularity
(`chat_parser.rs:48` — `YY-MM-DD HH:MM:SS`; `ChatMessage.timestamp` is a
`NaiveDateTime` with no sub-second component, and both guards format it as
`"%Y-%m-%d %H:%M:%S"`). Two genuine same-second receipts are byte-identical
on every column the guards compare, i.e. indistinguishable from a
replay/resend on content alone. No pure-predicate change on the current key
can separate "second genuine receipt" from "replay of the first" — the fix
must add distinguishing input (source-line identity, §6).

## Guard semantics today

### English guard — `insert_item_transaction_guarded` (`chat_status_dispatch.rs:759`)

Replay key (`:783-792`):

```sql
... WHERE timestamp=?1 AND character_name=?2 AND server_name=?3
  AND item_name=?4 AND quantity=?5 AND context=?6 AND source='chat_status'
  AND (?7 IS NULL OR from_entity_id IS ?7)
```

- Production English arms (`ItemGained` `:520`, `Summoned`) always pass
  `from_entity_id: None` (`:538`) — the `[Status]` line carries no entity.
- `None` matches **any** row: an identical resend collapses, and a
  JSON-first twin-second English line guard-hits the JSON row and skips
  (`:532-537`, twin tests prove the NULL-only key would duplicate there).
- Entity-aware only for entity-knowing callers (`Some` matches exact-entity;
  `english_guard_records_second_entity_when_known`). Distinct corpses are
  safe **when the caller knows the entity** — production English never does
  (documented inherent collapse, `:535-537`).
- False return = "already present" → caller stages nothing (`:541-550`).

### JSON guard — `insert_json_receipt_guarded` (`chat_status_dispatch.rs:826`)

Three steps (`:850-911`):

1. `exact_exists` — same content key + `(from_entity_id = ? OR
   (? IS NULL AND from_entity_id IS NULL))` (`:854-863`). A sourced replay
   hits here and never touches the twin claim (T44, `:2612`).
2. Twin claim — only when step 1 misses **and** entity is `Some`: stamp at
   most one unstamped ENGLISH row (`MIN(rowid)`, `json_twin`-marker-excluded,
   `:876-899`). Failed UPDATE returns `false` without inserting (no
   duplicate on broken connection).
3. `exists_after` — same predicate as step 1 (`:902-911`). Hit → `false`
   (AlreadyPresent, still links kill loot); miss → INSERT with
   `source_details = '{"json_twin":true}'` provenance (`:916-941`) and
   `link_kill_loot` on the receipt's entity (`:946-961`).

Second genuine JSON receipt, same entity/item/qty/second: hits step 1
(`exact_exists`) → collapses. No second row, no second link call with a new
key. This is the reported defect on the JSON side.

### What is pinned by tests today

- `json_additem_twins_converge_{english,json}_first`, `twins_two_entities_converge_{english,json}_first`,
  `json_receipt_enrichment_stamps_single_row`, T43/T44 claim-ordering tests
  (`chat_status_dispatch.rs` test module) — twin convergence both orders.
- `english_guard_collapses_identical_second_gain` — `None` resend collapses.
- `same_second_twins_stay_collapsed` (`kill_tracking_commands.rs:2314`) —
  explicitly pins the collapse as the *accepted limit* ("one stored receipt,
  one loot row"). **This test encodes the defect; the fix replaces it**
  (see §7). D5-56 receipt-identity keying (`kill_id, item, qty, receipt_ts`)
  deliberately does not span the ledger gap.

## Corpse / temporal identity available at each site

| Site | Corpse identity | Temporal identity |
|---|---|---|
| English guard | **None.** Line carries no entity; `from_entity_id` always `None` in production. Caller could only know the entity via out-of-band correlation (not available in `dispatch_chat_message`). | Second-granularity `timestamp` only. |
| JSON guard | `from_entity_id` (`Some` for loot) + `from_name`. `None` only for non-loot/unsourced receipts. | Same second-granularity `timestamp`. Arrival order exists as `rowid` but records arrival, not event order. |
| `ChatMessage` (both) | No line identity at all: `{timestamp, channel, sender, message, is_system, from_player, item_links, json_type, json_body}` (`chat_parser.rs:21-33`). No file/offset/rowid. Rescan re-parses the same bytes into an identical struct. | `NaiveDateTime`, second granularity. |

Consequence: at the guard layer a genuine duplicate is *provably*
indistinguishable from a replay today. The design must introduce identity,
not sharpen the content predicate.

## English ↔ JSON twin table

| | English `ItemGained` (`chat_status_parser.rs:10-15`) | JSON `ItemReceipt` (`chat_json_parser.rs:16-22`) |
|---|---|---|
| Source text | `"X added to inventory" / "X xN added to inventory"` | `AddItem`: `{Name, Stack, FromName?, FromEntityId?}` (`:88-96`); empty name / zero stack rejected (`:143-149`) |
| Timestamp | Log line stamp (UTC after `chat_local_to_utc`) | Same `ChatMessage.timestamp` → identical string |
| Item/qty | Parsed name + qty; CDN-resolved `internal_name`/`item_type_id` | Same CDN resolution ("twin writes identical values", `:1048-1050`); raw `TypeId` not stored |
| Entity | None | `FromName`/`FromEntityId`, `Some` only for loot |
| Ledger context | `'loot'` (`'summoned'` for `Summoned`, no JSON twin) | `'loot'` |
| Twin key (per `docs/features/vip-json-logging.md:26-36`) | `(timestamp, character, server, item, qty, context, source='chat_status')` — entity-blind by contract | Exact-entity `EXISTS`, then single-row unstamped-ENGLISH claim (`MIN(rowid)`), `{"json_twin":true}` provenance so unsourced JSON rows are never claimed |

Convergence today: JSON-first → JSON inserts with entity, English twin
guard-hits and skips; English-first → English inserts NULL-entity, JSON
claims (stamps) it via `MIN(rowid)`, `exists_after` hits, no insert. Staging
is exactly-once per pair (`staged_gains` only on write, `:520-543`,
`:1060-1077`; coordinator live-attribution follows the same gate).

## Proposed predicate change per guard

Direction (per the TODO entry): collapse **only across representations**
(English↔JSON twin), never within; genuine duplicates persist as distinct
`item_transactions.id`s for the read-time join. Prescription: **key
replays on source-line identity, keep content-key matching only for the
twin claim.**

### Prerequisite: carry source-line identity into the dispatch

Add an opaque source-line id to `DispatchCtx` → `ItemLedgerCtx` and to
`insert_json_receipt_guarded`'s params, and store it on the ledger row
(new nullable column, e.g. `source_line_id`, backfilled `NULL`).
Candidate identities (explicit option, §8.1): raw `chat_messages` rowid
(preferred if stable across rescan — must verify rescan reuses stored rows
rather than reinserting), else `(file, offset)`. Pre-identity rows
(`NULL`) keep legacy match-any semantics so old replays stay safe.

### English guard (new predicate)

Collapse iff content key matches **and** the matching row carries **this
line's** `source_line_id` (true replay). Otherwise INSERT — including a
second genuine English line with the same key (new `source_line_id`).
JSON-first twin-second still skips: keep the current `None`-matches-any arm
**but only against rows the JSON twin wrote or stamped** (entity-stamped or
`json_twin`-marked rows), never against another unstamped English row from
a different line. Truth table:

| Arriving E (line L2) vs stored | Verdict |
|---|---|
| Same key, same line L2 (= L1, replay) | skip |
| Same key, row stamped/JSON-marked (twin-second) | skip, stage nothing |
| Same key, unstamped English row, different line (genuine 2nd stack, JSON off or JSON twin pending) | **INSERT** (behavior change — the fix) |

`english_guard_collapses_identical_second_gain` stays green by passing the
same line id twice; a new test passes two line ids and expects two rows.

### JSON guard (new predicate + reorder)

1. `exact_exists` narrows to same-line replay OR twin-claimed row:
   content key + entity match + (`source_line_id` = this line's **or** row
   already stamped with this entity by the twin claim). A genuine second
   JSON receipt (different line, same entity) must **not** hit here.
2. Twin claim runs **before** the exact check when unstamped English rows
   exist: claim `MIN(rowid)` among unstamped English rows *excluding this
   line's own prior claim*, stamp entity. This lets J2 (genuine, same
   entity) claim the second English twin row E2 instead of collapsing onto
   J1's stamped row — the current order (exact-check-first) is what strands
   E2 unclaimed.
3. `exists_after` unchanged in shape: hit (claimed or replayed) → return
   `false` (AlreadyPresent, still `link_kill_loot`, still no staging —
   E2/J2's event staged exactly once at its English insert); miss →
   INSERT with `json_twin` provenance as today.

Genuine JSON-only duplicates (no English lines at all): different line ids
→ each INSERTs its own row. Replays (same line id) → collapse. T44/T43
invariants hold: sourced replay still never stamps an unsourced row
(step-1 same-line check precedes any claim; unsourced JSON rows keep the
`json_twin` marker and stay unclaimable).

### Link layer — deliberately unchanged

`enemy_kill_loot`'s `(kill_id, item, qty, receipt_ts)` key will still
collapse two *identical* same-second same-qty stacks from one kill into one
loot row even after the ledger holds two rows (both the receipt-side
`NOT EXISTS` and the kill-side `SELECT DISTINCT item, qty, timestamp`
heal). Do **not** widen the loot key to ledger rowids: the link-redundancy
entry supersedes rowid-keyed loot-table design — the end-state consumer is
the read-time `ledger.entity = kills.entity + window` join, which counts
ledger rows directly (both stacks in `SUM`, one kill in
`COUNT(DISTINCT)`). Until that join lands, document the residual: identical
same-second same-qty stacks link one loot row; distinct-qty stacks already
link separately. No loot-table change in this slice.

## Test plan

New tests live next to the existing twin tests in
`chat_status_dispatch.rs` (dispatch-level, both orders) plus one link-layer
documentation test in `kill_tracking_commands.rs`:

1. `english_genuine_duplicate_keeps_two_rows` — two English lines, same
   key, different line ids → 2 rows, staged twice. (Red: collapses to 1.)
2. `json_genuine_duplicate_same_entity_keeps_two_rows` — two JSON receipts,
   same entity/item/qty/second, different line ids, no English → 2 rows.
   (Red: `exact_exists` collapses.)
3. `twin_pair_times_two_events_both_orders` — E1,E2,J1,J2 (and J1,J2,E1,E2)
   same key+entity, four line ids → exactly 2 rows, both stamped with the
   entity, `staged_gains` total 2 across the pass. (Red: 1 row / stranded
   unstamped row.)
4. `full_redispatch_is_stable` — re-dispatch all four lines with their
   original line ids → still 2 rows, 0 staged. (Guards the rescan path the
   new INSERT-happy rules must not regress.)
5. Existing twin tests (`twins_two_entities_converge_*`,
   `json_additem_twins_converge_*`, T43/T44, enrichment single-row) keep
   passing unmodified — twin collapse across representations is preserved.
6. `english_guard_collapses_identical_second_gain` — updated to pass the
   **same** line id twice (true replay); still 1 row.
7. Kill-tracking: **replace** `same_second_twins_stay_collapsed` with a test
   asserting the new contract — two ledger rows (distinct ids) for the twin
   pair feed the read-time join / link two loot rows once quantities differ
   — and move the old collapse pin to history. Distinct-timestamp tests
   above it stay green.

Gates: `mise run lint:full`, each guard module's `--self-test`/test target
green, plus the entity-scope sibling sweep both guards share (distinct
corpses must still yield distinct rows — already covered by
`twins_two_entities_converge_*`).

## Risks

- **Rescan line-id stability.** If rescan reinserts raw chat rows (new
  rowids) instead of reusing them, stored `source_line_id`s never match and
  every rescan duplicates the ledger. Verify raw-storage identity *before*
  building; fall back to `(file, offset)` if rowids churn. Pre-identity
  `NULL` rows must retain match-any collapse or the first post-upgrade
  rescan duplicates years of history.
- **VIP on/off transitions across rescans.** Enabling VIP later adds JSON
  twins for old English-only rows: the claim path must stamp (not insert)
  them — current `MIN(rowid)` claim does this, but only one row per receipt;
  a backlog of duplicate English rows plus new JSON receipts needs the
  claim-then-check order (§5) or old duplicates strand unstamped.
- **Staging double-count.** Claim-then-check must return AlreadyPresent
  (no stage) after stamping, or the JSON twin re-attributes a gain the
  English line already staged (the `already_counted` survey guard masked
  this class before — do not rely on it).
- **Migration cost.** New nullable column on a 160k+ row ledger is cheap
  (no backfill, no reindex), but every dispatch call site (coordinator
  live, scan path, replay guard, game-state retro path) must plumb the new
  field — four call sites, mechanical, each a missed-plumbing duplicate bug
  if defaulted wrong. Default must be `NULL` = legacy collapse, never
  "fresh id" = always insert.
- **Timestamp prerequisite.** None for the ledger fix itself (keys are
  equality on the stored string), but the read-time join consumer needs the
  timestamp-contract normalization first (year-2000 `datetime(time_only)`
  bug bites window predicates identically at read time).

## Open options (explicit)

1. **Line-identity source**: raw `chat_messages` rowid vs `(file, offset)`
   vs per-pass occurrence index with persistent per-key counters (no schema
   change, but fragile across mixed VIP-transition rescans). Recommend
   rowid-then-file-offset, to be settled by verifying rescan row reuse.
2. **Storage**: dedicated nullable `source_line_id` column (indexable,
   queryable for the read-time join) vs `source_details` JSON key
   (no migration, unindexed — same perf class as today's unindexed
   `EXISTS`, but opaque to future readers). Recommend the column.
3. **Link-layer interim**: leave the `(kill_id, item, qty, receipt_ts)`
   collapse documented (§5 residual) vs emitting one loot row per linked
   ledger row by including the timestamp tie-break differently. Recommend
   leaving it — the read-time join deletes the question.
4. **Scope**: ledger-guard fix only (this design) vs bundling the read-time
   join + timestamp normalization. Recommend separate slices: the ledger
   fix is a prerequisite that stands regardless, and each half is
   independently testable.
