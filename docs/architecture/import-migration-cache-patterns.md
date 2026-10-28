# Import, Migration, Cache & Error Patterns

Lessons from the 2026-10 burn-down (PR #54 review). Follow these when touching
report imports, schema migrations, frontend caches, or fallible commands.

## 1. Duplicate imports must backfill new columns

`import_*_report_internal` uses `INSERT ... ON CONFLICT DO NOTHING` keyed on
`(character, server, timestamp)`. A re-imported report returns `was_duplicate:
true` and inserts nothing. Any migration that adds a column to an import-fed
table therefore leaves existing rows NULL **unless** one of these runs:

- (a) the migration itself replays `raw_json` into the new columns
  (`migration_v81_snapshot_item_details` is the template: parse each stored
  report's `Items` array, `UPDATE ... SET col = COALESCE(col, ?)` per row,
  best-effort `.ok()` per item so one bad row never fails the migration); or
- (b) the duplicate-import path refreshes the columns from the file
  (same COALESCE fill-only shape inside the `changes == 0` branch of
  `import_inventory_report_internal`).

Do both when the data matters: (a) covers users who never re-import, (b)
covers users who do. Fill-only (`COALESCE`) so a stale export never clobbers
newer values. Regression tests: `duplicate_import_backfills_v81_detail_columns`
+ `duplicate_import_backfill_never_clobbers_newer_values` +
`v81_backfills_detail_columns_from_stored_raw_json`.

## 2. Cross-server / post-import caches must key + invalidate

A Pinia cache shaped `loaded: boolean + rows` with no server key goes stale on
server switch and after any import that changes the underlying rows
(`loadAllCharacterWorkOrders` shipped exactly this bug). The pattern:

- keep `allXServer: string | undefined` beside `allXLoaded`;
- early-return only when `loaded && cacheServer === currentServer && !force`;
- set both on success;
- reset `loaded = false` after every import path that writes the source
  tables (manual import + `initForActiveCharacter` auto-import at minimum).

Mirror: `loadAllCharacterItems` has the same unkeyed shape — fixing one without
the other exceeds proportionate rigor, but do not copy the shape for new caches.

## 3. Fallible commands surface errors to the UI

Backend commands return human-readable `Err(String)`. A store action that
catches and returns `null` with only `console.error` leaves the user clicking a
button that does nothing (`recordUse` shipped this). The pattern: a nullable
`recordError`-style ref, cleared at the start of the attempt, set to
`String(err)` in catch, exported from the store, rendered beside the control
(`store.recordError` under the Record button). Clear it on the next attempt so
stale errors do not linger. `startSession`/`endSession` predate this pattern
and stay as-is (out of scope, not a defect).

## 4. Migration tests must simulate pre-migration state

`run_migrations_is_idempotent_and_atomic` pins the head version and must be
bumped with every new migration (80 → 81). New-migration tests must build the
PRE-migration schema by hand (CREATE TABLE without the new columns), never call
`run_migrations` first and re-run the migration fn — the ALTERs are not
idempotent and `duplicate column name` fails the test. One hand-built snapshot
+ item row with keys in `raw_json` is enough to prove the backfill.
