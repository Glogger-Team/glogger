## What's Changed since v1.5.2

### Features
- feat: outage-safe todo burn-down (survey manual framing, storage report fields, cross-character work orders, blocked consolidation, export names, dark-widget hide) (`1213f41`)

### Fixes
- fix: self-audit round — log remaining discards, queue comment, mixed-buffer test (`21a9ce7`)
- fix: survey round-5 — pending-queue kind guard, stored-spelling retro, claim logging (`cc52396`)
- fix: CodeRabbit round-4 — tooltip error ref, drain logging, doc staleness (`24dceca`)
- fix: portable review-patterns gate (no grep -P, loud search errors) (`8f49bb3`)
- fix: harden review-patterns gate to alias and multiline discards (`8433016`)
- fix: make review-patterns gate executable; invoke via bash (`3010225`)
- fix: correlate manual deferred drain by map; guard WO state on loaded+server (`4d1807b`)
- fix: propagate v81 backfill errors; define report/snapshot mapping (`697d004`)
- fix: WO cache retry on same-server invalidation; close stale doc claims (`6172f9d`)
- fix: wrap manual survey use insert + count in one transaction (`4c6addd`)
- fix: await retry holds loading flag; retry re-reads without backfill (`4c78ec5`)
- fix: WO invalidation on server import + retry same-server load after mid-flight invalidation (`63dc598`)
- fix: restore recordError display dropped by retry-button edit (`9f2606d`)
- fix: reload cross-character items on server switch while All-Characters on (`0cb96dd`)
- fix: add Retry button to survey catalog load error (`856b640`)
- fix: review fix-now — brace init bump, align WO catch comment (`bfa3bc2`)
- fix: altitude review — ordinal parity, items cache scope, survey UX, header (`4c08ad3`)
- fix: PR #54 round 3 — ordinal backfill matching, poll invalidation, failure refresh (`4430c35`)
- fix: review delta fix-now — WO server filter + stale clear, true conflicting-values test (`d60fabc`)
- fix: PR #54 round 2 — fill-only COALESCE order, WO null-server guard, recordError surface (`6c962b3`)
- fix: PR #54 review — v81 raw_json backfill, WO cache server-key, recordUse error surface, doc minors + regression guards (`1b0e758`)

---
*38 commits since v1.5.2*
