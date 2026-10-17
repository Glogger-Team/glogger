#!/usr/bin/env node
// glogger grid standards — docs/architecture/ux-standards.md § "Data tables"
//
// Fails (exit 1) when the DataTable standard has drifted:
//   1. a raw <table> appears in a .vue file that is not on the exemption list
//   2. an exemption is stale (listed here, no raw <table> left in the file)
//   3. docs/TODO.md's "**N instances across M files**" claim != the measured census
//   4. an exempt file is not named in the "Data tables" rule in
//      docs/architecture/ux-standards.md, so the list cannot grow silently
//
// The exemption list is the single machine-readable copy of the reasons
// ux-standards.md gives for keeping a raw table; keep the two in sync.
//
// Usage: npm run check:grids   (or: node scripts/check-grid-standards.mjs)

import { readFileSync, readdirSync } from 'node:fs'
import { basename, dirname, join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const SRC = join(ROOT, 'src')

// Path (repo-relative, forward slashes) -> why it may keep a raw <table>.
const RAW_TABLE_EXEMPT = new Map([
  ['src/components/Shared/DataTable.vue', 'the shared component itself'],
  ['src/components/Character/ComputedStatsCard.vue', 'label/value panel, colspan section rows'],
  ['src/components/Character/ReportStatsSection.vue', 'label/value, headings live outside the table'],
  ['src/components/Character/DeathsView.vue', 'parent row expands into a full-width colspan row'],
  ['src/components/Crafting/BreweryRecipeDetail.vue', "columns are the recipe's variable_slots"],
  ['src/components/Dashboard/AggregateView.vue', 'skills-across-characters pivot (runtime columns)'],
  ['src/components/Dashboard/widgets/ArenaWidget.vue', 'pivot with runtime columns'],
  ['src/components/StallTracker/StallRevenueTab.vue', 'pivot with a tfoot'],
  ['src/components/Settings/GameStateDebug.vue', 'raw debug dump'],
  ['src/dev-panel/tabs/GameStateTab.vue', 'raw debug dump'],
])

function walk(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) walk(path, out)
    else if (entry.name.endsWith('.vue')) out.push(path)
  }
  return out
}

const posix = (path) => relative(ROOT, path).split(sep).join('/')
const count = (text, re) => (text.match(re) ?? []).length

const violations = []
const rawTableFiles = []
let rawTableOccurrences = 0
let dataTableOccurrences = 0
const dataTableFiles = []

for (const file of walk(SRC)) {
  const text = readFileSync(file, 'utf8')
  // A `<table>` mentioned in prose (an HTML comment) is not markup.
  const markup = text.replace(/<!--[\s\S]*?-->/g, '')
  const raw = count(markup, /<table\b/g)
  // `<DataTable\b` deliberately excludes `<DataTableSkeleton`, a different component.
  const tables = count(markup, /<DataTable\b/g)
  if (raw > 0) {
    rawTableFiles.push(posix(file))
    rawTableOccurrences += raw
  }
  if (tables > 0) {
    dataTableFiles.push(posix(file))
    dataTableOccurrences += tables
  }
}

for (const file of rawTableFiles) {
  if (!RAW_TABLE_EXEMPT.has(file)) {
    violations.push(
      `raw <table> in ${file} — migrate it to DataTable (src/components/Shared/DataTable.vue) ` +
        `or add it to RAW_TABLE_EXEMPT here *and* to the exception list in docs/architecture/ux-standards.md`,
    )
  }
}
for (const file of RAW_TABLE_EXEMPT.keys()) {
  if (!rawTableFiles.includes(file)) {
    violations.push(`stale exemption: ${file} keeps no raw <table> — delete its RAW_TABLE_EXEMPT entry`)
  }
}

const todoPath = join(ROOT, 'docs/TODO.md')
const todo = readFileSync(todoPath, 'utf8')
// Read the sweep line, not the whole file: docs/TODO.md also keeps the
// superseded pre-sweep audit line ("**6 instances across 5 files** (not 10)").
const sweepLine = todo.split('\n').find((line) => line.includes('Table sweep done')) ?? ''
const claim = sweepLine.match(/\*\*(\d+) instances across (\d+) files\*\*/)
if (!claim) {
  violations.push(
    `docs/TODO.md: no "**N instances across M files**" claim on the "Table sweep done" line — ` +
      `restore the census sentence or update this check (a silently missing claim would stop the drift check working)`,
  )
} else if (+claim[1] !== dataTableOccurrences || +claim[2] !== dataTableFiles.length) {
  violations.push(
    `docs/TODO.md claims ${claim[1]} instances across ${claim[2]} files, ` +
      `measured ${dataTableOccurrences} across ${dataTableFiles.length}`,
  )
}

// Every exemption must be named where the standard is written down, so the
// list can only grow deliberately. Scoped to the data-tables bullet alone — the
// one line that gives raw-table *reasons*: a grid whose name merely appears in
// the accepted-drift ledger has no such reason written down, and TODO's sweep
// line is a status paragraph that names grids for unrelated reasons (the
// migration census, and the `<DataTableSkeleton>` importers), so neither may
// pass as documentation.
const uxStandards = readFileSync(join(ROOT, 'docs/architecture/ux-standards.md'), 'utf8')
const dataTablesRule = uxStandards.split('\n').find((line) => line.startsWith('- **Data tables:**')) ?? ''
if (!dataTablesRule) {
  violations.push('docs/architecture/ux-standards.md: the "- **Data tables:**" rule line is missing — restore it or update this check')
}
for (const file of RAW_TABLE_EXEMPT.keys()) {
  const name = basename(file, '.vue')
  if (!dataTablesRule.includes(name)) {
    violations.push(
      `exemption ${file} is not named in the "- **Data tables:**" rule in ` +
        `docs/architecture/ux-standards.md — document its raw-table reason there before exempting it here`,
    )
  }
}

console.log(
  `grid standards: ${dataTableOccurrences} DataTable instances in ${dataTableFiles.length} files · ` +
    `raw <table> ${rawTableOccurrences} in ${rawTableFiles.length} files ` +
    `(${RAW_TABLE_EXEMPT.size} documented exemptions)`,
)

if (violations.length > 0) {
  console.error(`\n${violations.length} violation(s):`)
  for (const violation of violations) console.error(`  - ${violation}`)
  process.exit(1)
}
console.log('OK')
