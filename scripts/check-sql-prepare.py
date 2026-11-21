#!/usr/bin/env python3
"""Gate: every static SQL statement in app code must prepare against the
migrated schema (catches stale table/column names and syntax errors in paths
no test executes — the doc-orphan gate's counterpart for SQL drift).

Builds the schema by replaying DDL (CREATE TABLE/INDEX/TRIGGER, ALTER TABLE,
DROP TABLE) from migrations.rs in dispatcher order (run_migrations call order,
string-literal DML statement (SELECT/WITH/UPDATE/INSERT/DELETE/REPLACE) from
src .rs files except migrations.rs (its DML is version-sensitive). Dynamic SQL
(format! placeholders) and extension table-valued functions (rarray) are
skipped, never guessed. DDL replay failures exit 2 (fail closed — a broken
schema build is theater, not a pass).
"""

from __future__ import annotations

import re
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri" / "src"

# Table-valued functions from extensions (CLI-only), not stdlib sqlite3.
EXTENSION_TVFS = ("rarray",)
# format! placeholders look like {name} / {?} / {:x} and never contain a
# double-quote; '{"k":…}' object literals always do. '{}' (empty braces)
# is handled by the lookbehind below, not this regex.
FORMAT_PH_RE = re.compile(r"\{[^{}'\"]+\}")
DML_RE = re.compile(r"^\s*(SELECT|WITH|VALUES|UPDATE|INSERT|DELETE|REPLACE|EXPLAIN)\b")
DDL_RE = re.compile(
    r"^\s*(CREATE\s+(?:UNIQUE\s+|VIRTUAL\s+|TEMP(?:ORARY)?\s+)?"
    r"(?:TABLE|INDEX|TRIGGER)|ALTER\s+TABLE|DROP\s+TABLE)\b"
)

ESCAPES = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", '"': '"', "0": "\0"}


_RAW_OPEN_RE = re.compile(r"r(#+)?\"")
_CHAR_LIT_RE = re.compile(r"'(?:[^'\\]|\\.)'")


def lex_strings(text: str) -> list[tuple[int, str]]:
    """(offset, content) for every string literal, skipping // and /* */
    comments (nested) in a single left-to-right pass — regex lexing cascades
    on quotes inside comments, so comments and strings resolve together.
    Raw strings keep verbatim content; \"...\" strings are escape-unescaped."""
    out: list[tuple[int, str]] = []
    i, n = 0, len(text)
    while i < n:
        if text.startswith("//", i):
            end = text.find("\n", i)
            i = n if end == -1 else end
            continue
        if text.startswith("/*", i):
            depth = 1
            i += 2
            while i < n and depth > 0:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        m = _RAW_OPEN_RE.match(text, i)
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = m.group(1) or ""
            closer = '"' + "#" * len(hashes)
            start = m.end()
            end = text.find(closer, start)
            if end == -1:
                break
            out.append((start, text[start:end]))
            i = end + len(closer)
            continue
        if text[i] == '"':
            cur: list[str] = []
            start = i + 1
            i += 1
            while i < n:
                ch = text[i]
                if ch == "\\" and i + 1 < n and text[i + 1] in ESCAPES:
                    cur.append(ESCAPES[text[i + 1]])
                    i += 2
                    continue
                if ch == "\\" and i + 1 < n and text[i + 1] in ("\n", "\r"):
                    # Rust string continuation: backslash-newline strips the
                    # break and all leading whitespace of the next line.
                    i += 1
                    if text[i] == "\r" and i + 1 < n and text[i + 1] == "\n":
                        i += 1
                    i += 1
                    while i < n and text[i] in (" ", "\t"):
                        i += 1
                    continue
                if ch == '"':
                    break
                cur.append(ch)
                i += 1
            out.append((start, "".join(cur)))
            i += 1
            continue
        m = _CHAR_LIT_RE.match(text, i)
        if m:
            i = m.end()
            continue
        i += 1
    return out


def literals(text: str) -> list[tuple[int, str]]:
    """(1-based lineno, unescaped string content) for every string literal."""
    return [
        (text.count("\n", 0, off) + 1, content) for off, content in lex_strings(text)
    ]


