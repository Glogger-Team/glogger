"""Single source of truth for conventional-commit classification.

Shared by scripts/next-version.py (bump) and scripts/generate-changelog.py
(notes) so the two can never drift again. Stdlib only.

One regex set, two outputs per commit:
- bump: major | minor | patch
- section: feat | impv | fix | none  (notes grouping; docs/test/build/chore,
  release, merges, and unprefixed subjects are none)

Rules (must match the pre-unification behavior of both callers):
- `!:` marker or BREAKING[ -]CHANGE body line -> major.
- `feat:` subject -> minor bump + feat section.
- `fix:` / `impv:` subject -> patch bump + own section.
- docs/test/build/chore/release subjects -> patch bump, no section.
- Squash merges (`... (#NN)` subject): body `* type:` bullets classify
  the same way (major/minor/section). Only squash records read body
  bullets, so prose examples in a normal commit body can't false-trigger.
- Merge subjects (`Merge ...`, MERGE_MESSAGE titles) -> patch, none.
"""

import json
import re
import sys

BREAKING_RE = re.compile(r"BREAKING[ -]CHANGE")
SUBJECT_RE = re.compile(
    r"^(feat|fix|impv|docs|test|build|chore|release)(\([^)]*\))?(!)?:"
)
BULLET_RE = re.compile(
    r"^\s*\*\s*(feat|fix|impv|docs|test|build|chore|release)(\([^)]*\))?(!)?:"
)
SQUASH_RE = re.compile(r"\(#\d+\)\s*$")
MERGE_RE = re.compile(r"^Merge ")
# Optional per-commit player line (grpc/k8s lesson: authorship at merge,
# triage at release). Footer in the squash body, e.g.:
#   Release-Note: Skill level-ups from chat are now tracked.
#   Release-Note: none
# Matched case-insensitively; first match wins. "none" (any case, with or
# without trailing period) marks a deliberate skip.
TRAILER_RE = re.compile(r"^Release-Note:\s*(.+?)\s*$", re.IGNORECASE)
INTERNAL_PHRASES = (
    # Tooling the user never sees: lint slices, hooks, gates, classifiers,
    # release automation, and review-process batches. Matched case-insensitively
    # against the subject only — body prose (squash `*` bullets excluded) can
    # mention these words innocently. Kept tight to observed shapes; a
    # user-facing subject never contains these ("humanize changelog" stays).
    "lint",
    "clippy",
    "ruff",
    "shellcheck",
    "yamllint",
    "zizmor",
    "pinact",
    "pre-commit",
    "commit-msg",
    "setup-hooks",
    "hook self-test",
    "parity",
    "classif",  # classifier / classify / unification
    "review round",
    "local-review",
    "frozen review",
    "review-patterns",
    "review fixes",
    "coderabbit",
    "autofix",
    "auto-fix",
    "d-review",
    "c-gate",
    "c majors",
    "c minors",
    "b-delta",
    "b findings",
    "markdown gate",
    "markdown lint",
    "rust edition",
    "edition 2021",
    "edition 2024",
    "edition",
    "toolchain",
    "mise run",
    "mise lint",
    "slice partition",
    "push lint",
    "drift-proof",
    "stdlib-only",
    "unify repo automation",
    "port remaining shell",
    "loop-doc",
    "flatpak",
    "version-sync",
    "version sync",
    "sync_pat",
    "sync pat",
    "artipacked",
    "setup-python",
    "setup python",
    "crlf",
    "ascii tag",
    "push retry",
    "tagless",
    "tag-only",
    "tag only",
    "tauri-action",
    "tauri action",
    "release test",
    "release workflow",
    "release channel",
    "prose detail",
    "internal filter",
    "release classifier",
    "release_notes",
    "concurrency",
    "reusable workflow",
    "local-path",
    "local path",
    "script +x",
    "+x bits",
    "exec bit",
    "shebang",
    "exe001",
    "next-version",
    "next version",
    "squash-body",
    "squash body",
    "pr title",
    "self-repo",
    "self repo",
    "apt-cache",
    "apt cache",
    "audit-script",
    "audit script",
    "validate-only",
    "validate only",
    "bump-version",
    "bump version",
    "bsd sed",
    "pr bot",
    "latest.json",
    "draft-until",
    "draft until",
    "typecheck",
)
DETAIL_MAX_CHARS = 600
DETAIL_MAX_LINES = 6

NOTES_TYPES = ("feat", "impv", "fix")


