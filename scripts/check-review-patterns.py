#!/usr/bin/env python3
"""Gate: mechanically-checkable defect shapes from frozen-review/CodeRabbit rounds."""

import re
import sys
from pathlib import Path

_STR_RE = re.compile(r'"(?:[^"\\]|\\.)*"')
_CHAR_RE = re.compile(
    r"'(?:[^'\\\n]|\\(?:[nrt\\0'\x22]|x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]{1,6}\}))'"
)


def code_braces(line: str) -> tuple[int, int]:
    """(opens, closes) ignoring string/char literals and // comments."""
    no_strings = _CHAR_RE.sub("", _STR_RE.sub("", line))
    code = no_strings.split("//", 1)[0]
    return code.count("{"), code.count("}")


FILTER_MAP_RE = re.compile(
    r"filter_map\(\s*Result::ok\s*\)"
    r"|filter_map\(\s*\|\s*[A-Za-z_][A-Za-z0-9_]*\s*\|\s*[A-Za-z_][A-Za-z0-9_]*\.ok\(\)\s*\)"
    r"|filter_map\(\s*\|\s*[A-Za-z_][A-Za-z0-9_]*\s*\|\s*\{[^}]*\.ok\(\)[^}]*\}\s*\)",
    re.DOTALL,
)
LET_DISCARD_RE = re.compile(
    r"let _\s*=\s*(conn|tx|stmt)\.(execute|execute_batch|query_row|query_map)",
    re.DOTALL,
)

FINDING_FILTER_MAP = (
    "review-patterns: filter_map(|x| x.ok()) row-read discard in migrations.rs body"
    " \u2014 collect::<Result<..>>()? instead"
)
FINDING_LET_DISCARD = (
    "review-patterns: discarded migration write/row-read (let _ = ...)"
    " \u2014 propagate with ? (v81 lesson)"
)
# A silenced DB Result outside migrations: `let _ =` on any call whose
# method name is a SQL op (execute/.../commit) — those only exist on
# rusqlite types, so a discarded call is a DB silence by definition (any
# receiver, incl. pool.get_write().unwrap()); on a db-verb helper
# (insert_/record_/link_/...); or on any call taking `conn`/`tx` as an arg
# (the class rule — AGENTS documents `conn`/`tx`/`stmt` handles, so `tx`-only
# args trip branch 3 too). Read-verb callees (get/fetch/...) are excluded.
# Multi-line rustfmt-wrapped writes are joined (cap 5) before matching. Test modules
# (`mod \w*tests?`, cfg(test) mod blocks) are skipped. Drops write/link
# failures with no log and no retry (kill-loot link lesson, PR #202).
# Emits, fs ops, and test cleanup are deliberately out of scope.
# Sanctioned shape (like the migration gate's EXISTS guards): ROLLBACK
# cleanup in an already-failing path — the real error is propagated
# either way, so only the compensation can fail silently.
DB_SILENCE_RE = re.compile(
    r"let\s+_\s*=\s*(?!.*\bROLLBACK\b)"
    r"(?:"
    r".*\.\s*(?:execute|execute_batch|query_row|query_map|query|transaction|prepare|commit)\b"
    r"|(?:::)?(?:(?:crate|super|self|db|[a-z_][a-z0-9_]*)::)*"
    r"(?:link|insert|record|persist|upsert|import|delete|update|backfill|"
    r"reconcile|claim|attach|tag|add|set|tighten)"
    r"_[a-z0-9_]*\s*\("
    r"|\b(?:[a-z_][a-z0-9_]*::)*+"
    r"(?!(?:get|fetch|load|read|find|search|list|check|exists|count|lookup|validate)(?:_|\b))"
    r"[a-z_][a-z0-9_]*\s*\([^;()]*\b(?:conn|tx)\b[^;()]*\)"
    r")"
)
MOD_TESTS_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+\w*tests?\b")
FINDING_DB_SILENCE = (
    "review-patterns: silenced DB write/link ({path}:{lineno})"
    " \u2014 propagate with ? or log like the receipt side (PR #202 link lesson)"
)


