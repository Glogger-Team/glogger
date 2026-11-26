#!/usr/bin/env python3
"""Install the project's git hooks.

Pre-commit-managed stages (pre-commit + pre-push + commit-msg, defined in
.pre-commit-config.yaml) via the mise-provided runner, plus a directly
written post-checkout hook that re-runs `npm install` when the lockfile
moved under a stale node_modules (pre-commit cannot manage post-checkout:
unsupported hook type, so the pre-commit-managed hooks are untouched).

Idempotent: safe to re-run. Skipped-locally work is never silent — CI runs
the same mise tasks, and `git commit/push --no-verify` bypasses loudly.

Handoff with scripts/new_worktree.py: cut-time `mise exec -- npm install`
covers the fresh checkout; this post-checkout hook covers everything after
(branch switches, lockfile updates, stale-branch reuse).

Port of scripts/setup-hooks.sh (stdlib only, works in cmd/PowerShell/
git-bash alike).

Usage: python scripts/setup-hooks.py [--self-test]
"""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Fail-open post-checkout hook: never blocks a checkout (every failure path
# exits 0 with a one-line stderr note), fires only on branch checkouts
# ($3 == 1), and installs only when node_modules is missing or older than
# package-lock.json (npm's own stamp lives at node_modules/.package-lock.json).
POST_CHECKOUT_SH = """#!/bin/sh
# post-checkout: re-run npm install when the lockfile moved under a stale
# node_modules. Installed by scripts/setup-hooks.py (pre-commit cannot
# manage post-checkout). Fail-open: never blocks a checkout.
if [ "$3" != "1" ]; then
  exit 0
fi
top=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "post-checkout: no toplevel, skipping npm sync" >&2; exit 0; }
cd "$top" || { echo "post-checkout: cannot cd to toplevel, skipping npm sync" >&2; exit 0; }
if [ -d node_modules ] && [ -f node_modules/.package-lock.json ] && [ ! package-lock.json -nt node_modules/.package-lock.json ]; then
  exit 0
fi
if command -v mise >/dev/null 2>&1; then
  mise exec -- npm install --no-audit --no-fund || { echo "post-checkout: npm install failed, continuing" >&2; exit 0; }
elif command -v npm >/dev/null 2>&1; then
  npm install --no-audit --no-fund || { echo "post-checkout: npm install failed, continuing" >&2; exit 0; }
elif command -v npm.cmd >/dev/null 2>&1; then
  npm.cmd install --no-audit --no-fund || { echo "post-checkout: npm install failed, continuing" >&2; exit 0; }
else
  echo "post-checkout: no npm toolchain found, skipping npm sync" >&2
  exit 0
fi
exit 0
"""


def hooks_dir() -> Path:
    proc = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--git-path", "hooks"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"git rev-parse --git-path hooks: {proc.stderr.strip()}")
    p = Path(proc.stdout.strip())
    # Linked worktrees resolve to the main repo hooks dir (absolute); a bare
    # "hooks"/".git/hooks" resolves against the repo root.
    return p if p.is_absolute() else ROOT / p


def install_post_checkout() -> Path:
    target = hooks_dir() / "post-checkout"
    target.parent.mkdir(parents=True, exist_ok=True)
    # Idempotent: one overwrite leaves exactly one working hook.
    target.write_text(POST_CHECKOUT_SH, encoding="utf-8", newline="\n")
    target.chmod(target.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return target


def self_test() -> int:
    """Pin the hook contract in /tmp scratch (never in the repo).

    Static pins on the embedded body plus live drills of the real hook text
    in temp git repos with a fake toolchain on PATH: stale/missing
    node_modules triggers exactly one install; clean tree exits silent;
    file checkouts ($3 == 0) never install; missing toolchain still exits 0.
    """
    import re

    passed = 0
    total = 0

    def check(name: str, ok: bool, detail: str = "") -> None:
        nonlocal passed, total
        total += 1
        if ok:
            passed += 1
        else:
            print(f"self-test FAIL: {name} {detail}")

    check("branch guard", 'if [ "$3" != "1" ]' in POST_CHECKOUT_SH)
    check("stamp check", "node_modules/.package-lock.json" in POST_CHECKOUT_SH)
    check(
        "mise preferred",
        POST_CHECKOUT_SH.index("command -v mise")
        < POST_CHECKOUT_SH.index("command -v npm"),
    )
    check("npm.cmd fallback", "npm.cmd" in POST_CHECKOUT_SH)
    check("no blocking exit", not re.search(r"exit [1-9]", POST_CHECKOUT_SH))
    check(
        "pre-commit hooks untouched",
        "pre-commit" not in POST_CHECKOUT_SH
        or "cannot\n# manage post-checkout" in POST_CHECKOUT_SH,
    )

    def make_repo(with_mise: bool = True) -> tuple[Path, Path]:
        tmp = Path(tempfile.mkdtemp(prefix="post-checkout-drill-"))
        bindir = tmp / "bin"
        bindir.mkdir()
        marker = tmp / "install-marker"
        if with_mise:
            (bindir / "mise").write_text(
                "#!/bin/sh\n"
                f'echo "mise $@" >> "{marker.as_posix()}"\n'
                "mkdir -p node_modules\n"
                "cp package-lock.json node_modules/.package-lock.json\n",
                encoding="utf-8",
            )
            os.chmod(bindir / "mise", 0o755)
        repo = tmp / "repo"
        repo.mkdir()
        env = dict(
            os.environ,
            PATH=f"{bindir.as_posix()}{os.pathsep}{os.environ.get('PATH', '')}",
        )
        subprocess.run(["git", "init", "-q"], cwd=repo, check=True, env=env)
        subprocess.run(
            ["git", "config", "user.email", "t@t"],
            cwd=repo,
            check=True,
            env=env,
        )
        subprocess.run(
            ["git", "config", "user.name", "t"], cwd=repo, check=True, env=env
        )
        (repo / "package-lock.json").write_text('{"v":1}\n', encoding="utf-8")
        hook = repo / "post-checkout"
        hook.write_text(POST_CHECKOUT_SH, encoding="utf-8", newline="\n")
        os.chmod(hook, 0o755)
        subprocess.run(
            ["git", "add", "package-lock.json"],
            cwd=repo,
            check=True,
            env=env,
            capture_output=True,
        )
        subprocess.run(
            ["git", "commit", "-qm", "init"],
            cwd=repo,
            check=True,
            env=env,
            capture_output=True,
        )
        return repo, marker

    def run_hook(
        repo: Path, flag: str, extra_path: str
    ) -> subprocess.CompletedProcess[str]:
        env = dict(
            os.environ, PATH=f"{extra_path}{os.pathsep}{os.environ.get('PATH', '')}"
        )
        return subprocess.run(
            ["sh", str(repo / "post-checkout"), "HEAD", "HEAD", flag],
            cwd=repo,
            capture_output=True,
            text=True,
            check=False,
            env=env,
        )

    # Drill 1: missing node_modules triggers exactly one install.
    repo, marker = make_repo()
    bindir = str(repo.parent / "bin")
    proc = run_hook(repo, "1", bindir)
    check(
        "missing node_modules installs",
        proc.returncode == 0
        and marker.exists()
        and len(marker.read_text().splitlines()) == 1,
        f"rc={proc.returncode} stderr={proc.stderr.strip()}",
    )
    # Drill 2: clean tree exits silent (fast path).
    start = time.monotonic()
    proc = run_hook(repo, "1", bindir)
    elapsed = time.monotonic() - start
    check(
        "clean tree silent fast-path",
        proc.returncode == 0 and proc.stdout == "" and proc.stderr == "",
        f"rc={proc.returncode} out={proc.stdout!r} err={proc.stderr!r}",
    )
    print(f"clean-tree no-op time: {elapsed:.3f}s")
    check("clean tree one install total", len(marker.read_text().splitlines()) == 1)
    # Drill 3: file checkout never installs, even when stale.
    (repo / "package-lock.json").write_text('{"v":2}\n', encoding="utf-8")
    proc = run_hook(repo, "0", bindir)
    check(
        "file checkout skips install",
        proc.returncode == 0 and len(marker.read_text().splitlines()) == 1,
        f"rc={proc.returncode} err={proc.stderr.strip()}",
    )
    # Drill 4: stale lockfile on branch checkout reinstalls once.
    proc = run_hook(repo, "1", bindir)
    check(
        "stale lockfile reinstalls",
        proc.returncode == 0 and len(marker.read_text().splitlines()) == 2,
        f"rc={proc.returncode} err={proc.stderr.strip()}",
    )
    # Drill 5: no toolchain fails open.
    shutil.rmtree(repo / "node_modules")
    (repo / "package-lock.json").write_text('{"v":3}\n', encoding="utf-8")
    empty = Path(tempfile.mkdtemp(prefix="post-checkout-nop-"))
    env = dict(os.environ, PATH=empty.as_posix())
    proc = subprocess.run(
        ["sh", str(repo / "post-checkout"), "HEAD", "HEAD", "1"],
        cwd=repo,
        capture_output=True,
        text=True,
        check=False,
        env=env,
    )
    check(
        "missing toolchain exits 0 with one line",
        proc.returncode == 0 and len(proc.stderr.strip().splitlines()) == 1,
        f"rc={proc.returncode} err={proc.stderr.strip()!r}",
    )

    print(f"setup-hooks self-test: {passed}/{total} passed")
    return 0 if passed == total else 1


def main(argv: list[str] | None = None) -> int:
    # Self-test: `python scripts/setup-hooks.py --self-test` pins the
    # post-checkout contract (static pins + live /tmp drills above).
    if (argv or sys.argv[1:])[:1] == ["--self-test"]:
        return self_test()
    proc = subprocess.run(
        ["git", "-C", str(ROOT), "config", "--get", "core.hooksPath"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    # Clean up the pre-framework layout: .githooks/ no longer exists.
    if proc.returncode == 0 and proc.stdout.strip() == ".githooks":
        unset = subprocess.run(
            ["git", "-C", str(ROOT), "config", "--unset", "core.hooksPath"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        if unset.returncode != 0:
            print(f"setup-hooks: {unset.stderr.strip()}", file=sys.stderr)
            return 1
        print("Removed legacy core.hooksPath=.githooks.")
    install = subprocess.run(
        [
            "mise",
            "exec",
            "--",
            "pre-commit",
            "install",
            "--hook-type",
            "pre-commit",
            "--hook-type",
            "pre-push",
            "--hook-type",
            "commit-msg",
        ],
        cwd=ROOT,
        check=False,
    )
    if install.returncode != 0:
        return install.returncode
    print("pre-commit hooks installed.")
    try:
        target = install_post_checkout()
    except RuntimeError as exc:
        print(f"setup-hooks: {exc}", file=sys.stderr)
        return 1
    print(f"post-checkout hook installed: {target}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
