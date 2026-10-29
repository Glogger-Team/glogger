#!/usr/bin/env bash
# Cut a release: create and push an annotated v<semver> tag. CI does the rest
# (in-workspace version sync, notes since last published release, signed
# builds, GitHub Release, updater channel, Flatpak).
#
# Usage: scripts/release.sh vX.Y.Z
set -euo pipefail
cd "$(dirname "$0")/.."
TAG="${1:-}"
case "$TAG" in
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "Usage: scripts/release.sh vX.Y.Z (e.g. scripts/release.sh v1.6.0)"; exit 1 ;;
esac
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
[ "$BRANCH" = "main" ] || { echo "Error: must be on the 'main' branch (currently '$BRANCH')"; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "Error: working tree not clean"; git status --short; exit 1; }
git fetch origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "Error: main is not up to date with origin/main — pull/rebase first"; exit 1; }
if git rev-parse "$TAG" >/dev/null 2>&1 || [ -n "$(git ls-remote --tags origin "refs/tags/$TAG" || true)" ]; then
  echo "Error: tag $TAG already exists (locally or on origin)"; exit 1
fi
git tag -a "$TAG" -m "glogger $TAG"
git push origin "$TAG"
echo "Pushed $TAG — the Release workflow builds, publishes, and refreshes the update channel."