# A discarded UPDATE with a same-table INSERT sibling in one function: the
# twin-claim hole (PR #202 full-review round) — `conn.execute("UPDATE ...")`
# `.ok()` fell through to a fallback INSERT of the same row on a broken
# connection. Best-effort `.ok()` writes with NO fallback (wisdom/roulette/
# currency accrual) stay silent, as does the fail-closed `if let Err`
# shape that reports and returns. INSERT-side `.is_ok()` guards are a
# different (boolean-flag) shape, out of scope here.
UPDATE_OK_TABLE_RE = re.compile(r'"UPDATE\s+([A-Za-z_][A-Za-z0-9_]*)', re.IGNORECASE)
FINDING_UPDATE_OK_FALLBACK = (
    "review-patterns: discarded UPDATE on {table} with same-table INSERT"
    " sibling ({path}:{lineno}) — fail closed (report + return) like the"
    " twin-claim fix, don't fall through to insert"
)
# A wholesale overwrite of the shared `source_details` JSON blob:
# `SET source_details = ?N` drops keys other writers rely on (the survey
# tagger dropped the `json_twin` provenance marker — PR #202 full-review
# round). Merge with `json_set(COALESCE(source_details,'{}'),...)` instead.
FINDING_BLOB_OVERWRITE = (
    "review-patterns: wholesale source_details overwrite ({path}:{lineno})"
    " — merge via json_set so other writers' keys survive"
)
# A machine-local absolute path in a Rust string literal: a committed test
# may never read a file by a path that only exists on the author's machine
# (the scratch repro that read the live Chat.log by its absolute game-dir
# path is the worked example). Drive roots, Unix home roots, and
# pasted-Explorer backslash paths with a known-root context flag — unless
# the user-or-root segment is synthetic by convention (`TestUser`, `Test`,
# `u`, `books`, `Games`, `...`). Bare `word:\x` shapes do NOT flag: fixture
# JSON unavoidably contains them (`Consumed:\n...`), identical at the value
# level to a real `C:\...` path. Escapes resolve before matching, so
# `Status:` + newline and JSON `\"` never read as drive roots. Raw lines
# (test modules included — scratch tests live there).
TEST_PATH_LITERAL_RE = re.compile(r'"(?:[^"\\]|\\.)*"')
TEST_PATH_DRIVE_RE = re.compile(r"[A-Za-z]:/")
TEST_PATH_HOME_RE = re.compile(r"/(?:Users|home)/")
TEST_PATH_BACKSLASH_CTX_RE = re.compile(
    r"[A-Za-z]:\\(?:Users|home|AppData|Program Files|ProgramData|Windows|Temp|tmp)"
)
TEST_PATH_ALLOW_FIRST = ("books", "Games", "...", "TestUser", "Test", "u")
TEST_PATH_ESCAPES = {
    "n": "\n",
    "t": "\t",
    "r": "\r",
    "0": "\0",
    "\\": "\\",
    '"': '"',
    "'": "'",
}
FINDING_TEST_PATH = (
    "review-patterns: machine-local absolute path {literal} ({path}:{lineno})"
    " — use a test_data fixture or a synthetic TestUser path"
)


def test_path_decoded(literal: str) -> str:
    """Value the Rust literal denotes, resolved for path detection."""
    body = literal[1:-1]
    out: list[str] = []
    i = 0
    while i < len(body):
        char = body[i]
        if char == "\\" and i + 1 < len(body):
            out.append(TEST_PATH_ESCAPES.get(body[i + 1], "\\" + body[i + 1]))
            i += 2
        else:
            out.append(char)
            i += 1
    return "".join(out)


def test_path_first_segment(path: str) -> str:
    """User-or-root segment of an absolute local path."""
    match = re.search(
        r"(?:[A-Za-z]:)?[/\\](?:Users|home)[/\\]([^/\\]+)|[A-Za-z]:[\\/]([^/\\]+)",
        path,
    )
    if not match:
        return ""
    return match.group(1) or match.group(2) or ""


