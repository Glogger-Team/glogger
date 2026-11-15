#!/usr/bin/env python3
"""Gate: version-file sync between the bumper and the release sync PR.

Two invariants the 1.12.0 lock drift (fixed in #193) proved need a check:

1. Staged-set coverage: every file scripts/bump-version.py mutates on disk
   must appear in the `git add` list of the `sync-dev-version` job in
   .github/workflows/release.yml. The drift froze package-lock.json at
   1.12.0 across five releases because the sync PR staged everything
   except the lock.
2. Stamp consistency: the version stamps in package.json,
   package-lock.json (top-level + packages[""]), src-tauri/tauri.conf.json,
   src-tauri/Cargo.toml ([package] version), and the glogger entry in
   src-tauri/Cargo.lock must all agree on the committed tree.

Marker contract: MUTATED maps each produced file to string literals that
must appear in bump-version.py's source. The markers are rename-tripwires,
not proof of writes — if the bumper is extended to a new file, or a path
literal is renamed, this gate fails closed until the map is updated in the
same commit.

Exit 0: clean. Exit 1: drift found (names the files/stamps). Exit 2: tool
error (unreadable/missing input, unparseable producer) — a broken search
never reads as a pass.

Self-test: `python scripts/check-version-sync.py --self-test` runs the
live-tree gate, then synthetic red/green fixtures in a temp dir (wired
into `mise run lint:version-sync`).
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Repo-relative path -> literals required in bump-version.py source.
MUTATED = {
    "src-tauri/tauri.conf.json": ('"tauri.conf.json"',),
    "src-tauri/tauri.release.conf.json": ('"tauri.release.conf.json"',),
    "src-tauri/tauri.experimental.conf.json": ('"tauri.experimental.conf.json"',),
    "package.json": ('"package.json"',),
    "package-lock.json": ('"package-lock.json"',),
    "src-tauri/Cargo.toml": ('"Cargo.toml"',),
    # Refreshed best-effort via `cargo update -p glogger` in src-tauri/:
    # markers pin the invocation, not a path literal (there is none).
    "src-tauri/Cargo.lock": ('"cargo"', '"update"', '"glogger"'),
}

VERSION_RE = re.compile(r'"version"\s*:\s*"(\d+\.\d+\.\d+)"')
LOCK_TOP_RE = re.compile(r'^  "version": "(\d+\.\d+\.\d+)",?\s*$', re.MULTILINE)
LOCK_PKG_RE = re.compile(r'^      "version": "(\d+\.\d+\.\d+)",?\s*$', re.MULTILINE)
CARGO_TOML_RE = re.compile(r'^version = "(\d+\.\d+\.\d+)"\s*$', re.MULTILINE)
CARGO_LOCK_RE = re.compile(r'name = "glogger"\r?\nversion = "(\d+\.\d+\.\d+)"')
GIT_ADD_RE = re.compile(r"^\s*git add\s")


def staged_set(release_yml: str) -> set[str] | None:
    """Parse the sync `git add` continuation block into staged paths.

    Line-based (not one regex): tolerates CRLF worktrees and trailing
    whitespace after the backslashes, either of which silently truncates
    a `\\\n`-anchored match to the first line.
    """
    text = release_yml.replace("\r\n", "\n")
    lines = text.split("\n")
    blocks: list[str] = []
    i = 0
    while i < len(lines):
        if GIT_ADD_RE.match(lines[i]) and "tauri.conf.json" in lines[i]:
            body = GIT_ADD_RE.sub("", lines[i])
            while body.rstrip().endswith("\\") and i + 1 < len(lines):
                i += 1
                body = body.rstrip()[:-1] + " " + lines[i].strip()
            blocks.append(body)
        i += 1
    if len(blocks) != 1:
        return None
    tokens = blocks[0].split()
    return {t for t in tokens if t not in ("git", "add")}


def check_staged(root: Path) -> int:
    bumper = read(root, "scripts/bump-version.py")
    workflow = read(root, ".github/workflows/release.yml")
    if bumper is None or workflow is None:
        print(
            "version-sync: cannot read bump-version.py or release.yml",
            file=sys.stderr,
        )
        return 2
    for rel, markers in MUTATED.items():
        for marker in markers:
            if marker not in bumper:
                print(
                    "version-sync: bump-version.py no longer contains "
                    + marker
                    + " (expected producer of "
                    + rel
                    + ") — update MUTATED in scripts/check-version-sync.py",
                    file=sys.stderr,
                )
                return 2
    staged = staged_set(workflow)
    if staged is None:
        print(
            "version-sync: sync `git add` block not found exactly once in release.yml",
            file=sys.stderr,
        )
        return 2
    missing = sorted(set(MUTATED) - staged)
    extra = sorted(staged - set(MUTATED))
    fail = 0
    for rel in missing:
        print(
            "version-sync: "
            + rel
            + " mutated by bump-version.py but missing "
            + "from the release sync `git add` (the #193 drift shape)"
        )
        fail = 1
    for rel in extra:
        print(
            "version-sync: "
            + rel
            + " staged by the release sync but unknown "
            + "to bump-version.py — add its marker or drop the staging"
        )
        fail = 1
    return fail


def first(pattern: re.Pattern[str], text: str | None) -> str | None:
    """First capture group, or None when the file is missing or stamp-less."""
    m = pattern.search(text) if text else None
    return m.group(1) if m else None


def stamp_versions(root: Path) -> dict[str, str | None]:
    """Read every committed version stamp; None where unreadable."""
    package = read(root, "package.json")
    lock = read(root, "package-lock.json")
    tauri = read(root, "src-tauri/tauri.conf.json")
    cargo_toml = read(root, "src-tauri/Cargo.toml")
    cargo_lock = read(root, "src-tauri/Cargo.lock")
    return {
        "package.json": first(VERSION_RE, package),
        "package-lock.json(top)": first(LOCK_TOP_RE, lock),
        'package-lock.json(packages[""])': first(LOCK_PKG_RE, lock),
        "src-tauri/tauri.conf.json": first(VERSION_RE, tauri),
        "src-tauri/Cargo.toml": first(CARGO_TOML_RE, cargo_toml),
        "src-tauri/Cargo.lock": first(CARGO_LOCK_RE, cargo_lock),
    }


def read(root: Path, rel: str) -> str | None:
    try:
        return (root / rel).read_text(encoding="utf-8")
    except OSError:
        return None


def check_stamps(root: Path) -> int:
    stamps = stamp_versions(root)
    unreadable = sorted(k for k, v in stamps.items() if v is None)
    if unreadable:
        print(
            "version-sync: unreadable version stamp in: " + ", ".join(unreadable),
            file=sys.stderr,
        )
        return 2
    values = sorted(set(stamps.values()))
    if len(values) > 1:
        print("version-sync: version stamps disagree on the committed tree:")
        for name in sorted(stamps):
            print("  " + name + ": " + str(stamps[name]))
        return 1
    return 0


def live_gate(root: Path) -> int:
    fail = check_staged(root)
    stamp_rc = check_stamps(root)
    if stamp_rc == 2:
        return 2
    return max(fail, stamp_rc)


GOOD_BUMP = """\
tauri_conf = root / "src-tauri" / "tauri.conf.json"
tauri_release_conf = root / "src-tauri" / "tauri.release.conf.json"
tauri_experimental_conf = root / "src-tauri" / "tauri.experimental.conf.json"
package_json = root / "package.json"
package_lock = root / "package-lock.json"
cargo_toml = root / "src-tauri" / "Cargo.toml"
# 6. Cargo.lock via cargo, failure ignored.
subprocess.run(["cargo", "update", "-p", "glogger"])
"""

GOOD_ADD = (
    "git add package.json package-lock.json src-tauri/tauri.conf.json \\\n"
    "            src-tauri/tauri.release.conf.json \\\n"
    "            src-tauri/tauri.experimental.conf.json "
    "src-tauri/Cargo.toml \\\n"
    "            src-tauri/Cargo.lock\n"
)


def fixture_tree(
    tmp: Path,
    version: str = "1.16.0",
    lock_pkg_version: str | None = None,
    stage_lock: bool = True,
    bump_text: str | None = None,
    drop: str | None = None,
    package_text: str | None = None,
    double_add: bool = False,
) -> None:
    """Write a minimal synthetic repo exercising the real parsers."""
    (tmp / "scripts").mkdir(parents=True, exist_ok=True)
    (tmp / "src-tauri").mkdir(parents=True, exist_ok=True)
    (tmp / ".github" / "workflows").mkdir(parents=True, exist_ok=True)
    (tmp / "scripts" / "bump-version.py").write_text(
        bump_text if bump_text is not None else GOOD_BUMP,
        encoding="utf-8",
    )
    add_line = GOOD_ADD
    if not stage_lock:
        add_line = GOOD_ADD.replace(" package-lock.json", "")
    if double_add:
        add_line = add_line + "          " + GOOD_ADD
    (tmp / ".github" / "workflows" / "release.yml").write_text(
        "jobs:\n  sync-dev-version:\n    steps:\n      - run: |\n          " + add_line,
        encoding="utf-8",
    )
    pkg_version = lock_pkg_version if lock_pkg_version is not None else version
    files = {
        "package.json": package_text
        if package_text is not None
        else '{\n  "version": "' + version + '"\n}\n',
        "package-lock.json": (
            '{\n  "name": "glogger",\n  "version": "'
            + version
            + '",\n  "packages": {\n    "": {\n      "version": "'
            + pkg_version
            + '"\n    }\n  }\n}\n'
        ),
        "src-tauri/tauri.conf.json": '{\n  "version": "' + version + '"\n}\n',
        "src-tauri/Cargo.toml": '[package]\nversion = "' + version + '"\n',
        "src-tauri/Cargo.lock": (
            '[[package]]\nname = "glogger"\nversion = "' + version + '"\n'
        ),
    }
    for rel, text in files.items():
        if rel != drop:
            (tmp / rel).write_text(text, encoding="utf-8")


def self_test() -> int:
    print("version-sync self-test: red lines below are the fixtures failing on purpose")
    cases = [
        ("clean tree passes", {}, 0),
        ("stale lock stamp fails", {"lock_pkg_version": "9.9.9"}, 1),
        ("unstaged lock fails (#193 shape)", {"stage_lock": False}, 1),
        (
            "renamed producer literal fails closed",
            {"bump_text": GOOD_BUMP.replace('"Cargo.toml"', '"Manifest.toml"')},
            2,
        ),
        ("missing file fails closed", {"drop": "package.json"}, 2),
        (
            "readable stamp-less file fails closed",
            {"package_text": '{"name": "glogger"}\n'},
            2,
        ),
        (
            "tool error dominates drift",
            {"lock_pkg_version": "9.9.9", "double_add": True},
            2,
        ),
    ]
    passed = 0
    for name, kwargs, expect in cases:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            fixture_tree(tmp, **kwargs)
            got = live_gate(tmp)
            ok = got == expect
            if not ok:
                print("self-test FAIL: " + name + ": exited " + str(got))
                continue
            passed += 1
    print("version-sync self-test: " + str(passed) + " passed")
    return 0 if passed == len(cases) else 1


def main(argv: list[str]) -> int:
    if len(argv) > 1 and argv[1] == "--self-test":
        live_rc = live_gate(ROOT)
        if live_rc != 0:
            print(
                "version-sync self-test: live tree gate failed first",
                file=sys.stderr,
            )
            return live_rc
        return self_test()
    return live_gate(ROOT)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
