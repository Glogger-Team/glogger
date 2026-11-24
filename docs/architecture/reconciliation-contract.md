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
- Session-vs-absolute progress semantics (TODO.md VIP JSON backlog) — open instance #1. The B-round entry keeps a session baseline and absolute counters distinct but names no mechanism; detail is unrecoverable from the entry alone. Re-derive the mechanism before touching; any merge needs the four fields above as a spec row first.

## Non-goals

- No shared merge implementation. Conflict rules differ per domain for real reasons (XP resets within-level balance on ding, currency does not; kills are append-only, inventory is replace-on-import; some domains have no absolute at all). One function would bury those differences.
- No per-commit gate. This convention is review-time only.
