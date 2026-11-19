#!/usr/bin/env python3
"""Gate: retired-identifier doc orphans.

Scans Rust files changed in the working tree (staged + unstaged + untracked +
unpushed commits — same scope resolution as scripts/lint-markdown-changed.py's
no-`--base` behavior; mirrored here, not imported) for removed identifiers,
then greps docs/ for any survivor. A hit means a doc still names something the
code no longer defines: the author must update/close that spot or narrow the
symbol.

CI passes --base <sha> (the PR base / push `before`; the one sanctioned
invocation difference, same as the markdown job): scope is then exactly the
.rs files changed between base and HEAD, because a clean CI checkout has no
worktree changes and no upstream - the local scope would be empty and the
gate could never fire. An all-zero base (manual dispatch) falls back to
HEAD~1, same as the markdown script.
"""
# Mirrors scope logic from scripts/lint-markdown-changed.py (no --base,
# plus the --base CI range). Do not import across scripts; keep the logic
# duplicated here intentionally.

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

STOPWORDS = {
    "id",
    "name",
    "conn",
    "tx",
    "msg",
    "event",
    "data",
    "path",
    "text",
    "title",
    "body",
    "self",
    "new",
    "get",
    "set",
    "default",
    "result",
    "error",
    # String-literal prose, not a symbol: the 'Unknown' server fallback
    # appears in docs as ordinary English (PR #202 H1 fix removed two such
    # lines while the fallback still lives in tree).
    "unknown",
}

MOD_TESTS_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+\w*tests?\b")


def git(*args: str) -> list[str]:
    out = subprocess.run(
        ["git", *args, "-z"], capture_output=True, text=False, check=True
    ).stdout
    return [p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p]


