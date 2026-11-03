#!/usr/bin/env python3
"""glogger grid-migration audit — the base→head padding/indent census behind the
accepted-drift ledger in docs/architecture/ux-standards.md (§ "Accepted drift
from the 2026-10-17 DataTable sweep").

Python port of scripts/grid-migration-audit.mjs (stdlib only).

Usage: python scripts/grid-migration-audit.py [base-ref] [head-ref]
       (defaults: v1.3.1 — the sweep's base — and HEAD)
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = sys.argv[1] if len(sys.argv) >= 2 else "v1.3.1"
HEAD = sys.argv[2] if len(sys.argv) >= 3 else "HEAD"

# Tailwind emits padding in this property order, so the last one present wins.
SCALE = {
    "0": 0,
    "0.5": 2,
    "1": 4,
    "1.5": 6,
    "2": 8,
    "2.5": 10,
    "3": 12,
    "3.5": 14,
    "4": 16,
    "5": 20,
    "6": 24,
    "8": 32,
}
ORDER = ["p", "px", "ps", "pe", "pl", "py", "pt", "pb"]


def px(token):
    step = re.sub(r"^\[|\]$", "", token[token.index("-") + 1 :])
    if step in SCALE:
        return SCALE[step]
    rem = re.match(r"^([\d.]+)rem$", step)
    if rem:
        return float(rem.group(1)) * 16
    m = re.match(r"^([\d.]+)px$", step)
    if m:
        return float(m.group(1))
    return None


def resolve(classes, axis):
    wanted = ["p", "px", "ps", "pl"] if axis == "left" else ["p", "py", "pt"]
    value = 0
    for key in ORDER:
        if key not in wanted:
            continue
        hit = next(
            (c for c in classes if c.startswith(key + "-") and px(c) is not None),
            None,
        )
        if hit is not None:
            value = px(hit)
    return value


def git(*args):
    proc = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    )
    return proc.stdout


TD_TH_RE = re.compile(r"<(td|th)\b")
TH_RE = re.compile(r"<th\b")
CLASS_RE = re.compile(r'class="([^"]*)"')
DATATABLE_RE = re.compile(r"<DataTable\b")
COMPACT_RE = re.compile(r"compact")
SM_RE = re.compile(r'size="sm"')
OVERRIDE_RE = re.compile(r"\[&>td[^\]]*\]:[\w./\[\]-]+")
CELLCELL_RE = re.compile(r"cellClass: '([^']*)'")


def tags_in(text):
    out = []
    for m in re.finditer(r"<DataTable\b", text):
        i = m.start() + len(m.group(0))
        quote = None
        while i < len(text):
            c = text[i]
            if quote is not None:
                if c == quote:
                    quote = None
            elif c == '"' or c == "'":
                quote = c
            elif c == ">":
                break
            i += 1
        out.append(text[m.start() : i])
    return out


def w(s, n):
    # JS String.padEnd(n) never truncates: pads with spaces only when shorter.
    s = str(s)
    return s if len(s) >= n else s + " " * (n - len(s))


def fmt_num(value):
    # JS template `${value}` prints 8 not 8.0; mirror that for int-valued floats.
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    return str(value)


def main():
    # Node writes "\n" even on Windows; match it byte-for-byte. encoding=utf-8:
    # the header/rows print →/×/— which cp1252 pipes cannot encode.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", newline="\n")
    changed = [
        f
        for f in git(
            "diff", "--name-only", "--diff-filter=M", f"{BASE}..{HEAD}", "--", "src"
        ).split("\n")
        if f.endswith(".vue")
    ]

    rows = []
    for file in changed:
        removed = [
            line
            for line in git("diff", "--unified=0", f"{BASE}..{HEAD}", "--", file).split(
                "\n"
            )
            if line.startswith("-")
            and not line.startswith("---")
            and TD_TH_RE.search(line)
        ]
        # Header and body cells have always carried different padding, so they are
        # tracked apart: the body mode is what the row's cells now render at.
        combos = {"td": {}, "th": {}}
        for line in removed:
            m = CLASS_RE.search(line)
            if not m:
                continue
            kind = "th" if TH_RE.search(line) else "td"
            classes = re.split(r"\s+", m.group(1))
            key = f"{fmt_num(resolve(classes, 'left'))}/{fmt_num(resolve(classes, 'top'))}"
            bucket = combos[kind]
            bucket[key] = bucket.get(key, 0) + 1
        head = (ROOT / file).read_text(encoding="utf-8")
        if not DATATABLE_RE.search(head):
            continue
        if len(combos["td"]) == 0:
            continue  # already a DataTable consumer at base

        props = [
            f"{'compact' if COMPACT_RE.search(tag) else 'default'}"
            f"{'+sm' if SM_RE.search(tag) else ''}"
            for tag in tags_in(head)
        ]
        seen = set()
        overrides = []
        for m in OVERRIDE_RE.findall(head):
            if m not in seen:
                seen.add(m)
                overrides.append(m)
        seen_cells = set()
        cell_classes = []
        for cm in CELLCELL_RE.finditer(head):
            val = cm.group(1)
            if "cellClass: '" + val + "'" not in seen_cells:
                seen_cells.add("cellClass: '" + val + "'")
                cell_classes.append(val)

        def as_text(mapping):
            return " ".join(
                f"{k}(×{n})"
                for k, n in sorted(mapping.items(), key=lambda kv: kv[1], reverse=True)
            )

        mode = max(combos["td"].items(), key=lambda kv: kv[1])
        left, top = (float(v) for v in mode[0].split("/"))
        if props and props[0].startswith("compact"):
            effective = {"left": 8, "top": 4}
        else:
            effective = {"left": 12, "top": 8}

        def signed(n):
            return f"{'+' if n >= 0 else ''}{fmt_num(n)}px"

        rows.append(
            {
                "file": file.replace("src/components/", "", 1),
                "cells": len(removed),
                "combos": as_text(combos["td"]),
                "head": " ".join(props) or "—",
                "delta": (
                    f"{signed(effective['left'] - left)} left, "
                    f"{signed(effective['top'] - top)} top"
                ),
                "hooks": " ".join([*overrides, *cell_classes]) or "",
            }
        )

    print(
        f"# grid migration census: {BASE}..{HEAD}  (base cell padding → DataTable's)\n"
    )
    print(
        f"{w('file', 42)} {w('cells', 5)} "
        f"{w('base px/py (mode, all combos)', 34)} "
        f"{w('head', 18)} {w('delta', 20)} hooks"
    )
    for r in rows:
        print(
            f"{w(r['file'], 42)} {w(r['cells'], 5)} "
            f"{w(r['combos'], 34)} {w(r['head'], 18)} {w(r['delta'], 20)} {r['hooks']}"
        )


main()
