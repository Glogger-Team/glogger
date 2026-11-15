# Benchmark task (frozen — implement exactly, no scope changes)

## Target

File: `src/stores/timerStore.ts` — the ONLY file you may modify.

## Change

A full timer backend exists (`get_user_timers` / `save_user_timer` /
`delete_user_timer` in `src-tauri/src/db/timer_commands.rs`, table
`user_timers`) but has zero frontend callers: this store is pure
localStorage. Switch persistence to the existing IPC, keeping every
behavioral surface identical:

1. Identity: read `useSettingsStore().settings.activeCharacterName` /
   `activeServerName` (same pattern as `pollForNewReports` in
   `characterStore.ts`); every invoke is keyed by active character+server.
   If no active character, `loadUserTimers` returns leaving timers as-is.
2. Time format: backend `started_at`/`paused_at` are local
   `"YYYY-MM-DD HH:MM:SS"` strings. Add module-level `parseLocal` /
   `formatLocal` helpers for exactly that shape (no new dependencies).
3. Mapping (lossless, in one place):
   - backend → store: `id: String(t.id)`, `durationSecs: t.duration_seconds`,
     running (`paused_at == null`): `endsAt: parseLocal(t.started_at) + t.duration_seconds*1000`, `pausedRemaining: null`;
     paused: `endsAt: null`, `pausedRemaining: max(0, round((parseLocal(t.paused_at) - parseLocal(t.started_at))/1000))`.
     `presetId` is local-only (backend has no column) — never sent, never overwritten by loads.
   - store → `SaveTimerInput`: `{ id: numeric id or null (parseInt, null if NaN), label, duration_seconds: durationSecs, started_at, paused_at, area_key: null, source: "manual" }` with
     `started_at`/`paused_at` derived per action: add/restart → `started_at: formatLocal(now)`, `paused_at: null`; pause → `paused_at: formatLocal(now)`, `started_at: formatLocal(now - (durationSecs - remaining)*1000)`; resume → `paused_at: null`, `started_at: formatLocal(now - (durationSecs - remaining)*1000)`.
4. Actions: `loadUserTimers()` (async, exported; `void loadUserTimers()` fire-and-forget at store setup so existing consumers are unaffected), `addTimer`/`pauseTimer`/`resumeTimer`/`restartTimer` persist via `save_user_timer` and adopt the returned row, `removeTimer` via `delete_user_timer(numericId)` (skip the invoke when the id is non-numeric, drop locally).
   On ANY invoke failure: `console.warn`, keep last-known timers (never wipe on failed load, never drop on failed remove).
5. Keep: presets (still localStorage), 1s tick + `startTicking`/`stopTicking`, `timersWithRemaining` sort, all existing exports, `acceptHMRUpdate` block.

## Non-goals (do NOT do any of these)

- No backend/Rust changes (commands, signatures, SQL all stay as-is), no migration, no changes to `TimerWidget.vue`, settings store, or any other file. Do not hardcode a character, do not change the store's public export names.

## Acceptance (all REQUIRED in this same turn)

- Print RUN-START as your first line, RUN-END as your last line.
- `npm run typecheck` (from repo root) is clean — full output returned. (`cargo clippy` runs harness-side to confirm the backend is untouched; do not run it.)
- Return: the unified diff of your change (`git diff` + `git status --porcelain`, proving ONLY this file changed) plus the full typecheck output.
