#!/usr/bin/env python3
"""Strip a Player.log down to LocalPlayer lines (port of minimize-player-log.sh)."""

from __future__ import annotations

import os
import sys


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        sys.stderr.buffer.write(f"Usage: {argv[0]} <input.log>\n".encode())
        return 1
    inp = argv[1]
    d = os.path.dirname(inp)
    if not d:
        d = "."
    base = os.path.basename(inp)
    if base.endswith(".log"):
        base = base[: -len(".log")]
    out = d + "/" + base + "-minimized.log"

    with open(inp, "rb") as f:
        raw = f.read()
    original = raw.count(b"\n")
    kept: list[bytes] = []
    for line in raw.splitlines(keepends=True):
        body = line[:-1] if line.endswith(b"\n") else line
        if b"LocalPlayer:" in body and b"ProcessMusicPerformance" not in body:
            kept.append(body + b"\n")
    with open(out, "wb") as f:
        f.writelines(kept)
    minimized = len(kept)
    sys.stdout.buffer.write(f"Done: {original} -> {minimized} lines\n".encode())
    sys.stdout.buffer.write(f"Saved to: {out}\n".encode())
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