def classify_subject(subject):
    """Classify one subject line -> (bump, section).

    bump is major/minor/patch; section is feat/impv/fix/none.
    """
    m = SUBJECT_RE.match(subject)
    if not m:
        return ("patch", "none")
    ctype = m.group(1)
    # Group 3 is the `!` immediately before the colon — a `!` inside the
    # scope (e.g. `feat(wow!)`) must not trigger a major bump.
    breaking = m.group(3) is not None
    if breaking:
        section = ctype if ctype in NOTES_TYPES else "none"
        return ("major", section)
    if ctype == "feat":
        return ("minor", "feat")
    if ctype in ("fix", "impv"):
        return ("patch", ctype)
    return ("patch", "none")


def classify_record(subject, body_lines):
    """Classify one full commit record -> (bump, sections).

    sections is a list (squash bodies can yield several bullets; normal
    commits yield zero or one). Merge subjects yield ([patch], []).
    """
    # `--no-merges` already excludes true merge commits; a single-parent
    # commit can still start with "Merge " (e.g. `Merge login (#42)`).
    # Only take the early return for non-squash records — squash bodies
    # classify by bullets regardless of the subject wording.
    if MERGE_RE.match(subject) and not SQUASH_RE.search(subject):
        return ("patch", [])
    subj_bump, subj_section = classify_subject(subject)
    if SQUASH_RE.search(subject):
        # Squash records: max(subject rank, bullet ranks, bare-BREAKING).
        # Old bump_kind checked bullets independently of the subject prefix,
        # so a prefixed subject with a higher-rank bullet still escalates.
        ranks = [subj_bump]
        sections = [subj_section] if subj_section != "none" else []
        # Bare BREAKING CHANGE footer in the body (not a bullet): major.
        # Scanned over all lines — the bullet loop below skips non-bullets,
        # and BREAKING_RE.match on a `* ...` line can never match.
        if any(BREAKING_RE.match(line) for line in body_lines):
            ranks.append("major")

        for line in body_lines:
            m = BULLET_RE.match(line)
            if not m:
                continue
            ctype = m.group(1)
            breaking = m.group(3) is not None
            # NOTE: bare BREAKING CHANGE footers are hoisted above (all-lines
            # scan); testing BREAKING_RE.match on a `* ...` line here could
            # never match, so the old branch was dead.
            if breaking:
                ranks.append("major")
                if ctype in NOTES_TYPES:
                    sections.append(ctype)
            elif ctype == "feat":
                ranks.append("minor")
                sections.append("feat")
            elif ctype in ("fix", "impv"):
                ranks.append("patch")
                sections.append(ctype)
            else:
                ranks.append("patch")
        if not sections and not ranks:
            return ("patch", [])
        bump = (
            "major" if "major" in ranks else ("minor" if "minor" in ranks else "patch")
        )
        # Preserve order, dedupe sections.
        seen = set()
        ordered = [s for s in sections if not (s in seen or seen.add(s))]
        return (bump, ordered)
    # Non-squash: subject classification stands; a BREAKING body line
    # escalates to major (section unchanged).
    sections = [subj_section] if subj_section != "none" else []
    if subj_bump != "major" and any(BREAKING_RE.match(line) for line in body_lines):
        return ("major", sections)
    if subj_bump == "major" or subj_section != "none":
        return (subj_bump, sections)
    return ("patch", [])


def entries_for_record(subject, body_lines):
    """One entry per notes line: [(section, text)].

    A Release-Note: footer wins verbatim (section from the subject or
    first notes bullet); "none" yields no entries. Otherwise normal
    commits yield zero or one entry (subject text). Unprefixed squash
    records yield one entry per `* type:` bullet (bullet text, `* `
    stripped) — matching pre-unification classify_squash_body, which
    emitted every bullet, not one per section. Prefixed subjects (squash
    or not) yield the subject alone: the PR title is the entry.
    """
    _, sections = classify_record(subject, body_lines)
    if not sections:
        return []
    trailer = release_note_trailer(body_lines)
    if trailer is not None:
        if trailer == "":
            return []
        return [(sections[0], trailer)]
    subj_bump, subj_section = classify_subject(subject)
    if subj_section != "none" or subj_bump == "major":
        return [(subj_section, subject)] if subj_section != "none" else []
    entries = []
    for line in body_lines:
        m = BULLET_RE.match(line)
        if not m:
            continue
        ctype = m.group(1)
        if ctype not in NOTES_TYPES:
            continue
        text = re.sub(r"^\s*\*\s*", "", line).strip()
        entries.append((ctype, text))
    return entries


