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
import tempfile
from collections.abc import Iterable
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
    # VIP JSON wire-field names (`FromName`/`FromEntityId`) occur in .rs
    # only inside JSON string literals and comments, and in docs only inside
    # JSON examples (the Rust fields are snake_case) — never code symbols.
    "fromname",
    "fromentityid",
}
MOD_TESTS_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+\w*tests?\b")

_CFG_TEST_ATTR_RE = re.compile(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]")
_ITEM_START_RE = re.compile(
    r"\b(?:fn|struct|enum|use|const|static)\s+[A-Za-z_][A-Za-z0-9_]*"
)


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


def _start_attr_item(
    code: str, opens: int, closes: int
) -> tuple[bool, int, bool, int, int]:
    """(track, depth, open_seen, paren, bracket) for an attributed item line.

    Keep tracking while the body is open; a `;` before any `{` ends a
    bodyless declaration (`fn f();`), unless nested in parens/brackets
    (`[u8; 4]` params).
    """
    depth = opens - closes
    open_seen = opens > 0
    paren = code.count("(") - code.count(")")
    bracket = code.count("[") - code.count("]")
    bodyless = ";" in code and paren <= 0 and bracket <= 0
    track = (open_seen and depth > 0) or (not open_seen and not bodyless)
    return track, depth, open_seen, paren, bracket


def test_module_lines(text: str) -> set[int]:
    """1-based line numbers that fall inside a `mod tests` / cfg(test) block."""
    out: set[int] = set()
    skip_depth = 0
    prev_sig = ""
    pending_cfg_test = False
    in_attr_item = False
    attr_depth = 0
    attr_open_seen = False
    attr_paren = 0
    attr_bracket = 0
    in_block = False
    raw_closer: str | None = None
    for i, raw in enumerate(text.splitlines(), 1):
        # Thread block-comment/raw-string state through the whole file so
        # braces inside them (`/* { */`, multi-line raw strings) never
        # move depth tracking; `code` is the same string/char/comment
        # blanked view the live scans use.
        code, in_block, raw_closer = code_only(raw, in_block, raw_closer)
        opens, closes = code.count("{"), code.count("}")
        if skip_depth:
            skip_depth = max(0, skip_depth + opens - closes)
            out.add(i)
            continue
        stripped = raw.strip()
        if in_attr_item:
            # Brace-balanced body of a bare `#[cfg(test)]` item: a `;`
            # before any `{` ends a bodyless declaration (`fn f();`),
            # unless it sits inside parens/brackets (`[u8; 4]` params).
            out.add(i)
            attr_depth += opens - closes
            attr_open_seen = attr_open_seen or opens > 0
            attr_paren += code.count("(") - code.count(")")
            attr_bracket += code.count("[") - code.count("]")
            if (attr_open_seen and attr_depth <= 0) or (
                not attr_open_seen
                and ";" in code
                and attr_paren <= 0
                and attr_bracket <= 0
            ):
                in_attr_item = False
        is_test_mod = (
            MOD_TESTS_RE.match(raw) is not None
            or (
                re.match(r"^\s*(?:pub\s+)?mod\s+\w+\s*\{", raw) is not None
                and prev_sig == "#[cfg(test)]"
            )
            or (
                _CFG_TEST_ATTR_RE.search(raw) is not None
                and re.search(r"\bmod\b\s+\w+\s*\{", raw) is not None
            )
        )
        if is_test_mod:
            # A lone `#[cfg(test)]` above a `mod` line must not leak past
            # the block and swallow the first live item after it.
            pending_cfg_test = False
            skip_depth = opens - closes
            out.add(i)
            if stripped:
                prev_sig = stripped
            continue
        if pending_cfg_test:
            if not stripped or stripped.startswith(("#", "//", "/*", "*")):
                if stripped.startswith("#"):
                    out.add(i)
                if stripped:
                    prev_sig = stripped
                continue
            pending_cfg_test = False
            if _ITEM_START_RE.search(code) is not None:
                out.add(i)
                state = _start_attr_item(code, opens, closes)
                in_attr_item, attr_depth = state[0], state[1]
                attr_open_seen, attr_paren, attr_bracket = state[2:]
                if stripped:
                    prev_sig = stripped
                continue
            # A non-item line ends the attribute's reach; fall through.
        if _CFG_TEST_ATTR_RE.search(raw) is not None:
            # Bare `#[cfg(test)]` on a `fn`/`struct`/`enum`/`use`/
            # `const`/`static` (same line or, via pending, the item below):
            # the item line and its brace-balanced body are test-only, so
            # both live scans inherit it.
            out.add(i)
            if _ITEM_START_RE.search(code) is not None:
                state = _start_attr_item(code, opens, closes)
                in_attr_item, attr_depth = state[0], state[1]
                attr_open_seen, attr_paren, attr_bracket = state[2:]
            else:
                pending_cfg_test = True
            if stripped:
                prev_sig = stripped
            continue
        if stripped:
            prev_sig = stripped
    return out


