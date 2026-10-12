#!/usr/bin/env bash
# Publish a release: bump version, generate the release notes, commit,
# tag v<version>, push. CI (.github/workflows/release.yml) builds, signs and
# publishes the GitHub Release with those notes (and embeds them in the
# updater's latest.json).
#
# Usage: scripts/release.sh [patch|minor|major|x.y.z] [--notes <file>]
#
#   --notes <file>   use hand-written notes instead of the changelog
#                    generated from the commits since the previous release
#                    tag (scripts/generate-changelog.sh). The file is copied
#                    to RELEASE_NOTES.md at the repo root.
#
# Nothing is modified until the notes have been produced and accepted, so an
# aborting run leaves the tree exactly as it was.
set -euo pipefail
cd "$(dirname "$0")/.."

BUMP="${1:-patch}"
NOTES_FILE=""
if [ "${2:-}" = "--notes" ]; then
  NOTES_FILE="${3:-}"
  if [ -z "$NOTES_FILE" ] || [ ! -f "$NOTES_FILE" ]; then
    echo "Error: --notes needs a readable file path (got '${NOTES_FILE}')"; exit 1
  fi
fi

BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" != "main" ]; then
  echo "Error: must be on the 'main' branch (currently '$BRANCH')"; exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "Error: working tree not clean"; git status --short; exit 1
fi

# Previous release tag — the base of the "What's Changed since ..." notes.
PREV_TAG="$(git describe --tags --abbrev=0 --match 'v[0-9]*' HEAD 2>/dev/null || true)"

STAGED_NOTES="$(mktemp)"
trap 'rm -f "$STAGED_NOTES"' EXIT

if [ -n "$NOTES_FILE" ]; then
  cp "$NOTES_FILE" "$STAGED_NOTES"
  echo "Using hand-written release notes from $NOTES_FILE"
else
  scripts/generate-changelog.sh "$PREV_TAG" > "$STAGED_NOTES"
fi

echo "===== RELEASE_NOTES.md ====="
cat "$STAGED_NOTES"
echo "============================"

# The notes are the only chance users get to read what changed: refuse to
# silently ship generated notes that list nothing (a hand-written file is a
# deliberate choice — leave it alone).
if [ -z "$NOTES_FILE" ] && ! grep -q '^- ' "$STAGED_NOTES"; then
  echo ""
  echo "Warning: no feat:/impv:/fix: commits since ${PREV_TAG:-the start} —"
  echo "         these notes list no changes. Re-run with a hand-written file:"
  echo "           scripts/release.sh ${BUMP} --notes <file>"
  printf 'Continue anyway? [y/N] '
  read -r answer || answer=""
  case "$answer" in
    [yY]*) ;;
    *) echo "Aborted — nothing was changed."; exit 1 ;;
  esac
fi

scripts/bump-version.sh "$BUMP"
VERSION="$(sed -n 's/.*"version"\s*:\s*"\([0-9]*\.[0-9]*\.[0-9]*\)".*/\1/p' src-tauri/tauri.conf.json | head -1)"
TAG="v${VERSION}"

# Committed as RELEASE_NOTES.md so the tagged tree carries the exact text the
# workflow publishes.
cp "$STAGED_NOTES" RELEASE_NOTES.md

git add -A
git commit -m "release: ${TAG}"
git tag "$TAG"
git push origin main
git push origin "$TAG"
echo "Pushed ${TAG} — the Release workflow will build and publish it."
