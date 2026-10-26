## What's Changed since v1.5.0

### Features
- feat: ingest chat skill level-ups and auto-import VIP reports (`35ebb54`)

### Fixes
- fix: parse chat timestamps in timestampToSeconds, accrue level-up XP to combat rate (`59cd420`)
- fix: fetch version over HTTPS; drop no-op Mod replace (`571315e`)
- fix: monotonic last_confirmed_at in apply_chat_level_up, log coordinator DB errors (`c9eced5`)
- fix: restore chat MilkedBy arm, correct tnl-unknown and timestamp-format docs (`e21c1a2`)
- fix: review findings on chat level-up ingest — recency guard, session fold-in, Reports-dir confinement (`4f84aff`)

---
*8 commits since v1.5.0*
