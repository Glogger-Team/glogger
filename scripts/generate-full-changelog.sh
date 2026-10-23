#!/usr/bin/env bash
#
# Generate the full CHANGELOG.md from every v<semver> tag to HEAD.
# Ranges use scripts/generate-changelog.sh grouping (feat:/impv:/fix:);
# ranges with no prefixed commits (pre-convention history) fall back to a
# raw commit-subject list so old history is still recorded.
#
# Usage: ./scripts/generate-full-changelog.sh > CHANGELOG.md
set -euo pipefail

mapfile -t TAGS < <(git tag --list 'v[0-9]*' | sort -V)
LATEST=""
if [ ${#TAGS[@]} -gt 0 ]; then
  LATEST="${TAGS[-1]}"
fi

echo "# Changelog"
echo ""
echo "Full release history for glogger, generated from git tags."
echo "Per-release notes use the same grouping as scripts/generate-changelog.sh"
echo "(\`feat:\` → Features, \`impv:\` → Improvements, \`fix:\` → Fixes)."
echo "Releases that predate the commit-prefix convention list all commit subjects."
echo ""

# Unreleased commits since the newest tag, if any. With no tags yet the whole
# history is unreleased.
if [ -z "$LATEST" ]; then
  echo "## Unreleased"
  echo ""
  bash scripts/generate-changelog.sh "" HEAD | tail -n +3
  echo ""
elif [ "$(git rev-list --count "${LATEST}..HEAD" 2>/dev/null || echo 0)" -gt 0 ]; then
  echo "## Unreleased (since ${LATEST})"
  echo ""
  bash scripts/generate-changelog.sh "$LATEST" HEAD | tail -n +3
  echo ""
fi
# Newest release first. Tag i covers (TAGS[i-1]..TAGS[i]); the oldest tag
# covers everything reachable from it.
for (( i=${#TAGS[@]}-1; i>=0; i-- )); do
  TAG="${TAGS[$i]}"
  DATE="$(git log -1 --format=%ad --date=short "$TAG" 2>/dev/null || echo "")"
  echo "## ${TAG}${DATE:+ ($DATE)}"
  echo ""
  if [ "$i" -gt 0 ]; then
    SINCE="${TAGS[$((i-1))]}"
    RANGE="${SINCE}..${TAG}"
    NOTES="$(bash scripts/generate-changelog.sh "$SINCE" "$TAG" | tail -n +3)"
  else
    RANGE="${TAG}"
    NOTES="$(bash scripts/generate-changelog.sh "" "$TAG" | tail -n +3)"
  fi
  if echo "$NOTES" | grep -q "No user-facing changes"; then
    echo "_Pre-convention history — all commits in range:_"
    echo ""
    git log --format='- %s (`%h`)' --no-decorate "$RANGE" 2>/dev/null || true
    COUNT="$(git rev-list --count "$RANGE" 2>/dev/null || echo 0)"
    echo ""
    echo "---"
    echo "*${COUNT} commits in ${RANGE}*"
  else
    echo "$NOTES"
  fi
  echo ""
done
