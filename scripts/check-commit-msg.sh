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
