#!/usr/bin/env python3
"""Gate: lint-parity drift between the places that name checks.

`mise.toml` owns the check list — `.pre-commit-config.yaml` entries and
`AGENTS.md` Linting-table mentions must resolve to tasks it defines.
Also guards the TODO.md location contract (grid census + docs index
must agree with the file on disk).

`--push` (pre-push hook via `lint:parity-push`): fail closed listing any
source file that differs between worktree and HEAD before checking
anything — the checks read worktree bytes but certify the pushed
revision, so divergent inputs would mask the committed content (same
rule as `lint-markdown-changed.py --push`). The commit hook never runs
`--push`: config edits are legitimately divergent at commit time.

Port of scripts/check-lint-parity.sh (stdlib only, works in
cmd/PowerShell/git-bash alike; no grep/sed/awk subprocesses).

Stderr/stdout discipline: each stage fails LOUD (exit 2) on tool
error, reports finding (exit 1 via fail=1) on drift, passes silent on
no drift — a broken search never reads as a pass.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

PUSH_GUARD_FILES = [
    ".pre-commit-config.yaml",
    "AGENTS.md",
    "mise.toml",
    "TODO.md",
    "docs/TODO.md",
    "docs/index.md",
    "scripts/check-grid-standards.py",
    "README.md",
    ".github/workflows/lint.yml",
]

# Entry IDs in .pre-commit-config.yaml that must call these mise tasks
# (the hook contract itself, not just whatever refs happen to be present).
HOOK_CONTRACT = (("lint-fast", "lint:fast"), ("lint-full", "lint"))

# Slices that must never sit in lint:fast (commit stage stays seconds-scale).
HEAVY_SLICES = (
    "lint:build",
    "lint:typecheck",
    "lint:clippy",
    "lint:test",
    "lint:audit",
    "lint:constraints",
    "lint:markdown-push",
)

PRE_COMMIT_REF_RE = re.compile(r"mise run ([a-z0-9:.\-]+)")
HOOK_ID_RE = re.compile(r"id:\s*(\S+)")
HOOK_ENTRY_RE = re.compile(r"entry:\s*(\S+(?:\s+\S+)*)")
LINT_MENTION_RE = re.compile(r"lint:[a-z0-9-]+")
FAST_TASK_RE = re.compile(r"""['"]lint:[a-z0-9-]+['"]""")
GRID_READS_ROOT = re.compile(
    r"""join\(ROOT,\s*['"]TODO\.md['"]\)|ROOT\s*/\s*["']TODO\.md["']"""
)
GRID_READS_DOCS = re.compile(
    r"""join\(ROOT,\s*['"]docs/TODO\.md['"]\)|ROOT\s*/\s*["']docs["']\s*/\s*["']TODO\.md["']"""
)
STALE_DOCS_TODO_RE = re.compile(r"docs/TODO\.md")
INDEX_LINKS_ROOT = re.compile(r"\(\.\./TODO\.md\)")
INDEX_LINKS_DOCS = re.compile(r"\(TODO\.md\)")


def die(msg: str) -> int:
    print(f"lint-parity: {msg}", file=sys.stderr)
    return 2


def run_git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def push_guard() -> int:
    proc = run_git(
        "diff",
        "--name-only",
        "HEAD",
        "--",
        *PUSH_GUARD_FILES,
    )
    if proc.returncode != 0:
        return die(f"git diff failed: {proc.stderr.strip()}")
    diverged = proc.stdout.strip()
    if diverged:
        print(
            "lint-parity: refusing to lint pushed config with uncommitted "
            "changes (worktree bytes would mask the committed revision):",
            file=sys.stderr,
        )
        for line in diverged.splitlines():
            print(f"  {line}", file=sys.stderr)
        print("Commit (or stash) these files first, then push again.", file=sys.stderr)
        return 1
    return 0


