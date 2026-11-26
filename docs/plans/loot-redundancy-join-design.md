# (Filed 2026-11-25 from S4 design handoff; implementation-ready post-#363.)

<!-- markdownlint-disable MD025 -- provenance header above the verbatim S4 title; S4 body untouched -->
# S4 REPORT - loot-redundancy removal design (TODO.md:219) - DESIGN ONLY, tree untouched (filed 2026-11-25, implementation-ready post-#363)

## 1. END STATE

Delete the `enemy_kill_loot` table entirely. All mine-scope loot readers become a read-time join: ledger rows (`item_transactions` with `from_entity_id` set) joined to `enemy_kills` on identity + 30-minute window, evaluated inside `get_enemy_kill_stats` and its four sibling readers. `enemy_kills` stays. `imported_enemy_kills_agg` / `imported_enemy_kill_loot_agg` (community imports) stay byte-identical. `item_transactions` stays, including `from_entity_id` and the twin-claim stamping that fills it.

Readers that change (mine scope only): `mine_total_kills`, `mine_loot_rows`, `get_item_drop_sources` (mine arm), `search_database_enemies` (mine arms), `search_database_items` (mine arm). `collect_export_enemies` is free - it delegates to `mine_total_kills` / `mine_loot_rows`.

Writers/heals deleted: the receipt-side loot-link fn, the kill-side loot-link heal fn, the newest-eligible-kill lookup (only used by those two), both heal call sites in `record_enemy_kill`, v94 `receipt_ts` column + both UPDATE backfills + `idx_kill_loot_receipt` (die with the table).

Untouched: `sweep_unlinked_gains`, twin-claim stamping, English guard, JSON guard, `record_enemy_kill` kill INSERT + rescan-None gating, all import/export/parse code, all frontend (same command names and payload shapes), delete/rename character scope loops (except census-list edits, section 7).

## 2. JOIN PREDICATE (exact replication spec - implementer uses verbatim)

A ledger row `it` belongs to kill `k` iff ALL hold (this is v94 UPDATE1 + both link fns, unified):

```sql
it.character_name = k.character_name
AND it.server_name = k.server_name
AND it.from_entity_id = CAST(k.enemy_entity_id AS INTEGER)
AND it.timestamp >= k.killed_at
AND it.timestamp <= datetime(k.killed_at, '+30 minutes')
```

Boundaries are inclusive both ends (pinned by `loot_link_window_includes_endpoints_but_excludes_future_and_expired_kills`: 12:30:00 in, 12:30:01 out — the backlinks shape merged into the window rewrite at implementation). CAST semantics preserved exactly, including the wart: non-numeric entity ids (test seed 'e1') CAST to 0/NULL and never match - same as v94 UPDATE1 today, preserved not fixed.

Newest-kill-wins (replicates the retired newest-eligible-kill lookup: newest `killed_at` <= receipt wins; v48 UNIQUE dedup on (character, server, entity, killed_at) makes `killed_at` decisive, so the id tie-break is unnecessary - exact dupes cannot arrive via the API):

```sql
AND NOT EXISTS (
  SELECT 1 FROM enemy_kills k2
  WHERE k2.character_name = k.character_name AND k2.server_name = k.server_name
    AND k2.enemy_entity_id = k.enemy_entity_id
    AND k2.killed_at > k.killed_at AND k2.killed_at <= it.timestamp
)
```

(`k2 > k` plus `k` in window implies `k2` in window; `k2 <= receipt` excludes future kills. This replaces the D5-55 older-linking-forward rule: a receipt in K1+K2 windows attributes to K2 only, from both read directions, with no heal needed.)

Implement as ONE shared Rust fn returning the ON-clause string (e.g. `kill_receipt_join_on()`) used by every reader and the veto below, so the five call sites cannot drift. SQL stays static string literals (`check-sql-prepare` gate; ZoneFilter/LoadoutFilter `format!` arms are the existing accepted pattern).

Context/source filter: use NONE - replicate the retired kill-side heal predicate exactly (it selected with no context/source filter). In practice only chat loot rows carry `from_entity_id` (English rows are NULL-entity unless twin-stamped; survey rows carry no entity), so the effective set is chat loot either way. If the owner wants narrowing to `context='loot' AND source='chat_status'`, that is a separate behavior decision with evidence, not part of this slice.