def split_statements(sql: str) -> list[str]:
    """Split on `;` outside quoted regions and `--` line comments."""
    out: list[str] = []
    cur: list[str] = []
    quote: str | None = None
    i = 0
    while i < len(sql):
        ch = sql[i]
        if quote is not None:
            cur.append(ch)
            if ch == "\\" and i + 1 < len(sql):
                cur.append(sql[i + 1])
                i += 1
            elif ch == quote:
                if i + 1 < len(sql) and sql[i + 1] == quote:
                    cur.append(quote)  # '' escape
                    i += 1
                else:
                    quote = None
        elif ch in ("'", '"'):
            quote = ch
            cur.append(ch)
        elif ch == "-" and i + 1 < len(sql) and sql[i + 1] == "-":
            end = sql.find("\n", i)
            cur.append(sql[i:] if end == -1 else sql[i:end])
            i = len(sql) if end == -1 else end
            continue
        elif ch == ";":
            cur.append(ch)
            if "".join(cur).strip().strip(";").strip():
                out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
        i += 1
    if "".join(cur).strip().strip(";").strip():
        out.append("".join(cur))
    return out


FN_SPAN_RE = re.compile(r"^(?:pub\s+)?fn\s+(\w+)\s*\(", re.MULTILINE)
MIG_CALL_RE = re.compile(r"\b(migration_v\d+_\w+)\s*\(")
FORMAT_DDL_RE = re.compile(r"format!\s*\(\s*\"([^\"]*)\"", re.DOTALL)


def test_ranges(text: str) -> list[tuple[int, int]]:
    """(start, end) 0-based line ranges of `#[cfg(test)] mod ...` blocks."""
    lines = text.splitlines()
    out: list[tuple[int, int]] = []
    i = 0
    while i < len(lines):
        if lines[i].strip() == "#[cfg(test)]" and i + 1 < len(lines):
            m = re.match(r"^\s*mod\s+\w+\s*\{", lines[i + 1])
            if m:
                depth = 0
                j = i + 1
                while j < len(lines):
                    depth += lines[j].count("{") - lines[j].count("}")
                    if depth <= 0:
                        break
                    j += 1
                out.append((i, j))
                i = j + 1
                continue
        i += 1
    return out


def fn_spans(text: str) -> list[tuple[str, int, int]]:
    """(name, start_line, end_line) for every top-level fn (0-based, end
    exclusive). Ends at the next top-level fn or a col-0 `}`."""
    lines = text.splitlines()
    starts: list[tuple[str, int]] = [
        (m.group(1), text.count("\n", 0, m.start())) for m in FN_SPAN_RE.finditer(text)
    ]
    bounds = sorted({s for _, s in starts})
    out: list[tuple[str, int, int]] = []
    for name, s in starts:
        e = len(lines)
        for b in bounds:
            if b > s:
                e = b
                break
        for k in range(s + 1, len(lines)):
            if lines[k].startswith("}"):
                e = min(e, k + 1)
                break
        out.append((name, s, e))
    return out


