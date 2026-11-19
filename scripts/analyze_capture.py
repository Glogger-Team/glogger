"""
Analyze a glogger debug capture JSON file.

Produces a summary of:
  - Capture metadata and quick stats
  - All ProcessXxx event types with counts and examples
  - Non-Process line pattern categories
  - Chat lines
  - Game state snapshot diff (start vs stop)
  - Signal-to-noise ratio

Usage:
    python scripts/analyze_capture.py path/to/capture.json
    python scripts/analyze_capture.py path/to/capture.json --full         # show all examples
    python scripts/analyze_capture.py path/to/capture.json --chat         # show chat lines
    python scripts/analyze_capture.py path/to/capture.json --diff         # show snapshot diff
    python scripts/analyze_capture.py path/to/capture.json --process-only # only ProcessXxx lines
"""

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

# ── Noise classification ────────────────────────────────────────────

NOISE_PREFIXES = [
    "Download appearance loop",
    "LoadAssetAsync",
    "IsDoneLoading",
    "Successfully downloaded Texture",
    "Cannot remove: entity doesn't have particle",
    "Ref-count cleanup",
    "ClearCursor",
    "Animator.GotoState",
    "BoxColliders created at Runtime",
    "Combined Static Meshes",
    "Either create the Box Collider",
    "MecanimEx:",
    "Told to do animation",
    "Shader ",
    "New Network State",
    "Playing sound ",
]

NOISE_CONTAINS = [
    "ProcessMusicPerformance(MusicPerformanceManager+PerformanceInfo)",
    ": Playing sound ",
]

# Regex to extract the ProcessXxx name from a log line
PROCESS_RE = re.compile(r"(?:LocalPlayer: |entity_\d+: )?(Process\w+)\(")
# Also match Process lines without LocalPlayer prefix (e.g. ProcessUpdateDescription)
BARE_PROCESS_RE = re.compile(r"^\[\d{2}:\d{2}:\d{2}\] (Process\w+)\(")
# Timestamp prefix
TIMESTAMP_RE = re.compile(r"^\[(\d{2}:\d{2}:\d{2})\] ")
# OnAttackHitMe
ATTACK_RE = re.compile(
    r"(entity_\d+|LocalPlayer): OnAttackHitMe\((.+?)\)\. Evaded = (\w+)"
)


def is_noise(line: str) -> bool:
    if len(line) < 4:
        return True
    for prefix in NOISE_PREFIXES:
        if line.startswith(prefix):
            return True
    for substr in NOISE_CONTAINS:
        if substr in line:
            return True
    return False


def classify_line(line: str) -> str:
    """Classify a player log line into a category."""
    if not line or len(line) < 4:
        return "fragment"

    # Process events
    m = PROCESS_RE.search(line)
    if m:
        return f"Process:{m.group(1)}"
    m = BARE_PROCESS_RE.search(line)
    if m:
        return f"Process:{m.group(1)}"

    # Attack events
    if "OnAttackHitMe(" in line:
        return "combat:OnAttackHitMe"
    if "OnAttackMiss(" in line:
        return "combat:OnAttackMiss"

    # Noise categories (with or without [HH:MM:SS] timestamp prefix)
    bare = line
    ts_match = TIMESTAMP_RE.match(line)
    if ts_match:
        bare = line[ts_match.end() :]

    if bare.startswith("Download appearance"):
        return "noise:appearance"
    if bare.startswith(("LoadAssetAsync", "IsDoneLoading", "Completed ")):
        return "noise:asset_loading"
    if bare.startswith("Successfully downloaded"):
        return "noise:texture"
    if "Playing sound " in bare:
        return "noise:sound"
    if bare.startswith(("MecanimEx:", "Told to do animation", "Animator.")):
        return "noise:animation"
    if bare.startswith("ClearCursor"):
        return "noise:cursor"
    if bare.startswith(("Ref-count", "ref-count")):
        return "noise:gc"
    if bare.startswith(("BoxColliders", "Combined Static", "Either create")):
        return "noise:collider"
    if bare.startswith("Shader "):
        return "noise:shader"
    if bare.startswith("New Network State"):
        return "noise:network"
    if "Cannot remove: entity" in bare:
        return "noise:particle"
    if "ProcessMusicPerformance" in bare:
        return "noise:music_perf"
    if bare.startswith("Auto-generated a selection collider"):
        return "noise:collider"
    if bare.startswith("Invalid Layer Index"):
        return "noise:animation"
    if bare.startswith("[ line "):
        return "noise:debug_trace"
    if bare.startswith("Vivox"):
        return "noise:vivox"

    # Stack trace / method signatures (contain :: with no timestamp context)
    if "::" in line and not ts_match and ":" not in line.split("::")[0]:
        return "noise:stack_trace"

    # Structured non-process events
    if "ShowBook(" in bare:
        return "event:ShowBook"
    if "StartMusic(" in bare:
        return "event:StartMusic"
    if "was active, turning off" in bare:
        return "event:WeaponToggle"
    if "LOADING LEVEL" in bare or "Initializing area" in bare:
        return "event:AreaTransition"
    if "Curl error" in bare:
        return "event:NetworkError"
    if "Logged in as character" in bare:
        return "event:Login"
    if (
        "UIHighlightSelectionController" in bare
        or "LocationSender" in bare
        or "MessageDispatcher" in bare
    ):
        return "noise:stack_trace"

    return "other"


