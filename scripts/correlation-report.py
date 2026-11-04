#!/usr/bin/env python3
"""Bi-directional correlation report between chat [Status] "added to inventory" messages
and Player.log inventory gain events, across paired datasets.

Python port of scripts/correlation_report.cjs (stdlib only).

Usage:
  python correlation-report.py <chat_log> <player_log> [--tz=-7] [--window=2] [--no-lists]
  python correlation-report.py --all                        # run all paired datasets

Name-map resolution: the original hardcoded
`e:/glogger/docs/samples/CDN-full-examples/items.json`. The port resolves
`docs/CDN-full-examples/items.json` relative to the repo root, overridable by
argv (`--items-json=...`) or env (`GLOGGER_ITEMS_JSON`); a missing file is a
clear stderr + exit 1 (never a silent empty map).
"""

import json
import math
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

DEFAULT_ITEMS_JSON = ROOT / "docs" / "CDN-full-examples" / "items.json"

CHAT_LINE_RE = re.compile(
    r"^(\d{2})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2}):(\d{2})\s+\[Status\]\s+(.+?)\s+added to inventory\.?$"
)
CHAT_QTY_RE = re.compile(r"^(.*?)\s+x(\d+)$")
TZ_RE_LOOSE = re.compile(r"Timezone Offset ([+\-]?\d{2}):(\d{2}):\d{2}")
TS_RE = re.compile(r"^\[(\d{2}):(\d{2}):(\d{2})\]")
ADD_ITEM_RE = re.compile(r"ProcessAddItem\((\w+)\((\d+)\),\s*(-?\d+),\s*(True|False)\)")
UPDATE_CODE_RE = re.compile(r"ProcessUpdateItemCode\((\d+),\s*(\d+),\s*(True|False)\)")
REMOVE_VAULT_RE = re.compile(
    r"ProcessRemoveFromStorageVault\(\d+,\s*-?\d+,\s*(\d+),\s*(\d+)\)"
)


def parse_args(argv=None):
    if argv is None:
        argv = sys.argv[1:]
    window = 2
    tz = None
    no_lists = False
    all_flag = False
    items_json = None
    positional = []
    for a in argv:
        if a == "--all":
            all_flag = True
        elif a == "--no-lists":
            no_lists = True
        elif a.startswith("--window="):
            window = js_parse_int(a[len("--window=") :])
            if isinstance(window, float) and math.isnan(window):
                print("error: --window must be a number", file=sys.stderr)
                sys.exit(1)
        elif a.startswith("--tz="):
            tz = js_parse_float(a[len("--tz=") :])
            if isinstance(tz, float) and math.isnan(tz):
                print("error: --tz must be a number", file=sys.stderr)
                sys.exit(1)
        elif a.startswith("--items-json="):
            items_json = a[len("--items-json=") :]
        else:
            positional.append(a)
    if items_json is None:
        items_json = os.environ.get("GLOGGER_ITEMS_JSON")
    return {
        "window": window,
        "tz": tz,
        "no_lists": no_lists,
        "all": all_flag,
        "items_json": items_json,
        "positional": positional,
    }


def js_parse_int(s):
    # Mirror JS parseInt(s, 10): leading whitespace, optional sign, leading
    # digits; NaN when no leading digits.
    m = re.match(r"^\s*[+\-]?\d+", s)
    if not m:
        return float("nan")
    try:
        return int(m.group(0).strip(), 10)
    except ValueError:
        return float("nan")


def js_parse_float(s):
    # Mirror JS parseFloat: longest valid float prefix; NaN when none.
    m = re.match(r"^\s*[+\-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+\-]?\d+)?", s)
    if not m:
        return float("nan")
    try:
        return float(m.group(0).strip())
    except ValueError:
        return float("nan")


def js_to_fixed_1(x):
    # Mirror JS Number.prototype.toFixed(1) (round-half-away on the decimal
    # representation), not Python's round-half-even format().
    if math.isnan(x):
        return "NaN"
    if math.isinf(x):
        return "Infinity" if x > 0 else "-Infinity"
    sign = "-" if x < 0 else ""
    x = abs(x)
    scaled = math.floor(x * 10 + 0.5 + 1e-9)
    return f"{sign}{scaled // 10}.{scaled % 10}"


def fmt_num(value):
    # JS template `${value}` prints -7 not -7.0; mirror that for int-valued floats.
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)


