"""Fail-closed cargo-audit gate: blocks on critical/high (CVSS >= 7.0) advisories.

Runs `cargo audit --json` against src-tauri/Cargo.lock, scores each advisory
via its inline CVSS vector, and falls back to an OSV lookup per locked version
for unrated advisories. Partial OSV failure with no blocking score fails
closed. Documented `--ignore` advisories below are the single source of truth
for both CI (`lint.yml: cargo-audit`) and local runs (`mise run lint`).

Usage (from anywhere; paths resolve relative to this script):
    python scripts/cargo-audit-gate.py [--summary FILE]
"""

import argparse
import json
import math
import subprocess
import sys
import urllib.request
from pathlib import Path

THRESHOLD = 7.0

# Documented ignores (re-evaluate if bundle formats change):
# RUSTSEC-2026-0367/0368/0369 (zip 4.6.1 via tauri-plugin-updater):
# zip extraction is unreachable — the updater endpoint serves
# NSIS/DMG/AppImage only (updaterJsonPreferNsis), never zips.
# No patched 4.x exists; updater hasn't migrated to zip 9.
IGNORED_ADVISORIES = [
    "RUSTSEC-2026-0367",
    "RUSTSEC-2026-0368",
    "RUSTSEC-2026-0369",
]


def cvss_base(vector):
    """Base score from a CVSS v3.x vector string (spec equation)."""
    try:
        tail = vector.split("CVSS:3.", 1)[1].split("/", 1)[1]
    except IndexError:
        return None
    m = dict(part.split(":", 1) for part in tail.split("/"))
    # metric weights per CVSS v3.1 spec
    w = {
        "AV": {"N": 0.85, "A": 0.62, "L": 0.55, "P": 0.2},
        "AC": {"L": 0.77, "H": 0.44},
        "PR": {"N": 0.85, "L": 0.62, "H": 0.27, "L:C": 0.68, "H:C": 0.5},
        "UI": {"N": 0.85, "R": 0.62},
        "C": {"H": 0.56, "L": 0.22, "N": 0},
        "I": {"H": 0.56, "L": 0.22, "N": 0},
        "A": {"H": 0.56, "L": 0.22, "N": 0},
    }
    try:
        iss = 1 - (1 - w["C"][m["C"]]) * (1 - w["I"][m["I"]]) * (1 - w["A"][m["A"]])
        scope_changed = m.get("S") == "C"
        impact = (
            7.52 * (iss - 0.029) - 3.25 * pow(iss - 0.02, 15)
            if scope_changed
            else 6.42 * iss
        )
        pr_key = m["PR"] + (":C" if scope_changed else "")
        expl = (
            8.22
            * w["AV"][m["AV"]]
            * w["AC"][m["AC"]]
            * w["PR"].get(pr_key, w["PR"][m["PR"]])
            * w["UI"][m["UI"]]
        )
        base = (
            min(1.08 * (impact + expl), 10) if scope_changed else min(impact + expl, 10)
        )
        return math.ceil(base * 10) / 10
    except (KeyError, IndexError, ValueError):
        return None


def osv_scores(pkg, version):
    """All CVSS base scores OSV reports for (package, version); [] if unknown."""
    try:
        req = urllib.request.Request(
            "https://api.osv.dev/v1/query",
            data=json.dumps(
                {
                    "package": {"name": pkg, "ecosystem": "crates.io"},
                    "version": version,
                }
            ).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=20) as r:
            scores = []
            for v in json.load(r).get("vulns", []):
                for s in v.get("severity", []):
                    if s.get("type") == "CVSS_V3" and s.get("score"):
                        parsed = cvss_base(s["score"])
                        if parsed is not None:
                            scores.append(parsed)
            return scores
    except (OSError, ValueError, KeyError) as e:
        print(f"::warning::OSV lookup failed for {pkg}@{version}: {e}")
        return None


def main():
    root = Path(__file__).resolve().parent.parent
    crate_dir = root / "src-tauri"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--summary", default=None, help="append markdown report here")
    args = parser.parse_args()

    cmd = ["cargo", "audit"]
    for adv in IGNORED_ADVISORIES:
        cmd += ["--ignore", adv]
    cmd += ["--json"]
    try:
        proc = subprocess.run(
            cmd, check=False, cwd=crate_dir, capture_output=True, text=True
        )
    except FileNotFoundError:
        print(
            "error: cargo-audit not installed — run: cargo install cargo-audit --locked",
            file=sys.stderr,
        )
        return 1
    try:
        report = json.loads(proc.stdout)
    except json.JSONDecodeError:
        print("error: cargo audit produced no JSON output:", file=sys.stderr)
        print(proc.stderr or proc.stdout, file=sys.stderr)
        return 1

    lock_versions, current = {}, None
    with open(crate_dir / "Cargo.lock") as lock:
        lock_lines = lock.readlines()
    for line in lock_lines:
        if line.startswith("name = "):
            current = line.split('"')[1]
        elif line.startswith("version = ") and current:
            lock_versions.setdefault(current, []).append(line.split('"')[1])
            current = None

    blocking, notes, unknowns = [], [], []
    for entry in report["vulnerabilities"]["list"]:
        adv = entry["advisory"]
        pkg = adv["package"]
        score = cvss_base(adv["cvss"]) if adv.get("cvss") else None
        versions = lock_versions.get(pkg, [])
        if score is None and versions:
            scored, failed_versions = [], []
            for ver in versions:
                res = osv_scores(pkg, ver)
                if res is None:
                    failed_versions.append(ver)
                else:
                    scored.extend(res)
            if scored:
                score = max(scored)
            if failed_versions and (score is None or score < THRESHOLD):
                # Partial data with no blocking score: fail closed.
                unknowns.append((adv, failed_versions))
                continue
        patched = bool(adv.get("patched"))
        label = f"{adv['id']} {pkg}: {adv['title']} (CVSS {score if score is not None else 'unrated'})"
        (blocking if score is not None and score >= THRESHOLD else notes).append(
            (label, patched)
        )
    for adv, versions in unknowns:
        label = (
            f"{adv['id']} {adv['package']}: {adv['title']} "
            f"(OSV lookup failed for {', '.join(versions)} — treating as blocking)"
        )
        blocking.append((label, bool(adv.get("patched"))))

    lines = ["## cargo audit", ""]
    for label, patched in blocking:
        lines.append(
            f"- CRITICAL/HIGH {label} {'[patch available]' if patched else '[no patch — add --ignore with justification]'}"
        )
    for label, patched in notes:
        lines.append(f"- advisory (below threshold, non-blocking) {label}")
    if not blocking and not notes and not unknowns:
        lines.append("No advisories.")
    summary = "\n".join(lines) + "\n"
    if args.summary:
        with open(args.summary, "a") as f:
            f.write(summary)
    else:
        print(summary, end="")
    if blocking:
        print("::error::cargo audit found critical/high advisories (see summary)")
        for label, _ in blocking:
            print("::error::" + label)
        return 1
    print(f"cargo audit: {len(notes)} below-threshold advisories noted, none blocking")
    return 0


if __name__ == "__main__":
    sys.exit(main())
