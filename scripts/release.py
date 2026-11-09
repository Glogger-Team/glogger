"""Cut a release: triage notes, commit them, push an annotated v<semver> tag.

CI does the rest (in-workspace version sync, signed builds, GitHub Release,
updater channel, Flatpak).

Usage:
    python scripts/release.py [--yes|-y] [--dry-run] [vX.Y.Z]
    python scripts/release.py vX.Y.Z --notes-file PATH   # agent triage path
    python scripts/release.py vX.Y.Z --tag-only          # post-merge re-run

With no version argument, suggests the next tag from commit history via
next-version.py and asks for confirmation unless --yes is given.
--dry-run renders the triage draft and tags nothing. The triage pass opens
$EDITOR with one draft line per in-range feat/fix/impv commit (trailer
verbatim when present, else a humanized guess); --yes skips the editor and
approves drafts as-is.

Agent recipe (see AGENTS.md release row):
    python scripts/release.py --dry-run                 # rows to humanize
    # write /tmp/notes.txt (STE-flavored, ste-lint 0 hard), then:
    python scripts/release.py vX.Y.Z --notes-file /tmp/notes.txt
    # merge the notes PR, then:
    python scripts/release.py vX.Y.Z --tag-only
"""

import argparse
import re
import shlex
import shutil
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


def parse_approved_lines(text, rows):
    """Parse editor-format triage text into [(section, text)] pairs.

    Same format $EDITOR produces: `[n] edited text` keeps the draft's
    section, bare lines go to Improved, `#`/blank lines ignored, and a
    `[n]` with deleted text drops the row.
    """
    import re as _re

    tag_re = _re.compile(r"^\[(\d+)\]\s*(.*)$")
    pairs = []
    for ln in text.splitlines():
        stripped = ln.strip()
        if not stripped or stripped.startswith("#"):
            continue
        m = tag_re.match(stripped)
        if m and int(m.group(1)) < len(rows) and m.group(2).strip():
            pairs.append((rows[int(m.group(1))]["section"], m.group(2)))
        elif not m or m.group(2).strip():
            pairs.append(("impv", tag_re.sub(r"\2", stripped)))
        # else: [n] with deleted text — dropped, not an empty bullet.
    return pairs


def write_notes_file(root, tag, prev_tag, pairs):
    """Write release-notes/<tag>.md from approved pairs; commit it.

    Returns True when a new commit landed, False when the file was
    already current (idempotent re-run).
    """
    order = [("feat", "### New"), ("fix", "### Fixed"), ("impv", "### Improved")]
    header = f"## What's Changed since {prev_tag}"
    out = [header.rstrip("\n"), ""]
    for key, title in order:
        lines = [text for section, text in pairs if section == key]
        if lines:
            out += [title] + [f"- {t}" for t in lines] + [""]
    body = "\n".join(out) + "\n"
    if not pairs:
        body = (
            header
            + "\n\n### Improved\n- No visible changes — stability and maintenance update.\n"
        )
    notes_dir = root / "release-notes"
    notes_dir.mkdir(exist_ok=True)
    notes_file = notes_dir / f"{tag}.md"
    notes_file.write_text(body, encoding="utf-8", newline="\n")
    git(["add", str(notes_file.relative_to(root))], root)
    if not git(["status", "--porcelain", "--", str(notes_file)], root).stdout.strip():
        print(f"Notes unchanged ({notes_file}); skipping commit")
        return False
    noted = subprocess.run(
        ["git", "commit", "-m", f"release: notes for {tag}", "--no-verify"],
        cwd=root,
        check=False,
    )
    if noted.returncode != 0:
        print("Error: could not commit triage notes")
        return None
    return True


def cut_tag(root, tag):
    """Create + push the annotated tag. Returns 0 on success."""
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


