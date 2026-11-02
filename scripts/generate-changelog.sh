#!/usr/bin/env bash
#
# Generate release notes from git commits since a base tag (normally the last
# *published* release — see scripts/last-published-tag.sh; basing notes on
# the nearest tag orphans the commits of any tag whose Release run failed).
# Outputs markdown to stdout (the release workflow generates it in CI and
# publishes it on the GitHub Release and in latest.json).
#
# Usage:
#   ./scripts/generate-changelog.sh                 # since the nearest v* tag, to HEAD
#   ./scripts/generate-changelog.sh v0.1.0          # since a specific tag
#   ./scripts/generate-changelog.sh v1.0.2 v1.0.3^  # a historical range (until ref)
#
# Commit prefixes are the source of truth for grouping:
#   feat: -> Features, impv: -> Improvements, fix: -> Fixes.
# docs:/test:/build:/chore:/release: and merges are intentionally excluded —
# anything a user should read about belongs in a feat:/impv:/fix: commit.
# Output is humanized for end users: the type prefix and `!` are stripped at
# render, the scope (if any) is kept as a `Scope: …` label, sentence-cased,
# with no hashes, PR refs, or commit counts.

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

# Render one conventional line for end users: strip the type prefix and any
# trailing ` (#NN)` PR ref, sentence-case the text, keep the scope (if any)
# as a `Scope: …` label. Anything without a type prefix passes through
# unchanged so classified lines are never dropped here.
humanize_entry() {
  local line="$1"
  local scope=""
  local rest=""
  if [[ "$line" =~ ^(feat|fix|impv)\(([^\)]*)\)!?:[[:space:]]*(.*)$ ]]; then
    scope="${BASH_REMATCH[2]}"
    rest="${BASH_REMATCH[3]}"
  elif [[ "$line" =~ ^(feat|fix|impv)!?:[[:space:]]*(.*)$ ]]; then
    rest="${BASH_REMATCH[2]}"
  else
    printf '%s' "$line"
    return
  fi
  rest="$(printf '%s' "$rest" | sed -E 's/[[:space:]]*\(#[0-9]+\)[[:space:]]*$//')"
  rest="$(printf '%s' "$rest" | sed -E 's/^[[:space:]]+//;s/[[:space:]]+$//')"
  if [ -z "$rest" ]; then
    printf '%s' "$line"
    return
  fi
  local first upper
  first="$(printf '%s' "$rest" | cut -c1)"
  upper="$(printf '%s' "$first" | tr '[:lower:]' '[:upper:]')"
  rest="${upper}$(printf '%s' "$rest" | cut -c2-)"
  if [ -n "$scope" ]; then
    printf '%s: %s' "$scope" "$rest"
  else
    printf '%s' "$rest"
  fi
}

# Classification lives in scripts/release_notes.py (shared with
# next-version.py). One Python invocation emits one JSON object per commit
# record (`{"bump": ..., "entries": [{"section", "text"}]}` — one entry per
# notes line; squash bodies yield one per `* type:` bullet). Bash keeps
# range resolution, humanize_entry rendering, and section grouping.
# Requires python3 with a JSON module: entries are parsed with a small
# python3 one-liner (no jq dependency). Falls back to no entries when
# python3 is unavailable — the range then reports no user-facing changes.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mapfile -t CLASSIFIED < <(git log --no-merges -z --format=%B "$RANGE" 2>/dev/null | python3 "$SCRIPT_DIR/release_notes.py" --batch 2>/dev/null)
# mapfile splits on newline: each JSON object is one line, aligned 1:1 with
# the NUL-separated records (empty records filtered on both sides). The
# render loop below uses --no-merges too: merge subjects (`Merge ...`)
# are excluded on both sides so the index never desyncs (merges carry no
# section by definition).
idx=0
while IFS= read -r line; do
  [ -z "$line" ] && continue
  entry_json="${CLASSIFIED[$idx]:-}"
  idx=$((idx + 1))
  [ -z "$entry_json" ] && continue
  # entries TSV: one `section\ttext` line per notes entry.
  while IFS=$'\t' read -r section text; do
    case "$section" in
      feat|impv|fix) ;;
      *) continue ;;
    esac
    [ -z "$text" ] && continue
    cleaned="$(humanize_entry "$text")"
    case "$section" in
      feat) features+=("- ${cleaned}") ;;
      impv) improvements+=("- ${cleaned}") ;;
      fix) fixes+=("- ${cleaned}") ;;
    esac
  done < <(python3 -c 'import json,sys; [print(e["section"]+"\t"+e["text"]) for e in json.loads(sys.argv[1]).get("entries",[])]' "$entry_json" 2>/dev/null)
done < <(git log --oneline --no-merges --no-decorate "$RANGE" 2>/dev/null)

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