def parse_minus_lines(diff_text: str, old_text: str | None) -> list[tuple[int, str]]:
    """Return (old-file line number, removed source line) pairs, with
    test-module lines filtered out using old-file line numbers. Numbers let
    the caller lex the complete old source (correct block-comment state)
    and select only the removed lines."""
    skip = test_module_lines(old_text) if old_text is not None else set()
    filtered: list[tuple[int, str]] = []
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
                filtered.append((old_lineno, raw[1:]))
            old_lineno += 1
        elif raw.startswith("+"):
            pass
        else:
            old_lineno += 1
    return filtered


def lex_source(text: str, keep_strings: bool = False) -> list[str]:
    """Code-only view of every line of a complete source file, threading
    block-comment and raw-string state across lines in file order.

    With `keep_strings`, comments are still blanked but string spans are
    preserved (char literals stay blanked): the live-string scan runs the
    same naive quote regex over this view so comment prose can never
    count as a live occurrence (sim-D on #212).
    """
    out: list[str] = []
    in_block = False
    raw_closer: str | None = None
    for line in text.splitlines():
        code, in_block, raw_closer = code_only(line, in_block, raw_closer, keep_strings)
        out.append(code)
    return out


def live_string_literals(text: str) -> set[str]:
    """Quoted contents of live code strings in one complete source file.

    Comments are blanked via the keep-strings lexer (same block/raw-string
    threading as the extractor side) and test-module lines are skipped, so
    a removed literal surviving only in a comment or test never counts as
    live — it still fires (sim-D on #212, fail-closed). Raw-aware
    `_quoted_contents` matches the removed-line extractor's, so both sides
    see nested-JSON raw strings identically (B1).
    """
    skip = test_module_lines(text)
    out: set[str] = set()
    in_block = False
    raw_closer: str | None = None
    for lineno, line in enumerate(text.splitlines(), 1):
        kept, in_block, raw_closer = code_only(line, in_block, raw_closer, True)
        if lineno in skip:
            continue
        for s in _quoted_contents(kept):
            out.add(s)
    return out


def live_definitions(text: str) -> set[str]:
    """Live `fn`/`struct`/`enum`/`type`/`trait` definition names in one source file.

    Lines inside `#[cfg(test)]` / `mod tests` blocks (per
    `test_module_lines`) are skipped, so a test-only helper never counts
    as a live definition that would suppress a docs-orphan hit. The scan
    runs over the code-only view (`lex_source`), so `//` / block-comment
    prose mentioning `fn foo` never forges a live def that would shield
    a truly orphaned docs mention (E1).
    """
    skip = test_module_lines(text)
    cleaned = lex_source(text)
    out: set[str] = set()
    for lineno, line in enumerate(cleaned, 1):
        if lineno in skip:
            continue
        for m in re.finditer(
            r"\b(?:fn|struct|enum|type|trait)\s+([A-Za-z_][A-Za-z0-9_]*)", line
        ):
            out.add(m.group(1))
    return out


def collect_live_symbols(
    rs_files: Iterable[Path],
) -> tuple[set[str], set[str], set[str]]:
    """(defined, live_pairs, live_strings) over per-file live scans.

    Single call site shared by `main` and `self_test`: mutating this
    shared function breaks the self-test pin below.
    """
    defined: set[str] = set()
    live_pairs: set[str] = set()
    live_strings: set[str] = set()
    for rs in rs_files:
        try:
            text = rs.read_text(encoding="utf-8")
        except OSError:
            continue
        defined |= live_definitions(text)
        # E2: test-module pairs are not live consumers — a pair surviving
        # only in `#[cfg(test)]` / `mod tests` must not suppress a removed
        # arm. Scan the code-only view with the same test skip the
        # definition and string scans use, so comment/string pairs never
        # count either.
        pair_skip = test_module_lines(text)
        for lineno, line in enumerate(lex_source(text), 1):
            if lineno in pair_skip:
                continue
            for m in PAIR_RE.finditer(line):
                live_pairs.add(m.group(0))
        live_strings |= live_string_literals(text)
    return defined, live_pairs, live_strings