## 3. READER REWRITES (one per function)

- 3a. `mine_loot_rows` - `SELECT it.item_name, SUM(it.quantity), COUNT(DISTINCT k.id) FROM enemy_kills k JOIN item_transactions it ON <shared clause> WHERE k.enemy_name = ?` + zone/loadout on `k.*` GROUP BY `it.item_name`. `times_dropped` stays per-kill (`COUNT DISTINCT k.id`), same as today.
- 3b. `mine_total_kills` - keep kill COUNT + death veto; veto's loot check becomes the attributed join: lootless = `NOT EXISTS (SELECT 1 FROM item_transactions it WHERE <shared clause for k>)`. Veto keeps forward-only shape (death in (`killed_at`, +5min]), identity scoping, and the looted-never-vetoed rule. `death_veto_is_forward_only_inclusive_and_identity_scoped` + `mine_total_kills_vetoes_lootless_kill_before_death` must pass UNCHANGED (preservation pins).
- 3c. `get_item_drop_sources` mine arm - same join, GROUP BY `k.enemy_name, k.zone`; per-(enemy,zone) `total_kills` loop unchanged (pre-existing N+1, out of scope - do not fix here).
- 3d. `search_database_enemies` - veto COUNT gets the 3b rewrite; DISTINCT loot query becomes `SELECT DISTINCT k.enemy_name, k.zone, it.item_name FROM enemy_kills k JOIN item_transactions it ON <shared clause>` (+ loadout where-clause, unchanged).
- 3e. `search_database_items` mine arm - `SELECT it.item_name, SUM(it.quantity), k.enemy_name, k.zone ... GROUP BY it.item_name, k.enemy_name, k.zone` with the join.
- 3f. `get_enemy_kill_stats` body - unchanged except via 3a/3b (scope branches, early return, sorting identical). `lib.rs` registrations unchanged (link fns were plain `pub` fns, never Tauri commands - nothing to unregister).

## 4. WRITER / HEAL DELETIONS

- Delete the newest-eligible-kill lookup, the receipt-side loot-link fn, the kill-side loot-link heal fn from `kill_tracking_commands.rs`.
- `record_enemy_kill`: keep kill INSERT + rescan-None return (still gates domains/events); delete BOTH kill-side heal call sites (fresh-insert heal, rescan-heal) and reword their comments (late receipts appear on next query - no heal exists).
- `chat_status_dispatch.rs` `insert_json_receipt_guarded`: delete the link attempt block (the `match from_entity_id` arm calling the receipt-side link fn); keep the ledger INSERT, staging, and twin-claim stamping. Update the comment: twin-claim entity stamping is now the SOLE entity source for English-first pairs (the join depends on it) - it must be preserved exactly, add a comment saying so.
- English `ItemGained` arm: untouched (never linked - entity None; English-only VIP-off loot stays unattributed, exactly as today - no regression).

## 5. SWEEP (no change - with reasoning)

`sweep_unlinked_gains` untouched. The guarantee holds without the link table: kill receipts carry `from_entity_id`, which is itself the exemption predicate (`WHERE source_kind IS NULL AND from_entity_id IS NULL ...`). No kill-linked row was ever pruned; after the change the rows the join needs are still exactly the spared set. NULL-entity loot rows older than 24h are still purged - same as today (they never linked either). Runs at startup + every dispatch, unchanged.

## 6. MIGRATION (next free version was 98 at design time - rebase-check at implement time, one-new-version-per-PR lint)

New `migration_v98_drop_kill_loot_link_table`, following the v95 drop precedent (never edit shipped migrations - v45 CREATE, v48 dedup, v94 receipt identity stay in the chain; fresh DBs CREATE then DROP the table, accepted):

```sql
DROP TABLE IF EXISTS enemy_kill_loot;   -- idx_kill_loot_kill + idx_kill_loot_receipt die with it
CREATE INDEX IF NOT EXISTS idx_item_tx_kill_join ON item_transactions(character_name, server_name, from_entity_id, timestamp);
```