def build_schema(conn: sqlite3.Connection, include_tests: bool = True) -> None:
    """Replay migration DDL in dispatcher order (file order is NOT version
    order — v73's def precedes v63's). run_migrations' own DDL first, then
    each called migration_vNN fn. Non-versioned helpers holding DDL, and
    format!-built DDL, fail closed instead of replaying silently wrong.
    App-code CREATEs follow: test fixtures only when include_tests (the
    prod schema must never see test-only tables, or test fixtures would
    mask a missing migration)."""
    mig = (SRC / "db" / "migrations.rs").read_text(encoding="utf-8")
    tested = test_ranges(mig)

    def in_test(start: int) -> bool:
        return any(a <= start <= b for a, b in tested)

    spans = {name: (s, e) for name, s, e in fn_spans(mig) if not in_test(s)}
    if "run_migrations" not in spans:
        print("sql-prepare: run_migrations fn not found")
        raise SystemExit(2)
    rs, re_ = spans["run_migrations"]
    dispatch = "\n".join(mig.splitlines()[rs:re_])
    ordered = ["run_migrations"] + MIG_CALL_RE.findall(dispatch)
    seen: set[str] = set()
    for name in ordered:
        if name in seen:
            continue
        seen.add(name)
        if name not in spans:
            print(f"sql-prepare: dispatched {name} has no fn span")
            raise SystemExit(2)
        s, e = spans[name]
        body = "\n".join(mig.splitlines()[s:e])
        for lit in FORMAT_DDL_RE.findall(body):
            if DDL_RE.match(lit):
                print(f"sql-prepare: dynamic DDL in {name} — inline it for replay")
                raise SystemExit(2)
        for _, lit in literals(body):
            if not any(DDL_RE.match(s) for s in split_statements(lit)):
                continue
            try:
                conn.executescript(lit)
            except sqlite3.Error as err:
                print(
                    f"sql-prepare: schema replay failed in {name}: {err}\n---\n{lit[:400]}"
                )
                raise SystemExit(2)
    for name, (s, e) in spans.items():
        if name in seen or name == "run_migrations":
            continue
        body = "\n".join(mig.splitlines()[s:e])
        for _, lit in literals(body):
            if DDL_RE.match(lit):
                print(
                    f"sql-prepare: DDL in non-dispatched helper {name} — move it into its version"
                )
                raise SystemExit(2)
    # App-code tables (TEMP work tables, export-file schemas, test fixtures):
    # CREATE them when the schema lacks the name. Existing names win, and
    # test fixtures stay out of the prod schema (their tables must never
    # mask a missing migration for app code).
    for _table, sql, _path, _lineno, is_test in app_table_defs():
        if is_test and not include_tests:
            continue
        try:
            conn.executescript(sql)
        except sqlite3.OperationalError as err:
            if "already exists" not in str(err):
                raise


def in_format_macro(text: str, lit_start: int) -> bool:
    """Heuristic: the literal looks like a format!() argument (macro name with
    unbalanced open paren and no statement end between it and the literal).
    Such SQL is dynamic — table names live in variables, not the literal."""
    idx = text.rfind("format!", 0, lit_start)
    if idx == -1:
        return False
    between = text[idx + len("format!") : lit_start]
    m = re.match(r"\s*\(", between)
    if not m:
        return False
    rest = between[m.end() :]
    return ";" not in rest and rest.count("(") >= rest.count(")")


PLACEHOLDER_RE = re.compile(r"\?\d*|[:@$][A-Za-z_][A-Za-z0-9_]*|[@$][A-Za-z0-9_]+")


def bind_nulls(stmt: str) -> str:
    """Replace ?/?N/:name/@name/$name placeholders with NULL outside quoted
    regions and comments ('12:30', 'http://x', and comment apostrophes like
    session's must survive verbatim)."""
    out: list[str] = []
    i, n = 0, len(stmt)
    quote: str | None = None
    while i < n:
        ch = stmt[i]
        if quote is not None:
            out.append(ch)
            if ch == quote:
                if i + 1 < n and stmt[i + 1] == quote:
                    out.append(quote)
                    i += 1
                else:
                    quote = None
            i += 1
            continue
        if ch in ("'", '"'):
            quote = ch
            out.append(ch)
            i += 1
            continue
        if ch == "-" and i + 1 < n and stmt[i + 1] == "-":
            end = stmt.find("\n", i)
            out.append(stmt[i:] if end == -1 else stmt[i:end])
            i = n if end == -1 else end
            continue
        if ch == "/" and i + 1 < n and stmt[i + 1] == "*":
            end = stmt.find("*/", i)
            if end == -1:
                out.append(stmt[i:])
                break
            out.append(stmt[i : end + 2])
            i = end + 2
            continue
        m = PLACEHOLDER_RE.match(stmt, i)
        if m:
            out.append("NULL")
            i = m.end()
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def lstrip_sql_comments(sql: str) -> str:
    """Strip leading `--` line comments and `/* */` blocks (and whitespace)
    for statement classification only — the full text still goes to EXPLAIN."""
    s = sql.lstrip()
    while True:
        if s.startswith("--"):
            end = s.find("\n")
            s = "" if end == -1 else s[end + 1 :].lstrip()
            continue
        if s.startswith("/*"):
            end = s.find("*/")
            if end == -1:
                return ""
            s = s[end + 2 :].lstrip()
            continue
        return s


