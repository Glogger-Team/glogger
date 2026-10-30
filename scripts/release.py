"""Cut a release: create and push an annotated v<semver> tag.

CI does the rest (in-workspace version sync, notes since last published
release, signed builds, GitHub Release, updater channel, Flatpak).

Usage:
    python scripts/release.py [--yes|-y] [vX.Y.Z]

With no version argument, suggests the next tag from commit history via
next-version.py and asks for confirmation unless --yes is given.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

TAG_RE = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")


def git(args, root):
    return subprocess.run(
        ["git"] + args, cwd=root, capture_output=True, text=True, check=False
    )


def main():
    parser = argparse.ArgumentParser(description="Cut a release tag and push it.")
    parser.add_argument("version", nargs="?")
    parser.add_argument("--yes", "-y", action="store_true")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent

    if args.version is not None:
        if not TAG_RE.match(args.version):
            print(f"Usage: {__file__} vX.Y.Z (e.g. {__file__} v1.6.0)")
            return 1
        tag = args.version
    else:
        script_dir = Path(__file__).resolve().parent
        suggestion = subprocess.run(
            [sys.executable, str(script_dir / "next-version.py")],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
        if suggestion.returncode != 0:
            sys.stderr.write(suggestion.stderr)
            print("Error: could not suggest a version - pass one explicitly")
            return 1
        tag = (
            suggestion.stdout.strip().splitlines()[0]
            if suggestion.stdout.strip()
            else ""
        )
        if not TAG_RE.match(tag):
            print("Error: could not suggest a version - pass one explicitly")
            return 1
        sys.stderr.write(suggestion.stderr)
        print(f"Suggesting {tag} ...")
        if not args.yes:
            answer = input(f"Tag and push {tag} from main? [y/N] ")
            if not re.match(r"^[Yy]$", answer):
                print("Aborted.")
                return 1

    branch = git(["rev-parse", "--abbrev-ref", "HEAD"], root)
    if branch.stdout.strip() != "main":
        print(
            f"Error: must be on the 'main' branch (currently '{branch.stdout.strip()}')"
        )
        return 1
    if git(["status", "--porcelain"], root).stdout.strip():
        print("Error: working tree not clean")
        print(git(["status", "--short"], root).stdout, end="")
        return 1
    git(["fetch", "origin"], root)
    if (
        git(["rev-parse", "HEAD"], root).stdout.strip()
        != git(["rev-parse", "origin/main"], root).stdout.strip()
    ):
        print("Error: main is not up to date with origin/main - pull/rebase first")
        return 1
    local_exists = git(["rev-parse", tag], root).returncode == 0
    remote_out = git(["ls-remote", "--tags", "origin", "refs/tags/" + tag], root)
    remote_exists = bool(remote_out.stdout.strip())
    if local_exists or remote_exists:
        print(f"Error: tag {tag} already exists (locally or on origin)")
        return 1

    tagged = subprocess.run(
        ["git", "tag", "-a", tag, "-m", "glogger " + tag], cwd=root, check=False
    )
    if tagged.returncode != 0:
        return 1
    pushed = subprocess.run(["git", "push", "origin", tag], cwd=root, check=False)
    if pushed.returncode != 0:
        print(f"Error: failed to push {tag} - the local tag is kept.")
        print(f"Verify with: git ls-remote --tags origin refs/tags/{tag}")
        print(f"Then retry with: git push origin {tag}")
        return 1
    print(
        f"Pushed {tag} - the Release workflow builds, publishes, and refreshes the channel."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
