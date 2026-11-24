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
    python scripts/check-migration-chain.py --self-test

With no <ref>, the base is the merge-base of HEAD with the PR target
(`origin/HEAD`, then `main`, then `@{u}` — NOT the feature upstream first:
after the branch is pushed, `@{u}` is the remote feature tip and would
undercount). CI passes an explicit base (PR base / push `before`).
"""

from __future__ import annotations

import contextlib
import io
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MIGRATIONS = ROOT / "src-tauri" / "src" / "db" / "migrations.rs"

FN_RE = re.compile(r"^[ \t]*fn\s+migration_v(\d+)_\w+\s*\(", re.MULTILINE)
CHAIN_RE = re.compile(r"^[ \t]*if\s+current_version\s*<\s*(\d+)\s*\{", re.MULTILINE)


def fail(msg: str) -> int:
    print(f"migration-chain: FAIL: {msg}", file=sys.stderr)
    return 1


_STR_RE = re.compile(r'"(?:[^"\\]|\\.)*"')
_CHAR_RE = re.compile(
    r"'(?:[^'\\\n]|\\(?:[nrt\\0'\x22]|x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]{1,6}\}))'"
)
_RAW_STR_RE = re.compile(r"(?:br|cr|r)(#+)?\"")


def _code_only(
    line: str, in_block: bool, raw_closer: str | None, keep_strings: bool = False
) -> tuple[str, bool, str | None]:
    """Blank comments (threading multi-line state) with string awareness.

    Same shape as check-review-patterns `_code_only` (#293 precedent): `//`
    starts a comment only outside strings/blocks, `/*` opens only outside
    strings, quotes inside blocks never open strings. `keep_strings`
    preserves double-quoted/raw spans but blanks comments (char literals
    stay blanked so `'"'` cannot forge a quote). An unterminated `"`
    runs to EOL (backslash-newline continuation) so braces in the tail
    never skew depth. Lifetimes survive via `_CHAR_RE` (needs closing
    `'`, handles `\\u{...}`).
    """
    out: list[str] = []
    i, n = 0, len(line)
    while i < n:
        if raw_closer is not None:
            end = line.find(raw_closer, i)
            if end == -1:
                if keep_strings:
                    out.append(line[i:])
                return "".join(out), in_block, raw_closer
            if keep_strings:
                out.append(line[i : end + len(raw_closer)])
            i = end + len(raw_closer)
            raw_closer = None
            continue
        if in_block:
            end = line.find("*/", i)
            if end == -1:
                return "".join(out), True, None
            i = end + 2
            in_block = False
            continue
        if line.startswith("//", i):
            break
        if line.startswith("/*", i):
            in_block = True
            i += 2
            continue
        m = _STR_RE.match(line, i)
        if m:
            if keep_strings:
                out.append(line[i : m.end()])
            i = m.end()
            continue
        if line[i] == '"':
            if keep_strings:
                out.append(line[i:])
            return "".join(out), in_block, raw_closer
        m = _RAW_STR_RE.match(line, i)
        if m and (i == 0 or not (line[i - 1].isalnum() or line[i - 1] == "_")):
            hashes = m.group(1) or ""
            raw_closer = '"' + "#" * len(hashes)
            if keep_strings:
                out.append(line[i : m.end()])
            i = m.end()
            continue
        m = _CHAR_RE.match(line, i)
        if m:
            i = m.end()
            continue
        out.append(line[i])
        i += 1
    return "".join(out), in_block, raw_closer


def _strip_comments(text: str) -> str:
    """Blank `//` and `/*...*/` comments (string-aware, multi-line threaded).

    Both chain scans run on this, never on raw text: `migrations.rs`
    documents the chain idiom in a `///` doc comment (`:11-17`), and a
    `/** ... */` rustdoc block or commented-out migration would otherwise
    match `FN_RE`/`CHAIN_RE` and phantom-pair (or phantom-split) the
    chain. `//` inside a string (`"http://x"`) stays intact.
    """
    kept: list[str] = []
    in_block = False
    raw_closer: str | None = None
    for line in text.splitlines() or [text]:
        code, in_block, raw_closer = _code_only(line, in_block, raw_closer, True)
        kept.append(code)
    return "\n".join(kept)


def check_integrity(text: str) -> int:
    text = _strip_comments(text)
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
    base_chained = set(CHAIN_RE.findall(_strip_comments(base_text)))
    worktree_text = _strip_comments(MIGRATIONS.read_text(encoding="utf-8"))
    worktree_chained = set(CHAIN_RE.findall(worktree_text))
    added = worktree_chained - base_chained
    if len(added) > limit:
        return fail(
            f"{len(added)} new migration versions vs {base[:12]} ({', '.join(sorted(added, key=int))}); "
            f"at most {limit} per PR — split the PR or squash same-branch iterations pre-merge"
        )
    print(f"migration-chain: OK: {len(added)} new migration version(s) vs {base[:12]}")
    return 0


def self_test() -> int:
    """Regression fixtures: doc/comment lines must not satisfy the chain."""
    # Verbatim `src-tauri/src/db/migrations.rs:11-17` doc example, plus the
    # real code pair it documents: the `///` lines must not count.
    doc_pair = (
        "/// Example of adding a new migration:\n"
        "/// ```text\n"
        "/// if current_version < 2 {\n"
        "///     migration_v2_add_foo(conn)?;\n"
        "///     super::record_migration(conn, 2)?;\n"
        "/// }\n"
        "/// ```\n"
        "fn migration_v2_add_foo(conn: &Connection) -> Result<()> {\n"
        "}\n"
        "    if current_version < 2 {\n"
        "        migration_v2_add_foo(conn)?;\n"
        "    }\n"
    )
    cases = [
        # (name, text, expected check_integrity rc)
        ("paired fn and indented chain passes", doc_pair, 0),
        (
            "doc-comment guard alone does not chain the fn",
            (
                "/// if current_version < 2 {\n"
                "fn migration_v2_add_foo(conn: &Connection) -> Result<()> {\n"
                "}\n"
            ),
            1,
        ),
        (
            "commented-out fn does not satisfy a chain block",
            (
                "// fn migration_v9_old_thing(conn: &Connection) -> Result<()> {\n"
                "    if current_version < 9 {\n"
                "    }\n"
            ),
            1,
        ),
        (
            "block-commented fn does not satisfy a chain block",
            (
                "/*\n"
                "fn migration_v9_old_thing(conn: &Connection) -> Result<()> {\n"
                "}\n"
                "*/\n"
                "    if current_version < 9 {\n"
                "    }\n"
            ),
            1,
        ),
        (
            "block rustdoc guard alone does not chain the fn",
            (
                "/**\n"
                "if current_version < 2 {\n"
                "*/\n"
                "fn migration_v2_add_foo(conn: &Connection) -> Result<()> {\n"
                "}\n"
            ),
            1,
        ),
        # Preservation lock (passes pre- and post-fix): a trailing `//`
        # URL comment must not eat the code before it.
        (
            "trailing // URL comment keeps the real chain",
            (
                "fn migration_v9_keep(conn: &Connection) -> Result<()> {\n"
                "}\n"
                "    if current_version < 9 { // see https://example.com/schema\n"
                "    }\n"
            ),
            0,
        ),
        (
            "unchained fn still fails",
            ("fn migration_v3_fix_things(conn: &Connection) -> Result<()> {\n}\n"),
            1,
        ),
    ]
    passed = 0
    for name, text, expect in cases:
        # Mute the gate's own stdout/stderr; only the verdict matters here.
        with (
            contextlib.redirect_stdout(io.StringIO()),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            got = check_integrity(text)
        if got != expect:
            print(f"self-test FAIL: {name}: exited {got}, want {expect}")
            continue
        passed += 1
    stripped_live = _strip_comments(MIGRATIONS.read_text(encoding="utf-8"))
    # Real sites still match after stripping, and the line-13 doc example
    # contributes no phantom version on its own.
    phantom_ok = (
        CHAIN_RE.findall(_strip_comments("/// if current_version < 2 {\n")) == []
        and CHAIN_RE.findall("/// if current_version < 2 {\n") == []
        and FN_RE.findall("/// fn migration_v2_add_foo(conn: &Connection) {\n") == []
        and CHAIN_RE.findall(_strip_comments("/*\nif current_version < 2 {\n*/\n"))
        == []
        and FN_RE.findall(
            _strip_comments("/*\nfn migration_v2_add_foo(conn: &Connection) {\n*/\n")
        )
        == []
        and bool(FN_RE.findall(stripped_live))
        and set(CHAIN_RE.findall(stripped_live)) == set(FN_RE.findall(stripped_live))
    )
    if not phantom_ok:
        print("self-test FAIL: live-tree strip sanity check failed")
    else:
        passed += 1
    total = len(cases) + 1
    print(f"migration-chain self-test: {passed}/{total} passed")
    return 0 if passed == total else 1


def main(argv: list[str]) -> int:
    if argv[:1] == ["--self-test"]:
        if check_integrity(MIGRATIONS.read_text(encoding="utf-8")) != 0:
            print(
                "migration-chain self-test: live tree gate failed first",
                file=sys.stderr,
            )
            return 1
        return self_test()
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
