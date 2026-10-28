#!/usr/bin/env bash
# Gate: mechanically-checkable defect shapes from frozen-review/CodeRabbit
# rounds. Bulk-UPDATE scoping and cache-guard checks need multi-line
# context grep can't do precisely — those stay on the AGENTS.md
# "Pre-commit self-review" checklist (human judgment per diff).
set -euo pipefail
fail=0

# No discarded write/row-read results in migration bodies. The
# single-transaction run turns a silent skip into a recorded version lie
# (v81: discarded `execute` + `filter_map(|r| r.ok())` swallowed I/O
# failures, then recorded v81). Test module excluded (split at `mod tests`).
body() { awk '/^mod tests \{/{exit} {print}' src-tauri/src/db/migrations.rs; }
if body | grep -n -Pzo 'filter_map\(\s*\|\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\|\s*\1\.ok\(\)' | tr '\0' '\n' | grep -q .; then
  echo "review-patterns: filter_map(|x| x.ok()) row-read discard in migrations.rs body — collect::<Result<..>>()? instead"
  fail=1
fi
if body | grep -n -Pzo 'let _\s*=\s*\n?\s*(conn|tx|stmt)\.(execute|execute_batch|query_row|query_map)' | tr '\0' '\n' | grep -q .; then
  echo "review-patterns: discarded migration write/row-read (let _ = ...) — propagate with ? (v81 lesson)"
  fail=1
fi

exit $fail