def pad_end(s, n):
    s = str(s)
    return s if len(s) >= n else s + " " * (n - len(s))


def pad_start(s, n):
    s = str(s)
    return s if len(s) >= n else " " * (n - len(s)) + s


# ========== Parsing ==========


def parse_chat_line(line):
    line = line.removesuffix("\r")
    m = CHAT_LINE_RE.match(line)
    if not m:
        return None
    yy, mm, dd, hh, mi, ss, payload = m.groups()
    hms = int(hh, 10) * 3600 + int(mi, 10) * 60 + int(ss, 10)
    qm = CHAT_QTY_RE.match(payload)
    item = qm.group(1) if qm else payload
    qty = int(qm.group(2), 10) if qm else 1
    return {
        "date": f"{yy}-{mm}-{dd}",
        "hms": hms,
        "raw_ts": f"{hh}:{mi}:{ss}",
        "item": item,
        "qty": qty,
        "raw": line,
    }


def detect_chat_tz(chat_lines):
    for ln in chat_lines[:20]:
        m = TZ_RE_LOOSE.search(ln)
        if m:
            sign = -1 if m.group(1).startswith("-") else 1
            h = int(m.group(1).replace("-", "").replace("+", ""), 10)
            mins = int(m.group(2), 10)
            return sign * (h + mins / 60)
    return None


def parse_player_line(line):
    line = line.removesuffix("\r")
    ts_match = TS_RE.match(line)
    if not ts_match:
        return None
    hms = (
        int(ts_match.group(1), 10) * 3600
        + int(ts_match.group(2), 10) * 60
        + int(ts_match.group(3), 10)
    )
    raw_ts = f"{ts_match.group(1)}:{ts_match.group(2)}:{ts_match.group(3)}"

    m = ADD_ITEM_RE.search(line)
    if m:
        return {
            "kind": "AddItem",
            "hms": hms,
            "raw_ts": raw_ts,
            "name": m.group(1),
            "id": m.group(2),
            "slot": int(m.group(3), 10),
            "is_new": m.group(4) == "True",
            "raw": line,
        }
    m = UPDATE_CODE_RE.search(line)
    if m:
        encoded = int(m.group(2), 10)
        return {
            "kind": "UpdateItemCode",
            "hms": hms,
            "raw_ts": raw_ts,
            "id": m.group(1),
            "encoded": encoded,
            # JS `>>>` is an unsigned 32-bit shift; encoded values fit in
            # int32 range so `>>` gives the same result here.
            "stack_actual": (encoded % 0x100000000 >> 16) + 1,
            "type_id": encoded & 0xFFFF,
            "from_server": m.group(3) == "True",
            "raw": line,
        }
    m = REMOVE_VAULT_RE.search(line)
    if m:
        return {
            "kind": "RemoveFromStorageVault",
            "hms": hms,
            "raw_ts": raw_ts,
            "id": m.group(1),
            "quantity": int(m.group(2), 10),
            "raw": line,
        }
    return None


