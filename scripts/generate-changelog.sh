#!/usr/bin/env bash
#
# Generate release notes from git commits since a base tag (normally the last
# *published* release — see scripts/last-published-tag.sh; basing notes on
# the nearest tag orphans the commits of any tag whose Release run failed).
# Outputs markdown to stdout (scripts/release.sh writes it to RELEASE_NOTES.md,
# which the release workflow publishes on the GitHub Release and in latest.json).
#
# Usage:
#   ./scripts/generate-changelog.sh                 # since the nearest v* tag, to HEAD
#   ./scripts/generate-changelog.sh v0.1.0          # since a specific tag
#   ./scripts/generate-changelog.sh v1.0.2 v1.0.3^  # a historical range (until ref)
#
# Commit prefixes are the source of truth for the notes:
#   feat: -> Features, impv: -> Improvements, fix: -> Fixes.
# docs:/test:/build:/chore:/release: and merges are intentionally excluded —
# anything a user should read about belongs in a feat:/impv:/fix: commit.

set -euo pipefail

SINCE_TAG="${1:-}"
UNTIL_REF="${2:-HEAD}"

if [ -z "$SINCE_TAG" ]; then
  # Find the most recent release tag (ignore non-release tags like `updater-latest`)
  SINCE_TAG=$(git describe --tags --abbrev=0 --match 'v[0-9]*' "$UNTIL_REF" 2>/dev/null || echo "")
fi

if [ -z "$SINCE_TAG" ]; then
  # No tags exist — use all commits
  RANGE="$UNTIL_REF"
  echo "## What's New"
  echo ""
  echo "Initial release."
  echo ""
else
  RANGE="${SINCE_TAG}..${UNTIL_REF}"
  echo "## What's Changed since ${SINCE_TAG}"
  echo ""
fi

# Categorize commits
declare -a features=()
declare -a improvements=()
declare -a fixes=()

while IFS= read -r line; do
  [ -z "$line" ] && continue

  # Extract short hash and message
  HASH="${line%% *}"
  MSG="${line#* }"

  case "$MSG" in
    feat:*|feat\(*)
      features+=("- ${MSG} (\`${HASH}\`)")
      ;;
    fix:*|fix\(*)
      fixes+=("- ${MSG} (\`${HASH}\`)")
      ;;
    impv:*|impv\(*)
      improvements+=("- ${MSG} (\`${HASH}\`)")
      ;;
    *)
      # release/merge/docs/build/chore/test commits are intentionally
      # excluded from user-facing release notes.
      ;;
  esac
done < <(git log --oneline --no-decorate "$RANGE" 2>/dev/null)

if [ ${#features[@]} -gt 0 ]; then
  echo "### Features"
  printf '%s\n' "${features[@]}"
  echo ""
fi

if [ ${#improvements[@]} -gt 0 ]; then
  echo "### Improvements"
  printf '%s\n' "${improvements[@]}"
  echo ""
fi

if [ ${#fixes[@]} -gt 0 ]; then
  echo "### Fixes"
  printf '%s\n' "${fixes[@]}"
  echo ""
fi

if [ ${#features[@]} -eq 0 ] && [ ${#improvements[@]} -eq 0 ] && [ ${#fixes[@]} -eq 0 ]; then
  echo "No user-facing changes in this release."
  echo ""
fi

# Stats
COMMIT_COUNT=$(git rev-list --count "$RANGE" 2>/dev/null || echo "0")
if [ "$COMMIT_COUNT" = "1" ]; then
  echo "---"
  echo "*1 commit since ${SINCE_TAG:-initial}*"
else
  echo "---"
  echo "*${COMMIT_COUNT} commits since ${SINCE_TAG:-initial}*"
fi