def test_path_is_local(literal: str) -> bool:
    body = test_path_decoded(literal)
    if "://" in body:
        return False
    if not (
        TEST_PATH_DRIVE_RE.search(body)
        or TEST_PATH_HOME_RE.search(body)
        or TEST_PATH_BACKSLASH_CTX_RE.search(body)
    ):
        return False
    return test_path_first_segment(body) not in TEST_PATH_ALLOW_FIRST


def read_body(root: Path) -> str:
    path = root / "src-tauri" / "src" / "db" / "migrations.rs"
    # migrations.rs is CRLF: strip \r too or the ^...$ anchor never matches
    # and the gate scans the test module the awk excludes (latent until a
    # test-module shape trips a pattern — fixture at 4371 lives pre-anchor).
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    out = []
    for line in lines:
        if re.match(r"^mod \w*tests?\b", line.rstrip("\r\n")):
            break
        out.append(line)
    return "".join(out)


def code_lines(path: Path) -> list[tuple[int, str]]:
    """(lineno, line) outside `mod tests` blocks (brace-depth tracked)."""
    out: list[tuple[int, str]] = []
    skip_depth = 0
    prev_sig = ""
    for i, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        opens, closes = code_braces(raw)
        if skip_depth:
            skip_depth = max(0, skip_depth + opens - closes)
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
            if stripped:
                prev_sig = stripped
            continue
        if stripped:
            prev_sig = stripped
        out.append((i, raw))
    return out


FN_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+[A-Za-z_][A-Za-z0-9_]*\s*\("
)


def split_statements(text: str) -> list[str]:
    """Split Rust text into `;`-terminated statements, ignoring `;` inside
    double-quoted strings (SQL text). Good enough for statement classification."""
    out: list[str] = []
    cur: list[str] = []
    in_str = False
    esc = False
    for ch in text:
        if in_str:
            cur.append(ch)
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
            continue
        if ch == '"':
            in_str = True
            cur.append(ch)
            continue
        if ch == ";" and not in_str:
            cur.append(ch)
            out.append("".join(cur))
            cur = []
            continue
        cur.append(ch)
    if "".join(cur).strip():
        out.append("".join(cur))
    return out


def execute_call_end(text: str, open_idx: int) -> int | None:
    """Index just past the `)` matching the `(` at open_idx, skipping
    double-quoted regions (SQL parens like MIN(rowid) must not count).
    Single quotes are NOT strings here: SQL literals never hold parens in
    this tree, and Rust lifetimes would misread as string starts."""
    depth = 0
    in_str = False
    esc = False
    i = open_idx
    while i < len(text):
        ch = text[i]
        if in_str:
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
        elif ch == '"':
            in_str = True
        elif ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return None


def update_ok_tables(fn_text: str) -> list[str]:
    """Tables of UPDATE statements executed with a discarded `.ok()`."""
    tables: list[str] = []
    for m in re.finditer(r"\.\s*execute\s*\(", fn_text):
        end = execute_call_end(fn_text, m.end() - 1)
        if end is None:
            continue
        if not re.match(r"\s*\.\s*ok\s*\(\s*\)", fn_text[end:]):
            continue
        sql = UPDATE_OK_TABLE_RE.search(fn_text[m.start() : end])
        if sql:
            tables.append(sql.group(1))
    return tables


def set_clause(stmt: str) -> str:
    """The SET assignment region of an UPDATE statement (SET..top-level WHERE
    or end): WHERE-predicate comparisons must not trip the overwrite rule."""
    depth = 0
    quote: str | None = None
    esc = False
    i, n = 0, len(stmt)
    start = -1
    while i < n:
        ch = stmt[i]
        if quote is not None:
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == quote:
                if quote == "'" and i + 1 < n and stmt[i + 1] == "'":
                    i += 1
                else:
                    quote = None
            i += 1
            continue
        if ch in ("'", '"'):
            quote = ch
            i += 1
            continue
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth = max(0, depth - 1)
        elif depth == 0:
            if start < 0 and re.match(r"SET\b", stmt[i:], re.IGNORECASE):
                start = i + 3
            elif start >= 0 and re.match(r"WHERE\b", stmt[i:], re.IGNORECASE):
                return stmt[start:i]
        i += 1
    return stmt[start:] if start >= 0 else ""