def release_note_trailer(body_lines):
    """Player line from a Release-Note: footer, or None.

    Returns "" when the footer says none (deliberate skip), None when
    absent (untriaged — the release-time pass drafts from the subject).
    """
    for line in body_lines:
        m = TRAILER_RE.match(line.strip())
        if m:
            text = m.group(1).strip()
            if text.lower().rstrip(".") == "none":
                return ""
            return text
    return None


def is_internal_subject(subject):
    """True when a notes-section subject names tooling, not the app.

    Subject-only: a user-facing entry can mention lint/classifiers in its
    body ("Markdown gate was red") without being internal. Checked before
    entries render, so filtered commits vanish from the notes entirely —
    bump classification is untouched (internal work still counts as patch).
    """
    lowered = subject.lower()
    return any(phrase in lowered for phrase in INTERNAL_PHRASES)


def prose_detail(body_lines):
    """First explanatory chunk of a squash body, else "".

    Squash `* type:` bullets are entry titles; the prose paragraphs and
    `- ` dash-bullets between them are the summaries. A second bullet
    marker ends the detail — joining a whole bullet list reads as one
    mega-sentence. Review-process boilerplate (Co-authored-by,
    declined/finding/verdict lines) and `word:` tooling blocks
    (slices:/hooks:/ci:/guard:) are skipped. Caps at DETAIL_MAX_LINES /
    DETAIL_MAX_CHARS so one entry cannot flood the notes.
    """
    SKIP_RE = re.compile(
        r"^(co-authored-by|signed-off-by|reviewed-by|"
        r"finding|declined|verified|delta finding|b-delta|b findings?)\b",
        re.IGNORECASE,
    )
    BLOCK_RE = re.compile(
        r"^(slices|hooks|ci|guard|hook|docs|entry pins|denylist):\s*$",
        re.IGNORECASE,
    )
    DASH_RE = re.compile(r"^-\s+(.*)$")
    paragraph = []
    for line in body_lines:
        stripped = line.strip()
        if not stripped:
            if paragraph:
                break
            continue
        if stripped.startswith("*"):
            if paragraph:
                break
            continue
        dash = DASH_RE.match(stripped)
        if dash:
            if paragraph:
                break
            content = dash.group(1).strip()
            if not content:
                continue
            if SUBJECT_RE.match(content):
                continue
            if SKIP_RE.match(content) or BLOCK_RE.match(content):
                continue
            if len(content) < 40 and content[-1] not in ".!?)":
                continue
            paragraph.append(content)
            continue
        if paragraph:
            paragraph.append(stripped)
            continue
        if SKIP_RE.match(stripped) or BLOCK_RE.match(stripped):
            continue
        if len(stripped) < 40 and stripped[-1] not in ".!?)":
            continue
        paragraph.append(stripped)
    if not paragraph:
        return ""
    text = " ".join(paragraph)
    words = text.split()
    if len(words) > DETAIL_MAX_LINES * 20:
        words = words[: DETAIL_MAX_LINES * 20]
    text = " ".join(words)
    if len(text) > DETAIL_MAX_CHARS:
        cut = text.rfind(" ", 0, DETAIL_MAX_CHARS)
        text = text[: cut if cut > 0 else DETAIL_MAX_CHARS].rstrip() + "…"
    return text


def bump_for_records(records):
    """Aggregate (subject, body_lines) records -> major|minor|patch."""
    order = {"patch": 0, "minor": 1, "major": 2}
    best = "patch"
    for subject, body in records:
        bump, _ = classify_record(subject, body)
        if order[bump] > order[best]:
            best = bump
            if best == "major":
                break
    return best


def main():
    # Batch mode for generate-changelog.py: read NUL-separated %B records
    # on stdin, emit one JSON object per record on stdout:
    # {"bump": ..., "entries": [{"section": ..., "text": ...}]}.
    # entries is one per notes line (squash bodies yield one per bullet).
    if len(sys.argv) > 1 and sys.argv[1] == "--batch":
        data = sys.stdin.read()
        # Preserve blank records as empty-entry objects: the bash reader
        records = data.split("\0")
        if records and records[-1] == "":
            records.pop()
        for record in records:
            lines = record.splitlines()
            subject = lines[0] if lines else ""
            bump, _ = classify_record(subject, lines[1:])
            entries = [
                {"section": s, "text": t}
                for s, t in entries_for_record(subject, lines[1:])
            ]
            print(json.dumps({"bump": bump, "entries": entries}))
        return 0
    # Self-test mode.
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        failures = run_tests()
        print(f"{failures} failures" if failures else "all classifier tests pass")
        return 1 if failures else 0
    print("usage: release_notes.py --batch < records  |  --self-test", file=sys.stderr)
    return 1


