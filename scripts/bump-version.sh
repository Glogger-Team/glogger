#!/usr/bin/env bash
#
# Bump the version across all project files.
#
# Usage:
#   ./scripts/bump-version.sh <new-version>
#   ./scripts/bump-version.sh patch|minor|major
#
# Examples:
#   ./scripts/bump-version.sh 0.2.0
#   ./scripts/bump-version.sh patch    # 0.1.2 -> 0.1.3
#   ./scripts/bump-version.sh minor    # 0.1.2 -> 0.2.0
#   ./scripts/bump-version.sh major    # 0.1.2 -> 1.0.0

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAURI_CONF="$ROOT/src-tauri/tauri.conf.json"
TAURI_RELEASE_CONF="$ROOT/src-tauri/tauri.release.conf.json"
TAURI_EXPERIMENTAL_CONF="$ROOT/src-tauri/tauri.experimental.conf.json"
PACKAGE_JSON="$ROOT/package.json"
CARGO_TOML="$ROOT/src-tauri/Cargo.toml"

# Read current version from tauri.conf.json (source of truth)
CURRENT=$(sed -n 's/.*"version"\s*:\s*"\([0-9]*\.[0-9]*\.[0-9]*\)".*/\1/p' "$TAURI_CONF" | head -1)

if [ -z "$CURRENT" ]; then
  echo "Error: Could not read current version from tauri.conf.json"
  exit 1
fi

echo "Current version: $CURRENT"

if [ $# -lt 1 ]; then
  echo "Usage: $0 <new-version | patch | minor | major>"
  exit 1
fi

INPUT="$1"

# Parse current version components
IFS='.' read -r CUR_MAJOR CUR_MINOR CUR_PATCH <<< "$CURRENT"

case "$INPUT" in
  patch)
    NEW_VERSION="$CUR_MAJOR.$CUR_MINOR.$((CUR_PATCH + 1))"
    ;;
  minor)
    NEW_VERSION="$CUR_MAJOR.$((CUR_MINOR + 1)).0"
    ;;
  major)
    NEW_VERSION="$((CUR_MAJOR + 1)).0.0"
    ;;
  [0-9]*)
    # Validate semver format
    if ! echo "$INPUT" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
      echo "Error: Version must be in semver format (e.g., 1.2.3)"
      exit 1
    fi
    NEW_VERSION="$INPUT"
    ;;
  *)
    echo "Error: Unknown argument '$INPUT'. Use patch, minor, major, or a semver string."
    exit 1
    ;;
esac

echo "Bumping to:      $NEW_VERSION"

# 1. Update tauri.conf.json — version field
sed -i "s/\"version\": \"$CURRENT\"/\"version\": \"$NEW_VERSION\"/" "$TAURI_CONF"

# 2. Update tauri.conf.json — dev window title
sed -i "s/\"title\": \"glogger v$CURRENT DEV\"/\"title\": \"glogger v$NEW_VERSION DEV\"/" "$TAURI_CONF"

# 3. Update tauri.release.conf.json — release window title
sed -i -E "s/\"title\": \"glogger v[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?\"/\"title\": \"glogger v$NEW_VERSION\"/" "$TAURI_RELEASE_CONF"

# 3b. Update tauri.experimental.conf.json — experimental window title
sed -i "s/\"title\": \"glogger v$CURRENT EXPERIMENTAL\"/\"title\": \"glogger v$NEW_VERSION EXPERIMENTAL\"/" "$TAURI_EXPERIMENTAL_CONF"

# 4. Update package.json
sed -i "s/\"version\": \"[0-9]*\.[0-9]*\.[0-9]*\"/\"version\": \"$NEW_VERSION\"/" "$PACKAGE_JSON"

# 5. Update Cargo.toml (only the package version, not dependency versions)
sed -i "0,/^version = \"[0-9]*\.[0-9]*\.[0-9]*\"/s//version = \"$NEW_VERSION\"/" "$CARGO_TOML"

# 6. Update Cargo.lock (by running cargo check, or sed as fallback)
if command -v cargo &> /dev/null; then
  echo "Updating Cargo.lock..."
  (cd "$ROOT/src-tauri" && cargo update -p glogger --precise "$NEW_VERSION" 2>/dev/null || true)
fi

echo ""
echo "Version bumped to $NEW_VERSION in:"
echo "  - src-tauri/tauri.conf.json"
echo "  - src-tauri/tauri.release.conf.json"
echo "  - src-tauri/tauri.experimental.conf.json"
echo "  - package.json"
echo "  - src-tauri/Cargo.toml"
echo ""
echo "Synced to $NEW_VERSION (CI release builds stop here — no commit; local use: commit as usual.)"
