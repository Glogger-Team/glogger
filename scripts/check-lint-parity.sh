#!/usr/bin/env bash
# Gate: lint-parity drift between the three places that name checks.
# `mise.toml` owns the check list — `.pre-commit-config.yaml` entries and
# `AGENTS.md` Linting-table mentions must resolve to tasks it defines.
# Also guards the TODO.md location contract (grid census + docs index
# must agree with the file on disk).
#
# Portable: POSIX sh + grep + sed only. Stderr/stdout discipline: each
# stage fails LOUD (exit 2) on tool error, reports finding (exit 1 via
# fail=1) on drift, passes silent on no drift — a broken search never
# reads as a pass.
set -eu
fail=0

# 1. Every `mise run <task>` in the pre-commit config resolves to a task
#    mise.toml defines. (Catches renames that update one file but not the
#    other — the stringly-typed seam the parity work left behind.)
tasks=$(mise tasks --name-only 2>/dev/null) \
  || { echo "lint-parity: mise tasks --name-only failed" >&2; exit 2; }
[ -n "$tasks" ] || { echo "lint-parity: empty mise task list" >&2; exit 2; }
[ -r .pre-commit-config.yaml ] || { echo "lint-parity: cannot read .pre-commit-config.yaml" >&2; exit 2; }
refs=$(grep -oE 'mise run [a-z0-9:.-]+' .pre-commit-config.yaml | sed 's/^mise run //') \
  || { echo "lint-parity: failed to read .pre-commit-config.yaml refs" >&2; exit 2; }
for ref in $refs; do
  printf '%s\n' "$tasks" | grep -qxF "$ref" \
    || { echo "lint-parity: .pre-commit-config.yaml calls 'mise run $ref', not defined in mise.toml"; fail=1; }
done

# 2. Every lint:* slice named in AGENTS.md's Linting table resolves too.
#    (Same seam, second consumer: task renames must land in docs as well.)
[ -r AGENTS.md ] || { echo "lint-parity: cannot read AGENTS.md" >&2; exit 2; }
mentions=$(sed -n '/^## Linting/,/^## /p' AGENTS.md | grep -oE 'lint:[a-z-]+' || true)
for m in $mentions; do
  printf '%s\n' "$tasks" | grep -qxF "$m" \
    || { echo "lint-parity: AGENTS.md names '$m', not defined in mise.toml"; fail=1; }
done

# 3. TODO.md location contract: check-grid-standards.mjs reads it off
#    disk (join(ROOT, 'TODO.md')) and docs/index.md links it — both must
#    agree with the file that actually exists.
if [ -f TODO.md ] && [ -f docs/TODO.md ]; then
  echo "lint-parity: TODO.md exists at both root and docs/ — pick one"
  fail=1
elif [ -f TODO.md ]; then
  grep -q "join(ROOT, 'TODO.md')" scripts/check-grid-standards.mjs \
    || { echo "lint-parity: root TODO.md exists but check-grid-standards.mjs reads elsewhere"; fail=1; }
  grep -q '](\.\./TODO\.md)' docs/index.md \
    || { echo "lint-parity: root TODO.md exists but docs/index.md links elsewhere"; fail=1; }
  grep -rn "docs/TODO\.md" AGENTS.md README.md docs/index.md scripts/check-grid-standards.mjs .github/workflows/lint.yml mise.toml .pre-commit-config.yaml 2>/dev/null \
    && { echo "lint-parity: stale docs/TODO.md pointer survives the root move"; fail=1; } || true
elif [ -f docs/TODO.md ]; then
  grep -q "join(ROOT, 'docs/TODO.md')" scripts/check-grid-standards.mjs \
    || { echo "lint-parity: docs/TODO.md exists but check-grid-standards.mjs reads elsewhere"; fail=1; }
  grep -q '](TODO\.md)' docs/index.md \
    || { echo "lint-parity: docs/TODO.md exists but docs/index.md links elsewhere"; fail=1; }
else
  echo "lint-parity: no TODO.md at root or docs/ — one must exist"
  fail=1
fi

# 4. lint:fast membership matches its contract: the commit stage must
#    stay free of build/test/network-weight slices (the pre-commit hook
#    budget is seconds). New heavy slices belong in `lint`, not `lint:fast`.
[ -r mise.toml ] || { echo "lint-parity: cannot read mise.toml" >&2; exit 2; }
block=$(sed -n '/\[tasks\."lint:fast"\]/,/^\[/p' mise.toml)
[ -n "$block" ] || { echo "lint-parity: lint:fast stanza missing in mise.toml" >&2; exit 2; }
fast=$(printf '%s\n' "$block" | grep -oE '"lint:[a-z-]+"' | tr -d '"')
for heavy in lint:build lint:clippy lint:test lint:audit lint:constraints; do
  printf '%s\n' "$fast" | grep -qxF "$heavy" \
    && { echo "lint-parity: heavy slice '$heavy' in lint:fast (commit stage must stay seconds-scale)"; fail=1; } || true
done

exit $fail
