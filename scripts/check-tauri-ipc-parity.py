#!/usr/bin/env python3
"""Gate: Tauri IPC parity for window-shaped stock replays.

Every stock-replaying query over `stall_events` (anything feeding an
in-stock set, phantom-stock-sensitive by construction) must honour the
same History-window predicate the Inventory view owns:

    AND (action = 'bought' OR event_at >= date('now', 'localtime', ...))

A replay without the window replays ancient `added` lines for
long-delisted items (the game logs no `removed` on takedown) as phantom
stock forever — live 2026-11 case: full history showed 110 phantom rows
for a 41-slot stall; a 100-day window showed 47. Caught once by review
(PR #181 F2: `preview_stall_recount` diffed a windowed Inventory set
against an unwindowed in-stock set); this gate pins the class.

Pure-text scan, stdlib only. Scope is deliberately narrow — a query is
in scope only when ALL of these hold:

- selects the full replay row shape (`action` + `quantity` columns), and
- carries the ledger predicate `action != 'unknown'`, and
- orders by `event_at ASC` (replay order; newest-first exports, paginated
  lists, bought-only revenue, and DISTINCT filter dropdowns are NOT
  replays and stay out of scope).

In-scope queries must carry the window predicate above. The full-history
arm of a matched window/full-history pair is the sanctioned exception:
it must sit in the same function as its windowed twin (matched here by
file order — the windowed query precedes it).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"

LEDGER_RE = re.compile(r"action\s*!=\s*'unknown'")
REPLAY_SHAPE_RE = re.compile(
    r"SELECT\b[^;]*?\baction\b[^;]*?\bquantity\b", re.IGNORECASE | re.DOTALL
)
REPLAY_ORDER_RE = re.compile(r"ORDER\s+BY\s+event_at\s+ASC", re.IGNORECASE)
WINDOW_RE = re.compile(
    r"\(\s*action\s*=\s*'bought'\s+OR\s+event_at\s*>=\s*date\(\s*'now'",
    re.IGNORECASE,
)
# Multi-line SQL string: SELECT ... FROM stall_events ... up to the
# closing quote or semicolon. DOTALL so the ledger/window predicates
# match even when the arms live on different lines.
QUERY_RE = re.compile(
    r"SELECT\b.*?FROM\s+stall_events\b.*?(?:\"|;|$)",
    re.IGNORECASE | re.DOTALL,
)


def sql_text_of(path: Path) -> str:
    # CRLF-safe: predicates never depend on line endings.
    return path.read_text(encoding="utf-8")


def findings_for(path: Path) -> list[str]:
    findings: list[str] = []
    try:
        text = sql_text_of(path)
    except OSError:
        print(f"ipc-parity: cannot read {path}", file=sys.stderr)
        return ["unreadable"]
    matches = list(QUERY_RE.finditer(text))
    # Sanctioned-exception pairing: a full-history arm is fine when a
    # windowed twin precedes it in the same file (the `match window_days`
    # shape `get_stall_inventory` / `preview_stall_recount` own).
    windowed_before = [bool(WINDOW_RE.search(m.group(0))) for m in matches]
    seen_windowed = False
    for m, is_windowed in zip(matches, windowed_before):
        seen_windowed = seen_windowed or is_windowed
        query = m.group(0)
        if not LEDGER_RE.search(query):
            continue  # Not a ledger replay (counts, distincts, deletes).
        if not REPLAY_SHAPE_RE.search(query):
            continue  # Not a stock replay (exports, revenue, dropdowns).
        if not REPLAY_ORDER_RE.search(query):
            continue  # Not replay order (newest-first, paginated lists).
        if is_windowed:
            continue
        if seen_windowed:
            continue  # Full-history arm of a matched window/full pair.
        lineno = text.count("\n", 0, m.start()) + 1
        rel = path.relative_to(ROOT)
        findings.append(
            f"ipc-parity: {rel}:{lineno} stock replay over "
            "stall_events has no History-window predicate "
            "(action = 'bought' OR event_at >= date('now'...) — "
            "unwindowed replays surface phantom stock"
        )
    return findings


def main() -> int:
    if not SRC.is_dir():
        print("ipc-parity: src-tauri/src missing", file=sys.stderr)
        return 2
    fail = 0
    for path in sorted(SRC.rglob("*.rs")):
        for finding in findings_for(path):
            print(finding)
            fail = 1
    return fail


if __name__ == "__main__":
    sys.exit(main())
