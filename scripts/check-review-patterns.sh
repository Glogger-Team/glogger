#!/usr/bin/env bash
# Gate: mechanically-checkable defect shapes from frozen-review/CodeRabbit
# rounds. Bulk-UPDATE scoping and cache-guard checks need multi-line
# context grep can't do precisely — those stay on the AGENTS.md
# "Pre-commit self-review" checklist (human judgment per diff).
set -euo pipefail
fail=0

# No discarded write/row-read results in migration bodies. The
# single-transaction run turns a silent skip into a recorded version lie
# (v81: `let _ = execute` + `filter_map(|r| r.ok())` swallowed I/O
# failures, then recorded v81). Test module excluded (line 3495+).
if head -n 3494 src-tauri/src/db/migrations.rs | grep -n -E 'let _ = .*execute|filter_map\(\|r\| r\.ok\(\)\)'; then
  echo "review-patterns: discarded write/row-read in migrations.rs body — propagate with ? (v81 lesson)"
  fail=1
fi

exit $fail