def run_tests():
    cases = [
        # (subject, body, expected bump, expected sections)
        ("feat: add thing", [], "minor", ["feat"]),
        ("feat(scope): add thing", [], "minor", ["feat"]),
        ("feat: add thing (#85)", ["* feat: inner"], "minor", ["feat"]),
        ("fix: correct scroll", [], "patch", ["fix"]),
        ("impv: polish widget", [], "patch", ["impv"]),
        ("impv(Stall): recount discipline", [], "patch", ["impv"]),
        ("docs: update readme", [], "patch", []),
        ("test: add coverage", [], "patch", []),
        ("build: sync version files to v1.11.0", [], "patch", []),
        ("build(deps): bump foo", [], "patch", []),
        ("fix:no space after colon", [], "patch", ["fix"]),
        ("feat!: break api", [], "major", ["feat"]),
        ("feat(scope)!: break api", [], "major", ["feat"]),
        ("fix!: drop support", [], "major", ["fix"]),
        ("fix: scope change", ["BREAKING CHANGE: api"], "major", ["fix"]),
        ("fix: scope change", ["BREAKING-CHANGE: api"], "major", ["fix"]),
        # docs body mentioning BREAKING mid-line must not false-trigger
        # (BREAKING_RE.match is anchored at line start, like the original).
        ("docs: explain semver", ["example: BREAKING CHANGE is rare"], "patch", []),
        ("Merge pull request #60 from x/y", [], "patch", []),
        ("Merge branch 'main' into feature/x", [], "patch", []),
        # "Merge "-prefixed squash subjects still classify by body bullets
        # (`--no-merges` only excludes true merge commits).
        ("Merge login (#42)", ["* feat: inner feature"], "minor", ["feat"]),
        (
            "Merge login (#42)",
            ["* fix: inner fix", "BREAKING CHANGE: api removed"],
            "major",
            ["fix"],
        ),
        ("unprefixed subject", [], "patch", []),
        ("release: v1.0", [], "patch", []),
        # Squash bodies.
        (
            "unprefixed title (#66)",
            ["* feat: inner feature", "* fix: inner fix"],
            "minor",
            ["feat", "fix"],
        ),
        (
            "unprefixed title (#66)",
            ["* impv: inner improvement"],
            "patch",
            ["impv"],
        ),
        (
            "unprefixed title (#66)",
            ["* fix!: breaking inner"],
            "major",
            ["fix"],
        ),
        (
            "unprefixed title (#66)",
            ["prose without bullets"],
            "patch",
            [],
        ),
        # impv must never collapse into fix.
        ("impv: counts match after login", [], "patch", ["impv"]),
        # Prefixed squash subjects still escalate on higher-rank bullets
        # (old bump_kind checked bullets independently of the prefix).
        (
            "fix: thing (#10)",
            ["* feat: inner feature"],
            "minor",
            ["fix", "feat"],
        ),
        (
            "feat: x (#85)",
            ["* fix!: breaking inner"],
            "major",
            ["feat", "fix"],
        ),
        # Bare BREAKING CHANGE footer in a squash body bumps major.
        (
            "unprefixed title (#66)",
            ["* fix: inner fix", "BREAKING CHANGE: api removed"],
            "major",
            ["fix"],
        ),
        # chore!/release! escalate like other bang prefixes (documents
        # the SUBJECT_RE extension beyond old MAJOR_RE's six types).
        ("chore!: drop support", [], "major", []),
        ("release!: rebuild all", [], "major", []),
        # `!` inside the scope is not a breaking marker (group 3 only).
        ("feat(wow!): add thing", [], "minor", ["feat"]),
        ("fix(api:v2): scoped colon", [], "patch", ["fix"]),
        # Colon in scope plus a real bang still breaks (the old
        # `split(":")[0]` check missed this shape entirely).
        ("feat(api:v2)!: scoped colon breaking", [], "major", ["feat"]),
    ]
    failures = 0
    for subject, body, want_bump, want_sections in cases:
        got_bump, got_sections = classify_record(subject, body)
        if got_bump != want_bump or got_sections != want_sections:
            print(
                f"FAIL {subject!r}: want ({want_bump}, {want_sections}), "
                f"got ({got_bump}, {got_sections})"
            )
            failures += 1
    # Entry-level: multi-bullet squash records yield one entry per bullet.
    entries = entries_for_record(
        "Title (#62)",
        ["* feat: first thing", "prose", "* fix: second thing", "* fix: third thing"],
    )
    if entries != [
        ("feat", "feat: first thing"),
        ("fix", "fix: second thing"),
        ("fix", "fix: third thing"),
    ]:
        print(f"FAIL entries multi-bullet: got {entries}")
        failures += 1
    entries = entries_for_record("feat: normal subject", [])
    if entries != [("feat", "feat: normal subject")]:
        print(f"FAIL entries normal: got {entries}")
        failures += 1
    agg = bump_for_records(
        [
            ("fix: a", []),
            ("impv: b", []),
        ]
    )
    if agg != "patch":
        print(f"FAIL agg patch: got {agg}")
        failures += 1
    agg = bump_for_records([("feat: a", []), ("fix!: b", [])])
    if agg != "major":
        print(f"FAIL agg major: got {agg}")
        failures += 1
    internal_cases = [
        ("impv: slim push lint, full gate manual", True),
        ("impv: push lint runs typecheck, full vite bundle stays CI-only", True),
        ("impv: unify repo automation on Python (stdlib-only scripts)", True),
        ("feat: lint-parity guard, hook self-test, loop-doc hardening", True),
        ("feat: drift-proof pre-commit lint sharing single entrypoint", True),
        ("impv: unify release classifiers behind shared release_notes.py", True),
        ("impv: address CodeRabbit review findings on classifiers", True),
        ("fix: classify Merge-prefixed squash subjects by body", True),
        ("fix: review round B fix batch (stale head comments)", True),
        ("impv: local-review fix batch — drop commented import", True),
        ("build: migrate Rust edition 2021 to 2024", True),
        ("build: port remaining shell scripts to Python stdlib", True),
        ("fix: collapse nested if-lets for edition 2024 clippy", True),
        ("feat(StallTracker): replace Recently Sold Out with Clear unsure", False),
        ("feat(StallTracker): baseline recount-after-restock discipline", False),
        ("feat: resolve backlog product decisions, rename UI", False),
        ("feat: humanize changelog entries for end users", False),
        ("impv: dark Statehelm dashboard widget until LocalPlayer lines", False),
    ]
    for subject, want in internal_cases:
        got = is_internal_subject(subject)
        if got != want:
            print(f"FAIL internal {subject!r}: want {want}, got {got}")
            failures += 1
    # Prose detail: first explanatory paragraph, tooling blocks skipped.
    detail = prose_detail(
        [
            "* feat(StallTracker): replace Recently Sold Out with Clear unsure",
            "",
            "Recently Sold Out was a second table to scan with its own window",
            "dropdown and sort state, for rows the user can't act on. Clear unsure",
            "rolls the guess rows into one action.",
            "",
            "* impv(StallTracker): single Refresh in action row",
        ]
    )
    if not detail.startswith("Recently Sold Out was a second table"):
        print(f"FAIL prose basic: got {detail!r}")
        failures += 1
    detail = prose_detail(
        [
            "* feat: drift-proof pre-commit lint sharing single entrypoint",
            "",
            "* slices: mise run lint partitioned into lint:ruff/shell/yaml",
            "",
            "* hooks: local-only .pre-commit-config.yaml (no upstream repos)",
        ]
    )
    if detail != "":
        print(f"FAIL prose tooling-blocks: got {detail!r}")
        failures += 1
    detail = prose_detail(
        [
            "* feat: resolve backlog product decisions, rename UI, drop dead tables",
            "",
            "- Product decisions (2026-11-03): rename-orphan -> rename_character",
            "  command (shared table consts, guard against merge); stall hired ->",
            "  parsed 'hired' action via Books-files path; vendor_prices dropped",
            "  (v85, dead table); No-Resell + angling dropped (no source)",
            "- Settings -> General Characters section: rename/move affordance with",
            "  active-identity follow + cross-character cache invalidation",
        ]
    )
    if not detail.startswith("Product decisions (2026-11-03): rename-orphan"):
        print(f"FAIL prose dash-first: got {detail!r}")
        failures += 1
    if "General Characters section" in detail:
        print(f"FAIL prose dash-second-leaks: got {detail!r}")
        failures += 1
    detail = prose_detail([])
    if detail != "":
        print(f"FAIL prose empty: got {detail!r}")
        failures += 1
    return failures


if __name__ == "__main__":
    sys.exit(main())