The permanent index is this slice's doing: v94's covering tmp index (`tmp_tx_receipt_lookup`) was dropped after backfill, and the live ledger has only (char,server), (item), (time) indexes - without it the join repeats the v94 full-core spin (`idx_item_tx_char` alone re-scans the character's ledger per row). Column order mirrors the join predicate (equality cols first, timestamp last). Kills side is covered by `idx_enemy_kills_dedup` + `idx_enemy_kills_name_zone(_skills)` + `idx_enemy_kills_entity`.

New test: drop removes table + both indexes, join index exists, mine readers return correct rows post-drop on a fixture, re-run idempotent. Keep v94 test `v94_backfills_loot_receipt_identity` (pins the shipped migration, still runs in-chain).

## 7. BACKFILL + PINNED-ROW DISPOSITION (per item - deletions carry a disposition)

- v94 UPDATE1 (earliest-receipt backfill): RETIRED - its product (`receipt_ts` values) dies with the table; the read-time join computes the same match live. No replacement.
- v94 UPDATE2 (orphan pin to `killed_at`): RETIRED - pinned rows represented unattributable/pruned-ledger loot; the join counts only attested receipts, so their phantom quantity leaves SUM. Intended behavior change, document with before/after numbers (section 10).
- No kill chat-log backfill exists (zero grep hits in `kill_tracking_commands.rs`; startup backfills cover other domains) - nothing else to retire on the kill side.
- `setup_commands.rs` orphan-child DELETE (`DELETE FROM enemy_kill_loot WHERE kill_id NOT IN ...`) - DELETE with the table; reword the comment (enemy_kills parents still scope-deleted; death_damage_sources arm stays).
- `setup_commands` seed INSERT INTO `enemy_kill_loot` (Bone row) - DELETE (kill seed row stays; it now exercises the join with zero receipts).
- Census lists (delete test + rename test) - remove the `enemy_kill_loot` entry from both; assertions otherwise unchanged. SCOPED_TABLES keeps `enemy_kills`; delete/rename need no other work (join keys are (character,server), which both operations move together in `item_transactions` already in SCOPED_TABLES).

## 8. TEST DISPOSITION (kill_tracking_commands.rs tests mod - per item)

KEEP UNCHANGED: `kill_identity_...`, `boss_kill_times_...`, `death_veto_is_forward_only...`, `mine_total_kills_vetoes_...` (veto assertions must pass byte-identical - preservation pins), all import/export/parse tests.

REWRITE as join tests (same fixture shapes, assert via `mine_loot_rows`/`mine_total_kills` instead of the table): `loot_link_window_...` (inclusive endpoints), `loot_uses_latest_matching_kill_...` (newest-wins + re-query idempotence), `same_item_receipts_link_one_row_each` (D5-56: two receipts now TWO sum contributions, `times_dropped` 1), `kill_backlinks_only_...` (merge into window rewrite if redundant), `kill_replay_heals_...` (becomes late-receipt-visible-without-heal pin), `rescan_heal_over_two_kills_...` (D5-55 becomes receipt-attributes-to-K2-only pin), `link_history_is_pure_function_...` (becomes join-purity pin).

DELETE with reason (no heal/txn/link API left to pin): `receipt_identity_links_agree_and_heals_stay_silent` (`receipt_ts` gone), `kill_heal_rolls_back_when_a_row_fails` (readers are single SELECTs, no txn), `both_sides_agree_on_latest_eligible_kill` (no sides; covered by newest-wins rewrite), `loot_link_distinguishes_no_match_...` (no Ok(false) API; errors are query errors only).

UPDATE residual assertion: `same_second_twins_keep_two_ledger_rows_and_link_by_qty` - identical-qty twins now BOTH counted in SUM (the same-second-design section-5 residual is fixed by the join); keep the two-ledger-rows pin, change one-loot-row to two-summed.

`chat_status_dispatch.rs`: `json_receipt_links_loot_to_kill` - rewrite to assert ledger row with stamped entity + join-equivalent count 1, keep rescan-idempotence half (replay still 1 ledger row). `unlooted_kill_...` - keep shape, assert zero join rows instead of zero loot rows. All twin/guard tests stay (ledger untouched).

New pins: EXPLAIN QUERY PLAN or row-count-bounded pin that the join hits `idx_item_tx_kill_join` (no ledger full-scan per kill); newest-wins anti-join pin with three kills sharing one window.

## 9. DOCS DISPOSITION (per file - doc claims must move with behavior)

- `docs/features/screens/economics/economics-farming.md` - REWRITE kill-to-loot section (entity match to read-time join; the Player.log-corpse-search framing stays - that pipeline is already noted dead).
- `docs/plans/same-second-receipts-design.md` section 5 (Link layer deliberately unchanged) + section 8 option 3 - MARK SUPERSEDED by this design when the join lands (the interim residual it documents is fixed, not worked around).
- `docs/features/vip-json-signal-wishlist.md` (30 min to `enemy_kill_loot` row) - reword to join.
- Sweep receipt required in commit body: `grep -rn enemy_kill_loot docs/ src/ scripts/` - every hit accounted (migrations history + tests above stay deliberately; all else moves or goes with justification).

## 10. S2 DEPENDENCIES (designed against the declared contract - implementer verifies, not assumes)

- D1. Timestamp contract lands first (shipped as #363): space-format naive UTC in `enemy_kills.killed_at` AND `item_transactions.timestamp`, legacy T-family rows backfilled by v98. The design's tests use space-format fixtures only and assert nothing about time-only rows.
- D2. v94 UPDATE1/UPDATE2 retired (section 7); no kill chat-log backfill exists. Any third kill/loot backfill dies with the table too.
- D3. Type stability: `enemy_entity_id` stays TEXT, `from_entity_id` stays INTEGER, CAST join key unchanged.
- D4. Covering index is THIS slice (section 6), not S2 - the TODO's ~12 us/row assumes it; implementer re-measures on a live-size ledger and pastes EXPLAIN QUERY PLAN + timing in the commit body.
- D5. Ledger-guard halves already landed (#352 line identity + #359 english_claimed_by) - relied upon for per-receipt ledger rows. Twin-claim stamping preservation (section 4) is load-bearing for the join.
- Suggested commit-body evidence: before/after SUM-per-item census on a Dev DB copy (expect: some SUMs down = phantom-pin removal, twin SUMs up = residual fix, denominators flat).

## 11. BEHAVIOR DELTAS (all intended - each documented in the commit body)

- B1. Identical same-second same-qty twins both counted in SUM (residual fix; `times_dropped` unchanged - per kill either way).
- B2. Pinned orphan quantity disappears from SUM (phantom removal).
- B3. Denominators (`total_kills`) unchanged - kill rows and veto untouched in shape.
- B4. Rescan/late receipts need no heal pass - visible on next query; import/export/FE shapes unchanged (5 FE call sites, zero FE changes).
- B5. `player_prev_ingests.loot_added` (v48 ingest tracking): OPEN - implementer sweeps its writers; if it counts link rows, repoint to join counts or drop the column with a disposition (not traced - flagged, not assumed).

## 12. OPEN QUESTIONS (implementer resolves before/with landing)

- Q1: enumerate the three backfills in TODO:222 suspects - which, if any, beyond v94 UPDATE1/UPDATE2 touch kill/loot timestamps? (S2 found two kill/loot backfills + no kill chat-log backfill; the third is likely almanac/WoP timestamp backfills, out of scope.)
- Q2 (needs owner sign-off): join with NO context/source filter (exact heal replication, recommended) vs narrowing to `context='loot' AND source='chat_status`.
- Q3: confirm the next free migration version at implement time (rebase check; migration-chain lint enforces one new version per PR).
- Q4: `loot_added` column fate (B5 above).
- Q5: check-grid/census scripts referencing `enemy_kill_loot` - covered by the section-9 sweep receipt.

## 13. EVIDENCE BASE (files read, nothing written)

TODO.md + :219/:222 raw; `kill_tracking_commands.rs` (readers, writers, `get_enemy_kill_stats`, drop-sources, search, export, tests); `migrations.rs` (v94, v96/v97, v15 ledger schema, v44/45 kill schema, v48 dedup, dispatch head, v94/v96/v97 tests); `unknown_source_commands.rs` (whole); `chat_status_dispatch.rs` (kill arm, English guard, JSON guard, JSON dispatch, English arm, tests); `setup_commands.rs`; `lib.rs`; `chat_commands.rs`; `docs/plans/same-second-receipts-design.md` (whole); doc greps for `enemy_kill_loot` (3 hits); src/ FE grep (5 call sites); greps run: `enemy_kill_loot` / `get_enemy_kill_stats` / `sweep_unlinked_gains` / `receipt-link+record_enemy_kill` / ledger-covering-index / backfill-in-kill_tracking / SCOPED_TABLES / v48+zone+combat_skills alters / same-second family / FE readers / test fns / migration head (97).
