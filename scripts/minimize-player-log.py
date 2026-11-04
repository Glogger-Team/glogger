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
    base = os.path.basename(inp).removesuffix(".log")
    out = d + "/" + base + "-minimized.log"

    original = 0
    minimized = 0
    with open(inp, "rb") as f, open(out, "wb") as fh:
        for raw_line in f:
            original += raw_line.count(b"\n")
            body = raw_line[:-1] if raw_line.endswith(b"\n") else raw_line
            if b"LocalPlayer:" in body and b"ProcessMusicPerformance" not in body:
                fh.write(body + b"\n")
                minimized += 1
    sys.stdout.buffer.write(f"Done: {original} -> {minimized} lines\n".encode())
    sys.stdout.buffer.write(f"Saved to: {out}\n".encode())
    if minimized == 0:
        sys.stderr.write("minimize-player-log: no LocalPlayer lines retained\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
