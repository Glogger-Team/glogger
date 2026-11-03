#!/usr/bin/env python3
"""Correlation analyzer: match [Status] chat "ItemGained" messages to Player.log AddItem/UpdateItemCode events.

Python port of scripts/analyze_chat_correlation.cjs (stdlib only).

For each [Status] "X added to inventory" (optionally "x N"):
  - find candidate Player.log events within +/- window seconds that could represent that gain
  - candidates: ProcessAddItem(X, _, True) or ProcessUpdateItemCode on an instance named X with positive delta
  - report: exact match, ambiguous (multiple candidates), orphan (no candidates)

Usage: python analyze-chat-correlation.py <chat_log> <player_log> [--tz=-7] [--window=2]

Chat timestamps are local time with TZ listed in the login line ("Timezone Offset -07:00:00").
Player.log timestamps are UTC. We convert chat→UTC by adding -tz hours.
"""
import math
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

CHAT_LINE_RE = re.compile(
    r"^(\d{2})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2}):(\d{2})\s+\[Status\]\s+(.+?)\s+added to inventory\.?$"
)
CHAT_QTY_RE = re.compile(r"^(.*?)\s+x(\d+)$")
TZ_RE = re.compile(r"Timezone Offset ([+\-]\d{2}):(\d{2}):\d{2}")
TS_RE = re.compile(r"^\[(\d{2}):(\d{2}):(\d{2})\]")
ADD_ITEM_RE = re.compile(r"ProcessAddItem\((\w+)\((\d+)\),\s*(-?\d+),\s*(True|False)\)")
UPDATE_CODE_RE = re.compile(r"ProcessUpdateItemCode\((\d+),\s*(\d+),\s*(True|False)\)")
REMOVE_VAULT_RE = re.compile(r"ProcessRemoveFromStorageVault\(\d+,\s*-?\d+,\s*(\d+),\s*(\d+)\)")


def parse_args(argv):
    positional = []
    window = 2
    tz = None
    for a in argv:
        if a.startswith("--window="):
            window = js_parse_int(a[9:])
        elif a.startswith("--tz="):
            window_tz = js_parse_float(a[5:])
            tz = window_tz
        else:
            positional.append(a)
    if len(positional) < 2:
        print(
            "Usage: python analyze-chat-correlation.py <chat_log> <player_log> [--tz=-7] [--window=2]",
            file=sys.stderr,
        )
        sys.exit(1)
    return {"chat_path": positional[0], "log_path": positional[1], "window": window, "tz": tz}


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
    # representation), not Python's round-half-even format(). On the fixture
    # percentages both agree, but keep the JS semantics for byte parity.
    if math.isnan(x):
        return "NaN"
    if math.isinf(x):
        return "Infinity" if x > 0 else "-Infinity"
    sign = "-" if x < 0 else ""
    x = abs(x)
    scaled = math.floor(x * 10 + 0.5 + 1e-9)
    return f"{sign}{scaled // 10}.{scaled % 10}"


# "26-04-14 08:46:07\t[Status] Great Phlogiston x1000 added to inventory." → {ts, item, qty}
def parse_chat_line(line):
    line = line[:-1] if line.endswith("\r") else line
    m = CHAT_LINE_RE.match(line)
    if not m:
        return None
    yy, mm, dd, hh, mi, ss, payload = m.groups()
    hms = int(hh, 10) * 3600 + int(mi, 10) * 60 + int(ss, 10)
    # "Great Phlogiston x1000" or "Egg" or "Gazluk Map"
    qm = CHAT_QTY_RE.match(payload)
    item = qm.group(1) if qm else payload
    qty = int(qm.group(2), 10) if qm else 1
    return {
        "date": f"{yy}-{mm}-{dd}",
        "chat_hms": hms,
        "raw_ts": f"{hh}:{mi}:{ss}",
        "item": item,
        "qty": qty,
    }


# Extract Timezone Offset from chat log header "Timezone Offset -07:00:00."
def detect_chat_tz(chat_lines):
    for ln in chat_lines[:20]:
        m = TZ_RE.search(ln)
        if m:
            sign = -1 if m.group(1).startswith("-") else 1
            h = int(m.group(1).replace("-", "").replace("+", ""), 10)
            mins = int(m.group(2), 10)
            return sign * (h + mins / 60)
    return None


