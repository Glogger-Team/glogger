#!/usr/bin/env python3
"""Gate: workflow expression identifiers + LF policy + needs/output wiring.

Three mechanical defect shapes:

1. Hyphenated filter/output names (e.g. `run-all`) are invalid GitHub
   expression identifiers: `needs.changes.outputs.run-all` never resolves,
   and the workflow fails at admission with `startup_failure` and zero
   jobs — no logs, banner text only. Every dorny/paths-filter key and every
   `needs.<job>.outputs.<name>` / `steps.<id>.outputs.<name>` dereference
   must be `[A-Za-z_][A-Za-z0-9_]*`.
2. `.gitattributes` mandates LF everywhere (`* text=auto eol=lf`), but a
   Python `str.replace` write silently flips a file to CRLF, failing
   `yamllint new-lines` at commit time. Workflows must be pure LF.
3. Dangling `needs.<job>.outputs.<name>` wiring (one-migration-per-PR gate,
   branch `feature/vip-json-logging`): removing `needs: [changes]` from a
   job while its steps still gate on `needs.changes.outputs.*` silently
   skips those steps on PRs (null dereference evaluates false — no error,
   just missing checks); likewise replacing a `changes` output mapping
   (e.g. `version_sync` → `migration_chain`) while downstream jobs still
   gate on the removed output. Every `needs.<J>.outputs.*` dereference
   inside a flow-form `needs: [...]` job requires `J` in that job's
   `needs:` list, and every `needs.changes.outputs.<N>` requires `N` in
   the `changes` job outputs. Block-form `needs:` lists are not parsed —
   jobs using them are skipped (none in-tree; see TODO backlog).

Stdlib only (matches check-lint-parity.py conventions).
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"

# `${{ needs.<job>.outputs.<name> }}` / `${{ steps.<id>.outputs.<name> }}`
DEREF_RE = re.compile(r"(?:needs|steps)\.[A-Za-z0-9_.]+\.outputs\.([A-Za-z0-9_.-]+)")
VALID_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
# `needs.<job>.outputs.<name>` — job and output name per dereference.
NEEDS_DEREF_RE = re.compile(r"needs\.([A-Za-z0-9_-]+)\.outputs\.([A-Za-z0-9_.-]+)")
# Two-space `job:` headers, their four-space `needs: [...]` lists, and the
# `changes` job's six-space `outputs:` mappings (`name: ${{ steps... }}`).
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
NEEDS_LIST_RE = re.compile(r"^    needs: \[(.*?)\]\s*$")
OUTPUT_MAP_RE = re.compile(r"^      ([A-Za-z0-9_-]+): \$\{\{")


def check_file(path: Path) -> list[str]:
    findings: list[str] = []
    raw = path.read_bytes()
    if b"\r\n" in raw:
        n = raw.count(b"\r\n")
        findings.append(
            f"workflow-expressions: {path.name} has {n} CRLF line "
            "endings (repo policy is LF-only; normalize before commit)"
        )
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        print(
            f"workflow-expressions: cannot decode {path.name} as UTF-8",
            file=sys.stderr,
        )
        return ["decode-error"]
    for i, line in enumerate(text.splitlines(), start=1):
        for m in DEREF_RE.finditer(line):
            name = m.group(1)
            if not VALID_RE.match(name):
                findings.append(
                    f"workflow-expressions: {path.name}:{i}: "
                    f"output name '{name}' is not a valid expression "
                    "identifier (hyphens/dots fail admission with "
                    "startup_failure, zero jobs) — rename to underscores"
                )
    findings.extend(check_wiring(path, text))
    return findings


def check_wiring(path: Path, text: str) -> list[str]:
    """Every `needs.<J>.outputs.*` deref in a flow-form `needs: [...]` job
    must resolve: J in the reader's `needs:` list, and (for J == changes)
    the output mapped by changes. Block-form `needs:` jobs are skipped."""
    findings: list[str] = []
    in_jobs = False
    job: str | None = None
    needs: set[str] = set()
    needs_block = False
    in_outputs = False
    changes_outputs: set[str] = set()
    # Pass 1: the changes job's output mappings.
    for line in text.splitlines():
        if re.match(r"^jobs:\s*$", line):
            in_jobs = True
            continue
        if not in_jobs:
            continue
        m = JOB_RE.match(line)
        if m:
            job = m.group(1)
            in_outputs = False
            continue
        if job == "changes" and re.match(r"^    outputs:\s*$", line):
            in_outputs = True
            continue
        if re.match(r"^(?:\S|  \S|    \S)", line):
            in_outputs = False
            continue
        m = OUTPUT_MAP_RE.match(line)
        if m and job == "changes" and in_outputs:
            changes_outputs.add(m.group(1))
    # Pass 2: every needs deref against its reader's needs + changes map.
    in_jobs = False
    job = None
    needs = set()
    needs_block = False
    for i, line in enumerate(text.splitlines(), start=1):
        if re.match(r"^jobs:\s*$", line):
            in_jobs = True
            continue
        if not in_jobs:
            continue
        m = JOB_RE.match(line)
        if m:
            job = m.group(1)
            needs = set()
            needs_block = False
            continue
        if job is None:
            continue
        if re.match(r"^    needs:\s*$", line):
            needs_block = True  # block form: unparseable by regex, skip job
            continue
        m = NEEDS_LIST_RE.match(line)
        if m:
            needs = {n.strip() for n in m.group(1).split(",") if n.strip()}
            continue
        for d in NEEDS_DEREF_RE.finditer(line):
            dep, name = d.group(1), d.group(2)
            if dep not in needs and not needs_block:
                findings.append(
                    f"workflow-expressions: {path.name}:{i}: "
                    f"`needs.{dep}.outputs.*` dereferenced without "
                    f"`{dep}` in this job's `needs:` — steps gate false "
                    "on PRs (add the need, don't just silence the step)"
                )
            elif dep == "changes" and name not in changes_outputs:
                findings.append(
                    f"workflow-expressions: {path.name}:{i}: "
                    f"`needs.changes.outputs.{name}` has no `changes` "
                    "outputs mapping — the gate never runs on PRs "
                    "(re-add the mapping, don't just silence the step)"
                )
    return findings


def main() -> int:
    if not WORKFLOWS.is_dir():
        print("workflow-expressions: .github/workflows/ missing", file=sys.stderr)
        return 2
    fail = 0
    for path in sorted(WORKFLOWS.glob("*.yml")):
        for finding in check_file(path):
            print(finding)
            fail = 1
    return fail


if __name__ == "__main__":
    sys.exit(main())