# Walk the player log, computing per-instance stack history and producing
# "gain events" — occasions where items entered the player's inventory.
def build_player_gain_events(log_path):
    with open(log_path, encoding="utf-8") as fh:
        content = fh.read()
    lines = content.split("\n")
    events = []
    name_by_id = {}
    last_stack = {}

    for line in lines:
        if "LocalPlayer: Process" not in line:
            continue
        ev = parse_player_line(line)
        if not ev:
            continue

        if ev["kind"] == "AddItem":
            name_by_id[ev["id"]] = ev["name"]
            if ev["is_new"] and ev["slot"] < 0:
                # Genuine new stack. Seed=1, count as +1 gain (we don't yet know the real qty).
                last_stack[ev["id"]] = 1
                events.append(
                    {
                        **ev,
                        "gain_qty": 1,
                        "resolved_name": ev["name"],
                        "gain_kind": "AddItem-new",
                    }
                )
            elif ev["is_new"] and ev["slot"] >= 0:
                # Storage withdrawal — defer; pair with RemoveFromStorageVault below.
                events.append(
                    {
                        **ev,
                        "gain_qty": 0,
                        "resolved_name": ev["name"],
                        "gain_kind": "AddItem-withdrawal",
                    }
                )
            else:
                # Session load; no gain.
                # Don't emit a gain event at all.
                pass
        elif ev["kind"] == "UpdateItemCode":
            name = name_by_id.get(ev["id"])
            prior = last_stack.get(ev["id"])
            delta = 0
            gain_kind = None
            if prior is not None:
                delta = ev["stack_actual"] - prior
                gain_kind = (
                    "UpdateItemCode-gain"
                    if delta > 0
                    else ("UpdateItemCode-loss" if delta < 0 else "UpdateItemCode-same")
                )
            else:
                gain_kind = "UpdateItemCode-baseline"
            last_stack[ev["id"]] = ev["stack_actual"]
            # Only emit events when there's a positive gain or a baseline establishment
            if delta > 0 or gain_kind == "UpdateItemCode-baseline":
                events.append(
                    {
                        **ev,
                        "gain_qty": max(0, delta),
                        "delta_signed": delta,
                        "resolved_name": name,
                        "gain_kind": gain_kind,
                        "prior_stack": prior,
                    }
                )
        elif ev["kind"] == "RemoveFromStorageVault":
            name = name_by_id.get(ev["id"])
            # Vault qty = the gain. Seed stack if not already.
            if ev["id"] not in last_stack:
                last_stack[ev["id"]] = ev["quantity"]
            events.append(
                {
                    **ev,
                    "gain_qty": ev["quantity"],
                    "resolved_name": name,
                    "gain_kind": "RemoveFromStorageVault",
                }
            )

    return {"events": events, "name_by_id": name_by_id}


# ========== Name matching ==========
# Chat uses display names (e.g., "Great Phlogiston", "Master Armor Patch Kit").
# Player.log uses internal names (e.g., "Phlogiston7", "ArmorPatchKit5").
# Build an authoritative map from items.json.


def load_name_map(items_json_path):
    with open(items_json_path, encoding="utf-8") as fh:
        items = json.load(fh)
    m = {}  # displayNameLower → set(internalName)
    for v in items.values():
        if not v or not isinstance(v, dict):
            continue
        internal = v.get("InternalName")
        display = v.get("Name")
        if not internal or not display:
            continue
        key = display.lower()
        m.setdefault(key, set()).add(internal)
    return m


def resolve_name_map(items_json_opt):
    # Resolution order: --items-json argv, GLOGGER_ITEMS_JSON env, repo default.
    # Missing file is a clear stderr + exit 1 (never a silent empty map).
    # An explicitly passed path is used verbatim; anything else resolves
    # relative paths against the repo root.
    if items_json_opt:
        p = Path(items_json_opt)
        if not p.is_absolute():
            p = ROOT / p
    else:
        p = Path(DEFAULT_ITEMS_JSON)
    if not p.is_file():
        print(
            f"correlation-report: items.json not found at {p} "
            f"(pass --items-json=<path> or set GLOGGER_ITEMS_JSON)",
            file=sys.stderr,
        )
        sys.exit(1)
    try:
        return load_name_map(str(p))
    except (OSError, ValueError) as e:
        print(
            f"correlation-report: could not load items.json from {p}: {e}",
            file=sys.stderr,
        )
        sys.exit(1)


def names_might_match(display_name, internal_name, display_to_internal=None):
    if not internal_name:
        return False
    if display_to_internal is None:
        display_to_internal = {}
    candidates = display_to_internal.get(display_name.lower())
    if candidates and internal_name in candidates:
        return True

    # Fallback heuristic for items not in the static CDN snapshot
    def norm(s):
        return re.sub(r"\d+$", "", re.sub(r"['\s\-]", "", s)).lower()

    a = norm(display_name)
    b = norm(internal_name)
    if a == b:
        return True
    if b in a and len(b) >= 4:
        return True
    return a in b and len(a) >= 4


# ========== Correlation ==========


