#!/usr/bin/env python3
"""Generate the full CHANGELOG.md from every v<semver> tag to HEAD.
Ranges use scripts/generate-changelog.py grouping (feat: → New, fix: →
Fixed, impv: → Improved) with one line per entry (no type prefix, hash,
PR ref, or commit count); ranges with no prefixed commits (pre-convention
history) fall back to a raw commit-subject list so old history is still
recorded.

Usage: python scripts/generate-full-changelog.py > CHANGELOG.md
"""

import importlib.util
import re
import subprocess
import sys
from pathlib import Path

# scripts/generate-changelog.py has a hyphenated name, so load it by path.
# Imported, not shelled out to.
_CHANGELOG_PATH = Path(__file__).resolve().parent / "generate-changelog.py"
_spec = importlib.util.spec_from_file_location("generate_changelog", _CHANGELOG_PATH)
generate_changelog = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(generate_changelog)
TAG_NUM_RE = re.compile(r"^v(\d+(?:\.\d+)*)(.*)$")

# Hand-polished player notes for the v1.5.0–v1.12.0 backfill (per-release
# signoff). Takes precedence over generated output so regeneration keeps
# the approved wording. Keys are exact tag names.
CURATED_NOTES = {
    "v1.5.0": """### New
- More accurate deaths, treasure, and inventory counts.
- Notices when others milk you are now tracked.
- Vendor view grouped into categories with favorites.

### Improved
- Milking history matches across views.

### Fixed
- Live data resumes updating after pauses.
- Switching characters clears old data reliably.""",
    "v1.5.1": """### New
- Skill level-ups from chat are now tracked.
- VIP reports import automatically.

### Fixed
- Level-up XP counts correctly in combat rate.
- Milking notices track again after chat fix.
- Version check works more reliably.""",
    "v1.5.2": """### New
- Past incoming milkings now appear in ability history.
- Skills shows Prodigy sessions and level progress.

### Improved
- Survey loot is counted once and correctly.
- Log imports run faster and more reliably.
- Screens refresh in the right order.

### Fixed
- Roulette, self-milking times, and scans are steadier.
- Item stacks, search, and shortcuts work better.
- Currency, arena, and hidden tabs update correctly.
- Status respects time zone; inactive trackers stay hidden.
- Survey rewards attach to the right items.""",
    "v1.6.0": """### New
- Cross-character work orders.
- Survey Retry after load errors.

### Improved
- Clearer storage and exports.

### Fixed
- Surveys and work orders reload correctly.
- Items reload when switching servers with All Characters.
- Loading states no longer stick.""",
    "v1.7.0": """### Improved
- No visible changes — stability and maintenance update.""",
    "v1.8.0": """### Improved
- NPC list loads faster and stays current.
- Removed the Inventory Live tab.

### Fixed
- Character picker stays open when adding.
- Wealth no longer shows ??? entries.""",
    "v1.9.0": """### Improved
- Merged XP view is now called XP Rate.
- Tidier XP, stall prices, and gift views.

### Fixed
- Weather matches the right area.
- Status shows live weather; outdated badges removed.""",
    "v1.9.1": """### Fixed
- Recipes with no progress still show as known.""",
    "v1.10.0": """### Improved
- Inventory items load faster.
- Portraits close with Esc and size better.""",
    "v1.11.0": """### Improved
- Stall stock stays accurate with less game data.""",
    "v1.12.0": """### Improved
- Clearer names across the app; unused views removed.
- StallTracker recounts correctly after restocking.
- Clear unsure items instead of sold-out list.""",
}


