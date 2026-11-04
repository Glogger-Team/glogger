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

    Normal commits yield zero or one entry (subject text). Unprefixed
    squash records yield one entry per `* type:` bullet (bullet text,
    `* ` stripped) — matching pre-unification classify_squash_body, which
    emitted every bullet, not one per section. Prefixed subjects (squash
    or not) yield the subject alone: the PR title is the entry.
    """
    _, sections = classify_record(subject, body_lines)
    if not sections:
        return []
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
    agg = bump_for_records([("fix: a", []), ("feat: b", [])])
    if agg != "minor":
        print(f"FAIL agg minor: got {agg}")
        failures += 1
    agg = bump_for_records([("feat: a", []), ("fix!: b", [])])
    if agg != "major":
        print(f"FAIL agg major: got {agg}")
        failures += 1
    return failures


if __name__ == "__main__":
    sys.exit(main())
