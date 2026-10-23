## What's Changed since v1.3.7

### Features
- feat: wire snapshot abilities, VIP NPC fields, achievements catalog to UI (`8f08ad8`)

### Improvements
- impv: ingest VIP report fields + achievements from 10/21 update (`0d26d7a`)

### Fixes
- fix: unique temp DB/dir names for parallel cargo tests (`8a24f20`)
- fix: bind Ctrl+K quick-search alias, correct repository URL (`623a37f`)
- fix: split chat import status into result/error refs, label Player.log error box (`1fd0c54`)
- fix: restore chat import result feedback, fix stuck importing flag (`f85745e`)
- fix: folder chat import nested-tx crash + progress indicator, CDP/wipe guidance (`0437ee7`)
- fix: filter placeholder character reports (??? name / error race) (`b3c37ab`)
- fix: clear snapshotAbilities on character switch (`a4d7a12`)
- fix: guard snapshot timer fallback on live-row existence (review fix) (`4344c43`)
- fix: clear areas in CDN persist (same UNIQUE re-persist class) (`2c6ccf4`)
- fix: clear item_uses in CDN persist (UNIQUE failure on v488 re-persist) (`9e01be1`)
- fix: vendor councils estimates + CDN tsys clear + drop Vite devtools (`3ac9707`)
- fix: review fixes — picker per-skill fallback, Known skills label, docstring (`2337687`)

---
*19 commits since v1.3.7*
