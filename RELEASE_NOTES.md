## What's Changed since v1.4.0

### Features
- feat: backlog ingest accuracy — symmetric rez dedup, directional treasure, inventory metrics/buffer/login-clear (`f32860f`)
- feat: track 'X has milked you' status messages as milked_by events (#31) (`a2d7087`)
- feat: vendor council overhaul — 14 buckets, newest-wins aggregate, favorites (#26) (`8c964c2`)
- feat: full CHANGELOG.md history across all 143 releases (`14891f2`)

### Improvements
- impv: milked_by backlog — shared normalizer, fixture tests, doc, frontend union (#32) (`45ebfd8`)

### Fixes
- fix: drop pinact-action from CI (aqua bootstrap broken) — pin check stays local-only (`d11dcb5`)
- fix: pinact validate-only mode; drop audit-script shebang (ruff EXE001) (`7a71459`)
- fix: frozen-review PR30 findings batch — login-batch latch, idle delete backstop, audit gate fail-closed (`8320025`)
- fix: frozen-review findings — restore poll processing, newest-wins correctness, audit gate hardening (`bad9113`)
- fix: gate login clear on identity stability, retention-aware poll drain (`4e8d1e1`)
- fix: frozen-review round 2 findings (critical + 13 highs + App.vue poll + stole anchor) (#29) (`41bb2b5`)
- fix: hand-fix markdown findings (blanks, fences, URLs), LF endings intact (`5071f4e`)
- fix: use zizmor-action inputs (not args), exclude config globs from markdown run (`598e8c3`)
- fix: split Ruff check/format steps, wire markdownlint config + scoped globs (`841ca7d`)
- fix: satisfy ruff check + format on helper scripts (`16f58a9`)
- fix: flatpak uses ref, narrow apt-cache writers (`57945ff`)
- fix: review fixes — script +x bits, backfill empty-set guard, tagless changelog (`cb3a072`)

---
*40 commits since v1.4.0*