def statements_in(
    path: Path,
) -> tuple[list[tuple[int, str, bool]], list[tuple[int, str]]]:
    """(DML statements, skipped dynamic literals) in one Rust file.

    Each statement is (lineno, sql, in_test). Each skip is (lineno,
    leading-80-chars) for a DML-shaped literal unresolvable statically
    (format! placeholders or a format! macro argument). Callers surface
    the skips so the dynamic-SQL hole stays visible instead of silent.
    """
    out: list[tuple[int, str, bool]] = []
    skipped: list[tuple[int, str]] = []
    text = path.read_text(encoding="utf-8")
    tested = test_ranges(text)
    lexed = lex_strings(text)
    for (lineno, lit), (off, _content) in zip(literals(text), lexed):
        if not DML_RE.match(lstrip_sql_comments(lit)):
            continue
        if FORMAT_PH_RE.search(lit) or in_format_macro(text, off):
            skipped.append((lineno, " ".join(lit.split())[:80]))
            continue  # dynamic SQL — cannot resolve statically
        tested_here = in_test_range(tested, lineno - 1)
        for stmt in split_statements(lit):
            if DML_RE.match(lstrip_sql_comments(stmt)):
                out.append((lineno, stmt, tested_here))
    return out, skipped


def check_statement(
    conn: sqlite3.Connection, path: Path, lineno: int, stmt: str
) -> str | None:
    # Bind NULL for every placeholder: EXPLAIN prepares without executing,
    # and NULL is valid in every position (comparisons, VALUES, LIMIT).
    probe = bind_nulls(stmt)
    try:
        conn.execute("EXPLAIN " + probe)
        return None
    except sqlite3.ProgrammingError as e:
        return f"{path.relative_to(ROOT)}:{lineno}: unbound placeholders: {e}\n---\n{stmt[:300]}"
    except sqlite3.OperationalError as e:
        msg = str(e)
        for tvf in EXTENSION_TVFS:
            if f"no such table: {tvf}" in msg:
                return None  # extension TVF — skipped, documented above
        return f"{path.relative_to(ROOT)}:{lineno}: unpreparable SQL: {msg}"


