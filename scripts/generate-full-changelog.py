"""Generate the full CHANGELOG.md from every v<semver> tag to HEAD.
Ranges use scripts/generate-changelog.py grouping (feat:/impv:/fix:) with
humanized entries (type prefix stripped, scope kept as `Scope: …`, no hash,
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
    out += ["Full release history for glogger, generated from git tags."]
    out += ["Per-release notes use the same grouping as scripts/generate-changelog.py"]
    out += ["(feat: → Features, impv: → Improvements, fix: → Fixes) with humanized"]
    out += ["entries (no type prefix, hash, PR ref, or commit count)."]
    out += [
        "Releases that predate the commit-prefix convention list all commit subjects."
    ]
    out += [""]

    # Unreleased commits since the newest tag, if any. With no tags yet the
    # whole history is unreleased.
    if not latest:
        out += ["## Unreleased", ""]
        out += [changelog_tail("", "HEAD").rstrip("\n")]
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
    for i in range(len(tags) - 1, -1, -1):
        tag = tags[i]
        dated = run_git(["log", "-1", "--format=%ad", "--date=short", tag])
        date = dated.stdout.strip() if dated.returncode == 0 else ""
        out += [f"## {tag} ({date})" if date else f"## {tag}", ""]
        if i > 0:
            since = tags[i - 1]
            range_ref = f"{since}..{tag}"
            notes = changelog_notes(since, tag)
        else:
            range_ref = tag
            notes = changelog_notes("", tag)
        if "No user-facing changes" in notes:
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
