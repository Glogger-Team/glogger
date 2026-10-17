#!/usr/bin/env node
// glogger grid-migration audit — the base→head padding/indent census behind the
// accepted-drift ledger in docs/architecture/ux-standards.md (§ "Accepted drift
// from the 2026-10-17 DataTable sweep").
//
// It reports, per migrated file, the cell padding of the *deleted* raw-markup
// cells (the mode of the combinations, since one file can hold several grids)
// next to the padding DataTable now renders, plus the `rowClass`/`cellClass`
// escape hatches in that file. It is a report, not a verdict: the point is that
// the base→head arithmetic is machine-derived rather than re-read from a diff.
//
// Usage: node scripts/grid-migration-audit.mjs [base-ref] [head-ref]
//        (defaults: v1.3.1 — the sweep's base — and HEAD)

import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const BASE = process.argv[2] ?? 'v1.3.1'
const HEAD = process.argv[3] ?? 'HEAD'

// Tailwind emits padding in this property order, so the last one present wins.
const SCALE = { '0': 0, '0.5': 2, '1': 4, '1.5': 6, '2': 8, '2.5': 10, '3': 12, '3.5': 14, '4': 16, '5': 20, '6': 24, '8': 32 }
const ORDER = ['p', 'px', 'ps', 'pe', 'pl', 'py', 'pt', 'pb']
const px = (token) => {
  const step = token.slice(token.indexOf('-') + 1).replace(/^\[|\]$/g, '')
  if (step in SCALE) return SCALE[step]
  const rem = step.match(/^([\d.]+)rem$/); if (rem) return +rem[1] * 16
  const p = step.match(/^([\d.]+)px$/); if (p) return +p[1]
  return null
}
const resolve = (classes, axis) => {
  const wanted = axis === 'left' ? ['p', 'px', 'ps', 'pl'] : ['p', 'py', 'pt']
  let value = 0
  for (const key of ORDER) {
    if (!wanted.includes(key)) continue
    const hit = classes.find((c) => c.startsWith(key + '-') && px(c) !== null)
    if (hit) value = px(hit)
  }
  return value
}

const git = (...args) => execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' })
const changed = git('diff', '--name-only', '--diff-filter=M', `${BASE}..${HEAD}`, '--', 'src')
  .split('\n')
  .filter((f) => f.endsWith('.vue'))

const tagsIn = (text) => {
  const out = []
  for (const m of text.matchAll(/<DataTable\b/g)) {
    let i = m.index + m[0].length, quote = null
    while (i < text.length) {
      const c = text[i]
      if (quote) { if (c === quote) quote = null }
      else if (c === '"' || c === "'") quote = c
      else if (c === '>') break
      i++
    }
    out.push(text.slice(m.index, i))
  }
  return out
}

const rows = []
for (const file of changed) {
  const removed = git('diff', '--unified=0', `${BASE}..${HEAD}`, '--', file)
    .split('\n')
    .filter((line) => line.startsWith('-') && !line.startsWith('---') && /<(td|th)\b/.test(line))
  // Header and body cells have always carried different padding, so they are
  // tracked apart: the body mode is what the row's cells now render at.
  const combos = { td: new Map(), th: new Map() }
  for (const line of removed) {
    const cls = line.match(/class="([^"]*)"/)?.[1]
    if (!cls) continue
    const kind = /<th\b/.test(line) ? 'th' : 'td'
    const classes = cls.split(/\s+/)
    const key = `${resolve(classes, 'left')}/${resolve(classes, 'top')}`
    combos[kind].set(key, (combos[kind].get(key) ?? 0) + 1)
  }
  const head = readFileSync(join(ROOT, file), 'utf8')
  if (!/<DataTable\b/.test(head)) continue
  if (combos.td.size === 0) continue // already a DataTable consumer at base

  const props = tagsIn(head).map((tag) => `${/compact/.test(tag) ? 'compact' : 'default'}${/size="sm"/.test(tag) ? '+sm' : ''}`)
  const overrides = [...new Set(head.match(/\[&>td[^\]]*\]:[\w./\[\]-]+/g) ?? [])]
  const cellClasses = [...new Set(head.match(/cellClass: '([^']*)'/g) ?? [])].map((s) => s.slice(12, -1))

  const asText = (map) => [...map.entries()].sort((a, b) => b[1] - a[1]).map(([k, n]) => `${k}(×${n})`).join(' ')
  const mode = [...combos.td.entries()].sort((a, b) => b[1] - a[1])[0]
  const [left, top] = mode[0].split('/').map(Number)
  const effective = props[0]?.startsWith('compact') ? { left: 8, top: 4 } : { left: 12, top: 8 }
  const signed = (n) => `${n >= 0 ? '+' : ''}${n}px`
  rows.push({
    file: file.replace('src/components/', ''),
    cells: removed.length,
    combos: asText(combos.td),
    head: props.join(' ') || '—',
    delta: `${signed(effective.left - left)} left, ${signed(effective.top - top)} top`,
    hooks: [...overrides, ...cellClasses].join(' ') || '',
  })
}

const w = (s, n) => String(s).padEnd(n)
console.log(`# grid migration census: ${BASE}..${HEAD}  (base cell padding → DataTable's)\n`)
console.log(`${w('file', 42)} ${w('cells', 5)} ${w('base px/py (mode, all combos)', 34)} ${w('head', 18)} ${w('delta', 20)} hooks`)
for (const r of rows) {
  console.log(`${w(r.file, 42)} ${w(r.cells, 5)} ${w(r.combos, 34)} ${w(r.head, 18)} ${w(r.delta, 20)} ${r.hooks}`)
}