# ── Analysis ────────────────────────────────────────────────────────


def analyze_capture(data: dict, args) -> None:
    lines = data.get("lines", [])
    player_lines = [l for l in lines if l.get("source") == "player"]
    chat_lines = [l for l in lines if l.get("source") == "chat"]

    # ── Metadata ──
    print("=" * 70)
    print("CAPTURE ANALYSIS")
    print("=" * 70)
    print()

    meta_fields = [
        ("Format version", data.get("format_version")),
        ("App version", data.get("app_version")),
        ("Filter mode", data.get("filter_mode", "n/a (v1)")),
        ("Notes", data.get("notes", "(none)")),
        ("Started", data.get("started_at", "?")),
        ("Stopped", data.get("stopped_at", "?")),
    ]
    for label, val in meta_fields:
        print(f"  {label:20s}: {val}")

    # Character / area from snapshots
    start_snap = data.get("state_at_start", {})
    stop_snap = data.get("state_at_stop", {})
    char_name = start_snap.get("character", "?")
    server = start_snap.get("server", "?")
    start_area = _get_area(start_snap)
    stop_area = _get_area(stop_snap)
    print(f"  {'Character':20s}: {char_name} on {server}")
    if start_area == stop_area:
        print(f"  {'Area':20s}: {start_area}")
    else:
        print(f"  {'Area':20s}: {start_area} -> {stop_area}")

    print()
    print(f"  Total lines:  {len(lines):,}")
    print(f"  Player lines: {len(player_lines):,}")
    print(f"  Chat lines:   {len(chat_lines):,}")
    if data.get("unfiltered_line_count"):
        print(f"  Unfiltered:   {data['unfiltered_line_count']:,}")

    # Time range from first/last timestamps
    if player_lines:
        first_ts = _extract_game_time(player_lines[0].get("line", ""))
        last_ts = _extract_game_time(player_lines[-1].get("line", ""))
        if first_ts and last_ts:
            print(f"  Game time:    {first_ts} - {last_ts}")

    # ── Classify all player lines ──
    categories = Counter()
    process_types = Counter()
    process_examples = defaultdict(list)
    noise_count = 0
    signal_count = 0

    for entry in player_lines:
        line = entry.get("line", "")
        cat = classify_line(line)
        categories[cat] += 1

        if cat.startswith("Process:"):
            ptype = cat.split(":", 1)[1]
            process_types[ptype] += 1
            if len(process_examples[ptype]) < 3:
                process_examples[ptype].append(line)

        if is_noise(line) or cat.startswith("noise:") or cat == "fragment":
            noise_count += 1
        else:
            signal_count += 1

    total = noise_count + signal_count
    noise_pct = (noise_count / total * 100) if total else 0
    print()
    print(f"  Signal lines: {signal_count:,} ({100 - noise_pct:.1f}%)")
    print(f"  Noise lines:  {noise_count:,} ({noise_pct:.1f}%)")

    # ── Process event types ──
    print()
    print("-" * 70)
    print("PROCESS EVENT TYPES")
    print("-" * 70)
    print()
    print(f"  {'Type':<45s} {'Count':>6s}")
    print(f"  {'-' * 45} {'-' * 6}")
    for ptype, count in process_types.most_common():
        print(f"  {ptype:<45s} {count:>6,}")
    print()

    if args.full:
        print("  EXAMPLES:")
        print()
        for ptype, count in process_types.most_common():
            print(f"  -- {ptype} ({count}x) --")
            for ex in process_examples[ptype]:
                # Truncate long lines
                display = ex if len(ex) <= 120 else ex[:117] + "..."
                print(f"    {display}")
            print()

    # ── Non-process categories ──
    print("-" * 70)
    print("NON-PROCESS LINE CATEGORIES")
    print("-" * 70)
    print()
    non_process = {k: v for k, v in categories.items() if not k.startswith("Process:")}
    print(f"  {'Category':<35s} {'Count':>6s}")
    print(f"  {'-' * 35} {'-' * 6}")
    for cat, count in sorted(non_process.items(), key=lambda x: -x[1]):
        print(f"  {cat:<35s} {count:>6,}")

    # ── Chat lines ──
    if chat_lines and (args.chat or args.full):
        print()
        print("-" * 70)
        print("CHAT LINES")
        print("-" * 70)
        print()
        for entry in chat_lines:
            ts = entry.get("captured_at", "?")
            print(f"  [{ts}] {entry.get('line', '')}")

    elif chat_lines:
        print()
        print(f"  ({len(chat_lines)} chat lines - use --chat to display)")

    # ── Combat summary ──
    attacks = []
    for entry in player_lines:
        m = ATTACK_RE.search(entry.get("line", ""))
        if m:
            attacks.append(
                {
                    "target": m.group(1),
                    "ability": m.group(2),
                    "evaded": m.group(3),
                }
            )
    if attacks:
        print()
        print("-" * 70)
        print(f"COMBAT ({len(attacks)} hits)")
        print("-" * 70)
        ability_counts = Counter(a["ability"] for a in attacks)
        evade_count = sum(1 for a in attacks if a["evaded"] == "True")
        print()
        print("  Abilities used against player:")
        for ability, count in ability_counts.most_common(10):
            print(f"    {ability:<40s} {count:>4}x")
        if evade_count:
            print(
                f"  Evaded: {evade_count}/{len(attacks)} ({evade_count / len(attacks) * 100:.0f}%)"
            )

    # ── Snapshot diff ──
    if args.diff or args.full:
        print()
        print("-" * 70)
        print("STATE SNAPSHOT DIFF (start vs stop)")
        print("-" * 70)
        print_snapshot_diff(start_snap, stop_snap)

    print()