# Player.log events we care about:
#   [HH:MM:SS] LocalPlayer: ProcessAddItem(Name(id), slot, isNew)
#   [HH:MM:SS] LocalPlayer: ProcessUpdateItemCode(id, encoded, fromServer)
def parse_player_line(line):
    line = line[:-1] if line.endswith("\r") else line
    ts_match = TS_RE.match(line)
    if not ts_match:
        return None
    hms = (
        int(ts_match.group(1), 10) * 3600
        + int(ts_match.group(2), 10) * 60
        + int(ts_match.group(3), 10)
    )

    m = ADD_ITEM_RE.search(line)
    if m:
        return {
            "kind": "AddItem",
            "hms": hms,
            "name": m.group(1),
            "id": m.group(2),
            "slot": int(m.group(3), 10),
            "is_new": m.group(4) == "True",
        }
    m = UPDATE_CODE_RE.search(line)
    if m:
        encoded = int(m.group(2), 10)
        return {
            "kind": "UpdateItemCode",
            "hms": hms,
            "id": m.group(1),
            "encoded": encoded,
            # JS `>>>` is an unsigned 32-bit shift; encoded values fit in
            # int32 range so `>>` gives the same result here.
            "stack_actual": (encoded % 0x100000000 >> 16) + 1,
            "type_id": encoded & 0xFFFF,
            "from_server": m.group(3) == "True",
        }
    m = REMOVE_VAULT_RE.search(line)
    if m:
        return {
            "kind": "RemoveFromStorageVault",
            "hms": hms,
            "id": m.group(1),
            "quantity": int(m.group(2), 10),
        }
    return None


# Build instance_id → name registry (most recent seen name), walking the log once.
# Also build per-instance stack history to compute deltas.
def build_player_history(log_path):
    with open(log_path, encoding="utf-8") as fh:
        lines = fh.read().split("\n")
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
            # Genuine new acquisition (slot<0, isNew=True): seed stack=1, contributes +1 gain
            # Storage withdrawal (slot>=0, isNew=True): gain qty unknown until RemoveFromStorageVault
            # Session load (isNew=False): no gain
            if ev["is_new"] and ev["slot"] < 0:
                last_stack[ev["id"]] = 1
                events.append({**ev, "gain_qty": 1, "resolved_name": ev["name"]})
            elif ev["is_new"] and ev["slot"] >= 0:
                # Defer: emitted below as placeholder with gainQty=0; actual gain comes from UpdateItemCode delta later
                events.append(
                    {
                        **ev,
                        "gain_qty": 0,
                        "resolved_name": ev["name"],
                        "is_storage_withdrawal_add": True,
                    }
                )
            else:
                events.append({**ev, "gain_qty": 0, "resolved_name": ev["name"]})
        elif ev["kind"] == "UpdateItemCode":
            name = name_by_id.get(ev["id"])
            prior = last_stack.get(ev["id"])
            delta = 0
            is_baseline_establish = False
            if prior is not None:
                delta = ev["stack_actual"] - prior
            else:
                # First sighting of this instance (session-load baseline). Don't count as gain.
                is_baseline_establish = True
            last_stack[ev["id"]] = ev["stack_actual"]
            events.append(
                {
                    **ev,
                    "gain_qty": delta if delta > 0 else 0,
                    "delta_signed": delta,
                    "resolved_name": name,
                    "is_baseline_establish": is_baseline_establish,
                }
            )
        elif ev["kind"] == "RemoveFromStorageVault":
            name = name_by_id.get(ev["id"])
            # Seed stack from vault qty (mimics our parser fix)
            if ev["id"] not in last_stack:
                last_stack[ev["id"]] = ev["quantity"]
            # The vault qty IS the gain to the player when withdrawing
            events.append({**ev, "gain_qty": ev["quantity"], "resolved_name": name})
    return events


# Simple display-name → internal-name candidate matching.
# Chat uses display names ("Great Phlogiston", "Large Prism", "Osslar Skull").
# Player.log uses internal names ("Phlogiston7", "Prism2", "OsslarSkull").
# We normalize both by stripping spaces+numerics and comparing case-insensitively.
def normalize_name(s):
    return re.sub(r"\d+$", "", re.sub(r"\s+", "", s)).lower()


def names_might_match(display_name, internal_name):
    if not internal_name:
        return False
    a = normalize_name(display_name)
    b = normalize_name(internal_name)
    if a == b:
        return True
    # loose containment for graded items like "ArmorPatchKit5" vs "Master Armor Patch Kit"
    if a in b or b in a:
        return True
    return False


def fmt_num(value):
    # JS template `${value}` prints -7 not -7.0; mirror that for int-valued floats.
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)


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


