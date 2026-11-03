#!/usr/bin/env python3
"""Build the shipped glogger.Personal-free release flavour (identifier glogger.Release).

Python port of scripts/release-build.sh (stdlib only).
"""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main(argv: list[str]) -> int:
    configs = ["--config", "src-tauri/tauri.release.conf.json"]
    if not os.environ.get("TAURI_SIGNING_PRIVATE_KEY", ""):
        print(
            "note: TAURI_SIGNING_PRIVATE_KEY unset -> "
            "building without updater artifacts (local install only)"
        )
        configs += ["--config", '{"bundle":{"createUpdaterArtifacts":false}}']
    cmd = ["node", "node_modules/@tauri-apps/cli/tauri.js", "build", *configs, *argv]
    proc = subprocess.run(cmd, check=False, cwd=ROOT)
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