def run_git(args, cwd=None):
    return subprocess.run(
        ["git"] + args,
        cwd=cwd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def version_key(tag):
    """Mirror `sort -V` for vX.Y.Z[-suffix] tags: numeric components first,
    bare tags before suffixed ones on the same numbers."""
    m = TAG_NUM_RE.match(tag)
    if not m:
        return ((), 1, tag)
    nums = tuple(int(p) for p in m.group(1).split("."))
    suffix = m.group(2)
    return (nums, 0 if not suffix else 1, suffix)


def changelog_tail(since, until):
    """Step-5 output minus its first two lines (the `tail -n +3` slice),
    trailing blank line preserved — the Unreleased path pipes, it does not
    use command substitution, so no trailing-newline stripping applies."""
    range_ref, header, initial = generate_changelog.collect_range(since, until)
    rendered = generate_changelog.render(range_ref, header, initial)
    return "\n".join(rendered.split("\n")[2:])


def changelog_notes(since, until):
    """Per-tag notes: the same slice with command-substitution semantics
    (trailing newlines stripped before `echo "$NOTES"` re-adds one)."""
    return changelog_tail(since, until).strip("\n")


def main(argv):
    del argv
    tags_out = run_git(["tag", "--list", "v[0-9]*"])
    tags = (
        sorted(tags_out.stdout.splitlines(), key=version_key)
        if tags_out.returncode == 0
        else []
    )
    latest = tags[-1] if tags else ""

    out = []
    out += ["# Changelog", ""]
    out += ["Per-release notes use the same grouping as scripts/generate-changelog.py"]
    out += ["(feat: → New, fix: → Fixed, impv: → Improved) with one line per entry"]
    out += ["(no type prefix, hash, PR ref, or commit count)."]
    out += [
        "Releases that predate the commit-prefix convention list all commit subjects."
    ]
    out += [""]

    # Unreleased commits since the newest tag, if any. With no tags yet the
    # whole history is unreleased.
    if not latest:
        out += ["## Unreleased", ""]
        out += [changelog_tail("", "HEAD").rstrip("\n")]
        # Piped (no command substitution): the tail's trailing blank line
        # survives, then the explicit `echo ""` adds one more.
        out += ["", ""]
    else:
        counted = run_git(["rev-list", "--count", f"{latest}..HEAD"])
        count = counted.stdout.strip() if counted.returncode == 0 else "0"
        try:
            pending = int(count)
        except ValueError:
            pending = 0
        if pending > 0:
            # Piped (no command substitution): the tail's trailing blank line
            # survives, then the explicit echo adds one more.
            out += [f"## Unreleased (since {latest})", ""]
            out += [changelog_tail(latest, "HEAD").rstrip("\n")]
            out += ["", ""]

    # Newest release first. Tag i covers (TAGS[i-1]..TAGS[i]); the oldest tag
    # covers everything reachable from it.
    # The commit-prefix convention starts at v1.3.0 (earliest CURATED_NOTES
    # tag); only ranges entirely below it take the raw-subject fallback.
    # Convention-era ranges with no visible entries keep the stability line so
    # filtered tooling subjects never leak into player notes.
    first_conventional = min(CURATED_NOTES)
    for i in range(len(tags) - 1, -1, -1):
        tag = tags[i]
        dated = run_git(["log", "-1", "--format=%ad", "--date=short", tag])
        date = dated.stdout.strip() if dated.returncode == 0 else ""
        out += [f"## {tag} ({date})" if date else f"## {tag}", ""]
        if i > 0:
            since = tags[i - 1]
            range_ref = f"{since}..{tag}"
            notes = CURATED_NOTES.get(tag) or changelog_notes(since, tag)
        else:
            range_ref = tag
            notes = CURATED_NOTES.get(tag) or changelog_notes("", tag)
        if tag < first_conventional and "No visible changes" in notes:
            out += ["_Pre-convention history — all commits in range:_", ""]
            listed = run_git(
                ["log", "--format=- %s (`%h`)", "--no-decorate", range_ref]
            )
            if listed.returncode == 0 and listed.stdout.strip("\n"):
                out += [listed.stdout.strip("\n")]
        else:
            out += [notes]
        out += [""]

    sys.stdout.buffer.write(("\n".join(out) + "\n").encode("utf-8"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