def push_notes_or_pr(root, tag):
    """Push main, else route the notes commit through a PR branch.

    Returns 0 when main (with the notes commit) is on origin, 1 when a
    PR was opened instead (caller must stop; tag comes after the merge).
    """
    if (
        git(["rev-parse", "HEAD"], root).stdout.strip()
        == git(["rev-parse", "origin/main"], root).stdout.strip()
    ):
        print("Notes already current - proceeding straight to tag.")
        return 0
    branch_push = None
    if main_protected(root):
        # Protected main (the normal case here): never attempt the doomed
        # push. Fall through to try-push only when the ruleset check is
        # itself inconclusive (no gh / offline), preserving old behavior.
        print("main is protected - routing notes through a PR, skipping direct push.")
    else:
        branch_push = subprocess.run(
            ["git", "push", "origin", "main"], cwd=root, check=False
        )
    if branch_push is not None and branch_push.returncode == 0:
        return 0
    # Reached either by a failed direct push or by the pre-check above
    # (protected: push never attempted). The tree was clean at entry
    # and the sync gate held, so the notes commit is the only commit
    # ahead of origin/main.
    notes_branch = f"release/{tag}-notes"
    if (
        git(["rev-parse", "--verify", f"refs/heads/{notes_branch}"], root).returncode
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
            f"Release notes for {tag}. Merge, then re-run `python scripts/release.py {tag} --tag-only` to tag.",
        ],
        cwd=root,
        check=False,
    )
    if pr.returncode != 0:
        print(f"Pushed {notes_branch} - open a PR manually, merge it, then re-run:")
        print(f"  gh pr create --title 'release: notes for {tag}'")
    else:
        print(f"Pushed {notes_branch} - merge the PR, then re-run to tag:")
    print(f"  python scripts/release.py {tag} --tag-only")
    git(["checkout", "main"], root)
    return 1


