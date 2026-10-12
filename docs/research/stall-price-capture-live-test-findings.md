# Stall Price Capture — live-test findings (2026-10-03, PAUSED/DROPPED)

> **Status note (2026-10-11):** the code shipped in v1.0.0+ anyway (the shelved
> fixes were committed as `3376d46`, and the capture panel + OCR commands are in
> the release tree). It is therefore labelled **experimental** in the UI
> (`StallPriceCapturePanel`, the Market tab's *Stall Price Observations*
> section) and in `docs/features/screens/economics/economics-market.md`, and the
> release notes for the versions that carried it no longer list it as a shipped
> feature. Everything below still describes unverified work.

**Status:** feature branch state committed to working tree but uncommitted to git.
User shelved the live testing of this method ("drop this entire method of price
updates for the moment"). Everything below is filed for a future session.

## What was being tested

`f53a53b` "feat: stall price capture for other players' stalls (capture panel
+ OCR)" — local-only commit on `main` (not pushed). Test harness: `npm run
tauri dev` (dev DB at `%APPDATA%\glogger.Dev`, V68 migrated, CDN items loaded)
alongside the live game.

## ✅ Verified working (live, TwinkleofToes/Dreva)

- **Panel appears** on browsing another player's stall — real log line caught
  live: `[02:19:02] LocalPlayer: ProcessPlayerVendorScreen(12841, "Remember:
  press SHIFT to buy multiples quickly.", …, 41, False, False, …)` — 5th arg
  `False` = someone else's stall. Panel opened bottom-right on the Market tab.
- **Typed capture (B1):** three lines saved → 3 rows in
  `stall_price_observations`: Cantaloupe Wine (resolved id 21890, 2500g),
  Carrot x80 (resolved 5307, 5g — `x<qty>` parsing works), ZzzBlorp Widget
  (unresolved, kept with NULL ids for correction). All attributed to the
  active character/server, source `capture`, stall "Unidentified Stall".
- **Dedup (B2):** re-saving identical lines → `Saved 0 prices`, row count
  unchanged (INSERT OR IGNORE on UNIQUE(observed_at, item, stall, price)).

## 🐛 Bug 1 (fixed): serde casing blocked Save

`record_stall_prices` rejected the FE payload:
`invalid args 'entries' for command 'record_stall_prices': missing field
'item_query'`. Rust `StallPriceEntry` lacked `#[serde(rename_all =
"camelCase")]` while the FE sends camelCase. Fix: added the attribute at
`src-tauri/src/db/stall_commands.rs:39` (repo convention, cf.
`stall_tracker_commands.rs:149`). Verified: `cargo test --lib` 58 pass for
stall modules; live Save then worked.

## 🐛 Bug 2 (fixed in code, live retest never done): purchases never detected

**Repro'd offline.** Live purchase sequence (CarrotWine, stall 12841):

```
[02:21:51] ProcessPlayerVendorScreen(12841, …)        ← stall opened
[02:23:19] ProcessAddItem(CarrotWine(279194804), -1, True)   ← purchase
[02:23:19] ProcessPlayerVendorScreenRemove(12841, 279194804) ← taken off stall
```

DB result: `item_transactions` row 15246 for CarrotWine with
`source_kind='unknown'`, context 'loot' — and **no** `stall_price_observations`
purchase row. Root cause: the coordinator's purchase detector
(`coordinator.rs` ~935-958) keys on `ItemAdded` carrying
`ItemProvenance::Attributed { source: VendorBrowsing { .. } }`, but that
activity context expires after **30s** (`DEFAULT_CONTEXT_LIFETIME_SECS = 30`
in `player_event_parser.rs:731`). Players browse stalls for minutes before
buying; nothing re-arms the deadline while the screen is open (no refresh
events), so provenance was `UnknownSource` by purchase time.

**Fix applied (uncommitted):** `parse_add_item` in
`src-tauri/src/player_event_parser.rs` (~1565-1592) now stamps a provisional
`VendorBrowsing` provenance on any new-instance `AddItem` while the
`vendor_screen` session is open (`Some((npc_id, false))` — the session
survives until `ProcessEndInteraction`, unlike the 30s activity context).
Offline repro (throwaway example, since deleted): sequence now yields
`ADDED CarrotWine provenance=Attributed(VendorBrowsing npc=12841,
Confident)` instead of `UnknownSource`. Full `cargo test --lib`: **582
passed, 0 failed** (includes all 6 vendor parser tests + 4 stall db tests).

**Open questions for the retest (never executed live):**
- Does a real purchase now write the `purchase` row with `price?` sentinel?
- Stacking case (C2): buying an item you already own emits AddItem with a
  different line shape (or no AddItem at all — verify with a live capture).
- Note: in the 02:23 purchase, the chat status line
  (`[Status] Corpse Lover's Carrot Wine added to inventory.` in Chat log)
  fed `consume_chat_gain_for_add_item` — displayed item name resolution came
  from the chat gain, good.

## Unverified checklist items (abandoned for now)

- C1/C2 live purchase retest after the fix (needs rebuild — dev exe predates
  both fixes)
- D1: NPC vendor purchase must NOT create a row (fixtures show NPC buys are
  NOT `ProcessPlayerVendorScreen*` lines at all — likely safe)
- D2: browsing without buying → no rows (expected safe)
- E: own stall (`isManager=True`, 5th arg) → silent (unit-tested, not live)
- F: OCR scan window (Tesseract portable download ~65MB into
  `%APPDATA%\glogger.Dev\tesseract\`, GDI BitBlt capture, trigram-Dice ≥0.75
  matching, scan_legacy full-image path — red-bar structured pass is future
  work). Entire OCR path never exercised on real hardware.
- G1/G2: tooltip "Last seen" line; curated `market_values` never overwritten
  (code guarantees it — observations are a separate table)

## State on disk at shelving

- Working tree: modified `src-tauri/src/db/stall_commands.rs` (serde fix) +
  `src-tauri/src/player_event_parser.rs` (provenance stamp fix) — **both
  uncommitted**. Base commit `f53a53b` is the feature itself.
- Dev DB has 3 test rows (ids 4-6, includes the fake "ZzzBlorp Widget") —
  delete when resuming or leave (observed_at purge will age them out).
- Local HEAD is 4 unpushed commits on `main` (stall-price feature,
  compliance gate, dependabot).

---

## Update (2026-10-03, later same day): OCR scan work — built, verified offline, then SHELVED

The OCR path was the only one of the "non-interactive price checking" methods
that got exercised. Three more bugs were found and fixed in code (all
uncommitted, working tree), and the whole pipeline was verified against a real
captured frame before shelving.

### Bug 3: `--psm 6` rejected by Tesseract CLI
`run_tesseract` passed `"--psm 6"` as ONE argv token; Tesseract requires two
(`--psm`, `6`). Live failure: `tesseract failed: Error, unknown command line
argument '--psm 6'` (the user's first Scan click). Fixed at
`stall_ocr.rs` run_tesseract: split into `.arg("--psm").arg(psm.to_string())`.

### Bug 4: the hardcoded OCR download URL is a 404
`OCR_DOWNLOAD_URL` points at
`UB-Mannheim tesseract/releases/download/5.5.0/tesseract-ocr-w64-setup-5.5.0.20241111.zip`
— that release never existed as a zip (repo has 4 releases; latest is a
5.4.0 `.exe`). "Install OCR engine…" would always fail. Worked around on this
machine by copying the system install (`C:\Program Files\Tesseract-OCR`,
v5.5.0.20241111) into `%APPDATA%\glogger.Dev\tesseract\`. **Still to fix in
code:** try the system install path, or point the URL at a real release asset.

### Bug 5 (the big one): full-image OCR is unusable at 3440×1440 — replaced
The pre-existing pipeline OCR'd the whole game window in one pass. On a live
3440×1440 frame the stall panel is one dark column among chat overlay/FPS
counter — full-image output yields ONE garbage pair ("can Oo ee ° @ 50008").
Diagnosed offline by capturing the same frame externally and replaying the
exact pipeline.

**What was built (all in `stall_ocr.rs`, verified against the real frame):**
1. `locate_panel` — find the stall panel as the widest dark-column run left of
   center (mean gray < 60), extend right edge by 16% of width (the dark-column
   run stops at the panel's internal scrollbar; price digits extend past it).
2. Binarize panel (gray > 100 → ink; stall text is bright-on-dark, panel bg ~37).
3. `detect_row_bands` — horizontal projection of bright pixels: bands with ≥3
   bright samples, gaps ≤4 rows merged, height ≥35px → one band per stall row.
4. Per-band two-column OCR: names x∈[1%,55%] of panel, prices x∈[89%,+2%]
   (skips the council-glyph column), names scale 3, prices scale 4, psm 7,
   ±4px vertical padding.
5. `extract_price` — first comma-grouped number (`6,000`), else digit runs
   ≥100. **Off-by-one bug found & fixed here during verification:** `end =
   j+3` read only `6,00` of `6,000` (comma at j, digits j+1..j+4 → must be
   `end = j+4`). This was invisible until the probe test; it silently turned
   6000 → 600.
6. Junk-row filter: overlay/HP-bar bands produce no CDN name match AND no
   price → dropped from the prefill.

**Result on the live frame (3440×1440, stall "Coast to Coast [CL-8]"):** 11
rows prefill, 9/13 prices exact (6000/9500/5500×3/5000/7000×4), 1 near-miss
price (7800 vs 2800 — digit confusion), 2 unreadable-price rows (95,000 →
'oscoo, 1,700 → 'sro04 — kept with price 0 for manual fix), names 11/11
resolve to the CDN (one near-miss "Yodka" matches at 0.83). vs 1/13 garbage
before. Full `cargo test --lib` 582 passed after every change.

### Also fixed while here
- Removed the now-dead `capture_window_bmp` wrapper (scan_window captures
  BGRA directly for the panel path).
- `parse_add_item` purchase provenance stamp (bug 2 above) unchanged and
  verified live: a real stall purchase (Quality Phlogiston, stall 12190)
  produced the `purchase` observation row with the `price?` sentinel, and
  resolved display/internal/CDN id correctly. NOTE: that purchase emitted no
  `ProcessPlayerVendorScreenRemove` — only `ProcessPlayerVendorScreenUpdate`
  — so the Remove-line pairing in the comment is not the only purchase shape;
  the provenance-stamp approach is what caught it.

### State at shelving (stall-capture + OCR both dropped for now)
- Working tree (all uncommitted, base = `f53a53b` feature commit):
  - `src-tauri/src/db/stall_commands.rs` — serde camelCase fix (bug 1)
  - `src-tauri/src/player_event_parser.rs` — purchase provenance stamp (bug 2)
  - `src-tauri/src/stall_ocr.rs` — psm fix (bug 3) + panel-scan pipeline (bug 5)
  - `src-tauri/src/lib.rs` — temporary re-export removed back to clean; only
    whitespace-level changes remain
- 582 tests pass; `cargo check` clean; throwaway example + frame file deleted.
- Live retest of the new OCR prefill in the app was never done (needs rebuild).
- The dev DB has 1 row: the Phlogiston purchase (price?), safe to delete.
