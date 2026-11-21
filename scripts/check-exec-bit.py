#!/usr/bin/env python3
"""Gate: new scripts must carry the owner-exec bit, and executable scripts
must open with a shebang.

Scope: files new to the worktree (untracked + staged-added). Only *.py and
*.sh are considered (the repo's script languages; CI invokes via
interpreters so the bit is conventional). For staged-added files the git
index mode is authoritative (on core.fileMode=false boxes the worktree bit
is decorative); untracked files fall back to os.access.

The shebang half exists because ruff EXE002 cannot fire on Windows
checkouts (FS modes are always 644, so nothing looks executable to ruff);
the index mode this gate reads is the only local signal, and CI's Linux
checkout flags the same files. Check both halves before pushing scripts.

CI passes --base <sha> (the PR base / push `before`; the one sanctioned
invocation difference, same as the markdown job): scope is then the files
added between base and HEAD, because a clean CI checkout has no untracked
or staged files - the local scope would be empty and the gate could never
fire. An all-zero base (manual dispatch) falls back to HEAD~1, same as the
markdown script. In-range files are committed, so the HEAD index mode
(`git ls-files -s`) is authoritative for them.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT_EXTS = {".py", ".sh"}


def run_git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def git_lines(*args: str) -> list[str]:
    proc = run_git(*args)
    if proc.returncode != 0:
        print(
            f"check-exec-bit: git {' '.join(args)} failed: {proc.stderr.strip()}",
            file=sys.stderr,
        )
        raise SystemExit(2)
    return [ln for ln in proc.stdout.splitlines() if ln.strip()]


def resolve_base(explicit: str | None) -> str | None:
    if explicit is None:
        return None
    if explicit.strip("0") == "":
        print(
            "zero base sha (manual dispatch); falling back to HEAD~1",
            file=sys.stderr,
        )
        return "HEAD~1"
    return explicit


def new_files(base: str | None) -> list[str]:
    files: list[str] = []
    if base is not None:
        files += git_lines("diff", "--name-only", "--diff-filter=A", base, "HEAD")
        return files
    files += git_lines("ls-files", "--others", "--exclude-standard")
    files += git_lines("diff", "--cached", "--name-only", "--diff-filter=A")
    seen: set[str] = set()
    out: list[str] = []
    for f in files:
        if f in seen:
            continue
        seen.add(f)
        out.append(f)
    return out


def git_check_stdout(*args: str) -> str:
    """run_git that fails closed: exit 2 on inspection failure so a broken
    git call can never parse as an empty (clean) result."""
    proc = run_git(*args)
    if proc.returncode != 0:
        print(
            f"check-exec-bit: git {' '.join(args)} failed: {proc.stderr.strip()}",
            file=sys.stderr,
        )
        raise SystemExit(2)
    return proc.stdout


def parse_raw_modes(raw_text: str) -> dict[str, tuple[str, str]]:
    """NUL-delimited raw-diff entries (`-z`, never quoted) keyed by path.

    Layout: `:oldmode newmode oldsha newsha status` NUL `old-path`
    [NUL `new-path` for renames/copies] — the destination (last) decides.
    Splits on NUL, never on lines: quoted pathnames (`core.quotePath`)
    would otherwise keep their quotes and defeat the suffix check.
    """
    modes: dict[str, tuple[str, str]] = {}
    parts = raw_text.split("\0")
    i = 0
    while i < len(parts):
        meta = parts[i]
        i += 1
        if not meta.startswith(":"):
            continue
        fields = meta[1:].split(" ")
        if len(fields) < 5:
            continue
        status = fields[4]
        take = 2 if status[:1] in ("R", "C") else 1
        paths = parts[i : i + take]
        i += take
        dest = paths[-1] if paths else ""
        if dest:
            modes[dest] = (fields[0], fields[1])
    return modes


def lost_exec_flips() -> list[str]:
    """Tracked scripts flipped 755->644 in the staged or unstaged diff."""
    flips: dict[str, str] = {}
    staged = parse_raw_modes(
        git_check_stdout("diff", "--cached", "--raw", "-z", "--diff-filter=MR")
    )
    unstaged = parse_raw_modes(
        git_check_stdout("diff", "--raw", "-z", "--diff-filter=MR")
    )
    for path, (old, new) in staged.items():
        if old == "100755" and new == "100644":
            flips[path] = f"staged {old}->{new}"
    for path, (old, new) in unstaged.items():
        if old == "100755" and new == "100644" and path not in flips:
            flips[path] = f"unstaged {old}->{new}"
    return [
        f"{path}: executable bit lost ({how}) - fix: git update-index --chmod=+x {path}"
        for path, how in sorted(flips.items())
        if Path(path).suffix in SCRIPT_EXTS
    ]


def has_shebang(first_line: str) -> bool:
    """Line-1 shebang probe (ruff EXE002 semantics for the executable rule)."""
    return first_line.startswith("#!")


def first_line_of(path: str, in_index: bool, base: str | None) -> str | None:
    """First line of a scoped file: index blob when staged/committed, else the worktree file. Returns None when unreadable (e.g. added then deleted inside the range) — a gone file cannot violate."""
    if in_index:
        rev = "HEAD" if base is not None else ""
        proc = run_git("show", f"{rev}:{path}")
        if proc.returncode != 0:
            return None
        text = proc.stdout
    else:
        try:
            text = (ROOT / path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            return None
    return text.split("\n", 1)[0]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--base",
        default=None,
        help="CI range: files added between base and HEAD "
        "(PR base sha / push before; all-zero falls back to HEAD~1)",
    )
    args = parser.parse_args()
    base = resolve_base(args.base)
    scoped: list[str] = []
    for f in new_files(base):
        if Path(f).suffix in SCRIPT_EXTS and os.path.isfile(ROOT / f):
            scoped.append(f)
    # Index modes look like `100755 <sha> 0\\t<path>`. Staged-added files
    # are in the index, so the recorded mode decides; untracked files are
    # absent and keep the os.access fallback. In-range files are committed,
    # so the HEAD index mode decides for them the same way.
    indexed: dict[str, str] = {}
    for ln in git_lines("ls-files", "-s"):
        left, _, path = ln.partition("	")
        indexed[path] = left.split(" ", 1)[0]
    findings: list[str] = []
    for f in scoped:
        in_index = f in indexed
        executable = (
            (indexed.get(f) == "100755") if in_index else os.access(ROOT / f, os.X_OK)
        )
        if executable:
            first = first_line_of(f, in_index, base)
            if first is not None and not has_shebang(first):
                findings.append(
                    f"{f}: executable but no line-1 shebang - ruff EXE002 fires on CI;"
                    f" Windows checkouts never flag it (FS modes are 644, index mode decides)"
                    f" - fix: add #!/usr/bin/env python3 (or appropriate) as line 1"
                )
        mode = indexed.get(f)
        if mode is not None:
            if mode != "100755":
                where = "committed with" if base is not None else "staged with"
                findings.append(
                    f"{f}: {where} index mode {mode}, not executable"
                    f" - fix: git update-index --chmod=+x {f}"
                )
        elif not os.access(ROOT / f, os.X_OK):
            findings.append(f"{f}: missing exec bit - fix: chmod +x {f}")
    findings += lost_exec_flips()
    for finding in findings:
        print(finding)
    return 1 if findings else 0


def self_test() -> int:
    """Adversarial fixtures: NUL layout, rename order, weird names, and the
    fail-closed git helper (the D-minor lessons — gates ship with probes)."""
    failures = 0

    def check(name: str, cond: bool) -> None:
        nonlocal failures
        print(("PASS " if cond else "FAIL ") + name)
        if not cond:
            failures += 1

    raw = (
        ":100755 100644 aaa bbb M\0scripts/foo.py\0"
        ":100644 100644 a b M\0src/x.rs\0"
        ":100755 100644 a b R100\0scripts/old.sh\0scripts/new.sh\0"
    )
    check(
        "nul flip+rename parse",
        parse_raw_modes(raw)
        == {
            "scripts/foo.py": ("100755", "100644"),
            "src/x.rs": ("100644", "100644"),
            "scripts/new.sh": ("100755", "100644"),
        },
    )
    check("empty diff clean", parse_raw_modes("") == {})
    check(
        "embedded quote harmless",
        parse_raw_modes(':100755 100644 a b M\0scripts/we"ird.py\0')
        == {'scripts/we"ird.py': ("100755", "100644")},
    )
    check("shebang present", has_shebang("#!/usr/bin/env python3"))
    check("shebang missing on docstring", not has_shebang('"""Docstring first.'))
    check("shebang missing on empty", not has_shebang(""))
    try:
        git_check_stdout("diff", "--raw", "--invalid-flag-xyz")
        check("bad git exits 2", False)
    except SystemExit as e:
        check("bad git exits 2", e.code == 2)
    return 1 if failures else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    sys.exit(main())