def unpushed_base() -> str | None:
    for base_ref in ("@{u}", "origin/HEAD"):
        try:
            base = subprocess.run(
                ["git", "merge-base", "HEAD", base_ref],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()
        except subprocess.CalledProcessError:
            continue
        if base:
            return base
    return None


_STR_RE = re.compile(r'"(?:[^"\\]|\\.)*"')
_CHAR_RE = re.compile(r"'(?:[^'\\]|\\.)*'")


def code_braces(line: str) -> tuple[int, int]:
    """(opens, closes) ignoring string/char literals and // comments."""
    no_strings = _CHAR_RE.sub("", _STR_RE.sub("", line))
    code = no_strings.split("//", 1)[0]
    return code.count("{"), code.count("}")


def test_module_lines(text: str) -> set[int]:
    """1-based line numbers that fall inside a `mod tests` / cfg(test) block."""
    out: set[int] = set()
    skip_depth = 0
    prev_sig = ""
    for i, raw in enumerate(text.splitlines(), 1):
        opens, closes = code_braces(raw)
        if skip_depth:
            skip_depth = max(0, skip_depth + opens - closes)
            out.add(i)
            continue
        stripped = raw.strip()
        is_test_mod = (
            MOD_TESTS_RE.match(raw) is not None
            or (
                re.match(r"^\s*(?:pub\s+)?mod\s+\w+\s*\{", raw) is not None
                and prev_sig == "#[cfg(test)]"
            )
            or (
                re.search(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]", raw) is not None
                and re.search(r"\bmod\b\s+\w+\s*\{", raw) is not None
            )
        )
        if is_test_mod:
            skip_depth = opens - closes
            out.add(i)
            if stripped:
                prev_sig = stripped
            continue
        if stripped:
            prev_sig = stripped
    return out


def parse_minus_lines(diff_text: str, old_text: str | None) -> list[str]:
    """Return removed source lines (without leading '-') with test-module
    lines filtered out using old-file line numbers."""
    skip = test_module_lines(old_text) if old_text is not None else set()
    filtered: list[str] = []
    old_lineno = 0
    in_hunk = False
    for raw in diff_text.splitlines():
        m = re.match(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", raw)
        if m:
            old_lineno = int(m.group(1))
            in_hunk = True
            continue
        if not in_hunk:
            continue
        if raw.startswith(("diff ", "index ", "---", "+++")):
            in_hunk = False
            continue
        if raw.startswith("\\"):
            continue
        if raw.startswith("-"):
            if not (old_lineno and old_lineno in skip):
                filtered.append(raw[1:])
            old_lineno += 1
        elif raw.startswith("+"):
            pass
        else:
            old_lineno += 1
    return filtered


PAIR_RE = re.compile(r"\b([A-Z][A-Za-z0-9_]*)::([A-Z][A-Za-z0-9_]*)\b")


def extract_identifiers(
    removed_lines: list[str],
) -> tuple[set[str], set[str]]:
    """Candidate doc-orphan names + `Enum::Variant` pairs from removed lines."""
    idents: set[str] = set()
    # fn/struct/enum definition names
    for line in removed_lines:
        for m in re.finditer(
            r"\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?"
            r"(?:fn|struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
            line,
        ):
            name = m.group(1)
            if len(name) >= 4 and name.lower() not in STOPWORDS:
                idents.add(name)
    # CamelCase tokens (enum-variant-ish, multi-word). Single-word
    # variants are covered by string-literal PascalCase when quoted.
    for line in removed_lines:
        for m in re.finditer(r"\b[A-Z][a-z]+[A-Z][A-Za-z0-9]*\b", line):
            name = m.group(0)
            if len(name) >= 4 and name.lower() not in STOPWORDS:
                idents.add(name)
        # also catch all-caps? no.
    # `Enum::Variant` uses on removed arms (a removed match arm is not a
    # removed variant while the pair still occurs in the tree).
    pairs: set[str] = set()
    for line in removed_lines:
        for m in PAIR_RE.finditer(line):
            pairs.add(m.group(0))
    # String literals: kebab-case or PascalCase
    for line in removed_lines:
        for m in re.finditer(r'"([^"]*)"', line):
            s = m.group(1)
            if len(s) < 4:
                continue
            if s.lower() in STOPWORDS:
                continue
            is_kebab = bool(re.fullmatch(r"[a-z][a-z0-9]*(?:-[a-z0-9]+)+", s))
            is_pascal = bool(re.fullmatch(r"[A-Z][A-Za-z0-9]*", s))
            if (is_kebab or is_pascal) and s.lower() not in STOPWORDS:
                idents.add(s)
    return idents, pairs


def old_content_for(path: str, ref: str) -> str | None:
    try:
        out = subprocess.run(
            ["git", "show", f"{ref}:{path}"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        return out
    except subprocess.CalledProcessError:
        return None


def collect_changed_rs(explicit_base: str | None) -> list[str]:
    files: list[str] = []
    if explicit_base is not None:
        try:
            files += git("diff", "--name-only", explicit_base, "HEAD")
        except subprocess.CalledProcessError:
            pass
    else:
        try:
            files += git("diff", "--name-only", "HEAD")
        except subprocess.CalledProcessError:
            pass
        try:
            files += git("ls-files", "--others", "--exclude-standard")
        except subprocess.CalledProcessError:
            pass
        base = unpushed_base()
        if base is not None:
            try:
                files += git("diff", "--name-only", base, "HEAD")
            except subprocess.CalledProcessError:
                pass
    out = []
    seen = set()
    for f in files:
        if f.endswith(".rs") and f not in seen:
            seen.add(f)
            out.append(f)
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--base",
        default=None,
        help="CI range: identifiers removed between base and HEAD "
        "(PR base sha / push before; all-zero falls back to HEAD~1)",
    )
    args = parser.parse_args()
    explicit_base = args.base
    if explicit_base is not None and explicit_base.strip("0") == "":
        print(
            "zero base sha (manual dispatch); falling back to HEAD~1",
            file=sys.stderr,
        )
        explicit_base = "HEAD~1"
    os.chdir(ROOT)
    changed = collect_changed_rs(explicit_base)
    idents: set[str] = set()
    removed_pairs: set[str] = set()

    # Worktree diff (staged + unstaged) against HEAD - local scope only.
    # An explicit CI base already names the range (and the tree is clean),
    # so there is nothing extra to collect from the worktree.
    if explicit_base is None:
        for path in changed:
            try:
                diff = subprocess.run(
                    ["git", "diff", "-U0", "HEAD", "--", path],
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout
            except subprocess.CalledProcessError:
                diff = ""
            old = old_content_for(path, "HEAD")
            names, pairs = extract_identifiers(parse_minus_lines(diff, old))
            idents |= names
            removed_pairs |= pairs

    # Unpushed commits (or the explicit CI range)
    base = explicit_base if explicit_base is not None else unpushed_base()
    if base is not None:
        for path in changed:
            try:
                diff = subprocess.run(
                    ["git", "diff", "-U0", base, "HEAD", "--", path],
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout
            except subprocess.CalledProcessError:
                diff = ""
            old = old_content_for(path, base)
            names, pairs = extract_identifiers(parse_minus_lines(diff, old))
            idents |= names
            removed_pairs |= pairs

    # Untracked files have no minus lines (nothing to extract).

    # A removed `fn` line is not a removed symbol: renames and signature
    # edits trip the extractor while the name still lives in the tree.
    # Only identifiers with no live definition can orphan a doc. Same for
    # `Enum::Variant` uses on removed arms: the variant survives while the
    # pair still occurs anywhere in the tree.
    defined: set[str] = set()
    live_pairs: set[str] = set()
    for rs in (ROOT / "src-tauri" / "src").rglob("*.rs"):
        try:
            text = rs.read_text(encoding="utf-8")
        except OSError:
            continue
        for m in re.finditer(r"\b(?:fn|struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)", text):
            defined.add(m.group(1))
        for m in PAIR_RE.finditer(text):
            live_pairs.add(m.group(0))
    idents -= defined
    for pair in removed_pairs & live_pairs:
        idents.discard(pair.split("::", 1)[1])

    # Grep docs/
    docs_root = ROOT / "docs"
    findings: list[str] = []
    if docs_root.exists():
        md_files = sorted(docs_root.rglob("*.md"))
        for ident in sorted(idents):
            pat = re.compile(
                r"(?<![A-Za-z0-9_])" + re.escape(ident) + r"(?![A-Za-z0-9_])"
            )
            for doc in md_files:
                try:
                    text = doc.read_text(encoding="utf-8")
                except OSError:
                    continue
                for i, line in enumerate(text.splitlines(), 1):
                    if pat.search(line):
                        rel = doc.relative_to(ROOT).as_posix()
                        findings.append(f"{rel}:{i}: {ident}")

    findings = sorted(set(findings))
    for f in findings:
        print(f)
    print(
        f"scanned {len(changed)} changed .rs files; "
        f"{len(idents)} identifiers; {len(findings)} doc hits",
        file=sys.stderr,
    )
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
