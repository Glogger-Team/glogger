## What's Changed since v1.5.1

### Features
- feat: Prodigy session count on Skills + base-level advancement ticks (`f41fda3`)
- feat: recover incoming milkings in ability-history backfill (`f7b8fe1`)
- feat: gate PRs on frontend build, Rust tests, TOS hard lines (#44) (`0776e56`)

### Improvements
- impv: release coordinator lock around poem backfill + poison-safe commands (`6f11b18`)
- impv: pool-is-the-lock — drop dead op locks, idle stub, skill-drop counter (`e0dc162`)
- impv: backlog triage PR3 — frontend refresh ordering (`64172e4`)
- impv: backlog triage PR2 — survey single loot authority + bounds (`9944452`)
- impv: backlog triage PR1 — ingest guards + fewer locks (`0c47970`)

### Fixes
- fix: exempt chat-sourced rows from hint boundary tick (`3cdd7b0`)
- fix: PR review loop — roulette local key, self-milk UTC, anchor v80, scan observability (`237e5fb`)
- fix: verification follow-ups — internal-name stack match, Ilmari arm, live search tab id, keyboard resolver (`9dc0c32`)
- fix: review fix batch — scan writer scoping, currency live-gate, stack/arena follow-ups, dark-tab reachability (`1524897`)
- fix: route scan-path Status dispatch through shared module, honor file tz, dark-state dead trackers (`50f741f`)
- fix: thread item identity into survey single-authority guards (`f50cf70`)

---
*26 commits since v1.5.1*
