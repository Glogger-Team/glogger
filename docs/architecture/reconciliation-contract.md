# Reconciliation contract (dual-write convention)

Several domains track one quantity through two lanes: an absolute (authoritative but infrequent — character export, report import) and a delta (live but lossy — chat accrual, session accumulation). Every ad-hoc merge of the two has produced one bug class: double-counted or vanished gains when an export, rescan, or character switch lands mid-session.

## Rule

Do not unify the mechanism. Declare the reconciliation per domain, in four fields:

| Field | Meaning |
|---|---|
| Anchor source | Where the absolute comes from, and its cadence. |
| Delta source | Where live accrual comes from. |
| Conflict rule | Which side wins on disagreement, per surface. There is no merge that does not pick a loser — state the loser. |
| Re-anchor points | The enumerated moments the anchor refreshes (load/login, fresh export, rescan). A re-anchor is also a baseline decision: state whether in-flight session progress restarts or backfills. |

New dual-write domains declare all four before shipping. Existing domains migrate opportunistically — no retrofit program.

## Verification shape

Each declared contract carries the same pins the tree already uses per domain:

- Both-orders tests where arrival order varies (twin pattern: `json_additem_twins_converge_*`).
- A re-anchor pin (fresh export mid-session does not move the session total).
- A rescan-convergence pin (repeat runs converge without doubling).

## Worked example (conforming)

Council-wallet estimate (`game_state_commands.rs` `get_currency_estimate`, `gameStateStore.ts` `loadCurrencyEstimate`): anchor is `anchor_amount` at `anchor_at`, seeded by character export; delta is `delta_since` from chat; `estimated = anchor_amount + delta_since`. Anchor wins on fresh export; delta wins intra-session. Re-anchors at load/login and fresh export.

## Known instances

- Currency estimate — conforming (above).
- Statehelm weekly gift totals (`useStatehelmTracker.ts` `mergedGiftsThisWeek`, shipped in `b583e786`) — declared:
  - Anchor source: latest character snapshot's per-NPC `gifts_this_week` (VIP report `GiftsThisWeek`, `src-tauri/src/db/character_commands.rs:66-69`, stored per snapshot `src-tauri/src/db/character_commands.rs:313-328`) plus the snapshot's own timestamp; snapshots read latest-first (`src-tauri/src/db/character_commands.rs:490`). Authoritative for gifts given while glogger was closed, when the chat path is dead (`src/composables/useStatehelmTracker.ts:177-182`). Read in `loadGiftLog` via `get_character_snapshots` + `get_snapshot_npc_favor` (`src/composables/useStatehelmTracker.ts:493-507`).
  - Delta source: `game_state_gift_log` live rows (`get_gift_log`, `src-tauri/src/db/game_state_commands.rs:814-827`) — manual gifts (`src-tauri/src/db/game_state_commands.rs:845-865`) plus chat `DeltaFavor` gift detection on giftable (uncapped) NPCs (`src-tauri/src/chat_status_dispatch.rs:1299-1311`; capped Statehelm counts are manual-only, `src-tauri/src/chat_status_dispatch.rs:1292-1295`) — filtered to the current cap week (Monday 00:00 America/New_York, `src/composables/useStatehelmTracker.ts:151-160`) and to rows after the anchor (`src/composables/useStatehelmTracker.ts:212-224`).
  - Conflict rule: anchor wins up to the export stamp — live rows at or before it are already inside its counts and never re-added. Totals merge as baseline + post-export rows; the drill-down keeps live rows only and nothing is written back to the gift log (`src/composables/useStatehelmTracker.ts:226-251`).
  - Re-anchor points: every gift-log load re-reads the latest snapshot (`src/composables/useStatehelmTracker.ts:488-510`); a previous-week snapshot is ignored — the baseline applies only while `anchorDb >= weekStartDb`, reactive across the Monday reset without a reload (`src/composables/useStatehelmTracker.ts:232-238`); a fresh export becomes the anchor on the next load.

## Non-goals

- No shared merge implementation. Conflict rules differ per domain for real reasons (XP resets within-level balance on ding, currency does not; kills are append-only, inventory is replace-on-import; some domains have no absolute at all). One function would bury those differences.
- No per-commit gate. This convention is review-time only.