def _get_area(snapshot: dict) -> str:
    world = snapshot.get("world", {})
    if isinstance(world, dict):
        area = world.get("area", {})
        if isinstance(area, dict):
            return area.get("area_name", "?")
    return "?"


def _extract_game_time(line: str) -> str | None:
    m = TIMESTAMP_RE.search(line)
    return m.group(1) if m else None


# ── Snapshot diffing ────────────────────────────────────────────────


def print_snapshot_diff(start: dict, stop: dict) -> None:
    if not start or not stop:
        print("  (snapshots not available)")
        return

    # Compare list-based tables (skills, inventory, effects, etc.)
    for table in ["skills", "inventory", "equipment", "effects", "favor", "currencies"]:
        start_items = start.get(table, [])
        stop_items = stop.get(table, [])
        diffs = diff_table(table, start_items, stop_items)
        if diffs:
            print()
            print(f"  {table.upper()}:")
            for diff in diffs:
                print(f"    {diff}")

    # Compare attributes (large list, only show changes)
    start_attrs = {
        a.get("attribute_name"): a
        for a in start.get("attributes", [])
        if isinstance(a, dict)
    }
    stop_attrs = {
        a.get("attribute_name"): a
        for a in stop.get("attributes", [])
        if isinstance(a, dict)
    }
    attr_changes = []
    for key in set(start_attrs) | set(stop_attrs):
        s = start_attrs.get(key, {}).get("attribute_value")
        e = stop_attrs.get(key, {}).get("attribute_value")
        if s != e:
            attr_changes.append(f"{key}: {s} -> {e}")
    if attr_changes:
        print()
        print(f"  ATTRIBUTES ({len(attr_changes)} changed):")
        for change in sorted(attr_changes):
            print(f"    {change}")

    # Area change
    start_area = _get_area(start)
    stop_area = _get_area(stop)
    if start_area != stop_area:
        print()
        print(f"  AREA: {start_area} -> {stop_area}")

    if not any(
        [
            diff_table("skills", start.get("skills", []), stop.get("skills", [])),
            diff_table(
                "inventory", start.get("inventory", []), stop.get("inventory", [])
            ),
            attr_changes,
            start_area != stop_area,
        ]
    ):
        print()
        print("  (no differences detected)")