def correlate_chat_with_player(
    chat_events, player_gain_events, tz_shift_secs, window_secs, display_to_internal
):
    # Each chat entry is assigned 0..N player.log events; each player.log event
    # can be assigned to at most one chat entry (best-match greedy).
    chat_matches = []
    for ce in chat_events:
        chat_utc_secs = ce["hms"] + tz_shift_secs
        chat_utc_mod = ((chat_utc_secs % 86400) + 86400) % 86400
        chat_matches.append(
            {"chat": ce, "chat_utc_mod": chat_utc_mod, "candidates": [], "assigned": []}
        )

    # Build candidate list per chat entry
    for cm in chat_matches:
        for i, pe in enumerate(player_gain_events):
            if not names_might_match(
                cm["chat"]["item"], pe["resolved_name"], display_to_internal
            ):
                continue
            d = abs(pe["hms"] - cm["chat_utc_mod"])
            if d > 43200:
                d = 86400 - d
            if d <= window_secs:
                cm["candidates"].append({"idx": i, "pe": pe, "dist": d})

    # Greedy exclusive assignment, preferring: (1) exact qty match, (2) closest timestamp
    claimed = set()
    # Pass 1: exact qty + time match
    for cm in chat_matches:
        exact = sorted(
            (
                c
                for c in cm["candidates"]
                if c["idx"] not in claimed and c["pe"]["gain_qty"] == cm["chat"]["qty"]
            ),
            key=lambda c: c["dist"],
        )
        if len(exact) > 0:
            cm["assigned"].append(exact[0])
            claimed.add(exact[0]["idx"])
    # Pass 2: try to find a subset whose gainQty sums to chat qty
    for cm in chat_matches:
        if len(cm["assigned"]) > 0:
            continue
        avail = [c for c in cm["candidates"] if c["idx"] not in claimed]
        if len(avail) == 0:
            continue
        subset = find_subset_summing_to(avail, cm["chat"]["qty"])
        if subset:
            for c in subset:
                cm["assigned"].append(c)
                claimed.add(c["idx"])
    # Pass 3: assign single nearest candidate even if qty mismatch (marks as mismatch)
    for cm in chat_matches:
        if len(cm["assigned"]) > 0:
            continue
        avail = [c for c in cm["candidates"] if c["idx"] not in claimed]
        if len(avail) > 0:
            avail.sort(key=lambda c: c["dist"])
            cm["assigned"].append(avail[0])
            claimed.add(avail[0]["idx"])

    return {"chat_matches": chat_matches, "claimed": claimed}


# Small subset-sum helper (up to ~6 events in a time window — fine for exhaustive search)
def find_subset_summing_to(items, target):
    if len(items) > 12:
        items = items[:12]  # safety cap
    n = len(items)
    for mask in range(1, 1 << n):
        total = 0
        subset = []
        for i in range(n):
            if mask & (1 << i):
                total += items[i]["pe"]["gain_qty"]
                subset.append(items[i])
        if total == target:
            return subset
    return None


# ========== Report ==========


def rel(path_str):
    # Normalize absolute paths to repo-relative in output before comparing.
    # Node prints the argv verbatim (forward slashes); keep relative input
    # text as-is and only relativize absolute paths, with forward slashes.
    if os.path.isabs(path_str):
        try:
            return Path(os.path.relpath(path_str, ROOT)).as_posix()
        except ValueError:
            return path_str
    return path_str