def self_test() -> int:
    failures = 0

    def check(name: str, cond: bool) -> None:
        nonlocal failures
        print(("PASS " if cond else "FAIL ") + name)
        if not cond:
            failures += 1

    ddl = """
        CREATE TABLE t1 (id INTEGER PRIMARY KEY, name TEXT);
        ALTER TABLE t1 ADD COLUMN score INTEGER DEFAULT 0;
        CREATE TABLE log (id INTEGER PRIMARY KEY);
        CREATE TRIGGER log_del AFTER DELETE ON t1 BEGIN DELETE FROM log WHERE id = OLD.id; END;
    """
    conn = sqlite3.connect(":memory:")
    conn.executescript(ddl)
    for stmt in (
        "SELECT id, name FROM t1 WHERE score > ?1",
        "INSERT INTO t1 (name) VALUES (?1)",
        "UPDATE t1 SET score = ?1 WHERE id = ?2",
        "DELETE FROM log WHERE id = ?1",
    ):
        check(
            f"good prepares: {stmt[:30]}",
            check_statement(conn, ROOT / "x.rs", 1, stmt) is None,
        )
    bad_col = check_statement(conn, ROOT / "x.rs", 7, "SELECT nope FROM t1")
    check("bad column fires", bad_col is not None and "no such column" in bad_col)
    bad_tab = check_statement(conn, ROOT / "x.rs", 8, "SELECT * FROM missing_t")
    check("bad table fires", bad_tab is not None and "no such table" in bad_tab)
    check(
        "named param binds",
        check_statement(conn, ROOT / "x.rs", 9, "SELECT * FROM t1 WHERE name = :n")
        is None,
    )
    check(
        "quoted colon survives",
        check_statement(conn, ROOT / "x.rs", 9, "SELECT * FROM t1 WHERE name = '12:30'")
        is None,
    )
    check(
        "comment apostrophe survives",
        check_statement(
            conn,
            ROOT / "x.rs",
            9,
            "SELECT id FROM t1 -- session's row\nWHERE score > ?1",
        )
        is None,
    )
    conn.executescript("CREATE TEMP TABLE w (k TEXT NOT NULL)")
    check(
        "temp table prepares",
        check_statement(conn, ROOT / "x.rs", 9, "INSERT INTO w (k) VALUES (?1)")
        is None,
    )
    import tempfile

    with tempfile.NamedTemporaryFile(
        "w", suffix=".rs", delete=False, encoding="utf-8"
    ) as f:
        f.write('conn.execute(&format!("DELETE FROM {t}"), [])')
        tmp = Path(f.name)
    try:
        got, skipped = statements_in(tmp)
        check("table placeholder skipped", got == [])
        check("table placeholder counted", len(skipped) == 1)
    finally:
        tmp.unlink()
    with tempfile.NamedTemporaryFile(
        "w", suffix=".rs", delete=False, encoding="utf-8"
    ) as f:
        f.write(
            'let q = format!("SELECT * FROM {} WHERE a = {}", t, v);\n'
            "conn.execute(\"SELECT * FROM t1 WHERE name = '{}'\", [])"
        )
        tmp = Path(f.name)
    try:
        got, skipped = statements_in(tmp)
        check("empty-brace format skipped via lookbehind", len(got) == 1)
        check("empty-brace format counted", len(skipped) == 1)
    finally:
        tmp.unlink()
    with tempfile.NamedTemporaryFile(
        "w", suffix=".rs", delete=False, encoding="utf-8"
    ) as f:
        f.write('conn.execute("INSERT INTO t1 (name) VALUES (\'{\\"a\\":1}\')", [])')
        tmp = Path(f.name)
    try:
        got, skipped = statements_in(tmp)
        check("non-empty JSON literal is kept, not skipped", len(got) == 1)
        check("kept literal not counted", skipped == [])
    finally:
        tmp.unlink()
    check(
        "backslash continuation lexes",
        lex_strings('"SELECT a \\\n     FROM t1"')[0][1] == "SELECT a FROM t1",
    )
    t1_conn = sqlite3.connect(":memory:")
    t1_conn.executescript("CREATE TABLE t1 (id INTEGER, name TEXT)")
    with tempfile.NamedTemporaryFile(
        "w", suffix=".rs", delete=False, encoding="utf-8"
    ) as f:
        f.write(
            '"DELETE FROM t1; -- next operation\nINSERT INTO t1 (missing_col) VALUES (1)"'
        )
        tmp = Path(f.name)
    try:
        got, _skipped = statements_in(tmp)
        check("comment-led INSERT classified", len(got) == 2)
        hit = check_statement(t1_conn, ROOT / "x.rs", 1, got[1][1])
        check(
            "comment-led bad column fires",
            hit is not None and "missing_col" in hit,
        )
    finally:
        tmp.unlink()
    with tempfile.NamedTemporaryFile(
        "w", suffix=".rs", delete=False, encoding="utf-8"
    ) as f:
        f.write(
            "#[cfg(test)]\nmod tests {\n"
            '    fn t() { conn.execute("SELECT * FROM t1", []).unwrap(); }\n'
            "}\n"
            'fn a() { conn.execute("SELECT * FROM t1", []).unwrap(); }\n'
        )
        tmp = Path(f.name)
    try:
        got, _skipped = statements_in(tmp)
        flags = sorted(in_test for _, _, in_test in got)
        check("test flag marks mod body", flags == [False, True])
    finally:
        tmp.unlink()
    check(
        "columns_of_create reads schema",
        columns_of_create("CREATE TABLE t (a INTEGER, b TEXT)", "t") == {"a", "b"},
    )
    check(
        "columns_of_create fails closed",
        columns_of_create("CREATE TABLE t (", "t") is None,
    )
    check(
        "split shape detected",
        columns_of_create("CREATE TABLE t (a INTEGER, c TEXT)", "t") != {"a", "b"},
    )
    split_conn = sqlite3.connect(":memory:")
    split_conn.executescript(
        "CREATE TABLE t1 (id INTEGER, name TEXT);CREATE TABLE t2 (id INTEGER);"
    )
    real_defs = app_table_defs
    try:
        globals()["app_table_defs"] = lambda: [
            (
                "t1",
                "CREATE TABLE t1 (id INTEGER, name TEXT, extra TEXT)",
                "app.rs",
                1,
                False,
            ),
            ("t2", "CREATE TABLE t2 (id INTEGER)", "app.rs", 2, False),
        ]
        split_hits = check_splits(split_conn)
    finally:
        globals()["app_table_defs"] = real_defs
    check(
        "split probe fires on divergent table",
        len(split_hits) == 1 and "t1" in split_hits[0],
    )
    check(
        "split probe silent on matching table",
        all("t2" not in hit for hit in split_hits),
    )
    return 1 if failures else 0


