# Time-Series History Plan (game_state_* + councils ledger)

Plan-only slice (2026-11-27): no migration, no tables, no writers ship here.
Supersedes the code slice on the same branch (reverted before drafting).
Covers TODO #7's game_state_* history gap (the `postv1` 8-set: the 7
user-confirmed 2026-11-27 plus `game_state_moon`, added on review) and the
councils per-event ledger (TODO #347, timeboxed handoff
`#401`). B review audits this doc.

## The time-question test

A domain keeps history only if a named consumer reads past points (widget,
window query, reconciliation math). Otherwise it is point-in-time by nature
and stays a last-known-value row.

Garden-history lesson (binding): `garden_almanac_history` appended one row
per import per season with zero readers, so v102 dropped it as dead surface
(`migration_v102_drop_garden_almanac_history`, `src-tauri/src/db/migrations.rs`).
Rule derived: every history table below ships with a named consumer or stays
in this plan. `harvest_almanac` follows the same precedent (snapshot-replace,
no history, entry closed).

Existing series family (no new tables needed): `item_transactions` (v15 +
v96 per-occurrence identity), `favor_events` (v91 + v97 `COALESCE` key),
`chat_xp_awards` + `xp_snapshot_at` anchor (v101), per-import snapshot
children (`character_currencies`, `character_recipe_completions`, ...),
`combat_wisdom_earns` (v51), `player_messages`, milking/pooping logs.

## 1. Per-domain disposition (the 8 post-v1 tables)

| Domain | Live writer | Time-question: would anyone read the past? | Disposition |
|---|---|---|---|
| `game_state_area` | `coordinator.rs` `AreaTransition` arm (upsert, emits `area-transition`) | Travel history / time-in-zone is a real question, but presence is already derivable from `chat_messages` timestamps + kill/death area columns | Point-in-time. No table |
| `game_state_npc_vendor` | Report seeder (`character_commands.rs` spend/reset) + `VendorGoldChanged` chat arm (`game_state_commands.rs` gold availability) | Vendor-gold availability is transient (restock consumes it); per-NPC spend already has a per-import series in `character_npc_favor` | Point-in-time live row. No table |
| `game_state_guild` | None (write-dead since v33; seed-only) | Join/leave log would need a producer that does not exist | No producer, no consumer. Wire-or-drop decision, not a history table |
| `game_state_directed_goals` | None (write-dead since v33; seed-only) | Goal-completion timestamps belong to the quest-tracking effort when it lands | Point-in-time. No table |
| `game_state_strings` | None (write-dead since v33; seed-only) | Generic KV cache by design; a key with time semantics gets its own table | Point-in-time. No table |
| `game_state_books` | `books_watcher.rs` + chat dispatch + coordinator (replace on re-read, `captured_at` = newest) | "What changed since last export" is answered by diffing stored `raw_json` at read time; Books/ files on disk are the archive | Point-in-time latest content. No table |
| `game_state_teleportation` | `ingest_teleportation_binds` + mushroom-circle command (`game_state_commands.rs`) | Bind moves are rare player actions; no bind-change events are parsed today | Point-in-time. No table |
| `game_state_moon` | None (write-dead since v33, world singleton like `weather`; seed/list-only) | Phase is a deterministic function of date — any past phase is computable, never stored; the row exists to render tonight without recomputing | Point-in-time. No table |

None of the 8 gets a history table. Four (`guild`, `directed_goals`,
`strings`, `moon`) are write-dead since v33 — zero live writers, zero live readers
(only the v33 CREATE, the lifecycle lists, and test seeds reference them).
That is a wire-or-drop observation for a future slice, not history scope:
either a producer arrives (guild/quest systems) or they follow the
`vendor_prices` v85 drop precedent. This plan proposes neither; it records
the state so the next slice does not re-audit it.

### Build triggers (one per domain)

- `area`: a zone-time widget or area-history view is requested. Then build a
  presence log (enter/exit events), not snapshot sampling — transitions are
  high-frequency and sampling aliases them.
- `npc_vendor`: a vendor-economy view wants gold availability over time.
  Requires a restock event source first (today only availability snapshots
  exist; nothing parses restocks).
- `guild`: a guild system produces join/leave/motd-change events. Until then
  there is nothing to ledger.
- `directed_goals`: the quest-tracking effort lands and wants
  goal-completion timestamps. History ships as part of that effort, not here.
- `strings`: a specific key gains time semantics. Then that key gets its own
  table; `strings` itself never gets history.
- `books`: a book-diff view ("what changed since last export") is requested.
  Build it as read-time diffing off stored `raw_json`, not a history table.
- `teleportation`: a bind-change audit ("when did my binds move") is
  requested. Requires bind-change events (book re-reads only replace; they do
  not timestamp moves).
- `moon`: effectively never — phase is computed from the date, not observed.
  Trigger only if the game ever decouples phase from the real moon (event-driven
  overrides), in which case ledger the override events, not nightly snapshots.

## 2. Councils ledger design

### Schema (exact, when triggered)

