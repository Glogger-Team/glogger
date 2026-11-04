"""Generate release notes from git commits since a base tag (normally the last
*published* release — see scripts/last-published-tag.py; basing notes on
the nearest tag orphans the commits of any tag whose Release run failed).
Outputs markdown to stdout (the release workflow generates it in CI and
publishes it on the GitHub Release and in latest.json).

Usage:
  python scripts/generate-changelog.py                 # since the nearest v* tag, to HEAD
  python scripts/generate-changelog.py v0.1.0          # since a specific tag
  python scripts/generate-changelog.py v1.0.2 v1.0.3^  # a historical range (until ref)

Commit prefixes are the source of truth for grouping:
  feat: -> Features, impv: -> Improvements, fix: -> Fixes.
docs:/test:/build:/chore:/release: and merges are intentionally excluded —
anything a user should read about belongs in a feat:/impv:/fix: commit.
Output is humanized for end users: the type prefix and `!` are stripped at
render, the scope (if any) is kept as a `Scope: …` label, sentence-cased,
with no hashes, PR refs, or commit counts.
"""

import re
import subprocess
import sys
from pathlib import Path

# Classification lives in scripts/release_notes.py (shared with
# next-version.py). Imported, not duplicated.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from release_notes import entries_for_record

SCOPED_RE = re.compile(r"^(feat|fix|impv)\(([^)]*)\)!?:\s*(.*)$")
BARE_RE = re.compile(r"^(feat|fix|impv)!?:\s*(.*)$")
PR_REF_RE = re.compile(r"\s*\(#[0-9]+\)\s*$")


def run_git(args, cwd=None):
    return subprocess.run(
        ["git"] + args,
        cwd=cwd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def humanize(text):
    """Render one conventional line for end users: strip the type prefix and
    any trailing ` (#NN)` PR ref, sentence-case the text, keep the scope (if
    any) as a `Scope: …` label. Anything without a type prefix passes through
    unchanged so classified lines are never dropped here."""
    m = SCOPED_RE.match(text)
    if m:
        scope = m.group(2)
        rest = m.group(3)
    else:
        m = BARE_RE.match(text)
        if not m:
            return text
        scope = ""
        rest = m.group(2)
    rest = PR_REF_RE.sub("", rest)
    rest = rest.strip(" \t\r\n\v\f")
    if not rest:
        return text
    # tr '[:lower:]' '[:upper:]' maps ASCII only; multibyte passes through.
    first = rest[0]
    if "a" <= first <= "z":
        first = chr(ord(first) - 32)
    rest = first + rest[1:]
    if scope:
        return f"{scope}: {rest}"
    return rest


def collect_range(since, until):
    if since:
        return f"{since}..{until}", f"## What's Changed since {since}", False
    described = run_git(
        ["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", until]
    )
    nearest = described.stdout.strip() if described.returncode == 0 else ""
    if not nearest:
        return until, "## What's New", True
    return f"{nearest}..{until}", f"## What's Changed since {nearest}", False


def eprint(msg):
    # Raw bytes: Windows text streams translate \n to \r\n, but gated
    # output must carry the same bare \n bytes the shell emits.
    sys.stderr.buffer.write((msg + "\n").encode("utf-8"))


FAIL_MSG = (
    "error: release_notes.py classification failed; "
    "refusing to publish incomplete notes"
)


def commit_records(range_ref):
    """Yield (subject, body_lines) newest-first, skipping blank subjects."""
    hashes = run_git(["log", "--no-merges", "--no-decorate", "--format=%H", range_ref])
    if hashes.returncode != 0:
        eprint(hashes.stderr.strip() or f"error: git log failed for range {range_ref}")
        sys.exit(1)
    for commit in hashes.stdout.splitlines():
        if not commit:
            continue
        shown = run_git(["show", "-s", "--format=%B", commit])
        if shown.returncode != 0:
            eprint(FAIL_MSG)
            sys.exit(1)
        lines = shown.stdout.splitlines()
        subject = lines[0] if lines else ""
        if not subject:
            continue
        try:
            entries = entries_for_record(subject, lines[1:])
        except Exception:  # noqa: BLE001 — any classifier failure must refuse output
            eprint(FAIL_MSG)
            sys.exit(1)
        yield entries


def sections_for(range_ref):
    features = []
    improvements = []
    fixes = []
    for entries in commit_records(range_ref):
        for section, text in entries:
            if section not in ("feat", "impv", "fix"):
                continue
            if not text:
                continue
            cleaned = humanize(text)
            if section == "feat":
                features.append(f"- {cleaned}")
            elif section == "impv":
                improvements.append(f"- {cleaned}")
            else:
                fixes.append(f"- {cleaned}")
    return features, improvements, fixes


def render(range_ref, header, initial):
    out = [header, ""]
    if initial:
        out += ["Initial release.", ""]
    features, improvements, fixes = sections_for(range_ref)
    if features:
        out += ["### Features", *features, ""]
    if improvements:
        out += ["### Improvements", *improvements, ""]
    if fixes:
        out += ["### Fixes", *fixes, ""]
    if not features and not improvements and not fixes:
        out += ["No user-facing changes in this release.", ""]
    return "\n".join(out) + "\n"


def main(argv):
    since = argv[1] if len(argv) > 1 else ""
    until = argv[2] if len(argv) > 2 else "HEAD"
    range_ref, header, initial = collect_range(since, until)
    sys.stdout.buffer.write(render(range_ref, header, initial).encode("utf-8"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