def main():
    # Node writes "\n" even on Windows; match it byte-for-byte.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(newline="\n")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(newline="\n")
    args = parse_args(sys.argv[1:])
    chat_path = args["chat_path"]
    log_path = args["log_path"]
    window_secs = args["window"]
    tz_override = args["tz"]

    with open(chat_path, encoding="utf-8") as fh:
        chat_lines = fh.read().split("\n")
    tz = tz_override if tz_override is not None else detect_chat_tz(chat_lines)
    if tz is None:
        print(
            "Could not detect timezone from chat log; pass --tz=-7 (or whatever)",
            file=sys.stderr,
        )
        sys.exit(1)
    tz_shift_secs = round(-tz * 3600)  # chat local → UTC: add -tz hours

    chat_events = []
    for line in chat_lines:
        c = parse_chat_line(line)
        if c:
            chat_events.append(c)

    player_events = build_player_history(log_path)
    gain_events = [e for e in player_events if e["gain_qty"] > 0]

    # Summary counters
    chat_total = 0
    chat_total_qty = 0
    exact_match = 0  # exactly one candidate with matching qty
    qty_match_ambiguous = 0  # multiple candidates, one with matching qty
    single_mismatch = 0  # one candidate but qty differs
    multiple_candidates = 0  # ambiguous, no single qty match
    orphan = 0  # no candidates
    orphan_samples = []
    mismatch_samples = []

    for ce in chat_events:
        chat_total += 1
        chat_total_qty += ce["qty"]
        chat_utc_secs = ce["chat_hms"] + tz_shift_secs
        # Wrap negative / overflow: normalize into 0..86400
        chat_utc_mod = ((chat_utc_secs % 86400) + 86400) % 86400

        candidates = []
        for pe in gain_events:
            if not names_might_match(ce["item"], pe["resolved_name"]):
                continue
            # distance modulo 24h
            d = abs(pe["hms"] - chat_utc_mod)
            if d > 43200:
                d = 86400 - d
            if d <= window_secs:
                candidates.append(pe)

        if len(candidates) == 0:
            orphan += 1
            if len(orphan_samples) < 10:
                orphan_samples.append({"chat": ce, "chat_utc": chat_utc_mod})
            continue

        qty_matches = [c for c in candidates if c["gain_qty"] == ce["qty"]]
        total_gain_sum = sum(c["gain_qty"] for c in candidates)
        sum_matches = total_gain_sum == ce["qty"]

        if len(qty_matches) == 1 and len(candidates) == 1:
            exact_match += 1
        elif len(qty_matches) >= 1:
            qty_match_ambiguous += 1
        elif sum_matches:
            # Multi-event coalesce: chat qty = sum of candidate gains
            qty_match_ambiguous += 1
        elif len(candidates) == 1:
            single_mismatch += 1
            if len(mismatch_samples) < 10:
                mismatch_samples.append(
                    {
                        "chat": ce,
                        "chat_utc": chat_utc_mod,
                        "cand": candidates[0],
                        "total_gain_sum": total_gain_sum,
                    }
                )
        else:
            multiple_candidates += 1
            if len(mismatch_samples) < 10:
                mismatch_samples.append(
                    {
                        "chat": ce,
                        "chat_utc": chat_utc_mod,
                        "cands": candidates,
                        "total_gain_sum": total_gain_sum,
                    }
                )

    print("=== Chat [Status] → Player.log correlation analysis ===")
    print(f"Chat log:    {rel(chat_path)}")
    print(f"Player log:  {rel(log_path)}")
    print(f"Timezone:    {fmt_num(tz)}h (chat → UTC)")
    print(f"Window:      ±{window_secs}s")
    print("")
    print(f"Total chat ItemGained messages: {chat_total}")
    print(f"Total chat quantity:            {chat_total_qty}")
    print(f"Total player.log gain events:   {len(gain_events)}")
    print("")
    print(f"Exact match (1 cand, qty ok):   {exact_match}")
    print(f"Ambiguous w/ qty match:         {qty_match_ambiguous}")
    print(f"Single cand, qty MISMATCH:      {single_mismatch}")
    print(f"Multiple cands, no qty match:   {multiple_candidates}")
    print(f"Orphan (no cand):               {orphan}")
    print("")
    matchable = exact_match + qty_match_ambiguous
    print(
        f"Quantity match rate: {matchable}/{chat_total} ({js_to_fixed_1((matchable / chat_total) * 100)}%)"
    )
    print("")

    if len(orphan_samples) > 0:
        print("--- Orphan samples (chat msg with no player.log candidate) ---")
        for o in orphan_samples:
            h = math.floor(o["chat_utc"] / 3600)
            m = math.floor((o["chat_utc"] % 3600) / 60)
            s = o["chat_utc"] % 60
            print(
                f"  [{o['chat']['date']} {o['chat']['raw_ts']} → UTC "
                f"~{h:02d}:{m:02d}:{s:02d}] {o['chat']['item']} x{o['chat']['qty']}"
            )
        print("")

    if len(mismatch_samples) > 0:
        print("--- Quantity mismatch samples ---")
        for m in mismatch_samples:
            h = math.floor(m["chat_utc"] / 3600)
            mm = math.floor((m["chat_utc"] % 3600) / 60)
            s = m["chat_utc"] % 60
            ts = f"~{h:02d}:{mm:02d}:{s:02d}"
            if "cand" in m:
                print(
                    f"  [{ts}] chat: {m['chat']['item']} x{m['chat']['qty']}  ||  "
                    f"log: {m['cand']['kind']} {m['cand']['resolved_name']}({m['cand']['id']}) "
                    f"gainQty={m['cand']['gain_qty']} (sum={m['total_gain_sum']})"
                )
            else:
                cand_summary = ", ".join(
                    f"{c['kind']}({c['id']}:{c['gain_qty']})" for c in m["cands"]
                )
                print(
                    f"  [{ts}] chat: {m['chat']['item']} x{m['chat']['qty']}  ||  "
                    f"{len(m['cands'])} cands: {cand_summary} (sum={m['total_gain_sum']})"
                )
        print("")


main()