def main_protected(root):
    """True when origin's default branch has a PR-required rule.

    Best-effort: any failure (no gh, no network, no permission) returns
    False and the caller falls back to try-push-then-route. Keeps the
    offline/air-gapped path working.

    Note: the rulesets *list* endpoint omits conditions/rules, so each
    candidate is re-fetched by id. Targets ~DEFAULT_BRANCH, default
    branch name, or main.
    """
    try:
        import json as _json

        repo_proc = subprocess.run(
            ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
            timeout=15,
        )
        slug = (repo_proc.stdout or "").strip()
        if repo_proc.returncode != 0 or not slug:
            return False
        branch_proc = subprocess.run(
            ["git", "symbolic-ref", "refs/remotes/origin/HEAD"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
            timeout=15,
        )
        ref = (branch_proc.stdout or "").strip()
        # refs/remotes/origin/<branch>: strip the remote prefix, keep the
        # full branch name (feature/x stays feature/x, not x).
        default_branch = ref.split("refs/remotes/origin/", 1)[-1] or "main"
        out = subprocess.run(
            ["gh", "api", f"repos/{slug}/rulesets?includes_parents=true"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
            timeout=15,
        )
        if out.returncode != 0 or not out.stdout.strip():
            return False
        for summary in _json.loads(out.stdout):
            if not summary.get("enforcement", "").startswith("active"):
                continue
            rid = summary.get("id")
            if rid is None:
                continue
            det = subprocess.run(
                ["gh", "api", f"repos/{slug}/rulesets/{rid}"],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
                timeout=15,
            )
            if det.returncode != 0 or not det.stdout.strip():
                continue
            rs = _json.loads(det.stdout)
            refs = rs.get("conditions", {}).get("ref_name", {}).get("include", [])
            # Exact ref forms only: substring matching ("main" in
            # "refs/heads/maintenance") false-positives a PR detour.
            # ~DEFAULT_BRANCH and ~ALL cover the default branch and all
            # refs respectively; otherwise match the concrete ref.
            want = {f"refs/heads/{default_branch}", default_branch}
            hits_default = any(
                (c in ("~DEFAULT_BRANCH", "~ALL")) or (c in want) for c in refs
            )
            if not hits_default:
                continue
            rules = rs.get("rules", [])
            if any(r.get("type") == "pull_request" for r in rules):
                return True
        return False
    except (OSError, ValueError, KeyError, AttributeError, subprocess.TimeoutExpired):
        # OSError: gh/git spawn failure. TimeoutExpired: hung gh (DNS
        # stall) past timeout=15. ValueError/KeyError/AttributeError:
        # unexpected ruleset JSON shape. Anything else propagates - a
        # best-effort probe must not swallow real bugs.
        return False


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description="Cut a release tag and push it.")
    parser.add_argument("version", nargs="?")
    parser.add_argument("--yes", "-y", action="store_true")
    parser.add_argument(
        "--dry-run", action="store_true", help="render triage draft, tag nothing"
    )
    parser.add_argument(
        "--notes-file",
        metavar="PATH",
        default=None,
        help="read approved triage lines from PATH instead of opening $EDITOR "
        "(agent path: write the file programmatically, same format the editor "
        "produces). Mutually exclusive with --yes.",
    )
    parser.add_argument(
        "--tag-only",
        action="store_true",
        help="skip triage; require committed release-notes/<tag>.md and cut "
        "the tag (post-merge re-run path).",
    )
    return parser.parse_args(argv)


def resolve_tag(args, root):
    """Return the version tag, or None with an error already printed."""
    if args.version is not None:
        if not TAG_RE.match(args.version):
            print(f"Usage: {__file__} vX.Y.Z (e.g. {__file__} v1.6.0)")
            return None
        return args.version
    if args.notes_file or args.tag_only:
        print("Error: --notes-file/--tag-only require an explicit version")
        return None
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
        return None
    tag = suggestion.stdout.strip().splitlines()[0] if suggestion.stdout.strip() else ""
    if not TAG_RE.match(tag):
        print("Error: could not suggest a version - pass one explicitly")
        return None
    sys.stderr.write(suggestion.stderr)
    print(f"Suggesting {tag} ...")
    if not args.dry_run and not args.yes:
        answer = input(f"Tag and push {tag} from main? [y/N] ")
        if not re.match(r"^[Yy]$", answer):
            print("Aborted.")
            return None
    return tag


def entry_gates(args, root, tag):
    """Branch, tree, sync, and tag-existence gates. Returns 0 to proceed."""
    branch = git(["rev-parse", "--abbrev-ref", "HEAD"], root)
    if branch.stdout.strip() != "main" and not args.dry_run:
        print(
            f"Error: must be on the 'main' branch (currently '{branch.stdout.strip()}')"
        )
        return 1
    branch_name = branch.stdout.strip()
    if branch_name != "main":
        print(
            f"Warning: not on main (currently '{branch_name}') - "
            "tag suggestion reads main's history, preview only"
        )
    if args.dry_run:
        return 0
    if git(["status", "--porcelain"], root).stdout.strip():
        print("Error: working tree not clean")
        print(git(["status", "--short"], root).stdout, end="")
        return 1
    fetch = git(["fetch", "origin"], root)
    if fetch.returncode != 0:
        print("Error: could not fetch origin - refusing to tag on stale refs.")
        print((fetch.stderr or "").strip()[:200])
        return 1
    head = git(["rev-parse", "HEAD"], root).stdout.strip()
    origin_head = git(["rev-parse", "origin/main"], root).stdout.strip()
    if head != origin_head:
        # A merged notes PR leaves the same file content under a different
        # hash (squash-merge): resetting there is safe and the script says
        # so instead of failing on a merge no --tag-only run should create.
        notes_path = f"release-notes/{tag}.md"
        same_notes = (
            git(["diff", "HEAD", "origin/main", "--", notes_path], root).stdout.strip()
            == ""
            and git(["show", f"origin/main:{notes_path}"], root).returncode == 0
        )
        if same_notes:
            print(
                f"Error: notes for {tag} already merged via PR "
                f"(same {notes_path} content, different commit)."
            )
            print("Run: git reset --hard origin/main")
            print(f"Then: python scripts/release.py {tag} --tag-only")
            return 1
        print("Error: main is not up to date with origin/main - pull/rebase first")
        return 1
    local_exists = git(["rev-parse", tag], root).returncode == 0
    remote_out = git(["ls-remote", "--tags", "origin", "refs/tags/" + tag], root)
    remote_exists = bool(remote_out.stdout.strip())
    if local_exists or remote_exists:
        print(f"Error: tag {tag} already exists (locally or on origin)")
        return 1
    return 0


def warn_ci_status(root):
    """Warn when the latest main CI run is red (vue lesson, non-blocking)."""
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


def prevailing_tag(script_dir, root):
    """Return the last published tag, or None with an error printed."""
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
        return None
    prev_tag = prev_proc.stdout.strip().splitlines()
    return prev_tag[0] if prev_tag else ""


def triage_editor_path(rows, root, tag):
    """Classic human path: write the triage temp file, open $EDITOR."""
    import os
    import tempfile

    notes_file = root / "release-notes" / f"{tag}.md"
    if notes_file.exists():
        # v1.14.0 lesson: never overwrite humanized notes with raw drafts.
        # The re-run that burned us hit exactly this path.
        print(
            f"Error: {notes_file} already exists in the worktree - refusing to re-triage."
        )
        print("Delete it to start triage over, or commit it and use --tag-only.")
        return 1
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
    # Empty EDITOR ("") must behave like unset: getenv's default only
    # fires on a missing key, and split("") yields [] (WinError 2 on
    # launch). which() names the next failure class instead of the
    # bare traceback.
    raw_editor = os.environ.get("EDITOR") or ("notepad" if os.name == "nt" else "vi")
    # POSIX shlex strips backslashes from Windows paths
    # (C:\WINDOWS\... -> C:WINDOWS...). posix=False keeps them.
    editor = shlex.split(raw_editor, posix=(os.name != "nt"))
    if not editor or shutil.which(editor[0]) is None:
        print(
            f"Error: editor {editor[0] if editor else ''!r} not found "
            f"(EDITOR={os.environ.get('EDITOR')!r}) - set EDITOR, use --yes, "
            "or pass --notes-file."
        )
        os.unlink(triage_path)
        return 1
    try:
        edited = subprocess.run([*editor, triage_path], check=False)
        if edited.returncode != 0:
            print(
                f"Error: editor exited {edited.returncode} - notes not saved, tag not created."
            )
            return 1
        with open(triage_path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
        pairs = parse_approved_lines(text, rows)
        committed = write_notes_file(root, tag, prev_tag_global, pairs)
        if committed is None:
            return 1
        print(f"Wrote {notes_file} ({len(pairs)} lines)")
        return 0
    finally:
        os.unlink(triage_path)


prev_tag_global = ""


def triage_notes_file(rows, root, tag, prev_tag, notes_path):
    """Agent path: approved lines come from a file, never an editor."""
    notes_file = root / "release-notes" / f"{tag}.md"
    if notes_file.exists():
        print(
            f"Error: {notes_file} already exists in the worktree - refusing to re-triage."
        )
        print("Delete it to start triage over, or commit it and use --tag-only.")
        return 1
    try:
        text = Path(notes_path).read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        print(f"Error: cannot read --notes-file {notes_path}: {e}")
        return 1
    pairs = parse_approved_lines(text, rows)
    if not pairs:
        print(f"Error: --notes-file {notes_path} approved no lines - tag not created.")
        return 1
    committed = write_notes_file(root, tag, prev_tag, pairs)
    if committed is None:
        return 1
    print(f"Wrote {notes_file} ({len(pairs)} lines from {notes_path})")
    return 0


def main():
    args = parse_args()
    # Repo root is the cwd, not the script's dir: agents may invoke a copy
    # from outside the checkout (guard verification, probes). The script
    # reads git state from the cwd throughout.
    root = Path.cwd()
    script_dir = root / "scripts"

    if args.notes_file and args.yes:
        print("Error: --notes-file and --yes are mutually exclusive")
        return 1
    if args.tag_only and (args.notes_file or args.yes or args.dry_run):
        print("Error: --tag-only takes no triage flags (--notes-file/--yes/--dry-run)")
        return 1

    if args.tag_only:
        # Post-merge re-run: notes are committed, nothing to triage.
        if args.version is None or not TAG_RE.match(args.version):
            print(f"Usage: {__file__} vX.Y.Z --tag-only")
            return 1
        tag = args.version
        if entry_gates(args, root, tag) != 0:
            return 1
        notes_path = f"release-notes/{tag}.md"
        if git(["show", f"HEAD:{notes_path}"], root).returncode != 0:
            print(f"Error: {notes_path} is not committed - cannot --tag-only.")
            print(
                f"Run triage first: python scripts/release.py {tag} [--notes-file PATH]"
            )
            return 1
        return cut_tag(root, tag)

    tag = resolve_tag(args, root)
    if tag is None:
        return 1
    if entry_gates(args, root, tag) != 0:
        return 1
    warn_ci_status(root)

    prev_tag = prevailing_tag(script_dir, root)
    if prev_tag is None:
        return 1
    global prev_tag_global
    prev_tag_global = prev_tag
    range_ref = f"{prev_tag}..HEAD" if prev_tag else "HEAD"

    rows = triage_draft_rows(range_ref, script_dir)
    if args.dry_run:
        for r in rows:
            print(f"[{r['section']}] {r['draft']}")
        return 0
    if args.notes_file:
        return triage_notes_file(rows, root, tag, prev_tag, args.notes_file)
    if rows and not args.yes:
        return triage_editor_path(rows, root, tag)
    # --yes: non-interactive automation path. Approve drafts as-is so CI
    # uses the triaged file instead of silently falling back.
    if rows:
        print("Warning: --yes skips the triage editor; approving drafts as-is")
        committed = write_notes_file(
            root, tag, prev_tag, [(r["section"], r["draft"]) for r in rows]
        )
        if committed is None:
            return 1
    # Empty range: still write the stability-update stub via the helper.
    elif not (root / "release-notes" / f"{tag}.md").exists():
        committed = write_notes_file(root, tag, prev_tag, [])
        if committed is None:
            return 1

    if push_notes_or_pr(root, tag) != 0:
        return 1
    return cut_tag(root, tag)


if __name__ == "__main__":
    sys.exit(main())
