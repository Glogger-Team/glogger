#!/usr/bin/env python3
"""glogger grid standards — docs/architecture/ux-standards.md section "Data tables"."""

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src"

RAW_TABLE_EXEMPT = {
    "src/components/Shared/DataTable.vue": "the shared component itself",
    "src/components/Character/ComputedStatsCard.vue": "label/value panel, colspan section rows",
    "src/components/Character/ReportStatsSection.vue": "label/value, headings live outside the table",
    "src/components/Character/DeathsView.vue": "parent row expands into a full-width colspan row",
    "src/components/Crafting/BreweryRecipeDetail.vue": "columns are the recipe's variable_slots",
    "src/components/Dashboard/AggregateView.vue": "skills-across-characters pivot (runtime columns)",
    "src/components/Dashboard/widgets/ArenaWidget.vue": "pivot with runtime columns",
    "src/components/StallTracker/StallRevenueTab.vue": "pivot with a tfoot",
    "src/components/Settings/GameStateDebug.vue": "raw debug dump",
    "src/dev-panel/tabs/GameStateTab.vue": "raw debug dump",
}

COMMENT_RE = re.compile(r"<!--[\s\S]*?-->")
RAW_RE = re.compile(r"<table\b")
DATATABLE_RE = re.compile(r"<DataTable\b")
CLAIM_RE = re.compile(r"\*\*(\d+) instances across (\d+) files\*\*")


def walk(directory):
    out = []
    walk_into(directory, out)
    return out


def walk_into(directory, out):
    with os.scandir(directory) as entries:
        for entry in entries:
            path = os.path.join(directory, entry.name)
            if entry.is_dir(follow_symlinks=False):
                walk_into(path, out)
            elif entry.name.endswith(".vue"):
                out.append(path)


def posix(path):
    return Path(os.path.relpath(path, ROOT)).as_posix()


def main():
    # Node writes "\n" even on Windows; match it byte-for-byte.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", newline="\n")
    violations = []
    raw_table_files = []
    raw_table_occurrences = 0
    data_table_occurrences = 0
    data_table_files = []

    for file in walk(str(SRC)):
        with open(file, encoding="utf-8") as handle:
            text = handle.read()
        markup = COMMENT_RE.sub("", text)
        raw = len(RAW_RE.findall(markup))
        tables = len(DATATABLE_RE.findall(markup))
        if raw > 0:
            raw_table_files.append(posix(file))
            raw_table_occurrences += raw
        if tables > 0:
            data_table_files.append(posix(file))
            data_table_occurrences += tables

    for file in raw_table_files:
        if file not in RAW_TABLE_EXEMPT:
            violations.append(
                f"raw <table> in {file} \u2014 migrate it to DataTable "
                "(src/components/Shared/DataTable.vue) "
                "or add it to RAW_TABLE_EXEMPT here *and* to the exception list "
                "in docs/architecture/ux-standards.md"
            )
    for file in RAW_TABLE_EXEMPT:
        if file not in raw_table_files:
            violations.append(
                f"stale exemption: {file} keeps no raw <table> \u2014 delete its RAW_TABLE_EXEMPT entry"
            )

    with open(ROOT / "TODO.md", encoding="utf-8") as handle:
        todo = handle.read()
    sweep_line = next(
        (line for line in todo.split("\n") if "Table sweep done" in line), ""
    )
    claim = CLAIM_RE.search(sweep_line)
    if not claim:
        violations.append(
            'TODO.md: no "**N instances across M files**" claim on the "Table sweep done" line \u2014 '
            "restore the census sentence or update this check (a silently missing claim would stop the drift check working)"
        )
    elif int(claim.group(1)) != data_table_occurrences or int(claim.group(2)) != len(
        data_table_files
    ):
        violations.append(
            f"TODO.md claims {claim.group(1)} instances across {claim.group(2)} files, "
            f"measured {data_table_occurrences} across {len(data_table_files)}"
        )

    with open(ROOT / "docs/architecture/ux-standards.md", encoding="utf-8") as handle:
        ux_standards = handle.read()
    data_tables_rule = next(
        (
            line
            for line in ux_standards.split("\n")
            if line.startswith("- **Data tables:**")
        ),
        "",
    )
    if not data_tables_rule:
        violations.append(
            'docs/architecture/ux-standards.md: the "- **Data tables:**" rule line is missing '
            "\u2014 restore it or update this check"
        )
    for file in RAW_TABLE_EXEMPT:
        name = Path(file).stem
        if name not in data_tables_rule:
            violations.append(
                f"exemption {file} is not named in the "
                '"- **Data tables:**" rule in '
                "docs/architecture/ux-standards.md \u2014 document its raw-table reason there before exempting it here"
            )

    print(
        f"grid standards: {data_table_occurrences} DataTable instances in {len(data_table_files)} files \u00b7 "
        f"raw <table> {raw_table_occurrences} in {len(raw_table_files)} files "
        f"({len(RAW_TABLE_EXEMPT)} documented exemptions)"
    )

    if violations:
        print(f"\n{len(violations)} violation(s):", file=sys.stderr)
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        sys.exit(1)
    print("OK")


if __name__ == "__main__":
    main()