def run_report(chat_path, log_path, opts, display_to_internal):
    with open(chat_path, encoding="utf-8") as fh:
        chat_lines = fh.read().split("\n")
    tz = opts["tz"]
    if tz is None:
        tz = detect_chat_tz(chat_lines)
    if tz is None:
        print(
            f"[{os.path.basename(chat_path)}] Could not detect timezone; pass --tz=N",
            file=sys.stderr,
        )
        return None
    tz_shift_secs = round(-tz * 3600)

    chat_events = []
    for line in chat_lines:
        c = parse_chat_line(line)
        if c:
            chat_events.append(c)

    player_gain_events = build_player_gain_events(log_path)["events"]
    window_secs = opts["window"]

    corr = correlate_chat_with_player(
        chat_events, player_gain_events, tz_shift_secs, window_secs, display_to_internal
    )
    chat_matches = corr["chat_matches"]
    claimed = corr["claimed"]

    # Classify chat matches
    exact_matches = 0
    coalesced = 0
    mismatches = 0
    unmatched = 0
    unmatched_chat = []
    mismatch_details = []

    for cm in chat_matches:
        if len(cm["assigned"]) == 0:
            unmatched += 1
            unmatched_chat.append(cm["chat"])
            continue
        total = sum(x["pe"]["gain_qty"] for x in cm["assigned"])
        if (
            len(cm["assigned"]) == 1
            and cm["assigned"][0]["pe"]["gain_qty"] == cm["chat"]["qty"]
        ):
            exact_matches += 1
        elif total == cm["chat"]["qty"]:
            coalesced += 1
        else:
            mismatches += 1
            mismatch_details.append(
                {"chat": cm["chat"], "sum": total, "assignments": cm["assigned"]}
            )

    # Unmatched player events
    unmatched_player = [
        pe for i, pe in enumerate(player_gain_events) if i not in claimed
    ]

    chat_dir = os.path.basename(os.path.dirname(chat_path))
    header = f"\n╔══ Report: {chat_dir}/{os.path.basename(chat_path)} × {os.path.basename(log_path)}"
    print(header)
    print(f"║  chat path:   {rel(chat_path)}")
    print(f"║  player path: {rel(log_path)}")
    print(f"║  timezone:    {fmt_num(tz)}h, window: ±{fmt_num(window_secs)}s")
    print("╠══ Counts")
    print(f'║  chat [Status] "added to inventory" entries:  {len(chat_events)}')
    print(
        f"║  chat total quantity:                          {sum(c['qty'] for c in chat_events)}"
    )
    print(f"║  player.log inventory gain events:            {len(player_gain_events)}")
    print("║    breakdown:")
    breakdown = {}
    for pe in player_gain_events:
        breakdown[pe["gain_kind"]] = breakdown.get(pe["gain_kind"], 0) + 1
    for k, v in breakdown.items():
        print(f"║      {k}: {v}")
    print("╠══ Chat → Player correlation")
    print(f"║  Exact match (1 event, qty ok):     {exact_matches}")
    print(f"║  Coalesced match (N events sum ok): {coalesced}")
    print(f"║  Single cand, qty MISMATCH:         {mismatches}")
    print(f"║  No candidate found:                {unmatched}")
    matchable_total = exact_matches + coalesced
    print(
        f"║  Match rate (exact+coalesced):      {matchable_total}/{len(chat_events)} "
        f"({js_to_fixed_1((matchable_total / max(1, len(chat_events))) * 100)}%)"
    )
    print("╠══ Player → Chat correlation")
    print(
        f"║  Player gain events unmatched:      {len(unmatched_player)}/{len(player_gain_events)} "
        f"({js_to_fixed_1((len(unmatched_player) / max(1, len(player_gain_events))) * 100)}%)"
    )
    print("╚══════════════════════════════════════════════════════════")

    if not opts["no_lists"]:
        if len(unmatched_chat) > 0:
            print("\n--- Unmatched CHAT entries (no player.log candidate) ---")
            bucketed = bucket_by_item(
                [
                    {"item": c["item"], "qty": c["qty"], "ts": c["raw_ts"]}
                    for c in unmatched_chat
                ]
            )
            for item, rows in bucketed.items():
                total_qty = sum(r["qty"] for r in rows)
                print(f"  [{len(rows)}× total qty {total_qty}] {item}")
                for r in rows[:5]:
                    print(f"    · {r['ts']} x{r['qty']}")
                if len(rows) > 5:
                    print(f"    · ...and {len(rows) - 5} more")
        if len(mismatch_details) > 0:
            print(
                "\n--- Quantity MISMATCH (assigned best candidate with wrong qty) ---"
            )
            for m in mismatch_details[:20]:
                a = " + ".join(
                    f"{x['pe']['gain_kind']}({x['pe']['resolved_name'] or '?'}:{x['pe'].get('id') or ''}, "
                    f"qty={x['pe']['gain_qty']})"
                    for x in m["assignments"]
                )
                print(
                    f"  [{m['chat']['raw_ts']}] chat: {m['chat']['item']} x{m['chat']['qty']}   ||   log: {a} (sum={m['sum']})"
                )
            if len(mismatch_details) > 20:
                print(f"  ...and {len(mismatch_details) - 20} more")
        if len(unmatched_player) > 0:
            print("\n--- Unmatched PLAYER.LOG gain events (no chat candidate) ---")
            bucketed = {}
            for pe in unmatched_player:
                key = f"{pe['gain_kind']} :: {pe['resolved_name'] or '?'}"
                bucketed.setdefault(key, []).append(pe)
            for key, rows in bucketed.items():
                total_qty = sum(r["gain_qty"] for r in rows)
                print(f"  [{len(rows)}× total gain {total_qty}] {key}")
                for r in rows[:5]:
                    print(
                        f"    · {r['raw_ts']} id={r.get('id') or '-'} gainQty={r['gain_qty']}"
                    )
                if len(rows) > 5:
                    print(f"    · ...and {len(rows) - 5} more")

    return {
        "dataset": os.path.basename(os.path.dirname(chat_path)),
        "chat_total": len(chat_events),
        "player_total": len(player_gain_events),
        "exact_matches": exact_matches,
        "coalesced": coalesced,
        "mismatches": mismatches,
        "unmatched": unmatched,
        "unmatched_player": len(unmatched_player),
    }