SQL_STR_RE = re.compile(r'"((?:[^"\\]|\\.)*)"', re.DOTALL)


def sql_text_of(stmt: str) -> str:
    """Concatenated string-literal contents of a Rust statement (the SQL),
    with Rust backslash-newline continuations removed per language semantics.
    Keyword/column matching runs on this, never on Rust code — `SET`/`WHERE`
    live at SQL depth 0 here while the literals sat inside `execute(` parens."""
    parts: list[str] = []
    for m in SQL_STR_RE.finditer(stmt):
        content = m.group(1)
        content = re.sub(r"\\\r?\n[ \t]*", "", content)
        parts.append(content)
    return "\n".join(parts)


def has_blob_overwrite(fn_text: str) -> bool:
    """An UPDATE statement assigning `source_details = ?N` in its SET clause
    instead of merging with `json_set` (WHERE-predicate comparisons are
    scoped out — SET-slice of the SQL text, not line-scoped)."""
    for stmt in split_statements(fn_text):
        sql = sql_text_of(stmt)
        if not re.search(r"\bUPDATE\b", sql, re.IGNORECASE):
            continue
        for m in re.finditer(
            r"\bsource_details\s*=\s*(\?\d*|json_set\s*\()", set_clause(sql)
        ):
            if m.group(1).startswith("?"):
                return True
    return False


def functions(cl: list[tuple[int, str]]) -> list[tuple[int, str]]:
    """(start lineno, joined body) per `fn` item, brace-depth segmented
    against the depth where the `fn` opened (methods live inside impl). The
    body must open before a close can end the item — a multiline signature
    has no brace on its first line, so emitting there would drop the body
    the rules need to see."""
    out: list[tuple[int, str]] = []
    depth = 0
    cur: list[str] | None = None
    start = 0
    base = 0
    opened = False
    for lineno, line in cl:
        opens, closes = code_braces(line)
        if cur is None and FN_RE.match(line):
            cur = []
            start = lineno
            base = depth
            opened = False
        if cur is not None:
            cur.append(line.split("//")[0])
        depth += opens - closes
        if depth > base:
            opened = True
        if cur is not None and opened and depth <= base:
            out.append((start, "\n".join(cur)))
            cur = None
    return out


def update_ok_findings(fn_text: str) -> list[str]:
    """Tables with a discarded UPDATE *and* a same-table INSERT sibling —
    the fall-through-to-duplicate shape (detection alone is not a finding:
    best-effort UPDATE-ok writes with no fallback stay silent)."""
    out: list[str] = []
    for table in update_ok_tables(fn_text):
        if re.search(
            rf"\bINSERT\s+INTO\s+{re.escape(table)}\b", fn_text, re.IGNORECASE
        ):
            out.append(table)
    return out


