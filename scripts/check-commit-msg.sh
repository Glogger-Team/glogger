#!/usr/bin/env bash
#
# Warn (but don't block) when a commit message doesn't start with a known
# conventional prefix. This improves changelog generation quality.
#
# Known prefixes: feat, fix, impv, docs, test, build
#
# Runs as the pre-commit `commit-prefix` hook (commit-msg stage).
# Install: scripts/setup-hooks.sh
#
# Portable: POSIX sh + grep only.

# Self-test first: `bash scripts/check-commit-msg.sh --self-test` (wired
# into `mise run lint:shell`). Pins the warn-but-never-block contract:
# every path exits 0, prefixed/exempt messages stay silent, unprefixed warn.
if [ "${1:-}" = "--self-test" ]; then
  tmpdir=$(mktemp -d) || exit 2
  trap 'rm -rf "$tmpdir"' EXIT
  pass=0; total=0
  # usage: check <message> <expect-output: silent|warn>
  check() {
    total=$((total + 1))
    printf '%s\n' "$1" > "$tmpdir/msg"
    out=$(bash "$0" "$tmpdir/msg" 2>&1); code=$?
    if [ "$code" -ne 0 ]; then
      echo "self-test FAIL: '$1' exited $code (must always exit 0)"; return
    fi
    case "$2:$out" in
      silent:?*) echo "self-test FAIL: '$1' warned, expected silence"; return;;
      warn:"") echo "self-test FAIL: '$1' silent, expected warning"; return;;
    esac
    pass=$((pass + 1))
  }
  check "fix: correct scroll behavior" silent
  check "feat(api)!: breaking change" silent
  check "impv(scope): tidy" silent
  check "Merge branch 'x' into y" silent
  check "release: cut v1.2.3" silent
  check "no prefix here" warn
  check "chore: not a known prefix" warn
  [ "$pass" -eq "$total" ] || { echo "self-test: $pass/$total passed" >&2; exit 1; }
  echo "commit-msg self-test: $pass/$total passed"
  exit 0
fi

MSG_FILE="$1"
FIRST_LINE="$(head -1 "$MSG_FILE")"

# Allow merge commits and release commits unconditionally
if echo "$FIRST_LINE" | grep -qiE '^(Merge |release:)'; then
  exit 0
fi

# Check for a known prefix (with optional scope)
if echo "$FIRST_LINE" | grep -qE '^(feat|fix|impv|docs|test|build)(\(.*\))?!?:'; then
  exit 0
fi

echo ""
echo "  ⚠  Commit message has no conventional prefix."
echo "     Expected: feat|fix|impv|docs|test|build"
echo "     Example:  fix: correct scroll behavior on gourmand screen"
echo "     Write the subject for end users (what changed in the app), not the process (round N, CodeRabbit, backlog batch, SYNC_PAT)."
echo "     Example:  fix: StallTracker counts match after login without LocalPlayer sections"
echo ""
echo "     Your message: $FIRST_LINE"
echo ""
echo "  Committing anyway — this is just a warning to help changelog quality."
echo ""

exit 0