```sql
CREATE TABLE IF NOT EXISTS council_events (
    character_name TEXT NOT NULL,
    server_name TEXT NOT NULL,
    event_ts TEXT NOT NULL,        -- space-format naive UTC (v98 contract)
    delta INTEGER NOT NULL,        -- signed; negative = spend/theft/gift
    kind TEXT NOT NULL,            -- 'councils_changed' | 'coins_looted'
    source_line_id TEXT            -- "file:offset" per-occurrence identity
);
CREATE UNIQUE INDEX idx_council_events_line ON council_events(
    character_name, server_name, event_ts, delta, kind,
    COALESCE(source_line_id, ''));
CREATE INDEX idx_council_events_char_ts
    ON council_events(character_name, server_name, event_ts);
```

No PRIMARY KEY (same v97 `favor_events` precedent: a plain UNIQUE index
treats NULLs as distinct and rescans would reduplicate; the `COALESCE`
expression key keeps legacy NULL rows collapsing among themselves).
`captured_at`-style normalization via the shared `normalize_contract_ts`
(`game_state_commands.rs`) so ordering never mixes `T` and space families.
No backfill (pre-existing rows are unrecoverable line identities, same
accepted limit as v96/v97).

### Writer sites (exact, when triggered)

1. Parse (already exists, verified): `try_coins_looted`
   (`chat_status_parser.rs:419`), `try_councils_changed` (`:433`),
   `try_councils_misc` (`:463`) produce `CoinsLooted` / `CouncilsChanged`
   events. No parser changes needed.
2. Dispatch (existing arms): `CoinsLooted` (`chat_status_dispatch.rs:627`)
   and `CouncilsChanged` (`:645`) both funnel into the single choke point
   `accrue_currency_delta` (`:1586`).
3. Change: extend `accrue_currency_delta` with `kind` + `source_line_id`
   params (threaded from `DispatchCtx.source_line_id`, already populated
   live as `file:offset` in `coordinator.rs`); wrap the `delta_since` UPDATE
   and the ledger INSERT in one unchecked transaction (favor-arm precedent,
   same file); return contract unchanged (true iff accrued).
4. Gating: the ledger INSERT is NOT gated on the anchor guard — pre-anchor
   and anchor-less events still happened, and the anchor filter applies at
   read time. `count_currency = false` scan paths record neither accrual nor
   ledger (live owns accumulation, as today).

### Anchor-reconciliation contract (unchanged anchor)

`currency_estimate` (v56) stays the estimate: `anchor_amount` at `anchor_at`
(seed: character-export GOLD, re-anchor on newer export only) plus
`delta_since` from chat. The ledger reconciles against it, never replaces it:

`delta_since == SUM(council_events.delta WHERE event_ts > anchor_at)`

holding while (a) every post-anchor event accrued exactly once (live owns
accrual; `count_currency = true` rescans double-accrue — the accumulator's
documented limit — while the ledger `OR IGNORE`s to one row), and (b) no
second-granularity same-content twin collapsed on an unstamped line (same
accepted limit as `favor_events`). Pins that will prove it: ledger rows land
per event with timestamp + delta + kind; rescan/replay converges to one row;
existing currency pins pass unmodified.

Lifecycle: new table joins `SCOPED_TABLES` (`setup_commands.rs`, delete +
rename coverage) and the test seed; it does NOT join `PURGE_TABLES` (same as
`favor_events` / `chat_xp_awards` — time-series are not age-purged).
Readers are omitted from the build slice (backend-only, like the v101
ledger first landed); the consumer below is named.

### Named consumer — build trigger FIRED

User-requested council wealth history view: line graph of council amount
over time (TODO entry filed 2026-11-27, user-reported via slice handoff;
no line number in this checkout to cite). This is the consumer that breaks
out the ledger. Build order: ledger table + writer first, then the wealth
view reusing timeboxed window machinery + chart theme tokens. Secondary
consumer: timeboxed views ("councils in window W", handoff #401).

## 3. Build-trigger summary

| Item | Trigger (what breaks out the plan) | Ships |
|---|---|---|
| area history | Zone-time / area-history view requested | Presence log (enter/exit) |
| npc_vendor history | Vendor-economy view + a restock event source | Availability series |
| guild history | Guild events exist (join/leave/motd) | Join/leave log (or drop the dead table first) |
| directed_goals history | Quest-tracking effort wants completion timestamps | Part of that effort |
| strings history | Never (per-key tables instead) | Per-key table for the key with time semantics |
| books history | Book-diff view requested | Read-time `raw_json` diff, no table |
| teleportation history | Bind-change audit requested + bind-change events | Bind-move log |
| moon history | Game decouples phase from the real moon (not observed; likely never) | Override-event log, not nightly snapshots |

## Non-goals

Timeboxed view changes, vendor/anchor semantics (anchor is read-only here),
currency absolutes series (`character_currencies` per-import rows already
answer per-export balances; the wealth view pins level via the anchor),
second migration version (ledger ships in one version when triggered),
TODO.md edits.