def findings_for_tree(root: Path) -> list[str]:
    findings: list[str] = []
    src = root / "src-tauri" / "src"
    for path in sorted(src.rglob("*.rs")):
        cl = code_lines(path)
        i = 0
        n = len(cl)
        while i < n:
            lineno, line = cl[i]
            code = line.split("//")[0]
            joined = code
            consumed = 1
            if "let _" in code and "=" in code:
                opens = code.count("(") - code.count(")")
                # rustfmt may wrap before the call's opening paren
                # (`let _ =` / `call(` on separate lines): join the
                # continuation in that case too.
                if opens <= 0 and code.rstrip().endswith("="):
                    opens = 1
                if opens > 0:
                    cur = code
                    while opens > 0 and consumed < 5 and i + consumed < n:
                        _nlineno, nline = cl[i + consumed]
                        ncode = nline.split("//")[0]
                        cur = cur + " " + ncode
                        opens += ncode.count("(") - ncode.count(")")
                        consumed += 1
                        if opens <= 0:
                            break
                    joined = cur
            if DB_SILENCE_RE.search(joined):
                findings.append(
                    FINDING_DB_SILENCE.format(
                        path=path.relative_to(root), lineno=lineno
                    )
                )
            i += consumed
        for start, fn_text in functions(cl):
            for table in update_ok_findings(fn_text):
                findings.append(
                    FINDING_UPDATE_OK_FALLBACK.format(
                        table=table,
                        path=path.relative_to(root),
                        lineno=start,
                    )
                )
            if has_blob_overwrite(fn_text):
                findings.append(
                    FINDING_BLOB_OVERWRITE.format(
                        path=path.relative_to(root), lineno=start
                    )
                )
        # Machine-local paths: raw lines (test modules included — the
        # silence rules above skip `mod tests`, scratch tests live there).
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for match in TEST_PATH_LITERAL_RE.finditer(line):
                if test_path_is_local(match.group(0)):
                    findings.append(
                        FINDING_TEST_PATH.format(
                            literal=match.group(0),
                            path=path.relative_to(root),
                            lineno=lineno,
                        )
                    )
    return findings


def findings_for(body: str) -> list[str]:
    findings: list[str] = []
    try:
        m1 = FILTER_MAP_RE.search(body)
    except re.error:
        print("review-patterns: filter_map search failed", file=sys.stderr)
        sys.exit(2)
    else:
        if m1:
            findings.append(FINDING_FILTER_MAP)
    try:
        m2 = LET_DISCARD_RE.search(body)
    except re.error:
        print("review-patterns: let-discard search failed", file=sys.stderr)
        sys.exit(2)
    else:
        if m2:
            findings.append(FINDING_LET_DISCARD)
    return findings


