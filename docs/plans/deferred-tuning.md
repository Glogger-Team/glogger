# Deferred Tuning — `feat/books-watcher`

Items found during the 2026-10-04 review round (commit `d1533ec` and
predecessors) that are deliberate, documented deferrals: each one needs
real-world data or a subjective call before it can be tuned. Work them
one at a time while using the app day-to-day.

Related plan files: none yet — promote an item to its own
`docs/plans/*.md` if it grows beyond a checklist.

---

## 1. Rez dedup 30-second window — needs real-log calibration

**Where:** `src-tauri/src/coordinator.rs` — `rez_within_dedup_window`
(`WINDOW = 30s`), guard at the `parse_resuscitate_message` dispatch
(~L1500), tests in `mod rez_dedup_tests`.

**Why deferred:** The guard suppresses the second phrasing of one rez
("X resuscitates Y" → "Y comes back to life!") only if the pair is ≤30s
apart and forward-ordered. Both assumptions are unverified against real
game behavior — tuning requires a capture of actual paired lines.

**Concrete open items:**
- [ ] Capture a real session containing at least one rez pair; measure
      the actual timestamp gap between the two phrasings. If any pair
      exceeds 30s, widen `WINDOW` (there's no re-rez-cooldown evidence
      either way yet, so the ceiling is unknown).
- [ ] Backward-ordered pairs fail OPEN (both rows persist): if the
      paired line's timestamp is *earlier* than the first persisted
      event, `rez_within_dedup_window` returns false (guard is
      forward-only, documented as deliberate in the code + test
      `distinct_events_and_malformed`). Fix options: make the window
      bidirectional (`|cur - prev| <= WINDOW`), or first prove chat-log
      timestamps are monotonic within a file and document that as an
      invariant instead.
- [ ] Symptom to watch for: duplicate rez rows / inflated "times rezzed"
      counts in the death-tracker widget. `character_resuscitations` has
      no UNIQUE key, so dupes are silent — cosmetic only, no corruption.
- [ ] Same-minute-bucket behavior intentionally gone: the old
      `(target, minute)` scheme suppressed pairs up to ~60s apart and
      straddling minute boundaries were MISSED; the new rolling window
      fixes straddles but shortens coverage. If capture shows gaps of
      30–60s, consider a 60s window as the old-equivalent compromise.

**How to verify after changing:** `cargo test --lib rez_dedup_tests`
(adjust window-boundary assertions if `WINDOW` changes).

---

## 2. Brewing bulk scan — memory vs. report volume

**Where:** `src-tauri/src/db/brewing_commands.rs` — Phase 1 of
`scan_all_snapshots_for_brewing` materializes every report's full
`raw_json` + all parsed `BrewingItem` vecs simultaneously (previously
one report at a time). The all-or-nothing Phase 3 transaction is an
improvement and should be kept regardless.

**Why deferred:** Memory scales with total report JSON size. Whether
that matters depends on real report counts/sizes on the user's
machine — pure tuning, no correctness bug.

**Concrete open items:**
- [x] **Measured 2026-10-13 — leave as-is.** On the real dev DB
      (`%APPDATA%/glogger.Dev/glogger.db`, 149 MB): **12 report rows,
      158 KB average `raw_json`, 1.79 MB total** — an order of magnitude
      under the "low tens of MB" threshold, so Phase 1's all-at-once
      materialization stays. Re-measure with
      `SELECT COUNT(*), AVG(LENGTH(raw_json)), SUM(LENGTH(raw_json))/1048576.0
      FROM character_item_snapshots` if report volume grows.
- [ ] If it grows past that: batch the scan (transaction per ~50
      reports, parsing inside each batch window). Keep the
      error-all-or-nothing semantics per batch, and keep Phase 1's
      read-connection scoping (read conns dropped before any `await`
      — that pattern was verified safe; don't regress it).
- [x] TOCTOU between read/write phases is nominal (values captured
      once, report rows are write-once) — no action needed, noted
      here so nobody "fixes" it into a re-read later.

---

## 3. ChatMessageList activeDay scroll reset — nextTick race

**Status: superseded 2026-10-12 (Session 38, `d4b83a0` + `a54817d`).** The day-log jump rewrite
replaced this path — see "pre-flush watchers see stale props" in that session's HANDOFF entry. Current
shape in [ChatMessageList.vue](../../src/components/Chat/ChatMessageList.vue):

- The `dateNav.activeDay` watcher is `{ flush: 'post' }`, so it *does* see the jump's `highlightId`
  prop, and it returns early for a highlight (the row's own scroll-into-view owns the position).
- A `dayResetPending` flag hands the position to the `messages` watcher on the next window arrival, so
  the parent's late `messages` assignment can no longer re-clamp a reset that fired against the old
  scroll height — which is the failure this item described.

Items below are kept for reference only; re-verify live before touching this path again (the app has
no test for it). The "no scroll fight" constraint still holds and is now enforced by that flag.

**Where:** `src/components/Chat/ChatMessageList.vue` — the `watch` on
`props.dateNav?.activeDay` (~L330) that resets `scrollTop = 0` after a
day jump / "Back to Live".

**Why deferred:** The watcher awaits a single `nextTick()` before
resetting, but the parent (ChatSearchView) assigns `messages` only
after its own `await invoke(...)` resolves. If the parent's assignment
lands *after* this watcher's nextTick fires, the reset hits the OLD
scroll height and the newly rendered list can re-clamp `scrollTop` to
non-zero — day-jump would land mid-list. Intermittent by nature; not
statically provable. Needs live observation.

**Concrete open items (all moot — kept for reference):**
- [x] Repro attempt: slow network/dev-tools throttling + jump to a day
      far from the current window; check whether the list sometimes
      opens mid-list instead of at the top. Desc AND asc sort orders
      (top of window is the anchor either way — verified).
- [x] If it reproduces: preferred fix is watching both sources —
      `watch([() => props.dateNav?.activeDay.value, () => props.messages],
      ...)` so the reset runs only after BOTH change; alternative is
      double `nextTick`. **Not needed** — the `dayResetPending` flag is
      the chosen mechanism instead.
- [x] Known-good today (verified in review): no live conflict with the
      `messages.length`/`highlightId` watchers (no view passes
      `auto-scroll`; context mode clears highlightId before the day
      jump). If a future view passes both `auto-scroll` and `date-nav`,
      re-check ordering here.
- [x] Do NOT add a scroll fight: any fix must keep the
      "scroll to highlight" watcher winning when `highlightId` is set.
      **Still true** — the early `return` on `highlightId` is the guard.
