#!/usr/bin/env python3
"""Gate: migration chain integrity + one-migration-per-PR.

Two checks over `src-tauri/src/db/migrations.rs` (pure-text scan, stdlib only):

1. Chain integrity (always): every `fn migration_v<N>_*` has a matching
   `if current_version < N` block in `run_migrations`, and vice versa. The
   head-pin tests (`run_migrations_is_idempotent_and_atomic`,
   `migration_v82/83_*`) catch a stale head, but nothing catches a new
   function that was never chained (it would silently never run) — this
   closes that hole.

2. One migration per PR (`--added-since [<ref>]`): the versions chained in
   the worktree file, minus those chained in the base file, may number at
   most one (worktree scope, so uncommitted iterations count — fail early
   at commit time). A release bundles many PRs, so "one per release"
   cannot be enforced without serializing all schema work; one-per-PR is
   the enforceable unit: it forces either separate PRs or a deliberate
   pre-merge squash of same-branch iterations (the V91-V94 sprawl this
   gate exists to prevent — four versions for one release batch, one of
   them a never-shipped marker its successor dropped).

Usage:
    python scripts/check-migration-chain.py
    python scripts/check-migration-chain.py --added-since [<ref>]

With no <ref>, the base is the merge-base of HEAD with the upstream
(`@{u}`, `origin/HEAD`, `main` — same shape as lint-markdown-changed.py).
CI passes an explicit base (PR base / push `before`).
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MIGRATIONS = ROOT / "src-tauri" / "src" / "db" / "migrations.rs"

FN_RE = re.compile(r"fn\s+migration_v(\d+)_\w+\s*\(")
CHAIN_RE = re.compile(r"if\s+current_version\s*<\s*(\d+)\s*\{")


def fail(msg: str) -> int:
    print(f"migration-chain: FAIL: {msg}", file=sys.stderr)
    return 1


def check_integrity(text: str) -> int:
    fns = set(FN_RE.findall(text))
    chained = set(CHAIN_RE.findall(text))
    errors = []
    for n in sorted(fns - chained, key=int):
        errors.append(
            f"migration_v{n}_* has no `current_version < {n}` chain block (would never run)"
        )
    for n in sorted(chained - fns, key=int):
        errors.append(
            f"`current_version < {n}` chain block has no migration_v{n}_* function (typo?)"
        )
    if errors:
        return fail("; ".join(errors))
    print(f"migration-chain: OK: {len(fns)} migrations chained")
    return 0


def git(*args: str) -> str:
    proc = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed: {(proc.stderr or '').strip()}"
        )
    return proc.stdout


def resolve_base(explicit: str | None) -> str:
    if explicit:
        return explicit
    # Local default: the PR target branch, NOT the feature upstream. After the
    # feature branch is pushed, `@{u}` resolves to the remote feature tip, so
    # comparing against it undercounts (local says 1 new version, CI-against-main
    # says 2). `origin/HEAD` (default branch, usually main) then `main` mirror
    # the CI explicit-base check; `@{u}` stays last for stacked-diff setups with
    # no default branch resolvable. Stale local `main` overcounts (fail-closed,
    # the documented direction).
    for base_ref in ("origin/HEAD", "main", "@{u}"):
        try:
            base = git("merge-base", "HEAD", base_ref).strip()
        except RuntimeError:
            continue
        if base:
            return base
    raise RuntimeError("no base ref resolves (origin/HEAD, main, @{u})")


def check_added_since(ref: str | None, limit: int = 1) -> int:
    try:
        base = resolve_base(ref)
        base_text = git("show", f"{base}:src-tauri/src/db/migrations.rs")
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as e:
        return fail(str(e))
    base_chained = set(CHAIN_RE.findall(base_text))
    worktree_chained = set(CHAIN_RE.findall(MIGRATIONS.read_text(encoding="utf-8")))
    added = worktree_chained - base_chained
    if len(added) > limit:
        return fail(
            f"{len(added)} new migration versions vs {base[:12]} ({', '.join(sorted(added, key=int))}); "
            f"at most {limit} per PR — split the PR or squash same-branch iterations pre-merge"
        )
    print(f"migration-chain: OK: {len(added)} new migration version(s) vs {base[:12]}")
    return 0


def main(argv: list[str]) -> int:
    text = MIGRATIONS.read_text(encoding="utf-8")
    rc = check_integrity(text)
    if rc:
        return rc
    if "--added-since" in argv:
        idx = argv.index("--added-since")
        ref = argv[idx + 1] if idx + 1 < len(argv) else None
        return check_added_since(ref)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