def self_test() -> int:
    """Adversarial fixtures for the function-scope rules (the twin-claim +
    tagger lessons — gates ship with their own probes, version-sync precedent)."""
    failures = 0

    def check(name: str, cond: bool) -> None:
        nonlocal failures
        print(("PASS " if cond else "FAIL ") + name)
        if not cond:
            failures += 1

    pos_update_ok = """
fn claim_receipt(conn: &Connection) {
    let exact = conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).is_ok();
    if !exact {
        conn.execute(
            "UPDATE item_transactions SET from_name = ?1 WHERE rowid = (SELECT MIN(rowid) FROM t)",
            params![name],
        )
        .ok();
    }
    conn.execute(
        "INSERT INTO item_transactions (a) VALUES (?1)",
        params![a],
    )
    .is_ok();
}
"""
    neg_update_ok_alone = """
fn accrue(conn: &Connection) {
    conn.execute(
        "UPDATE currency_estimate SET delta_since = delta_since + ?1 WHERE id = 1",
        params![d],
    )
    .ok();
}
"""
    neg_update_fail_closed = """
fn claim_fixed(conn: &Connection) {
    if let Err(e) = conn.execute(
        "UPDATE item_transactions SET from_name = ?1 WHERE rowid = 3",
        params![name],
    ) {
        eprintln!("{e}");
        return;
    }
    conn.execute("INSERT INTO item_transactions (a) VALUES (1)", []).is_ok();
}
"""
    check(
        "update-ok+insert fires",
        update_ok_findings(pos_update_ok) == ["item_transactions"],
    )
    check(
        "update-ok detected",
        update_ok_tables(neg_update_ok_alone) == ["currency_estimate"],
    )
    check("update-ok alone silent", update_ok_findings(neg_update_ok_alone) == [])
    check("fail-closed update silent", update_ok_findings(neg_update_fail_closed) == [])

    pos_blob = """
fn tag(conn: &Connection) {
    tx.execute(
        "UPDATE item_transactions SET source_kind = 'x', source_details = ?1 WHERE rowid = 2",
        params![blob],
    ).unwrap();
}
"""
    neg_blob_merge = """
fn tag_fixed(conn: &Connection) {
    tx.execute(
        "UPDATE item_transactions SET source_details = json_set(COALESCE(source_details, '{}'), '$.k', ?1) WHERE rowid = 2",
        params![v],
    ).unwrap();
}
"""
    neg_blob_select = """
fn read_it(conn: &Connection) {
    conn.query_row("SELECT a FROM t WHERE source_details = ?1", params![v], |r| r.get(0)).unwrap();
}
"""
    neg_blob_where = """
fn tag_where(conn: &Connection) {
    tx.execute(
        "UPDATE item_transactions SET source_kind = ?1 WHERE source_details = ?2",
        params![kind, blob],
    ).unwrap();
}
"""
    multi_sig = """
fn claim_receipt(
    conn: &Connection,
    item: &str,
) -> bool {
    conn.execute("UPDATE t SET a = 1", []).ok();
    conn.execute("INSERT INTO t (a) VALUES (1)", []).is_ok()
}
"""
    check("bare blob overwrite fires", has_blob_overwrite(pos_blob))
    check("json_set merge silent", not has_blob_overwrite(neg_blob_merge))
    check("select filter silent", not has_blob_overwrite(neg_blob_select))
    check("where predicate silent", not has_blob_overwrite(neg_blob_where))

    fns = functions([(i + 1, ln) for i, ln in enumerate(pos_update_ok.splitlines())])
    check("fn segmentation finds claim", len(fns) == 1 and fns[0][0] == 2)
    fns_multi = functions([(i + 1, ln) for i, ln in enumerate(multi_sig.splitlines())])
    check(
        "multiline signature keeps body",
        len(fns_multi) == 1
        and "INSERT INTO t" in fns_multi[0][1]
        and update_ok_findings(fns_multi[0][1]) == ["t"],
    )
    check(
        "lifetime pair keeps braces",
        code_braces("impl<'a, 'b> Foo<'a, 'b> {") == (1, 0),
    )
    check(
        "lifetime plus close-char keeps opens",
        code_braces("fn f(x: &'a str) { if c == '}' {") == (2, 0),
    )
    check(
        "lifetime plus open-char keeps opens",
        code_braces("fn f(x: &'a str, c: char) { if c == '{' {") == (2, 0),
    )
    check("open-char ignored", code_braces("if c == '{' {") == (1, 0))
    check("close-char ignored", code_braces("if c == '}' {") == (1, 0))
    check("escaped char ignored", code_braces("let c = '\\n'; {") == (1, 0))

    for literal in [
        '"C:/Users/TestUser/AppData/Reports/x.json"',
        '"C:/Users/Test/ChatLogs/Chat-26-03-06.log"',
        '"com.example.glogger|/home/u/.local/share/glogger"',
        '"C:/books/SkillReport_1.txt"',
        '"C:/Games/Reports/Character_Alice.json"',
        '"Saved report to C:/.../Reports/Character_Foo.json"',
        '"Teleportation Status:\\n\\nPrimary Bind Location: Serbule"',
        '"{\\"Title\\":\\"Skill Info\\"}"',
        '"https://cdn.projectgorgon.com/v1/data/x.json"',
    ]:
        check(f"synthetic path silent: {literal[:40]}", not test_path_is_local(literal))
    for literal in [
        '"C:/Users/cprivitere/AppData/LocalLow/Elder Game/Project Gorgon/ChatLogs/Chat-26-11-19.log"',
        '"D:/data/glogger.db"',
        '"C:\\\\Users\\\\cprivitere\\\\glogger.db"',
        '"/Users/cprivitere/Library/Logs/x.log"',
        '"/home/dev/.local/share/glogger.db"',
    ]:
        check(f"machine path fires: {literal[:40]}", test_path_is_local(literal))
    return 1 if failures else 0


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    try:
        body = read_body(root)
    except OSError:
        print("review-patterns: failed to read migrations.rs body", file=sys.stderr)
        return 2
    findings = findings_for(body)
    findings += findings_for_tree(root)
    for finding in findings:
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    sys.exit(main())
