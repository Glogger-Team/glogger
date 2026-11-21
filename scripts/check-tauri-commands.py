#!/usr/bin/env python3
"""Gate: every `#[tauri::command]` fn is registered in `generate_handler!`.

Scans `src-tauri/src/**/*.rs` for command definitions and cross-checks them
against the registration list in `src-tauri/src/lib.rs`. A command that is
defined but never registered is dead (frontend `invoke()` would fail at
runtime), so it is reported as a finding.

Pre-existing drift (slice-5 review): the eight `price_helper_commands`
commands below were never registered on `main`; they are allowlisted here
with their pre-existing status cited. Any *new* unregistered command fires.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"
LIB_RS = SRC / "lib.rs"

ATTR_RE = re.compile(r"#\[tauri::command\b")
FN_RE = re.compile(r"(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)")
# Entries in generate_handler![...] are comma-separated paths, possibly with
# `::` prefixes and `//` line comments (stripped first).
HANDLER_OPEN = "generate_handler!["
IDENT_RE = re.compile(r"^[A-Za-z_]\w*$")

# Pre-existing unregistered commands (slice-5 review, 2026-11): never wired
# into generate_handler! on main; allowlisted so the gate stays green while
# the eight remain dormant. A 9th unregistered command must still fire.
ALLOWLISTED_UNREGISTERED = frozenset(
    {
        "create_price_helper_quote",
        "get_price_helper_quotes",
        "get_price_helper_quote",
        "update_price_helper_quote",
        "delete_price_helper_quote",
        "add_price_helper_entry",
        "update_price_helper_entry",
        "remove_price_helper_entry",
    }
)


def fail(msg: str) -> int:
    print(f"tauri-commands: FAIL: {msg}", file=sys.stderr)
    return 2


def collect_defined() -> list[tuple[Path, int, str]]:
    """Return (path, lineno, name) for every `#[tauri::command]` fn."""
    out: list[tuple[Path, int, str]] = []
    for path in sorted(SRC.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        i = 0
        n = len(lines)
        while i < n:
            if ATTR_RE.search(lines[i]):
                # The fn may sit below doc comments or extra attributes:
                # skip blank/attribute/doc/comment lines (bounded) and
                # require the fn on the landing line.
                j = i + 1
                while j < n and j < i + 9:
                    s = lines[j].strip()
                    if not s or s.startswith(("#[", "///", "//!", "//")):
                        j += 1
                        continue
                    break
                if j < n:
                    m = FN_RE.search(lines[j])
                    if m:
                        out.append((path, j + 1, m.group(1)))
                        i = j
            i += 1
    return out


def collect_registered() -> set[str] | None:
    """Return the set of command names in generate_handler![...]."""
    if not LIB_RS.is_file():
        return None
    text = LIB_RS.read_text(encoding="utf-8")
    # Strip line comments first (string-aware): a comment like
    # `// Slots [0, capacity)` must not unbalance the bracket depth.
    cleaned_chars: list[str] = []
    in_string = False
    escape = False
    k = 0
    while k < len(text):
        c = text[k]
        if in_string:
            cleaned_chars.append(c)
            if escape:
                escape = False
            elif c == "\\":
                escape = True
            elif c == '"':
                in_string = False
        elif c == '"':
            in_string = True
            cleaned_chars.append(c)
        elif c == "/" and k + 1 < len(text) and text[k + 1] == "/":
            while k < len(text) and text[k] != "\n":
                k += 1
            continue
        else:
            cleaned_chars.append(c)
        k += 1
    text = "".join(cleaned_chars)
    start = text.find(HANDLER_OPEN)
    if start < 0:
        return None
    bracket = text.find("[", start)
    if bracket < 0:
        return None
    depth = 0
    end = -1
    for k in range(bracket, len(text)):
        c = text[k]
        if c == "[":
            depth += 1
        elif c == "]":
            depth -= 1
            if depth == 0:
                end = k
                break
    if end < 0:
        return None
    inner = text[bracket + 1 : end]
    # Line comments were already stripped (string-aware) before the
    # bracket scan above; entries split on commas from here.
    names: set[str] = set()
    for entry in inner.split(","):
        entry = entry.strip()
        if not entry:
            continue
        # Drop any trailing macro/args just in case.
        name = entry.split("::")[-1].strip()
        if IDENT_RE.match(name):
            names.add(name)
    return names


def main() -> int:
    try:
        defined = collect_defined()
    except OSError as e:
        return fail(f"cannot scan sources: {e}")
    registered = collect_registered()
    if registered is None:
        return fail("could not parse generate_handler! in src-tauri/src/lib.rs")

    findings: list[str] = []
    for path, lineno, name in defined:
        if name in registered:
            continue
        if name in ALLOWLISTED_UNREGISTERED:
            continue
        rel = path.relative_to(ROOT).as_posix()
        findings.append(f"{rel}:{lineno}: unregistered tauri command `{name}`")

    if findings:
        for f in findings:
            print(f)
        return 1
    print("tauri-commands: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
