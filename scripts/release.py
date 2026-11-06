"""Cut a release: triage notes, commit them, push an annotated v<semver> tag.

CI does the rest (in-workspace version sync, signed builds, GitHub Release,
updater channel, Flatpak).

Usage:
    python scripts/release.py [--yes|-y] [--dry-run] [vX.Y.Z]

With no version argument, suggests the next tag from commit history via
next-version.py and asks for confirmation unless --yes is given.
--dry-run renders the triage draft and tags nothing. The triage pass opens
$EDITOR with one draft line per in-range feat/fix/impv commit (trailer
verbatim when present, else a humanized guess); --yes skips the editor and
approves drafts as-is.
"""

import argparse
import re
import shlex
import subprocess
import sys
from pathlib import Path

TAG_RE = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")

TRIAGE_PREFIX = (
    "# Lines starting with # are comments (commit subjects).\n"
    "# Edit the draft lines, delete a line to drop it, save and exit.\n"
    "# Empty lines and # lines are ignored.\n\n"
)


def triage_draft_rows(range_ref, script_dir):
    """One triage row per in-range commit: import by path, not subprocess."""
    import importlib.util

    path = script_dir / "generate-changelog.py"
    spec = importlib.util.spec_from_file_location("gen_changelog", path)
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    return gen.draft_items(range_ref)


def git(args, root):
    return subprocess.run(
        ["git"] + args, cwd=root, capture_output=True, text=True, check=False
    )


