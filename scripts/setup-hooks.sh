#!/usr/bin/env bash
#
# Install the project's pre-commit hooks (pre-commit + pre-push + commit-msg
# stages, defined in .pre-commit-config.yaml) via the mise-provided runner.
# Idempotent: safe to re-run. Skipped-locally work is never silent — CI runs
# the same mise tasks, and `git commit/push --no-verify` bypasses loudly.
#
# Usage: ./scripts/setup-hooks.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Clean up the pre-framework layout: .githooks/ no longer exists.
if [ "$(git -C "$ROOT" config --get core.hooksPath || true)" = ".githooks" ]; then
  git -C "$ROOT" config --unset core.hooksPath
  echo "Removed legacy core.hooksPath=.githooks."
fi

mise exec -- pre-commit install --hook-type pre-commit --hook-type pre-push --hook-type commit-msg
echo "pre-commit hooks installed."
