#!/usr/bin/env bash
#
# Print the newest v<semver> tag that has a published (non-draft,
# non-prerelease) GitHub Release. Used by scripts/release.sh and the release
# workflow's validate job so release notes cover everything since the last
# release users actually received — not just the previous tag. A tag whose
# Release run failed (v1.3.8: the tag exists but no GitHub Release was ever
# created) would otherwise orphan its commits from the next release's notes.
#
# Usage: scripts/last-published-tag.sh <owner/repo>
#
# Exits 1 (prints nothing to stdout) when the lookup is impossible — gh
# missing/unauthenticated, or no published releases yet. Callers fall back to
# `git describe` (nearest tag) or the initial-release path.
set -euo pipefail

REPO="${1:-}"
if [ -z "$REPO" ]; then
  echo "Usage: $0 <owner/repo>" >&2; exit 1
fi

command -v gh >/dev/null 2>&1 || { echo "last-published-tag: gh not installed" >&2; exit 1; }

# `gh release list` is newest-first. The updater-latest pointer release is
# neither draft nor prerelease, so filter to strict vX.Y.Z tags explicitly.
TAG="$(gh release list -R "$REPO" --exclude-drafts --exclude-pre-releases \
  --json tagName -q '[.[] | .tagName | select(test("^v[0-9]+\\.[0-9]+\\.[0-9]+$"))][0]' 2>/dev/null || true)"

if [ -z "$TAG" ] || [ "$TAG" = "null" ]; then
  echo "last-published-tag: no published v* release found for ${REPO} (or gh auth failed)" >&2
  exit 1
fi

printf '%s\n' "$TAG"