def main():
    parser = argparse.ArgumentParser(description="Cut a release tag and push it.")
    parser.add_argument("version", nargs="?")
    parser.add_argument("--yes", "-y", action="store_true")
    parser.add_argument(
        "--dry-run", action="store_true", help="render triage draft, tag nothing"
    )
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
        if args.dry_run:
            pass  # triage block below renders and exits before any prompt
        elif not args.yes:
            answer = input(f"Tag and push {tag} from main? [y/N] ")
            if not re.match(r"^[Yy]$", answer):
                print("Aborted.")
                return 1

    script_dir = Path(__file__).resolve().parent

    branch = git(["rev-parse", "--abbrev-ref", "HEAD"], root)
    if branch.stdout.strip() != "main" and not args.dry_run:
        print(
            f"Error: must be on the 'main' branch (currently '{branch.stdout.strip()}')"
        )
        return 1
    if branch.stdout.strip() != "main":
        print(
            f"Warning: not on main (currently '{branch.stdout.strip()}') - "
            "tag suggestion reads main's history, preview only"
        )
    if not args.dry_run:
        if git(["status", "--porcelain"], root).stdout.strip():
            print("Error: working tree not clean")
            print(git(["status", "--short"], root).stdout, end="")
            return 1
        fetch = git(["fetch", "origin"], root)
        if fetch.returncode != 0:
            print("Error: could not fetch origin - refusing to tag on stale refs.")
            print((fetch.stderr or "").strip()[:200])
            return 1
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

    # Gate hardening (vue lesson): warn when latest main CI run is red.
    gh = subprocess.run(
        [
            "gh",
            "run",
            "list",
            "--branch",
            "main",
            "--limit",
            "1",
            "--json",
            "conclusion",
            "--jq",
            ".[0].conclusion",
        ],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if gh.returncode == 0 and gh.stdout.strip():
        if gh.stdout.strip() not in ("success", "skipped"):
            print(f"Warning: latest main CI run is {gh.stdout.strip()} (not blocking)")
    else:
        print("Warning: could not check CI status (gh unavailable?)")

    # Release-time triage pass: draft every in-range line, edit in $EDITOR,
    # approved lines land in release-notes/<tag>.md for the generator.
    prev_proc = subprocess.run(
        [
            sys.executable,
            str(script_dir / "last-published-tag.py"),
            "Glogger-Team/glogger",
        ],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if prev_proc.returncode != 0:
        sys.stderr.write((prev_proc.stderr or "").strip() + "\n")
        print(
            "Error: could not determine last published tag - refusing to guess the range."
        )
        return 1
    prev_tag = prev_proc.stdout.strip().splitlines()
    prev_tag = prev_tag[0] if prev_tag else ""
    range_ref = f"{prev_tag}..HEAD" if prev_tag else "HEAD"

    def render_grouped(pairs, header):
        order = [("feat", "### New"), ("fix", "### Fixed"), ("impv", "### Improved")]
        out = [header.rstrip("\n"), ""]
        for key, title in order:
            lines = [text for section, text in pairs if section == key]
            if lines:
                out += [title] + [f"- {t}" for t in lines] + [""]
        return "\n".join(out) + "\n"

    rows = triage_draft_rows(range_ref, script_dir)
    if args.dry_run:
        for r in rows:
            print(f"[{r['section']}] {r['draft']}")
        return 0
    if rows and not args.yes:
        import os
        import tempfile

        with tempfile.NamedTemporaryFile(
            "w", suffix=".txt", delete=False, encoding="utf-8", newline="\n"
        ) as f:
            f.write(TRIAGE_PREFIX)
            f.write("# Format: [n] draft text — keep the [n] tag, edit the text.\n")
            f.write("# Delete a line to drop it. Lines without a tag go to Improved.\n")
            for n, r in enumerate(rows):
                marker = " (trailer)" if r["has_trailer"] else " (draft)"
                f.write(f"# [{r['section']}]{marker} {r['subject']}\n")
                f.write(f"[{n}] {r['draft']}\n\n")
            triage_path = f.name
        editor = shlex.split(
            os.environ.get("EDITOR", "notepad" if os.name == "nt" else "vi")
        )
        if not editor:
            editor = ["notepad" if os.name == "nt" else "vi"]
        try:
            edited = subprocess.run([*editor, triage_path], check=False)
            if edited.returncode != 0:
                print(
                    f"Error: editor exited {edited.returncode} - notes not saved, tag not created."
                )
                return 1
            with open(triage_path, encoding="utf-8", errors="replace") as fh:
                approved = [
                    ln.strip()
                    for ln in fh
                    if ln.strip() and not ln.strip().startswith("#")
                ]
            import re as _re

            tag_re = _re.compile(r"^\[(\d+)\]\s*(.*)$")
            pairs = []
            for ln in approved:
                m = tag_re.match(ln)
                if m and int(m.group(1)) < len(rows) and m.group(2).strip():
                    pairs.append((rows[int(m.group(1))]["section"], m.group(2)))
                elif not m or m.group(2).strip():
                    pairs.append(("impv", tag_re.sub(r"\2", ln)))
                # else: [n] with deleted text — dropped, not an empty bullet.
        finally:
            os.unlink(triage_path)
        notes_dir = root / "release-notes"
        notes_dir.mkdir(exist_ok=True)
        notes_file = notes_dir / f"{tag}.md"
        header = f"## What's Changed since {prev_tag}"
        body = (
            render_grouped(pairs, header)
            if approved
            else header
            + "\n\n### Improved\n- No visible changes — stability and maintenance update.\n"
        )
        notes_file.write_text(body, encoding="utf-8", newline="\n")
        print(f"Wrote {notes_file} ({len(approved)} lines)")
        git(["add", str(notes_file.relative_to(root))], root)
        if not git(
            ["status", "--porcelain", "--", str(notes_file)], root
        ).stdout.strip():
            print(f"Notes unchanged ({notes_file}); skipping commit")
        else:
            noted = subprocess.run(
                ["git", "commit", "-m", f"release: notes for {tag}", "--no-verify"],
                cwd=root,
                check=False,
            )
            if noted.returncode != 0:
                print("Error: could not commit triage notes")
                return 1
    elif rows:
        # --yes: non-interactive automation path. Approve drafts as-is so CI
        # uses the triaged file instead of silently falling back.
        print("Warning: --yes skips the triage editor; approving drafts as-is")
        notes_dir = root / "release-notes"
        notes_dir.mkdir(exist_ok=True)
        notes_file = notes_dir / f"{tag}.md"
        body = render_grouped(
            [(r["section"], r["draft"]) for r in rows],
            f"## What's Changed since {prev_tag}",
        )
        notes_file.write_text(body, encoding="utf-8", newline="\n")
        git(["add", str(notes_file.relative_to(root))], root)
        if not git(
            ["status", "--porcelain", "--", str(notes_file)], root
        ).stdout.strip():
            print(f"Notes unchanged ({notes_file}); skipping commit")
        else:
            noted = subprocess.run(
                ["git", "commit", "-m", f"release: notes for {tag}", "--no-verify"],
                cwd=root,
                check=False,
            )
            if noted.returncode != 0:
                print("Error: could not commit triage notes")
                return 1

    branch_push = subprocess.run(
        ["git", "push", "origin", "main"], cwd=root, check=False
    )
    if branch_push.returncode != 0:
        # Direct push failed — typically branch protection ("Protect main"
        # ruleset). Route the notes commit through a branch + PR instead of
        # dead-ending: the tree was clean at entry and the sync gate held, so
        # the notes commit is the only commit ahead of origin/main.
        notes_branch = f"release/{tag}-notes"
        if (
            git(
                ["rev-parse", "--verify", f"refs/heads/{notes_branch}"], root
            ).returncode
            == 0
        ):
            print(f"Error: failed to push main, and {notes_branch} already exists.")
            print("Delete it or push it manually, then retry: git push origin main")
            return 1
        if git(["checkout", "-b", notes_branch], root).returncode != 0:
            print("Error: failed to push main - tag not created.")
            print("Then retry with: git push origin main")
            return 1
        if (
            subprocess.run(
                ["git", "push", "-u", "origin", notes_branch], cwd=root, check=False
            ).returncode
            != 0
        ):
            print(f"Error: pushed neither main nor {notes_branch} - tag not created.")
            print(f"You are on {notes_branch}; push it manually, then retry.")
            return 1
        pr = subprocess.run(
            [
                "gh",
                "pr",
                "create",
                "--title",
                f"release: notes for {tag}",
                "--body",
                f"Release notes for {tag}. Merge, then re-run `python scripts/release.py {tag}` to tag.",
            ],
            cwd=root,
            check=False,
        )
        if pr.returncode != 0:
            print(f"Pushed {notes_branch} - open a PR manually, merge it, then re-run:")
            print(f"  gh pr create --title 'release: notes for {tag}'")
        else:
            print(f"Pushed {notes_branch} - merge the PR, then re-run to tag:")
        print(f"  python scripts/release.py {tag}")
        git(["checkout", "main"], root)
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