_RAW_STR_RE = re.compile(r"(?:br|cr|r)(#+)?\"")
_CHAR_LIT_RE = re.compile(r"'(?:[^'\\]|\\.)'")


def code_only(
    line: str, in_block: bool, raw_closer: str | None, keep_strings: bool = False
) -> tuple[str, bool, str | None]:
    """Blank Rust string/char literals and comments; lifetimes are kept.

    Only real code tokens reach the identifier regexes: JSON wire-field
    prose inside string literals (`"FromEntityId"`) and `//` comments never
    fires the gate (PR #202 full-review round — the hit came from a removed
    prose comment, not a symbol). String-literal extraction still runs on
    the original line.

    With `keep_strings`, comments are still blanked but double-quoted
    and raw string spans are preserved verbatim (char literals stay
    blanked so `'"'` cannot forge a quote pair): both string scans
    extract quoted contents from this view with raw-aware
    `_quoted_contents`, so nested-JSON raw strings agree on both sides
    (B1) while comment prose never counts as live (sim-D #212).
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
        m = _RAW_STR_RE.match(line, i)
        if m and (i == 0 or not (line[i - 1].isalnum() or line[i - 1] == "_")):
            hashes = m.group(1) or ""
            raw_closer = '"' + "#" * len(hashes)
            if keep_strings:
                out.append(line[i : m.end()])
            i = m.end()
            continue
        m = _CHAR_LIT_RE.match(line, i)
        if m:
            i = m.end()
            continue
        out.append(line[i])
        i += 1
    return "".join(out), in_block, raw_closer


_HASHED_RAW_RE = re.compile(r"(?<![A-Za-z0-9_])(?:br|cr|r)(#+)\"(.*?)\"\1")
_UNHASHED_RAW_RE = re.compile(r"(?<![A-Za-z0-9_])(?:br|cr|r)\"([^\"]*)\"")
_QUOTED_RE = re.compile(r'"([^"]*)"')


def _quoted_contents(kept: str) -> list[str]:
    """Quoted contents in a comment-blanked line, raw-aware (B1).

    `kept` is a `code_only(..., keep_strings=True)` view: comments are
    blanked, string spans preserved. Hashed raw strings (`r#"..."#`)
    may contain nested `"..."` (e.g. JSON `r#"{"event": "kebab"}"#`):
    the naive first-quote scan pairs the outer opener with the first
    inner opener and misses the inner kebab. Extract nested quotes
    from inside hashed-raw spans first (whole inner when it holds no
    nested quotes, e.g. `r#"game-state-updated"#`), blank those spans,
    then scan the remainder for normal/unhashed-raw strings.
    """
    out: list[str] = []
    spans: list[tuple[int, int]] = []
    for m in _HASHED_RAW_RE.finditer(kept):
        inner = m.group(2)
        nested = _QUOTED_RE.findall(inner)
        if nested:
            out.extend(nested)
        else:
            out.append(inner)
        spans.append((m.start(), m.end()))
    if spans:
        buf = list(kept)
        for s, e in spans:
            for i in range(s, e):
                buf[i] = " "
        kept = "".join(buf)
    for m in _UNHASHED_RAW_RE.finditer(kept):
        out.append(m.group(1))
    kept = _UNHASHED_RAW_RE.sub(lambda m: " " * (m.end() - m.start()), kept)
    out.extend(m.group(1) for m in _QUOTED_RE.finditer(kept))
    return out


PAIR_RE = re.compile(r"\b([A-Z][A-Za-z0-9_]*)::([A-Z][A-Za-z0-9_]*)\b")


def extract_identifiers(
    numbered: list[tuple[int, str]],
    cleaned_full: list[str],
    kept_full: list[str] | None = None,
) -> tuple[set[str], set[str], set[str], set[str]]:
    """Candidate doc-orphan names + `Enum::Variant` pairs from removed lines.

    `numbered` is (old-file line number, removed text) pairs;
    `cleaned_full` is the code-only view of the COMPLETE old source
    (`lex_source`), so block-comment/raw-string state at each removed line
    reflects the real file — lexing disconnected removals alone would
    misread state when delimiters sit on unchanged lines.

    Returns `(idents, pairs, literals, defs)`: `idents` is the full candidate
    set (unchanged), `pairs` the `Enum::Variant` uses, `literals` the
    string-literal-derived subset minus same-file `fn`/`struct`/`enum`
    definition names, and `defs` those definition names. `main` unions
    `defs` across files and subtracts them from the global literals:
    file A removing `pub struct RetiredWidget` plus file B removing
    `emit("RetiredWidget")` keeps the symbol governed by the definition
    path even though neither file holds both spellings (sim-D on #212).
    """
    idents: set[str] = set()
    def_names: set[str] = set()
    lit_names: set[str] = set()
    removed_lines = [text for _, text in numbered]
    cleaned = [cleaned_full[n - 1] for n, _ in numbered if 1 <= n <= len(cleaned_full)]
    # fn/struct/enum definition names
    for line in cleaned:
        for m in re.finditer(
            r"\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?"
            r"(?:fn|struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
            line,
        ):
            name = m.group(1)
            if len(name) >= 4 and name.lower() not in STOPWORDS:
                idents.add(name)
                def_names.add(name)
    # CamelCase tokens (enum-variant-ish, multi-word). Single-word
    # variants are covered by string-literal PascalCase when quoted.
    for line in cleaned:
        for m in re.finditer(r"\b[A-Z][a-z]+[A-Z][A-Za-z0-9]*\b", line):
            name = m.group(0)
            if len(name) >= 4 and name.lower() not in STOPWORDS:
                idents.add(name)
        # also catch all-caps? no.
    # `Enum::Variant` uses on removed arms (a removed match arm is not a
    # removed variant while the pair still occurs in the tree).
    pairs: set[str] = set()
    for line in cleaned:
        for m in PAIR_RE.finditer(line):
            pairs.add(m.group(0))

    # String literals: kebab-case or PascalCase. With `kept_full` (the
    # keep-strings view of the COMPLETE old source) literals come from the
    # threaded line, so a removed line inside multi-line `/*...*/` yields
    # nothing — #279 residual. Without it (legacy self-tests) fall back to
    # per-line `code_only` so `//` / single-line `/* */` quotes never count,
    # then raw-aware `_quoted_contents` so nested-JSON raw strings
    # (`r#"{"event": "kebab"}"#`) yield their inner kebabs instead of
    # empty (B1) — same helper the live scan uses, so both sides agree.
    def _collect_from_kept(kept: str) -> None:
        for s in _quoted_contents(kept):
            if len(s) < 4:
                continue
            if s.lower() in STOPWORDS:
                continue
            is_kebab = bool(re.fullmatch(r"[a-z][a-z0-9]*(?:-[a-z0-9]+)+", s))
            is_pascal = bool(re.fullmatch(r"[A-Z][A-Za-z0-9]*", s))
            if (is_kebab or is_pascal) and s.lower() not in STOPWORDS:
                idents.add(s)
                lit_names.add(s)

    if kept_full is not None:
        for n, _ in numbered:
            if 1 <= n <= len(kept_full):
                _collect_from_kept(kept_full[n - 1])
    else:
        for line in removed_lines:
            kept, _, _ = code_only(line, False, None, True)
            _collect_from_kept(kept)
    literals = lit_names - def_names
    return idents, pairs, literals, def_names


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
    removed_literals: set[str] = set()
    removed_defs: set[str] = set()

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
            numbered = parse_minus_lines(diff, old)
            cleaned_full = lex_source(old) if old is not None else []
            kept_full = lex_source(old, True) if old is not None else []
            names, pairs, lits, defs = extract_identifiers(
                numbered, cleaned_full, kept_full
            )
            idents |= names
            removed_pairs |= pairs
            removed_literals |= lits
            removed_defs |= defs

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
            numbered = parse_minus_lines(diff, old)
            cleaned_full = lex_source(old) if old is not None else []
            kept_full = lex_source(old, True) if old is not None else []
            names, pairs, lits, defs = extract_identifiers(
                numbered, cleaned_full, kept_full
            )
            idents |= names
            removed_pairs |= pairs
            removed_literals |= lits
            removed_defs |= defs

    # Untracked files have no minus lines (nothing to extract).
    # A removed `fn` line is not a removed symbol: renames and signature
    # edits trip the extractor while the name still lives in the tree.
    # Only identifiers with no live definition can orphan a doc. Same for
    # `Enum::Variant` uses on removed arms: the variant survives while the
    # pair still occurs anywhere in the tree. Same for removed string
    # literals (#212): an emit made conditional trips the extractor while
    # the identical string still lives two lines down — only literals with
    # no live quoted occurrence can orphan a doc. `literals` excludes
    # same-file `fn`/`struct`/`enum` definition names, and `main` unions
    # `defs` across files before subtracting: file A removing the
    # definition plus file B removing the same-spelled string keeps the
    # symbol on the definition path (sim-D). Live strings come from the
    # keep-strings lexer with test modules skipped, so comment prose or
    # test-only occurrences never count as live (sim-D, fail-closed).
    # Live definitions likewise skip `#[cfg(test)]` / `mod tests` lines:
    # a helper that survives only in tests never counts as a live
    # definition, so it cannot suppress a docs-orphan hit.
    defined, live_pairs, live_strings = collect_live_symbols(
        (ROOT / "src-tauri" / "src").rglob("*.rs")
    )
    idents -= defined
    for pair in removed_pairs & live_pairs:
        idents.discard(pair.split("::", 1)[1])
    idents -= (removed_literals - removed_defs) & live_strings

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


def self_test() -> int:
    """Adversarial fixtures for the code-only lexer (the D-minor lessons:
    disconnected removals misread state; lifetimes must survive; gates ship
    with probes, version-sync precedent)."""
    failures = 0

    def check(name: str, cond: bool) -> None:
        nonlocal failures
        print(("PASS " if cond else "FAIL ") + name)
        if not cond:
            failures += 1

    cases = [
        ("    // Presence of FromEntityId locates the twin", "    "),
        ('    x = "a // b"; // FromName here', "    x = ; "),
        ("    /* FromEntityId */ let a = 1;", "     let a = 1;"),
        (
            "    fn f<'a>(x: &'a str) -> &'a str {",
            "    fn f<'a>(x: &'a str) -> &'a str {",
        ),
        ("    let c = 'a';", "    let c = ;"),
        ('    let s = r#"{"FromName": 1}"#;', "    let s = ;"),
        ('    let u = "http://x";', "    let u = ;"),
        ("    /* open block", "    "),
        ("    still FromEntityId comment", ""),
        ("    end */ let B = 1;", " let B = 1;"),
    ]
    in_block, raw = False, None
    for src, want in cases:
        out, in_block, raw = code_only(src, in_block, raw)
        check(f"strip {src.strip()[:40]!r}", out == want)
    check("block state closed", (in_block, raw) == (False, None))

    # Removal inside a block comment (delimiters on unchanged lines) stays
    # hidden; a def after the closed block stays visible.
    old = "/* heading\nstill comment\ndef FakeThing():\n    pass\n*/\nreal_code = 1\n"
    full = lex_source(old)
    check("comment-hidden line blank", full[2] == "")
    check("post-block line kept", full[5] == "real_code = 1")
    idents, _, _, _ = extract_identifiers(
        [(3, "def FakeThing():"), (6, "real_code = 1")], full
    )
    check("comment-hidden def suppressed", "FakeThing" not in idents)

    old2 = "/* c */\npub struct VisibleWidget {\n}\n"
    idents2, _, _, _ = extract_identifiers(
        [(2, "pub struct VisibleWidget {")], lex_source(old2)
    )
    check("post-block def fires", "VisibleWidget" in idents2)
    idents3, _, _, _ = extract_identifiers(
        [(1, "    // FromEntityId retired")], ["    "]
    )
    check("prose comment silent", not idents3)

    # #212: a removed string literal that still occurs quoted in the new
    # tree is not an orphan (unconditional emit made conditional, same
    # string two lines down); a truly removed literal still fires; a
    # retired definition with the same spelling as a live string stays a
    # symbol only the definition path can clear.
    lit_idents, _, lit_lits, _ = extract_identifiers(
        [(1, '    emit("game-state-updated");')], ["    emit();"]
    )
    check("kebab literal extracted", "game-state-updated" in lit_idents)
    check("kebab literal tracked", "game-state-updated" in lit_lits)
    check(
        "still-live literal suppressed",
        not (lit_idents - (lit_lits & {"game-state-updated"})),
    )
    check(
        "truly-removed literal fires",
        (lit_idents - (lit_lits & {"something-else"})) == {"game-state-updated"},
    )
    both_idents, _, both_lits, both_defs = extract_identifiers(
        [
            (1, "    pub struct RetiredWidget {"),
            (2, '    emit("RetiredWidget");'),
        ],
        ["    pub struct RetiredWidget {", "    emit();"],
    )
    check("retired def fires", "RetiredWidget" in both_idents)
    check("retired def not a literal", "RetiredWidget" not in both_lits)
    check(
        "live string never clears a retired symbol",
        "RetiredWidget"
        in (both_idents - ((both_lits - both_defs) & {"RetiredWidget"})),
    )

    # sim-D cross-file split: file A removes the definition, file B
    # removes the same-spelled string, and the string still lives
    # elsewhere. Per-file subtraction misses it; the global defs union
    # in main keeps the retired symbol firing.
    _, _, a_lits, a_defs = extract_identifiers(
        [(1, "    pub struct RetiredWidget {")], ["    pub struct RetiredWidget {"]
    )
    b_idents, _, b_lits, b_defs = extract_identifiers(
        [(1, '    emit("RetiredWidget");')], ["    emit();"]
    )
    x_lits = a_lits | b_lits
    x_defs = a_defs | b_defs
    check("cross-file literal tracked", "RetiredWidget" in x_lits)
    check("cross-file def union", "RetiredWidget" in x_defs)
    check(
        "cross-file def not suppressible",
        "RetiredWidget" not in (x_lits - x_defs),
    )
    check(
        "cross-file retired symbol fires",
        "RetiredWidget" in (b_idents - ((x_lits - x_defs) & {"RetiredWidget"})),
    )

    # sim-D live scan: comment prose and test-only occurrences never
    # count as live; real and raw-string code occurrences do; a '"'
    # char literal forges no quote pair.
    check(
        "line-comment survivor not live",
        "game-state-updated"
        not in live_string_literals('// emit("game-state-updated");\n'),
    )
    check(
        "block-comment survivor not live",
        "game-state-updated"
        not in live_string_literals('/* emit("game-state-updated"); */\n'),
    )
    check(
        "test-only survivor not live",
        "game-state-updated"
        not in live_string_literals(
            '#[cfg(test)]\nmod tests {\n    emit("game-state-updated");\n}\n'
        ),
    )
    check(
        "real-code survivor live",
        "game-state-updated" in live_string_literals('emit("game-state-updated");\n'),
    )
    check(
        "raw-string survivor live",
        "game-state-updated"
        in live_string_literals('emit(r#"game-state-updated"#);\n'),
    )
    check(
        "char quote forges nothing",
        live_string_literals("let c = '\"';\n") == set(),
    )
    # Test-module fns are not live definitions: a helper surviving only
    # in `#[cfg(test)]` must not suppress its docs-orphan hit, while a
    # live `fn` with the same name still suppresses (no false-positive
    # flip when both exist).
    test_mod_src = (
        "pub fn live_helper() {}\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    fn only_in_tests() {}\n"
        "    fn live_helper() {}\n"
        "}\n"
    )
    check(
        "test-only fn not a live definition",
        "only_in_tests" not in live_definitions(test_mod_src),
    )
    check(
        "live fn still a live definition",
        "live_helper" in live_definitions(test_mod_src),
    )
    check(
        "pure test file defines nothing live",
        live_definitions("#[cfg(test)]\nmod tests {\n    fn only_in_tests() {}\n}\n")
        == set(),
    )
    # Bare `#[cfg(test)]` items (no `mod tests` wrapper): the attribute
    # line, the item line, and its brace-balanced body are all skipped,
    # so neither live scan sees them — while a neighboring live item
    # stays visible.
    bare_attr_src = (
        "#[cfg(test)]\n"
        "pub fn bare_test_helper() {}\n"
        "#[cfg(test)] pub struct BareTestStruct {}\n"
        "#[cfg(test)]\n"
        "enum BareTestEnum {\n"
        "    A,\n"
        "}\n"
        "pub fn live_fn() {}\n"
    )
    check(
        "bare-attribute fn not a live definition",
        "bare_test_helper" not in live_definitions(bare_attr_src),
    )
    check(
        "same-line attributed struct not live",
        "BareTestStruct" not in live_definitions(bare_attr_src),
    )
    check(
        "next-line attributed enum not live",
        "BareTestEnum" not in live_definitions(bare_attr_src),
    )
    check(
        "live fn beside attributes stays live",
        "live_fn" in live_definitions(bare_attr_src),
    )
    check(
        "bare-attribute lines fully skipped",
        test_module_lines(bare_attr_src) == {1, 2, 3, 4, 5, 6, 7},
    )
    # Type-alias / trait blindness (stack-correction removal orphaned live
    # `GameDataState` docs mentions via a removed `use` line): nominal type
    # definitions count as live, so a removed import of a still-live type
    # never fires the gate.
    check(
        "type alias is a live definition",
        "LiveAlias" in live_definitions("pub type LiveAlias = Arc<RwLock<Foo>>;\n"),
    )
    check(
        "trait is a live definition",
        "LiveTrait" in live_definitions("pub trait LiveTrait {\n    fn m(&self);\n}\n"),
    )
    check(
        "test-only type alias is not live",
        live_definitions("#[cfg(test)]\nmod tests {\n    type TestOnly = u32;\n}\n")
        == set(),
    )
    # Stale-pending probe: `#[cfg(test)]` on its own line directly above
    # a `mod` block must not leak past the block — the live item after
    # it stays a live definition (coordinator `report_kind_tests` shape).
    pending_mod_src = (
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    fn only_in_tests() {}\n"
        "}\n"
        "pub fn live_after() {}\n"
    )
    check(
        "attr-mod block does not swallow live fn",
        "live_after" in live_definitions(pending_mod_src),
    )
    check(
        "attr-mod test fn still skipped",
        "only_in_tests" not in live_definitions(pending_mod_src),
    )
    # Multi-line signature (`from_pools` shape): the body stays skipped
    # too, so test-only strings inside never count as live — while the
    # live method after the block stays visible.
    multiline_attr_src = (
        "impl DbPool {\n"
        "    /// Test/dev constructor.\n"
        "    #[cfg(test)]\n"
        "    pub fn from_pools(\n"
        "        reads: u8,\n"
        "    ) -> Self {\n"
        '        emit("test-only-string");\n'
        "        Self {}\n"
        "    }\n"
        "    pub fn live_method() {}\n"
        "}\n"
    )
    check(
        "multiline-signature attr fn not live",
        "from_pools" not in live_definitions(multiline_attr_src),
    )
    check(
        "attr-fn body strings not live",
        "test-only-string" not in live_string_literals(multiline_attr_src),
    )
    check(
        "live method after attr block stays live",
        "live_method" in live_definitions(multiline_attr_src),
    )
    # The shared collector stays pinned: mutating `collect_live_symbols`
    # breaks this fixture (a caller reverting to a raw inline scan would
    # not).
    with tempfile.TemporaryDirectory() as tmp:
        probe = Path(tmp) / "probe.rs"
        probe.write_text(test_mod_src, encoding="utf-8")
        shared_defined, _, _ = collect_live_symbols([probe])
    check(
        "shared collector excludes test-only fn",
        "only_in_tests" not in shared_defined,
    )
    check(
        "shared collector keeps live fn",
        "live_helper" in shared_defined,
    )
    keep, _, _ = code_only('    x = "a // b"; // c', False, None, True)
    check("keep strings blanks comment", keep == '    x = "a // b"; ')
    keep2, _, _ = code_only('    let s = r#"x"#;', False, None, True)
    check("keep raw string span", keep2 == '    let s = r#"x"#;')
    keep3, _, _ = code_only('    /* c "q" */ let a = 1;', False, None, True)
    check("keep blanks comment quotes", keep3 == "     let a = 1;")
    # B1 mutant-killing: nested-JSON raw string hides an inner kebab
    # from the naive first-quote scan (pre-fix: empty literal set).
    b1_line = '    emit(r#"{"event": "game-state-updated"}"#);'
    b1_idents, _, b1_lits, _ = extract_identifiers([(1, b1_line)], lex_source(b1_line))
    check(
        "B1 nested-json raw inner kebab extracted",
        "game-state-updated" in b1_lits and "game-state-updated" in b1_idents,
    )
    check(
        "B1 nested-json raw inner live",
        "game-state-updated" in live_string_literals(b1_line + "\n"),
    )
    # B2 mutant-killing: braceless `use` under bare `#[cfg(test)]` leaks
    # (pre-fix: line 2 not skipped).
    check(
        "B2 braceless use under bare cfg skipped",
        2 in test_module_lines("#[cfg(test)]\nuse foo::bar;\n"),
    )
    # E1 mutant-killing: line-comment `fn` mention forges a live def
    # (pre-fix: comment shields the orphan).
    check(
        "E1 comment fn forgery suppressed",
        live_definitions("// fn shielded_orphan() {}\n") == set(),
    )
    check(
        "E1 block-comment fn forgery suppressed",
        live_definitions("/* fn blocked_orphan() {} */\n") == set(),
    )
    # E2 mutant-killing: test-only `Enum::Variant` pair counted as live
    # (pre-fix: pair suppresses the removal).
    with tempfile.TemporaryDirectory() as tmp2:
        probe2 = Path(tmp2) / "pair_test.rs"
        probe2.write_text(
            "#[cfg(test)]\nmod tests {\n    fn f() { let x = Foo::Bar; }\n}\n",
            encoding="utf-8",
        )
        _, e2_pairs, _ = collect_live_symbols([probe2])
    check("E2 test-only pair not live", "Foo::Bar" not in e2_pairs)
    # T1 mutant-killing: a byte-raw `br#"..."#` holding a bare quote
    # mis-splits under the prefix-blind opener (pre-fix code view
    # `br#bhi";` swallows the following string; plain `r` stays clean).
    t1_line = '    let s = br#"a"b"#; let t = "hi";'
    t1_code, _, _ = code_only(t1_line, False, None, False)
    check(
        "T1 byte-raw with quote blanks like plain raw",
        t1_code == "    let s = ; let t = ;",
    )
    t1_plain, _, _ = code_only('    let s = r#"a"b"#; let t = "hi";', False, None, True)
    t1_kept, _, _ = code_only(t1_line, False, None, True)
    check(
        "T1 byte-raw quoted contents match plain raw",
        _quoted_contents(t1_kept) == _quoted_contents(t1_plain),
    )
    # T2 mutant-killing: `const`/`static` with a string init under bare
    # `#[cfg(test)]` leaks a live literal (pre-fix: line 2 unskipped),
    # definition side and pair side alike.
    check(
        "T2 const with string init under bare cfg skipped",
        2 in test_module_lines('#[cfg(test)]\nconst X: &str = "kebab-lit";\n'),
    )
    check(
        "T2 const string init not live",
        "kebab-lit"
        not in live_string_literals('#[cfg(test)]\nconst X: &str = "kebab-lit";\n'),
    )
    with tempfile.TemporaryDirectory() as tmp3:
        probe3 = Path(tmp3) / "const_pair.rs"
        probe3.write_text(
            "#[cfg(test)]\nconst Y: Foo = Foo::Bar;\n",
            encoding="utf-8",
        )
        _, t2_pairs, _ = collect_live_symbols([probe3])
    check("T2 const pair under bare cfg not live", "Foo::Bar" not in t2_pairs)
    # T3 mutant-killing: `/* { */` inside a bare-attr item body
    # double-counts braces (pre-fix: the skip swallows the live fn after).
    t3_src = "#[cfg(test)]\nfn foo() { /* { */ }\nfn live_after() {}\n"
    check(
        "T3 block-comment braces do not extend attr item",
        test_module_lines(t3_src) == {1, 2},
    )
    check(
        "T3 live fn after comment-brace attr stays live",
        "live_after" in live_definitions(t3_src),
    )
    # 279 residual mutant-killing: removed line inside multi-line `/*...*/`
    # yields a literal via the per-line fallback (pre-fix FAIL: the
    # disconnected lex with `in_block=False` extracts `game-state-updated`
    # from inside the comment); the threaded `kept_full` view suppresses it.
    _ml_old = '/* heading\nemit("game-state-updated");\n*/\nreal = 1\n'
    _ml_cleaned = lex_source(_ml_old)
    _ml_kept = lex_source(_ml_old, True)
    _ml_fallback_idents, _, _ml_fallback_lits, _ = extract_identifiers(
        [(2, 'emit("game-state-updated");')], _ml_cleaned
    )
    check(
        "279 fallback extracts inside multi-line comment (documents gap)",
        "game-state-updated" in _ml_fallback_lits,
    )
    _ml_idents, _, _ml_lits, _ = extract_identifiers(
        [(2, 'emit("game-state-updated");')], _ml_cleaned, _ml_kept
    )
    check(
        "279 threaded kept suppresses inside multi-line comment",
        "game-state-updated" not in _ml_lits and "game-state-updated" not in _ml_idents,
    )
    # 281-analogue preservation: live single-line literal still extracts via
    # the threaded view (comment stripping must not hide real code).
    _live_old = 'emit("game-state-updated");\n'
    _live_idents, _, _live_lits, _ = extract_identifiers(
        [(1, 'emit("game-state-updated");')],
        lex_source(_live_old),
        lex_source(_live_old, True),
    )
    check(
        "281-analogue live literal still extracted via threaded kept",
        "game-state-updated" in _live_lits and "game-state-updated" in _live_idents,
    )
    return 1 if failures else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    sys.exit(main())