def bucket_by_item(items):
    buckets = {}
    for it in items:
        buckets.setdefault(it["item"], []).append(it)
    return buckets


# ========== Dataset discovery ==========


def discover_paired_datasets():
    pairs = [
        {
            "chat": str(ROOT / "test_data" / "inventory" / "InventoryChatLog.txt"),
            "log": str(ROOT / "test_data" / "inventory" / "InventoryActions.log"),
        },
        {
            "chat": str(
                ROOT
                / "test_data"
                / "surveyLogs"
                / "100x-serbcrystal-withring"
                / "ChatLog.txt"
            ),
            "log": str(
                ROOT
                / "test_data"
                / "surveyLogs"
                / "100x-serbcrystal-withring"
                / "Player.log"
            ),
        },
        {
            "chat": str(
                ROOT
                / "test_data"
                / "surveyLogs"
                / "100x-eltmetal-ringandpick"
                / "ChatLog.txt"
            ),
            "log": str(
                ROOT
                / "test_data"
                / "surveyLogs"
                / "100x-eltmetal-ringandpick"
                / "Player-prev.log"
            ),
        },
        {
            "chat": str(
                ROOT
                / "test_data"
                / "surveyLogs"
                / "50x-povusmarvelous-ringandpick"
                / "ChatLog.txt"
            ),
            "log": str(
                ROOT
                / "test_data"
                / "surveyLogs"
                / "50x-povusmarvelous-ringandpick"
                / "Player.log"
            ),
        },
        {
            "chat": str(
                ROOT
                / "docs"
                / "samples"
                / "chat+logCombos"
                / "Gazluk-Motherlodes-and-Nodes-Chat.log"
            ),
            "log": str(
                ROOT
                / "docs"
                / "samples"
                / "chat+logCombos"
                / "Gazluk-Motherlodes-and-Nodes.log"
            ),
        },
    ]
    return [p for p in pairs if os.path.exists(p["chat"]) and os.path.exists(p["log"])]


# ========== Entrypoint ==========


def main():
    # Node writes "\n" even on Windows; match it byte-for-byte.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", newline="\n")
    opts = parse_args()

    if opts["all"]:
        display_to_internal = resolve_name_map(opts["items_json"])
        pairs = discover_paired_datasets()
        results = []
        for p in pairs:
            r = run_report(p["chat"], p["log"], opts, display_to_internal)
            if r:
                results.append(r)
        print("\n==== Aggregate summary ====")
        print(
            f"{pad_end('Dataset', 42)} {pad_start('chat', 5)} {pad_start('log', 5)} "
            f"{pad_start('exact', 6)} {pad_start('coalsc', 6)} {pad_start('misma', 5)} "
            f"{pad_start('unmch', 5)} {pad_start('plrUnmch', 9)}"
        )
        for r in results:
            print(
                f"{pad_end(r['dataset'], 42)} {pad_start(r['chat_total'], 5)} {pad_start(r['player_total'], 5)} "
                f"{pad_start(r['exact_matches'], 6)} {pad_start(r['coalesced'], 6)} "
                f"{pad_start(r['mismatches'], 5)} {pad_start(r['unmatched'], 5)} {pad_start(r['unmatched_player'], 9)}"
            )
    elif len(opts["positional"]) >= 2:
        display_to_internal = resolve_name_map(opts["items_json"])
        run_report(
            opts["positional"][0], opts["positional"][1], opts, display_to_internal
        )
    else:
        print("Usage:", file=sys.stderr)
        print(
            "  python correlation-report.py <chat_log> <player_log> [--tz=-7] [--window=2] [--no-lists]",
            file=sys.stderr,
        )
        print(
            "  python correlation-report.py --all                        # run all paired datasets",
            file=sys.stderr,
        )
        sys.exit(1)


main()
