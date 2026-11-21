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
# format! placeholders look like {name} / {?} / {:x}; '{}' is a JSON literal.
FORMAT_PH_RE = re.compile(r"\{[^{}']+\}")
# Rust string literal: "..." (escapes) or r#"..."# (any hash depth).
STR_RE = re.compile(r'r(#+)"(.*?)"\1|"(?:[^"\\]|\\.)*"', re.DOTALL)
DML_RE = re.compile(r"^\s*(SELECT|WITH|VALUES|UPDATE|INSERT|DELETE|REPLACE|EXPLAIN)\b")
DDL_RE = re.compile(
    r"^\s*(CREATE\s+(?:UNIQUE\s+|VIRTUAL\s+|TEMP(?:ORARY)?\s+)?"
    r"(?:TABLE|INDEX|TRIGGER)|ALTER\s+TABLE|DROP\s+TABLE)\b"
)

ESCAPES = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", '"': '"', "0": "\0"}


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


def strip_comments(text: str) -> str:
    """Identity helper kept for call-site shape: extraction is comment-aware
    at the lexer, so no pre-stripping is needed."""
    return text


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


def build_schema(conn: sqlite3.Connection) -> None:
    """Replay migration DDL in dispatcher order (file order is NOT version
    order — v73's def precedes v63's). run_migrations' own DDL first, then
    each called migration_vNN fn. Non-versioned helpers holding DDL, and
    format!-built DDL, fail closed instead of replaying silently wrong."""
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
    # App-code tables (TEMP work tables, export-file schemas): same CREATE
    # shape, so CREATE them when the migrated schema lacks the name — this
    # also column-checks temp/export statements. Existing names win (test
    # fixtures must never override the migrated schema).
    for path in sorted(SRC.rglob("*.rs")):
        if path.name == "migrations.rs":
            continue
        for _, lit in literals(path.read_text(encoding="utf-8")):
            for stmt in split_statements(lit):
                if re.match(r"\s*CREATE\s+(?:TEMP(?:ORARY)?\s+)?TABLE\b", stmt):
                    try:
                        conn.executescript(stmt)
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


def statements_in(path: Path) -> list[tuple[int, str]]:
    out: list[tuple[int, str]] = []
    text = path.read_text(encoding="utf-8")
    lexed = lex_strings(text)
    for (lineno, lit), (off, _content) in zip(literals(text), lexed):
        if not DML_RE.match(lit):
            continue
        if FORMAT_PH_RE.search(lit) or in_format_macro(text, off):
            continue  # dynamic SQL — cannot resolve statically
        for stmt in split_statements(lit):
            if DML_RE.match(stmt):
                out.append((lineno, stmt))
    return out


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
        check("table placeholder skipped", statements_in(tmp) == [])
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
        got = statements_in(tmp)
        check("empty-brace format skipped via lookbehind", len(got) == 1)
    finally:
        tmp.unlink()
    check(
        "backslash continuation lexes",
        lex_strings('"SELECT a \\\n     FROM t1"')[0][1] == "SELECT a FROM t1",
    )
    check(
        "trigger with BEGIN/END splits sanely",
        len(split_statements("DELETE FROM a; DELETE FROM b;")) == 2,
    )
    return 1 if failures else 0


def main() -> int:
    conn = sqlite3.connect(":memory:")
    build_schema(conn)
    tables = {
        r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")
    }
    print(f"sql-prepare: schema has {len(tables)} tables", file=sys.stderr)
    findings: list[str] = []
    for path in sorted(SRC.rglob("*.rs")):
        if path.name == "migrations.rs":
            continue
        for lineno, stmt in statements_in(path):
            hit = check_statement(conn, path, lineno, stmt)
            if hit:
                findings.append(hit)
    for finding in sorted(set(findings)):
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    sys.exit(main())
