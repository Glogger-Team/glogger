#!/usr/bin/env bash
#
# Backfill GitHub Release objects for tags that have none on this repo.
# Notes come from CHANGELOG.md sections; no binaries are attached (historical
# notes only — the updater channel only needs the newest release).
# Safe: `gh release create` on an existing tag does not push anything, so the
# tag-push release workflow is NOT triggered.
#
# Usage: ./scripts/backfill-releases.sh [tags-file]
#   defaults to all v* tags with no Release on Glogger-Team/glogger.
set -euo pipefail

REPO="Glogger-Team/glogger"
TAGS_FILE="${1:-}"

if [ -z "$TAGS_FILE" ]; then
  git tag --list 'v[0-9]*' | sort -V > /tmp/all_tags.txt
  gh release list -R "$REPO" --limit 200 --json tagName --jq '.[].tagName' > /tmp/existing_tags.txt
  grep -vxFf /tmp/existing_tags.txt /tmp/all_tags.txt > /tmp/missing_tags.txt
  TAGS_FILE=/tmp/missing_tags.txt
fi

extract_section() {
  local tag="$1"
  awk -v hdr="## ${tag} " '
    index($0, hdr) == 1 { found=1; next }
    found && /^## / { exit }
    found { print }
  ' CHANGELOG.md
}

COUNT=0
FAILED=0
while IFS= read -r tag; do
  [ -z "$tag" ] && continue
  BODY="$(extract_section "$tag")"
  if [ -z "$BODY" ]; then
    echo "SKIP $tag: no CHANGELOG section"
    FAILED=$((FAILED+1))
    continue
  fi
  # Title mirrors the old repos: bare tag for v0.x, "glogger vX" for v1.x+
  if [[ "$tag" == v0.* ]]; then
    TITLE="$tag"
  else
    TITLE="glogger $tag"
  fi
  NOTES_FILE="$(mktemp)"
  printf '%s\n' "$BODY" > "$NOTES_FILE"
  cat >> "$NOTES_FILE" <<'EOF'

---
_Backfilled historical release — installer binaries for this version are not attached here._
EOF
  if gh release create "$tag" --title "$TITLE" --notes-file "$NOTES_FILE" -R "$REPO" >>/tmp/backfill.log 2>&1; then
    echo "OK $tag"
    COUNT=$((COUNT+1))
  else
    echo "FAIL $tag (see /tmp/backfill.log)"
    FAILED=$((FAILED+1))
  fi
  rm -f "$NOTES_FILE"
done < "$TAGS_FILE"

echo "Created: $COUNT, failed/skipped: $FAILED"