def diff_table(table: str, start_items: list, stop_items: list) -> list[str]:
    """Diff two lists of row dicts, using a heuristic key."""
    diffs = []

    key_field = _key_field_for(table)
    if not key_field:
        # Fall back to count comparison
        if len(start_items) != len(stop_items):
            diffs.append(f"count: {len(start_items)} -> {len(stop_items)}")
        return diffs

    start_map = {}
    for item in start_items:
        if isinstance(item, dict):
            k = item.get(key_field, id(item))
            start_map[k] = item

    stop_map = {}
    for item in stop_items:
        if isinstance(item, dict):
            k = item.get(key_field, id(item))
            stop_map[k] = item

    # Added
    for k in set(stop_map) - set(start_map):
        item = stop_map[k]
        label = _item_label(table, item)
        diffs.append(f"+ {label}")

    # Removed
    for k in set(start_map) - set(stop_map):
        item = start_map[k]
        label = _item_label(table, item)
        diffs.append(f"- {label}")

    # Changed
    for k in set(start_map) & set(stop_map):
        s = start_map[k]
        e = stop_map[k]
        if s != e:
            changes = []
            for field in set(s) | set(e):
                sv = s.get(field)
                ev = e.get(field)
                if sv != ev and field not in (
                    key_field,
                    "character_name",
                    "server_name",
                    "updated_at",
                ):
                    changes.append(f"{field}: {sv} -> {ev}")
            if changes:
                label = _item_label(table, s)
                diffs.append(f"~ {label}: {', '.join(changes)}")

    return diffs


def _key_field_for(table: str) -> str | None:
    return {
        "skills": "skill_name",
        "inventory": "instance_id",
        "equipment": "slot",
        "effects": "effect_instance_id",
        "favor": "npc_name",
        "currencies": "currency_name",
    }.get(table)


def _item_label(table: str, item: dict) -> str:
    if table == "skills":
        return f"{item.get('skill_name', '?')} (lv {item.get('level', '?')})"
    if table == "inventory":
        name = item.get("item_name", item.get("instance_id", "?"))
        stack = item.get("stack_size", "?")
        return f"{name} x{stack}"
    if table == "effects":
        name = item.get("effect_name") or f"#{item.get('effect_instance_id', '?')}"
        return f"effect {name}"
    if table == "favor":
        return f"{item.get('npc_name', '?')} ({item.get('favor_tier', '?')})"
    if table == "currencies":
        return f"{item.get('currency_name', '?')}: {item.get('amount', '?')}"
    if table == "equipment":
        return f"slot {item.get('slot', '?')}"
    return str(next(iter(item.values())) if item else "?")


# ── Entry point ─────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(
        description="Analyze a glogger debug capture JSON file.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python scripts/analyze_capture.py capture.json
  python scripts/analyze_capture.py capture.json --full
  python scripts/analyze_capture.py capture.json --chat --diff
  python scripts/analyze_capture.py capture.json --process-only
        """,
    )
    parser.add_argument("file", help="Path to the capture JSON file")
    parser.add_argument(
        "--full", action="store_true", help="Show all details (examples, chat, diff)"
    )
    parser.add_argument("--chat", action="store_true", help="Display chat lines")
    parser.add_argument(
        "--diff", action="store_true", help="Show game state snapshot diff"
    )
    parser.add_argument(
        "--process-only",
        action="store_true",
        help="Only output ProcessXxx lines (for piping)",
    )
    args = parser.parse_args()

    path = Path(args.file)
    if not path.exists():
        print(f"File not found: {path}", file=sys.stderr)
        sys.exit(1)

    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)

    if args.process_only:
        for entry in data.get("lines", []):
            line = entry.get("line", "")
            if PROCESS_RE.search(line) or BARE_PROCESS_RE.search(line):
                print(line)
        return

    analyze_capture(data, args)


if __name__ == "__main__":
    main()
