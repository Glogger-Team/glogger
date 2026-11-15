# Benchmark task (frozen — implement exactly, no scope changes)

## Target

File: `src/stores/characterStore.ts` — the ONLY file you may modify.

## Change

`pollForNewReports` invalidates the cross-character work-order cache
(`allCharacterWorkOrdersLoaded = false`, `Generation++`) when a new report
is imported, but never reloads it — so a `workOrderStateFor` consumer mounted
at import time (e.g. `QuestTooltip.vue`, `ItemTooltip.vue`) flips from data
to `null` permanently until remount. Fix the invalidate-then-reload cycle
in that function only:

1. In the `if (result)` branch, capture `const wasLoaded = allCharacterWorkOrdersLoaded.value` BEFORE the existing invalidation lines.
2. After the snapshot reload completes (after the `selectSnapshot(snapshots.value[0])` block, still inside `if (result)`), add:
   `if (wasLoaded) void loadAllCharacterWorkOrders()`
   — no `force` argument (`Loaded=false` already forces the re-read; the
   existing no-server clear and generation-retry guards inside
   `loadAllCharacterWorkOrders` / `fetchAllCharacterWorkOrders` cover scope
   and races). Do not invent new guards.
3. Keep the reader (`workOrderStateFor` loaded + server-match guards), the
   generation counters, `acceptHMRUpdate`, and all exports exactly as-is.

## Non-goals (do NOT do any of these)

- Do NOT add a test framework, test file, dependency, or npm script (this repo has no frontend unit harness — deliberately). Do NOT add a `game-state-updated` listener (no work-order domain exists; out of scope). Do NOT reload unconditionally (that spams `get_all_character_work_orders` when nobody consumes the cache) and do not touch any other file.

## Acceptance (all REQUIRED in this same turn)

- Print RUN-START as your first line, RUN-END as your last line.
- `npm run typecheck` (from repo root) is clean — full output returned. (There is no frontend unit runner; this task is scored on gates + review. Do not create one.)
- Return: the unified diff of your change (`git diff` + `git status --porcelain`, proving ONLY this file changed) plus the full typecheck output.
