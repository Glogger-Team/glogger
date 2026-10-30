"""Suggest the next release tag from commit history since a base tag.

Resolution order, first hit wins:
  1. argv[1], validated against ^v\\d+\\.\\d+\\.\\d+$
  2. latest published (non-draft, non-prerelease) GitHub release tag
     matching ^v\\d+\\.\\d+\\.\\d+$
  3. newest ancestor-of-HEAD lightweight/annotated tag matching
     ^v\\d+\\.\\d+\\.\\d+$ (max by semver tuple)

Bump rule over `git log --no-merges <base>..HEAD` subjects: major on a
`!:` conventional-commit marker or a BREAKING[ -]CHANGE body line; elif any
`feat:` subject -> minor; else patch. Empty ranges suggest base+patch with a
stderr warning.

Usage:
    python scripts/next-version.py [base-tag]

Stdout is exactly one `vA.B.C` line; human-readable reasons go to stderr.
Exit 1 when no base is resolvable.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

TAG_RE = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")
BREAKING_RE = re.compile(r"BREAKING[ -]CHANGE")
MAJOR_RE = re.compile(r"(feat|fix|impv|docs|test|build)(\([^)]*\))?!:")
MINOR_RE = re.compile(r"feat(\([^)]*\))?:")


def run_git(args, cwd=None):
    result = subprocess.run(
        ["git"] + args,
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    return result


def repo_from_origin(root):
    result = run_git(["remote", "get-url", "origin"], cwd=root)
    if result.returncode != 0:
        return None
    match = re.search(r"github\.com[:/]([^/]+/[^/]+?)(\.git)?$", result.stdout.strip())
    if not match:
        return None
    return match.group(1)


def published_base(root):
    repo = repo_from_origin(root)
    if not repo:
        return None
    try:
        result = subprocess.run(
            [
                "gh",
                "release",
                "list",
                "-R",
                repo,
                "--exclude-drafts",
                "--exclude-pre-releases",
                "--json",
                "tagName",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return None
    if result.returncode != 0:
        return None
    try:
        releases = json.loads(result.stdout or "[]")
    except json.JSONDecodeError:
        return None
    for entry in releases:
        tag = (entry.get("tagName") or "").strip()
        if TAG_RE.match(tag):
            return tag
    return None


def ancestor_base(root):
    result = run_git(["tag", "--list", "v*"], cwd=root)
    if result.returncode != 0:
        return None
    candidates = []
    for tag in result.stdout.split():
        tag = tag.strip()
        if not TAG_RE.match(tag):
            continue
        ancestor = run_git(["merge-base", "--is-ancestor", tag, "HEAD"], cwd=root)
        if ancestor.returncode == 0:
            candidates.append(tag)
    if not candidates:
        return None
    return max(candidates, key=lambda t: tuple(int(x) for x in t[1:].split(".")))


def bump_kind(root, base):
    result = run_git(
        ["log", "--no-merges", "-z", "--format=%B", base + "..HEAD"], cwd=root
    )
    if result.returncode != 0:
        print(f"Error: could not list commits since {base}", file=sys.stderr)
        return None
    records = [r for r in result.stdout.split("\0") if r.strip()]
    if not records:
        print(f"warning: no commits since {base}", file=sys.stderr)
        return "patch"
    for record in records:
        lines = record.splitlines()
        subject = lines[0] if lines else ""
        if MAJOR_RE.match(subject):
            return "major"
        if any(BREAKING_RE.match(line) for line in lines[1:]):
            return "major"
    for record in records:
        lines = record.splitlines()
        if lines and MINOR_RE.match(lines[0]):
            return "minor"
    return "patch"


def main():
    root = Path(__file__).resolve().parent.parent
    base = None
    if len(sys.argv) > 1:
        if not TAG_RE.match(sys.argv[1]):
            print(
                f"Error: base tag must match vX.Y.Z (e.g. v1.6.0), got '{sys.argv[1]}'",
                file=sys.stderr,
            )
            return 1
        base = sys.argv[1]
    if base is None:
        base = published_base(root)
    if base is None:
        base = ancestor_base(root)
    if base is None:
        print("Error: no base tag resolvable (pass one explicitly)", file=sys.stderr)
        return 1

    kind = bump_kind(root, base)
    if kind is None:
        return 1
    major, minor, patch = (int(x) for x in base[1:].split("."))
    if kind == "major":
        nxt = (major + 1, 0, 0)
    elif kind == "minor":
        nxt = (major, minor + 1, 0)
    else:
        nxt = (major, minor, patch + 1)
    print("v{}.{}.{}".format(*nxt))
    print("base {} + {} -> v{}.{}.{}".format(base, kind, *nxt), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
