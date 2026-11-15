# Benchmark task (frozen — implement exactly, no scope changes)

## Target

- CREATE `src/components/Dashboard/widgets/ActiveEffectsWidget.vue`
- MODIFY `src/components/Dashboard/dashboardWidgets.ts` (one import + one registry entry)

These are the ONLY files you may modify.

## Change

New read-only "Active Effects" widget over the existing store domain:

1. `ActiveEffectsWidget.vue` (`<script setup>`):
   - Reads `useGameStateStore()` (from `src/stores/gameStateStore.ts`) — no `invoke`, no `listen`, no local fetching. Reactivity comes from the store's existing global `game-state-updated` → `refreshDomain('effects')` path.
   - Renders `store.namedEffects` sorted ascending by `effect_name` (null-safe; `namedEffects` already excludes nulls).
   - One row per effect: `effect_name` + `formatAnyTimestamp(last_confirmed_at)` (`formatAnyTimestamp` from `src/composables/useTimestamp.ts`).
   - Header shows the count; empty state text exactly `"No active effects."`.
2. Registry entry appended at the END of `DASHBOARD_WIDGETS` (not inserted elsewhere):
   `{ id: "active-effects", name: "Active Effects", component: ActiveEffectsWidget, defaultSize: "small" }`
   plus the matching `import ActiveEffectsWidget from "./widgets/ActiveEffectsWidget.vue";`.

## Non-goals (do NOT do any of these)

- No `invoke('...')`, no `game-state-updated` listener, no new store, no backend/Rust changes, no migration, no router changes, no edits to `DashboardView.vue`, `gameStateStore.ts`, or any other file. No `effect_name!` non-null assertion (sort null-safe).

## Acceptance (all REQUIRED in this same turn)

- Print RUN-START as your first line, RUN-END as your last line.
- `npm run typecheck` (from repo root) is clean — full output returned.
- `npm run build` (from repo root) succeeds — tail of output returned.
- Return: the unified diff of your change (`git diff` + `git status --porcelain`) plus the full typecheck output and build tail.
