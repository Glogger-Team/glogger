#!/usr/bin/env bash
# Gate: mechanically-checkable defect shapes from frozen-review/CodeRabbit
# rounds. Bulk-UPDATE scoping and cache-guard checks need multi-line
# context grep can't do precisely — those stay on the AGENTS.md
# "Pre-commit self-review" checklist (human judgment per diff).
#
# Portable: POSIX sh + awk + perl only. No `grep -P`/`-z` (macOS stock
# grep has neither), no PCRE-grep dependency. Stderr/stdout discipline:
# each search stage fails LOUD (exit 2) on tool error, reports finding
# (exit 1 via fail=1) on match, passes silent on no match — a broken
# search never reads as a pass.
set -eu
fail=0

body() { awk '/^mod tests \{/{exit} {print}' src-tauri/src/db/migrations.rs; }

buf=$(body) || { echo "review-patterns: failed to read migrations.rs body" >&2; exit 2; }

# No filter_map(|x| x.ok()) row-read discards in migration bodies. The
# single-transaction run turns a silent skip into a recorded version lie
# (v81 swallowed I/O failures, then recorded v81).
m1=$(printf '%s' "$buf" | perl -0777 -ne 'print "filter_map-discard\n" while /filter_map\(\s*\|\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\|\s*\1\.ok\(\)/g') \
  || { echo "review-patterns: filter_map search failed" >&2; exit 2; }
if [ -n "$m1" ]; then
  echo "review-patterns: filter_map(|x| x.ok()) row-read discard in migrations.rs body — collect::<Result<..>>()? instead"
  fail=1
fi

# No discarded write/row-read bindings on DB handles in migration bodies.
# Multiline form (`let _ =` newline `conn.execute(`) included; handle
# whitelist (conn|tx|stmt) keeps best-effort file deletes out of scope.
m2=$(printf '%s' "$buf" | perl -0777 -ne 'print "let-discard\n" while /let _\s*=\s*(conn|tx|stmt)\.(execute|execute_batch|query_row|query_map)/g') \
  || { echo "review-patterns: let-discard search failed" >&2; exit 2; }
if [ -n "$m2" ]; then
  echo "review-patterns: discarded migration write/row-read (let _ = ...) — propagate with ? (v81 lesson)"
  fail=1
fi

exit $fail