CREATE_TABLE_RE = re.compile(
    r"\s*CREATE\s+(?:TEMP(?:ORARY)?\s+)?TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)",
    re.IGNORECASE,
)


def in_test_range(ranges: list[tuple[int, int]], lineno0: int) -> bool:
    return any(a <= lineno0 <= b for a, b in ranges)


def app_table_defs() -> list[tuple[str, str, str, int, bool]]:
    """(table, create_sql, path, lineno, in_test) for CREATE TABLE in app
    code (migrations excluded — they own the canonical schema)."""
    out: list[tuple[str, str, str, int, bool]] = []
    for path in sorted(SRC.rglob("*.rs")):
        if path.name == "migrations.rs":
            continue
        text = path.read_text(encoding="utf-8")
        tested = test_ranges(text)
        for (lineno, lit), (off, _content) in zip(literals(text), lex_strings(text)):
            tested_here = in_test_range(tested, lineno - 1)
            for stmt in split_statements(lit):
                m = CREATE_TABLE_RE.match(stmt)
                if m:
                    out.append((m.group(1), stmt, path.as_posix(), lineno, tested_here))
    return out


def table_columns(conn: sqlite3.Connection, table: str) -> set[str] | None:
    try:
        rows = conn.execute(f"PRAGMA table_info({table})").fetchall()
    except sqlite3.Error:
        return None
    return {r[1] for r in rows} or None


def check_splits(conn: sqlite3.Connection) -> list[str]:
    """Same table defined differently in app DDL vs the migrated schema
    (the survey_types split: clear_cdn_data recreated it WITH kind while
    migrations never had it — two live schemas for one table)."""
    findings: list[str] = []
    reported: set[str] = set()
    for table, sql, path, lineno, is_test in app_table_defs():
        if table in reported or is_test:
            continue  # test fixtures define their own schemas — not app drift
        base = table_columns(conn, table)
        if base is None:
            continue  # temp/export-only table, no canonical schema to split from
        other = columns_of_create(sql, table)
        if other is None:
            continue  # unbuildable in isolation (depends on migration state) — skip
        if other != base:
            reported.add(table)
            findings.append(
                f"{path}:{lineno}: schema split: app DDL for {table} has columns"
                f" {sorted(other)} but migrations have {sorted(base)} — unify them"
            )

    return findings


def columns_of_create(sql: str, table: str) -> set[str] | None:
    """Column names of a standalone CREATE TABLE (isolated scratch DB)."""
    scratch = sqlite3.connect(":memory:")
    try:
        scratch.executescript(sql)
    except sqlite3.Error:
        return None
    return table_columns(scratch, table)


def main() -> int:
    prod = sqlite3.connect(":memory:")
    build_schema(prod, include_tests=False)
    full = sqlite3.connect(":memory:")
    build_schema(full, include_tests=True)
    tables = {
        r[0] for r in prod.execute("SELECT name FROM sqlite_master WHERE type='table'")
    }
    print(f"sql-prepare: schema has {len(tables)} tables", file=sys.stderr)
    findings: list[str] = []
    findings += check_splits(prod)
    skipped_total = 0
    for path in sorted(SRC.rglob("*.rs")):
        if path.name == "migrations.rs":
            continue
        stmts, skipped = statements_in(path)
        for lineno, snippet in skipped:
            skipped_total += 1
            print(
                f"sql-prepare: skipped dynamic SQL at"
                f" {path.relative_to(ROOT)}:{lineno}: {snippet}",
                file=sys.stderr,
            )
        for lineno, stmt, in_test in stmts:
            # Test DML prepares against the full schema (fixtures included);
            # app DML must prepare against prod — a test-only table must
            # never mask a missing migration for shipped code.
            hit = check_statement(full if in_test else prod, path, lineno, stmt)
            if hit:
                findings.append(hit)
    print(f"sql-prepare: skipped {skipped_total} dynamic statements", file=sys.stderr)
    for finding in sorted(set(findings)):
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    sys.exit(main())
