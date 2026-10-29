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

# Portable in-place sed: macOS runners ship BSD sed, where bare `-i` fails
# and GNU-only addresses like `0,/.../` don't exist. A backup suffix works on
# both; the backup is removed right away.
sedi() {
  local file="$1"; shift
  sed -i.bak "$@" "$file"
  rm -f "${file}.bak"
}

# Read current version from tauri.conf.json (source of truth)
CURRENT=$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([0-9]*\.[0-9]*\.[0-9]*\)".*/\1/p' "$TAURI_CONF" | head -1)

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
sedi "$TAURI_CONF" "s/\"version\": \"$CURRENT\"/\"version\": \"$NEW_VERSION\"/"

# 2. Update tauri.conf.json — dev window title
sedi "$TAURI_CONF" "s/\"title\": \"glogger v$CURRENT DEV\"/\"title\": \"glogger v$NEW_VERSION DEV\"/"

# 3. Update tauri.release.conf.json — release window title
sedi "$TAURI_RELEASE_CONF" -E "s/\"title\": \"glogger v[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?\"/\"title\": \"glogger v$NEW_VERSION\"/"

# 3b. Update tauri.experimental.conf.json — experimental window title
sedi "$TAURI_EXPERIMENTAL_CONF" "s/\"title\": \"glogger v$CURRENT EXPERIMENTAL\"/\"title\": \"glogger v$NEW_VERSION EXPERIMENTAL\"/"

# 4. Update package.json
sedi "$PACKAGE_JSON" "s/\"version\": \"[0-9]*\.[0-9]*\.[0-9]*\"/\"version\": \"$NEW_VERSION\"/"

# 5. Update Cargo.toml (only the package version, not dependency versions).
# Portable first-occurrence replace: BSD sed has no `0,/.../` address, so split
# at the first match with awk and reassemble.
awk -v old="version = \"$CURRENT\"" -v new="version = \"$NEW_VERSION\"" '
  !done && index($0, old) { sub(old, new); done = 1 }
  { print }
' "$CARGO_TOML" > "${CARGO_TOML}.new" && mv "${CARGO_TOML}.new" "$CARGO_TOML"

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