def mise_tasks() -> set[str] | None:
    try:
        proc = subprocess.run(
            ["mise", "tasks", "--name-only"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
    except FileNotFoundError:
        print("lint-parity: mise tasks --name-only failed", file=sys.stderr)
        return None
    if proc.returncode != 0:
        print("lint-parity: mise tasks --name-only failed", file=sys.stderr)
        return None
    tasks = {line.strip() for line in proc.stdout.splitlines() if line.strip()}
    if not tasks:
        print("lint-parity: empty mise task list", file=sys.stderr)
        return None
    return tasks


def check_pre_commit_refs(tasks: set[str]) -> int:
    fail = 0
    path = ROOT / ".pre-commit-config.yaml"
    if not path.is_file():
        print("lint-parity: cannot read .pre-commit-config.yaml", file=sys.stderr)
        return 2
    text = path.read_text(encoding="utf-8")
    for ref in PRE_COMMIT_REF_RE.findall(text):
        if ref not in tasks:
            print(
                f"lint-parity: .pre-commit-config.yaml calls 'mise run {ref}', "
                "not defined in mise.toml"
            )
            fail = 1
    # Hook contract: these entry IDs must call these tasks, so swapping an
    # entry to a non-mise command fails closed instead of passing on the
    # survivors.
    current_id: str | None = None
    id_to_entry: dict[str, str] = {}
    for line in text.splitlines():
        id_match = HOOK_ID_RE.search(line)
        if id_match:
            current_id = id_match.group(1)
            continue
        entry_match = HOOK_ENTRY_RE.search(line)
        if entry_match and current_id is not None:
            id_to_entry[current_id] = entry_match.group(1)
    for want_id, want_task in HOOK_CONTRACT:
        if id_to_entry.get(want_id) != f"mise run {want_task}":
            print(
                f"lint-parity: pre-commit hook '{want_id}' must call "
                f"'mise run {want_task}'"
            )
            fail = 1
    return fail


def linting_section(text: str) -> str:
    lines = text.splitlines()
    in_section = False
    kept: list[str] = []
    for line in lines:
        if line.startswith("## Linting"):
            in_section = True
            continue
        if in_section and line.startswith("## "):
            break
        if in_section:
            kept.append(line)
    return "\n".join(kept)


def check_agents_mentions(tasks: set[str]) -> int:
    fail = 0
    path = ROOT / "AGENTS.md"
    if not path.is_file():
        print("lint-parity: cannot read AGENTS.md", file=sys.stderr)
        return 2
    section = linting_section(path.read_text(encoding="utf-8"))
    for mention in LINT_MENTION_RE.findall(section):
        if mention not in tasks:
            print(f"lint-parity: AGENTS.md names '{mention}', not defined in mise.toml")
            fail = 1
    return fail


def check_todo_location() -> int:
    fail = 0
    root_todo = ROOT / "TODO.md"
    docs_todo = ROOT / "docs" / "TODO.md"
    grid = ROOT / "scripts" / "check-grid-standards.py"
    index = ROOT / "docs" / "index.md"

    def read_if_exists(path: Path) -> str:
        try:
            return path.read_text(encoding="utf-8")
        except OSError:
            return ""

    grid_text = read_if_exists(grid)
    index_text = read_if_exists(index)
    search_roots = [ROOT / "AGENTS.md", ROOT / "README.md"]
    search_roots.append(index)
    search_roots.append(grid)
    search_roots.append(ROOT / ".github" / "workflows" / "lint.yml")
    search_roots.append(ROOT / "mise.toml")
    search_roots.append(ROOT / ".pre-commit-config.yaml")

    stale_hit = any(STALE_DOCS_TODO_RE.search(read_if_exists(p)) for p in search_roots)

    if root_todo.is_file() and docs_todo.is_file():
        print("lint-parity: TODO.md exists at both root and docs/ — pick one")
        return 1
    if root_todo.is_file():
        if not GRID_READS_ROOT.search(grid_text):
            print(
                "lint-parity: root TODO.md exists but check-grid-standards.py "
                "reads elsewhere"
            )
            fail = 1
        if not INDEX_LINKS_ROOT.search(index_text):
            print("lint-parity: root TODO.md exists but docs/index.md links elsewhere")
            fail = 1
        if stale_hit:
            print("lint-parity: stale docs/TODO.md pointer survives the root move")
            fail = 1
    elif docs_todo.is_file():
        if not GRID_READS_DOCS.search(grid_text):
            print(
                "lint-parity: docs/TODO.md exists but check-grid-standards.py "
                "reads elsewhere"
            )
            fail = 1
        if not INDEX_LINKS_DOCS.search(index_text):
            print("lint-parity: docs/TODO.md exists but docs/index.md links elsewhere")
            fail = 1
    else:
        print("lint-parity: no TODO.md at root or docs/ — one must exist")
        fail = 1
    return fail


def fast_stanza_body() -> str | None:
    path = ROOT / "mise.toml"
    if not path.is_file():
        print("lint-parity: cannot read mise.toml", file=sys.stderr)
        return None
    in_fast = False
    body: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip() == '[tasks."lint:fast"]':
            in_fast = True
            continue
        if in_fast and line.startswith("["):
            break
        if in_fast:
            body.append(line)
    text = "\n".join(body).strip()
    if not text:
        print(
            "lint-parity: lint:fast stanza missing or empty in mise.toml",
            file=sys.stderr,
        )
        return None
    return text


def check_fast_weight() -> int:
    fail = 0
    body = fast_stanza_body()
    if body is None:
        return 2
    fast = {m.strip("'\"") for m in FAST_TASK_RE.findall(body)}
    for heavy in HEAVY_SLICES:
        if heavy in fast:
            print(
                f"lint-parity: heavy slice '{heavy}' in lint:fast "
                "(commit stage must stay seconds-scale)"
            )
            fail = 1
    return fail


def main(argv: list[str]) -> int:
    if "--push" in argv[1:]:
        rc = push_guard()
        if rc != 0:
            return rc
    fail = 0
    tasks = mise_tasks()
    if tasks is None:
        return 2
    for check in (check_pre_commit_refs, check_agents_mentions):
        rc = check(tasks)
        if rc == 2:
            return 2
        fail |= rc
    for check in (check_todo_location, check_fast_weight):
        rc = check()
        if rc == 2:
            return 2
        fail |= rc
    return fail


if __name__ == "__main__":
    sys.exit(main(sys.argv))
