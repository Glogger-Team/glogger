# Changelog

Full release history for glogger, generated from git tags.
Per-release notes use the same grouping as scripts/generate-changelog.sh
(`feat:` → Features, `impv:` → Improvements, `fix:` → Fixes).
Releases that predate the commit-prefix convention list all commit subjects.

## v1.5.2 (2026-10-27)

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
*27 commits since v1.5.1*

## v1.5.1 (2026-10-26)

### Features
- feat: ingest chat skill level-ups and auto-import VIP reports (`35ebb54`)

### Fixes
- fix: parse chat timestamps in timestampToSeconds, accrue level-up XP to combat rate (`59cd420`)
- fix: fetch version over HTTPS; drop no-op Mod replace (`571315e`)
- fix: monotonic last_confirmed_at in apply_chat_level_up, log coordinator DB errors (`c9eced5`)
- fix: restore chat MilkedBy arm, correct tnl-unknown and timestamp-format docs (`e21c1a2`)
- fix: review findings on chat level-up ingest — recency guard, session fold-in, Reports-dir confinement (`4f84aff`)

---
*9 commits since v1.5.0*

## v1.5.0 (2026-10-25)

### Features
- feat: backlog ingest accuracy — symmetric rez dedup, directional treasure, inventory metrics/buffer/login-clear (`f32860f`)
- feat: track 'X has milked you' status messages as milked_by events (#31) (`a2d7087`)
- feat: vendor council overhaul — 14 buckets, newest-wins aggregate, favorites (#26) (`8c964c2`)
- feat: full CHANGELOG.md history across all 143 releases (`14891f2`)

### Improvements
- impv: milked_by backlog — shared normalizer, fixture tests, doc, frontend union (#32) (`45ebfd8`)

### Fixes
- fix: install Linux system deps in release test job; docs: stop-and-ask rule for CI startup_failure (`b1e0c3d`)
- fix: move flatpak concurrency from top level into jobs (inputs unavailable at top level) (`6a21a13`)
- fix: use local-path syntax for flatpak reusable workflow call (`231a8a8`)
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
*49 commits since v1.4.0*

## v1.4.0 (2026-10-23)

### Fixes
- fix: base release notes on last published release, repair v1.3.9 notes (`8422581`)

---
*4 commits since v1.3.9*

## v1.3.9 (2026-10-22)

### Fixes
- fix: unique temp DB/dir names for parallel cargo tests (`8a24f20`)
- fix: bind Ctrl+K quick-search alias, correct repository URL (`623a37f`)

---
*4 commits since v1.3.8*

## v1.3.8 (2026-10-22)

### Features
- feat: wire snapshot abilities, VIP NPC fields, achievements catalog to UI (`8f08ad8`)

### Improvements
- impv: ingest VIP report fields + achievements from 10/21 update (`0d26d7a`)

### Fixes
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
*16 commits since v1.3.7*

## v1.3.7 (2026-10-21)

### Improvements
- impv: post-major-bump cleanup + opt-ins (TS 6, Vite 8, rusqlite 0.40, zip 9) (`c6ec646`)

### Fixes
- fix: revert Map.getOrInsert in InventoryView grouping for older WebView2 (`589bb52`)

---
*7 commits since v1.3.6*

## v1.3.6 (2026-10-20)

### Fixes
- fix: strict session-boundary gates for tagged chat lines (`3cac8dd`)
- fix: key catch-up staged gains by producing chat session (`83e04eb`)
- fix: restore live cow-milk path; gate clear-on-adopt to login-line adoption (`1fa856d`)
- fix: session-keyed catch-up stack correction + clear-on-adopt inventory dump (`55f69fc`)

---
*7 commits since v1.3.5*

## v1.3.5 (2026-10-18)

### Fixes
- fix: seed live inventory from ItemAdded initial_quantity so multi-stack pickups count at once (`feee54e`)

---
*7 commits since v1.3.4*

## v1.3.4 (2026-10-17)

### Fixes
- fix: rename Statehelm favor filter to hide at/above tier (`a2287db`)

---
*2 commits since v1.3.3*

## v1.3.3 (2026-10-17)

### Features
- feat: let /frozen take its target from the argument (`c924788`)
- feat: add /frozen — this repo's one-word review call (`d439fd7`)

### Improvements
- impv: retire the repo review command in favour of the shared skill (`3b8f95d`)

### Fixes
- fix: give keyboard-focusable DataTable rows a visible focus indicator (`712b6d1`)
- fix: correct the review-loop and gate claims; scope the exemption check to the rule (`b77e4bd`)

---
*12 commits since v1.3.2*

## v1.3.2 (2026-10-17)

### Improvements
- impv: migrate every data grid to the shared DataTable (`f34eb3b`)

### Fixes
- fix: correct the ledger's padding rows and scope the gate's exemption check (`01b203a`)
- fix: restore the sub-row indents the DataTable sweep flattened (`6482561`)
- fix: gate DataTable row keydown to the row itself; stable MaterialTable keys (`8b5c3c5`)
- fix: repair DataTable fallthrough + row keyboard access; third review pass (`ff7ba13`)
- fix: restore row keys, dense empty states and pane scrollers after the DataTable sweep (`82708c3`)
- fix: repair the sticky-header wrapper and the compact/size conflation in DataTable (`a600e3b`)

---
*9 commits since v1.3.1*

## v1.3.1 (2026-10-17)

### Fixes
- fix: address review findings on the Statehelm gift rules (`9a205b5`)
- fix: count only real gifts in the Statehelm tracker (`060cca4`)

---
*4 commits since v1.3.0*

## v1.3.0 (2026-10-15)

### Features
- feat: backfill Statehelm gift status from the game's own logs (`6324d55`)

### Fixes
- fix: attribute each Player.log session to its own character during catch-up (`335a410`)
- fix: review pass over the Statehelm gift backfill (`3a6350e`)

---
*5 commits since v1.2.1*

## v1.2.1 (2026-10-13)

### Improvements
- impv: drop dead chat-retention settings (`df82bcd`)

### Fixes
- fix: show learned-but-uncooked recipes in Cook's Helper (`697f8fc`)

---
*4 commits since v1.2.0*

## v1.2.0 (2026-10-12)

### Features
- feat(chat): open search results in the day's real log (`d4b83a0`)
- feat: track cow-curse abilities and make milking timestamps honest (`628e8d1`)

### Improvements
- impv(chat): pin row-availability status to the list edges (`d84c779`)

### Fixes
- fix(chat): keep "Jump to Present" honest while newer rows exist above (`ada4039`)
- fix(chat): close the review findings on the day log (`a54817d`)
- fix(chat): bucket chat days in the viewer's display timezone (`edf1312`)

---
*10 commits since v1.1.1*

## v1.1.1 (2026-10-11)

### Improvements
- impv(market): label stall price capture as experimental (`04e1a4a`)

---
*7 commits since v1.1.0*

## v1.1.0 (2026-10-11)

### Fixes
- fix(gourmand): show every Gourmand-XP food and bucket unknown categories (`80302d5`)

---
*12 commits since v1.0.3*

## v1.0.3 (2026-10-06)

### Improvements
- impv: remove leftover resuscitation debug logging (`7927744`)
- impv(crafting): Cook's Helper auto-imports the gourmand report and persists eaten state (`91159b0`)

### Fixes
- fix(chat): repair search-context flow and context-query performance (`8132e90`)
- fix(db): make residual multi-statement writers atomic (`66ec481`)
- fix(db): scope single-instance guard to the install identity (`bfb72ed`)
- fix(db): route stall captures through the write pool; atomic writes + single-instance (`8e0eb1f`)
- fix(coordinator): record stall purchases via the write pool (`a412b32`)

---
*8 commits since v1.0.2*

## v1.0.2 (2026-10-05)

### Improvements
- impv: credit fork maintainer in the status bar (`ab52fcd`)

---
*2 commits since v1.0.1*

## v1.0.1 (2026-10-05)

_Pre-convention history — all commits in range:_

- release: v1.0.1 (`e0373ae`)

---
*1 commits in v1.0.0..v1.0.1*

## v1.0.0 (2026-10-05)

### Features
- feat(books): ingest the game's Books directory on startup (`9b7f986`)
- feat(chat): FTS deep search + day navigation across chat views (`1b07cfe`)
- feat: stall price capture for other players' stalls (capture panel + OCR) (`f53a53b`)

### Improvements
- impv(db): route remaining CRUD/import writers through the write pool (`a175382`)
- impv(db): single-writer pool architecture (app-side write queue) (`6185655`)

### Fixes
- fix(build): repair the personal-title sed (it emitted '?' and broke the JSON) and the corrupted conf (`c927593`)
- fix(ocr): NSIS silent-install flags for the tesseract downloader; bump-version personal-title match (`705b90e`)
- fix: disable updater artifacts on the personal profile (`3aa8d5a`)
- fix(stall): price capture fixes from live testing (`3376d46`)
- fix(db,chat,ui): review round fixes - accent-highlight index map, FTS fallback, watcher retry semantics (`d1533ec`)
- fix(db,books): self-review fixes for d0e53ab (`5dca53e`)
- fix(db,chat): seventh-round review fixes (`d0e53ab`)
- fix(db): explicit cursor-reset path so MAX guard doesn't block rotation (`f7d2684`)
- fix(db,chat): sixth-round review fixes (`9bcdeb7`)
- fix(db,chat): fifth-round review fixes (previously-missed notes) (`81da818`)
- fix(db,chat): fourth-round review fixes (`9927ddf`)
- fix(db,chat): teleport binds on writer; count survives pagination (`4c8b104`)
- fix(books): propagate first hoplology insert failure (`a663ae6`)
- fix(db,chat): third-round review fixes (`e91b0de`)
- fix(books): propagate ingest errors; route writers through the write pool (`1d578b9`)
- fix(books): prune removed files from mtime map + live-path year resolver for shop logs (`8bce3fa`)
- fix(books): apply code-review fixes to books watcher (`9868e3c`)
- fix(db): scope the writer connection per file in startup backfills (`43371ef`)
- fix(startup): run log backfills sequentially to stop BEGIN IMMEDIATE starvation (`ef198d2`)
- fix(startup): per-file backfill transactions so concurrent writers fit busy_timeout (`7ebb4cc`)
- fix(startup): use BEGIN IMMEDIATE for all startup backfill transactions (`fbb397c`)
- fix(db): set PRAGMA query_only on read-pool connections (`8d3c15d`)
- fix(db): route every remaining writer through the single write connection (`f5e7934`)
- fix(arena): convert bets backfill to per-file IMMEDIATE transactions (`74486b8`)
- fix(db): route the last missed writers through the write pool + drop dead helper (`0df0b18`)
- fix(chat): day-anchor only around_time; missing generation captures (`87c269a`)
- fix(chat): shared request-generation guard across day-jump and live loads (`7a9b7cb`)
- fix(chat): second-round review fixes (`80dbc05`)
- fix(chat): chips remove raw tokens; day-scoped count label (`aeecb4b`)
- fix(chat): review fixes — wildcard rules, FTS highlight tokens, filter-preserving day windows (`fc454cc`)
- fix(chat): review fixes for date navigation (`e59c822`)
- fix(chat): keep date toolbar reachable when a day filter yields nothing (`d3b2548`)
- fix(rez): dedup paired rez lines; keep Unknown out of name-based metrics (`791d5a6`)
- fix(rez): parse the target-only "comes back to life" phrasing (`2d39371`)
- fix: pre-push hook failed silently on Windows (`2178026`)

---
*66 commits since v0.12.18*

## v0.12.18 (2026-06-26)

### Fixes
- fix(build-planner): parse indirect-damage mod prose the CDN actually stores (`1a06113`)

---
*5 commits since v0.12.17*

## v0.12.17 (2026-07-30)

### Features
- feat(gourmand): classify foods by source (crafted vs event vs drop) (`75ffa9b`)

---
*4 commits since v0.12.16*

## v0.12.16 (2026-07-29)

### Features
- feat(build-planner): allow up to 3 mods per skill and unlimited generic mods (`5b88b9a`)

---
*4 commits since v0.12.15*

## v0.12.15 (2026-07-28)

### Features
- feat(recipes): surface currency costs (Combat Wisdom, Fae Energy) on recipes (`3302041`)

---
*4 commits since v0.12.14*

## v0.12.14 (2026-07-28)

### Features
- feat(dashboard): Almanac of Corruption widget for daily harvesting foci (`875fc66`)
- feat(farming-db): clear both equipped-skill filters to Any (`abb8f41`)

---
*5 commits since v0.12.13*

## v0.12.13 (2026-07-25)

### Features
- feat(build-planner): type-to-search filter for long dropdowns app-wide (`1e28db8`)

---
*4 commits since v0.12.12*

## v0.12.12 (2026-07-24)

### Features
- feat(combat-wisdom): draggable split to resize monster cooldowns (`3111850`)

---
*3 commits since v0.12.11*

## v0.12.11 (2026-07-21)

### Features
- feat(consolidate): dedupe account transfer chests + enlarge/widen move labels (`4daac54`)

---
*4 commits since v0.12.10*

## v0.12.10 (2026-07-21)

### Features
- feat(arena): track personal betting rate + P&L from Player.log (`f4aea8f`)

---
*4 commits since v0.12.9*

## v0.12.9 (2026-07-21)

### Features
- feat(arena): add Casino Arena fight tracker + dashboard widget (`0d722a7`)

---
*4 commits since v0.12.8*

## v0.12.8 (2026-07-20)

### Fixes
- fix(milking): de-duplicate cow timers by keying identity per-cow (`8a84c2f`)

---
*4 commits since v0.12.7*

## v0.12.7 (2026-07-20)

### Features
- feat(consolidate): pool alt storage with bank targeting and category/gear filters (`343728e`)

---
*4 commits since v0.12.6*

## v0.12.6 (2026-07-19)

### Fixes
- fix(consolidate): account for target vault slot capacity (`35d3704`)

---
*4 commits since v0.12.5*

## v0.12.5 (2026-07-19)

### Fixes
- fix(drop-tracker): stop filing ground/gathered loot under monster kills (`0e60ddd`)

---
*3 commits since v0.12.4*

## v0.12.4 (2026-07-19)

### Fixes
- fix(build-planner): convert direct damage to Fire for parenthesized type-change mods (`d19b004`)

---
*4 commits since v0.12.3*

## v0.12.3 (2026-07-19)

### Features
- feat(build-planner): add Racial jewelry equipment slot (`958d4c7`)
- feat(build-planner): fold Fairy Magic abilities and mods into Mentalism (`684fcd5`)

### Fixes
- fix(ability-tooltip): show cost, cooldown, range, and Special effect text (`11750af`)

---
*6 commits since v0.12.2*

## v0.12.2 (2026-07-18)

### Fixes
- fix(dashboard): categorize vendors that buy equipment/armor and other gear (`774c806`)

---
*4 commits since v0.12.1*

## v0.12.1 (2026-07-18)

### Features
- feat(dashboard): right-click vendor row to set councils to 0 (`6fbcb7a`)

---
*4 commits since v0.12.0*

## v0.12.0 (2026-07-17)

### Features
- feat(crafting): import/export projects via shareable codes (`3252b4c`)

### Fixes
- fix(crafting): tidy leveling toolbar and project entry layout (`0d9affa`)

---
*5 commits since v0.11.37*

## v0.11.37 (2026-06-26)

### Features
- feat(inventory): add cross-character storage search toggle (`83cc25a`)

---
*7 commits since v0.11.36*

## v0.11.36 (2026-07-13)

### Fixes
- fix(database): dedup Player-prev backfill kills against live rows regardless of server (`6a7ef8c`)

---
*4 commits since v0.11.35*

## v0.11.35 (2026-07-12)

### Features
- feat(chat): add Poems tab to Chat Logs (`35a46b8`)

---
*4 commits since v0.11.34*

## v0.11.34 (2026-07-12)

### Fixes
- fix(build-planner): default mod/augment tier to target level deterministically (`8cade1f`)

---
*6 commits since v0.11.33*

## v0.11.33 (2026-07-11)

### Fixes
- fix(crafting): leveling planner understated materials for multi-yield recipes (`7b665b7`)
- fix(release): only an open PR should suppress the release PR (`2a2ad8a`)

---
*7 commits since v0.11.32*

## v0.11.32 (2026-07-11)

### Features
- feat(brewery): collapsible + resizable discoveries and untried lists (`c54d2cb`)

### Fixes
- fix(flatpak): invoke node generator as a module, unsilence pip (exit-127 on gnome-50 image) (`5994ccb`)

---
*8 commits since v0.11.30*

## v0.11.30 (2026-07-10)

### Features
- feat(words-of-power): auto-remove words when spoken in-game (`8cf9888`)

### Fixes
- fix(flatpak): bump runtime to GNOME 50 (47 is EOL) (`c01c0ff`)

---
*10 commits since v0.11.29*

## v0.11.29 (2026-07-07)

### Features
- feat(build-planner): add "Import Equipped" gear loadout picker (`8f34810`)

### Fixes
- fix(build-planner): apply skill-wide "<Skill> Base Damage +N%" prose mods (`42813aa`)

---
*8 commits since v0.11.27*

## v0.11.27 (2026-07-07)

### Features
- feat(quests): tabbed quest screen, work-order backlog, and board tracking (`6e99705`)
- feat(farming): relabel session "Gold" stat to "Councils" (`31c23be`)

---
*7 commits since v0.11.26*

## v0.11.26 (2026-07-06)

### Fixes
- fix(farming): stop session history fragmenting for 12-hour-clock users (`c5a055d`)

---
*4 commits since v0.11.25*

## v0.11.25 (2026-07-06)

### Features
- feat(crafting): add Recipe Book tab — unlocked vs missing recipes per skill (`8e82a6d`)

---
*4 commits since v0.11.24*

## v0.11.24 (2026-07-05)

### Features
- feat(farming): make the 2h session cap configurable in App Settings (`a8e3eac`)
- feat(sessions): auto-roll survey + farming sessions past 2h (`bea3cda`)

### Fixes
- fix(build-planner): resolve ability names past monster-ability collisions (`91f4166`)

---
*7 commits since v0.11.23*

## v0.11.23 (2026-07-04)

### Fixes
- fix(survey): index survey-attributed transactions and harden session capture (`92850dc`)

---
*4 commits since v0.11.22*

## v0.11.22 (2026-06-26)

### Features
- feat(data-browser): project needs + friendly locations on item detail (`5ccc6c8`)

---
*5 commits since v0.11.21*

## v0.11.21 (2026-07-02)

### Features
- feat(surveying): extend chat-loot identity to sulfur, saltpeter, base crystals (`705813d`)
- feat(surveying): item-identity fallback for chat loot attribution (`ea33aa6`)

---
*8 commits since v0.11.20*

## v0.11.20 (2026-07-02)

### Features
- feat(surveying): only track items actually collected from surveys (`9a8d59c`)

---
*5 commits since v0.11.19*

## v0.11.19 (2026-07-02)

_Pre-convention history — all commits in range:_

- Merge pull request #57 from crisp-oddio/release/v0.11.19 (`e833ee8`)
- release: v0.11.19 (`8cacac1`)

---
*2 commits in v0.11.18..v0.11.19*

## v0.11.18 (2026-07-02)

### Features
- feat(build-planner): prose damage mods + damage-type conversions in effective stats (`0e749b9`)

---
*5 commits since v0.11.17*

## v0.11.17 (2026-07-01)

_Pre-convention history — all commits in range:_

- Merge pull request #54 from crisp-oddio/release/v0.11.17 (`0894a55`)
- release: v0.11.17 (`457806d`)
- Merge pull request #53 from crisp-oddio/dev (`3120af7`)
- style(roulette): fit widget within fixed card height (`bd348bd`)

---
*4 commits in v0.11.16..v0.11.17*

## v0.11.16 (2026-07-01)

### Features
- feat(brewery): track untried combos and build crafting projects from them (`0ddc7d7`)

---
*4 commits since v0.11.15*

## v0.11.15 (2026-06-30)

_Pre-convention history — all commits in range:_

- Merge pull request #50 from crisp-oddio/release/v0.11.15 (`ad37fdc`)
- release: v0.11.15 (`ff5ad7e`)
- Merge pull request #49 from crisp-oddio/dev (`bb1d66d`)
- style(roulette): roomier board cells for legibility (`be1eb40`)

---
*4 commits in v0.11.14..v0.11.15*

## v0.11.14 (2026-06-30)

### Features
- feat(roulette): last-10 rolls tracker + hottest-3 board highlight (`45e67ff`)
- feat(build-planner): group equipped mods by skill with color stripes (`c769ad0`)

---
*5 commits since v0.11.13*

## v0.11.13 (2026-06-30)

### Features
- feat(roulette): table-felt number board + true casino colors (`44864db`)

---
*4 commits since v0.11.12*

## v0.11.12 (2026-06-29)

### Features
- feat(roulette): casino roulette outcome tracker widget (`54a50c5`)

---
*4 commits since v0.11.11*

## v0.11.11 (2026-06-28)

### Features
- feat(db): user-data maintenance — purge big tables, compact, fix FTS triggers (`e5357db`)
- feat: add CSV import for words of power widget (`5f0cc6e`)

---
*24 commits since v0.11.10*

## v0.11.10 (2026-06-26)

### Fixes
- fix(crafting): move consume-buffer arrows outside the input box (`f8c5c26`)

---
*4 commits since v0.11.9*

## v0.11.9 (2026-06-26)

### Features
- feat(crafting): configurable consume-chance safety buffer on project materials (`2e33e05`)

---
*4 commits since v0.11.8*

## v0.11.8 (2026-06-26)

### Features
- feat(crafting): per-line clear button + live XP-buff recompute in leveling planner (`92a5788`)

---
*4 commits since v0.11.7*

## v0.11.7 (2026-06-26)

### Fixes
- fix(build-planner): apply Core/Nice/Epic attack & Signature Debuff damage to abilities (`8c6c250`)

---
*4 commits since v0.11.6*

## v0.11.6 (2026-06-25)

### Features
- feat(farming): add SQLite .db export/import for the drop-rate database (`50343e3`)

---
*5 commits since v0.11.5*

## v0.11.5 (2026-06-25)

### Fixes
- fix(build-planner): apply conditional mods, item bonuses, and DoT-add mods to ability stats (`3321d89`)

---
*4 commits since v0.11.4*

## v0.11.4 (2026-06-24)

### Features
- feat(farming): auto-start session logging with status light + checkbox (`efbd096`)

### Fixes
- fix(build-planner): show health/armor-specific damage components (`b5f9e3b`)

---
*5 commits since v0.11.3*

## v0.11.3 (2026-06-24)

### Fixes
- fix(build-planner): register direct-hit damage from HealthSpecificDamage (`0b1b7f9`)
- fix(build-planner): apply indirect-damage rule to every DoT component (`69a3f34`)
- fix(build-planner): DoT damage uses indirect modifiers only (`df944df`)

---
*7 commits since v0.11.2*

## v0.11.2 (2026-06-24)

### Features
- feat(build-planner): effective ability stats on hover + slot deselect toggle (`2cd8bfa`)
- feat(dashboard): live council-balance estimate anchored on last export (`4a72ac1`)

---
*5 commits since v0.11.1*

## v0.11.1 (2026-06-23)

### Features
- feat(build-planner): ability mod tooltips, collapsible sections, global mod search + apply (`01a3701`)

---
*5 commits since v0.11.0*

## v0.11.0 (2026-06-23)

### Features
- feat(build-planner): default combat-skill dropdowns in Set Defaults (`5ce1191`)
- feat(farming): serialized export filenames (charname-NNNN.csv) (`675af66`)
- feat(farming): periodic auto-save of active session for crash recovery (`368a312`)

### Fixes
- fix(dashboard): widget resize reflows neighbors via grid-column span (`6268972`)

---
*8 commits since v0.10.1*

## v0.10.1 (2026-06-22)

### Fixes
- fix(dashboard): correct widget resize max-width clamp and stray-click pin (`31fd90b`)

---
*3 commits since v0.10.0*

## v0.10.0 (2026-06-22)

### Features
- feat(dashboard): resizable widgets via right-edge X-axis drag (`0eb03a5`)

### Fixes
- fix(stall-tracker): deepen Trends chart palette for dark/HDR legibility (`445967b`)

---
*12 commits since v0.9.23*

## v0.9.23 (2026-06-22)

### Features
- feat(stall-tracker): add Trends tab with sales time-series chart (`6ba059b`)

---
*3 commits since v0.9.22*

## v0.9.22 (2026-06-21)

### Features
- feat(database): equipped combat-skill loadout filter + live selector (`909ef2b`)

---
*3 commits since v0.9.21*

## v0.9.21 (2026-06-21)

### Features
- feat(data-browser): add drop-rate % to enemy Harvest Stats (`1208f02`)
- feat(database): capture equipped combat-skill loadout per kill (migration v55) (`ba4389b`)
- feat(database): CSV import/export, friend-format raw-event import, zone-aware drop rates (`554e7e4`)

### Fixes
- fix: point changleog page at oddio repo. (`c867cf7`)

---
*7 commits since v0.9.20*

## v0.9.20 (2026-06-21)

### Features
- feat(database): list-first drop DB, permanent imports, harvest stats + app scale (`49c7544`)

---
*3 commits since v0.9.19*

## v0.9.19 (2026-06-21)

### Fixes
- fix(linux): disable WebKitGTK DMABUF renderer on all Linux, not just NVIDIA (`82d1dd2`)
- fix(linux): disable WebKitGTK DMABUF renderer on NVIDIA to stop native crash (`c6b43a2`)

---
*4 commits since v0.9.18*

## v0.9.18 (2026-06-21)

_Pre-convention history — all commits in range:_

- release: v0.9.18 (`8549d6f`)

---
*1 commits in v0.9.17..v0.9.18*

## v0.9.17 (2026-06-21)

### Features
- feat(vendors): Add historical council earnings tracker with two-tab widget (`bb91b0b`)
- feat(dashboard): Combat Wisdom tracker widget (`fd4763c`)

---
*5 commits since v0.9.16*

## v0.9.16 (2026-06-20)

### Features
- feat(farming): corpse-search drop-rate model, history drop-rate hover, butchering harvest detail (`03e33e8`)

---
*3 commits since v0.9.15*

## v0.9.15 (2026-06-20)

### Features
- feat(farming): split gathered boxes, Active/History parity, drop-rate Database tab (`39c22f6`)

---
*3 commits since v0.9.14*

## v0.9.14 (2026-06-20)

### Features
- feat(dashboard): drop high-favor Statehelm NPCs from gifting widget (`ffb6344`)
- feat(survey): chat-authoritative loot summary from [Status] log (`34160f8`)

---
*8 commits since v0.9.13*

## v0.9.13 (2026-06-20)

### Features
- feat(dashboard): skill-driven Statehelm gifting widget + gift-count fixes (`327be5a`)

---
*4 commits since v0.9.12*

## v0.9.12 (2026-06-20)

### Features
- feat(menubar): one-click live-tailing toggle for all logs (`aa26337`)
- feat(gourmand): track eaten foods live from Player.log (`b635038`)

---
*6 commits since v0.9.11*

## v0.9.11 (2026-06-20)

### Features
- feat(words-of-power): category/level grouping + CSV export (`ed7375e`)

---
*3 commits since v0.9.10*

## v0.9.10 (2026-06-19)

### Features
- feat(farming): looted-items + gathered columns with per-source drop rates (`824423b`)

---
*3 commits since v0.9.9*

## v0.9.9 (2026-06-19)

### Features
- feat(settings): path auto-detect buttons + startup auto-detect (`69523ff`)

### Fixes
- fix: parse signed 32-bit item instance IDs; update handoff (`5c49423`)

---
*3 commits since v0.9.8-exp*

## v0.9.8-exp (2026-06-19)

### Features
- feat: default file pickers to configured game data folders (`5e013a7`)
- feat: add Start Tailing button to Inventory tab warning banner (`7315f21`)
- feat: make MenuBar status dots clickable to toggle tailing (`6478c38`)

### Fixes
- fix: pin @tauri-apps/api & plugin-dialog to match Rust crate minors (`22fdd31`)
- fix: pin tauri-action to @v0, not @v1 (v1 tag does not exist) (`990df75`)
- fix: prevent Cmd+Q from being caught by tab cycling handler (`086239b`)
- fix: macOS Project Gorgon file path split — Player.log and game data are in separate locations (`ff38c98`)

---
*14 commits since v0.9.5*

## v0.9.5 (2026-06-18)

### Features
- feat: add Prodigy Tracker widget with manual current-XP baseline (`45cee9e`)
- feat: add XP Rate dashboard widget (combat + prodigy XP/hr & prodigy ETA) (`dfdacec`)

### Fixes
- fix: correct crafting leveling XP model (flat 4x first craft + repeat-craft drop-off) (`adc6565`)

---
*4 commits since v0.9.4*

## v0.9.4 (2026-06-18)

### Features
- feat: enable auto-updater with fork signing key (`d0fd6cd`)

### Fixes
- fix: apply static first-time bonus to leveling restore path (0.9.3.1) (`df4b9cc`)

---
*3 commits since v0.9.3*

## v0.9.3 (2026-06-18)

### Fixes
- fix: correct crafting leveling XP math (first-time bonus + synergy) (`1c22ae4`)

---
*4 commits since v0.9.2*

## v0.9.2 (2026-06-18)

### Features
- feat: v0.9.1 — Flatpak packaging for Linux (`4d5c2bd`)

### Fixes
- fix: Flatpak production build renders correctly; release v0.9.2 (`935d8ac`)

---
*4 commits since v0.9.0*

## v0.9.0 (2026-06-18)

### Features
- feat: oddio fork v0.9.0 — font/size settings, widget opacity, crafting XP scaling fix (`6a78d37`)

---
*1 commit since v0.8.7*

## v0.8.7 (2026-05-02)

### Fixes
- fix: bug with db migrations only had manual marking gourmand items working for fresh installs. (`4004675`)

---
*2 commits since v0.8.6*

## v0.8.6 (2026-05-01)

### Features
- feat: devtools: can now do a simple, non-persisted playback of debug captures to validate loot attribution stuff (`499f577`)

### Fixes
- fix: actually fixing the kill loot attribution bug i hope (`f74482f`)
- fix: synergies should work for multi-word skills now (`3086e19`)

---
*6 commits since v0.8.5*

## v0.8.5 (2026-05-01)

### Fixes
- fix: kill loot attribution and farming tracker missing new stacks (`b2e5941`)

---
*3 commits since v0.8.4*

## v0.8.4 (2026-05-01)

### Improvements
- impv: words of power effects are collapsable now (`ccc8c31`)

---
*6 commits since v0.8.3*

## v0.8.3 (2026-05-01)

_Pre-convention history — all commits in range:_

- release: v0.8.3 (`ca813fd`)
- Merge branch 'main' of https://github.com/danielout/glogger (`620ec79`)
- chore: prepping for swapping to singular repo (`e145506`)

---
*3 commits in v0.8.2..v0.8.3*

## v0.8.2 (2026-05-01)

### Features
- feat: enemy browser now uses player facing names from string table. (`0e1fb95`)
- feat: improved item info layout in data browser (`8032b1d`)
- feat: area browser with monster and npc info (`54fbd0c`)
- feat: downloading string data as well now as part of CDN retrieval (`e754dbe`)

### Improvements
- impv: consolidation workflow changes (`0dd7ad7`)

---
*7 commits since v0.8.1*

## v0.8.1 (2026-05-01)

### Features
- feat: added drop data to item views (`08914c4`)
- feat: add kills and drop rate info to data browser (`f79d6cf`)
- feat: kill and loot tracking (`ec133a6`)

### Fixes
- fix: making sure the widget and vendor details are communicating with each other. (`8eb8846`)

---
*6 commits since v0.8.0*

## v0.8.0 (2026-05-01)

### Features
- feat: vendor gold counter with multi-character aggregation support. (`2076f9b`)
- feat: hoplology widget (`369f99e`)
- feat: auto-detect item moves and add next-zone navigation to wizard (`06a2e3f`)
- feat: add storage consolidation tool (`2e5014c`)

### Improvements
- impv: dashboard card scrolling improvement. temp improvement, need to do a better job with how we think about this. (`77e6f34`)
- impv: consolidation workflow changes (`b4d00bf`)
- impv: consolidation workflow changes (`d73178c`)

### Fixes
- fix: time presets (`a28264f`)
- fix: item search was too sensitive about its cases (`3af97fe`)
- fix: wizard tracks carry bag to prevent premature dropoff instructions (`d5f78ee`)
- fix: redesign consolidation UI — vault cards in responsive grid (`ad7f1c6`)
- fix: handle portable vaults and missing zones in consolidation routing (`e39a9b8`)
- fix: consolidation route planner casinoPortal type mismatch (`469aaf2`)

---
*17 commits since v0.7.14-exp*

## v0.7.14-exp (2026-04-24)

### Fixes
- fix: correcting max card height (`63e234c`)

---
*3 commits since v0.7.13-exp*

## v0.7.13-exp (2026-04-24)

### Fixes
- fix: experimental build now experimental (`f046e0d`)

---
*3 commits since v0.7.12-exp*

## v0.7.12-exp (2026-04-24)

### Features
- feat: add backend persistence for timer widget (`8bf6e53`)
- feat: add bulk price setting for market values (`1a9a2f7`)
- feat: add seedling/plant/milling product chain linkage (`4ffaa45`)
- feat: add garden almanac widget (`b11671a`)
- feat: improve market price add UX with autocomplete and validation (`0184a3c`)
- feat: switch report detection from folder polling to chat log events (`662ae73`)
- feat: add recurrent event timer dashboard widget (`f9a48bf`)
- feat: add hoplology equipment study tracker (`b17e2a9`)
- feat: persist screen and sub-tab state across navigation (`a4611f4`)
- feat: add Show Totals mode and persist view settings in inventory (`e15d2b9`)
- feat: manual brewery recipe/discovery adding (`d4f0408`)
- feat: add "Show Totals" mode to storage view (`67184b4`)
- feat: add general-purpose timer dashboard widget (`254d6e4`)
- feat: manual food eaten/not-eaten marking for gourmand (`4df6afe`)
- feat: implement audio alerts for watchword matches (`c7dbc92`)
- feat: collapsible recipe list and split raw/intermediate materials (`350ed58`)
- feat: add enemy database to data browser (`4b7e4a1`)
- feat: make critical resources widget configurable (`6dd128d`)
- feat: add home zone setting for route planner (`87683b2`)
- feat: add area tooltips with NPC cross-reference (`325b3b3`)
- feat: add skeleton loading states to crafting project materials (`7698115`)
- feat: add shared DataTable, FilterBar, and Skeleton components (`e21b769`)
- feat: trip planner widget (and initial trip routing work) (`7df584c`)

### Fixes
- fix: Status channel messages with colons parsed as player chat (`c62fe9b`)
- fix: resolve all 19 Rust compiler warnings (`355bf42`)
- fix: correct survey session start/end timestamps (`e3d6fae`)
- fix: disable hoplology widget until book/report parsing is implemented (`5611707`)
- fix: changelog downloads section formatting and auto-collapse (`034e5c2`)
- fix: register toggle_food_eaten_status command in Tauri handler (`398dc88`)
- fix: add debug logging for rez counter and stop swallowing emit errors (`5f951db`)
- fix: hide stack size in item tooltips for non-stackable items (`48c5fa3`)
- fix: enforce uniform dashboard widget height and fix scroll patterns (`152c770`)
- fix: improve login inventory handling (`5b14738`)

---
*66 commits since v0.7.11*

## v0.7.11 (2026-04-21)

### Features
- feat: parse ProcessUpdateDescription for entity state changes (`3da0102`)
- feat: show learned/unlearned status in recipe tooltips (`1fa5953`)
- feat: show last import timestamps on character name hover (`e34c4bc`)

### Fixes
- fix: survey analytics time calculation was inflated by use count (`f84021a`)
- fix: food tooltip parsing on gourmand comparison panel (`ed3cde4`)
- fix: NPCs without an area now appear in search results (`e9f00d7`)
- fix: crafting planner restores first-time XP when entry is removed (`cb34247`)
- fix: crafting projects not properly calculating all intermediate crafts (`560ae88`)

---
*14 commits since v0.7.10*

## v0.7.10 (2026-04-21)

### Improvements
- impv: update notif moved to about (`de7f06f`)

---
*3 commits since v0.7.9*

## v0.7.9 (2026-04-21)

### Improvements
- impv: adding "self-milked" counter. sighing heavily. (`d7fc474`)

---
*2 commits since v0.7.8*

## v0.7.8 (2026-04-20)

### Improvements
- impv: about page tweaks (`eb96e93`)

---
*2 commits since v0.7.7*

## v0.7.7 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.7 (`460cc97`)
- build: pipe pipe pipe (`92911a1`)

---
*2 commits in v0.7.6..v0.7.7*

## v0.7.6 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.6 (`ee7a435`)
- Merge branch 'main' of https://github.com/danielout/glogger (`6b044e7`)
- build: fix the release script maybne (`40dbb25`)

---
*3 commits in v0.7.5..v0.7.6*

## v0.7.5 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.5 (`3e56d6c`)

---
*1 commits in v0.7.4..v0.7.5*

## v0.7.4 (2026-04-20)

### Features
- feat: version comparison tool in data browser (`f3a37ce`)

### Fixes
- fix: mac updater fix (`a03f6cd`)

---
*4 commits since v0.7.3*

## v0.7.3 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.3 (`7c878e4`)
- Merge branch 'main' of https://github.com/danielout/glogger (`ccd6e06`)
- build: fixing release script (`42273dd`)

---
*3 commits in v0.7.2..v0.7.3*

## v0.7.2 (2026-04-20)

_Pre-convention history — all commits in range:_

- release: v0.7.2 (`451e3dd`)
- Merge branch 'main' of https://github.com/danielout/glogger (`ad6f2bf`)
- fear: updater and new build process (`e0709d0`)

---
*3 commits in v0.7.1..v0.7.2*

## v0.7.1 (2026-04-20)

### Features
- feat: wop tracker (`b271f2b`)
- feat: GST launch and download button, and link to info (`4494a8d`)

### Improvements
- impv: hovering the moon gives you good moon info! (`dd86171`)

### Fixes
- fix: attribution (`4bb416e`)

---
*6 commits since v0.7.0*

## v0.7.0 (2026-04-20)

### Features
- feat: adding milking widget and stat tracker widget to dashboard (`0bcd8ec`)
- feat: character stats page now tracks all attributes, /age, and /behaviorreport - also initial batch of computed stats. (`c712d5b`)

### Improvements
- impv: update gourmand report just on log view, saving not required (`6f06aac`)
- impv: better log parsing, updating documentation, additional game state tracking (`0ae4bba`)

### Fixes
- fix: fixing dashboard flashing, hopefully (`2b52e27`)

---
*8 commits since v0.6.6*

## v0.6.6 (2026-04-20)

### Features
- feat: devtools capture lets you build a single file limited capture, stripped of player chat messages (`96752ee`)

### Improvements
- impv: devmode log captures are a little thinner, and truncate lines less. (`1c53d31`)
- impv: crafting project performance improvements. (`87918bf`)

### Fixes
- fix: fixed polling issue with chat logs that caused it to potentially sleep the js thread on windows. (`c14f97a`)
- fix: fixed bug with not all intermediate crafts resolving their materials correctly (`7280f22`)
- fix: build cleanup, snapshot favor fix, and crafting UX improvements (`161668c`)

---
*11 commits since v0.6.5*

## v0.6.5 (2026-04-18)

### Improvements
- impv: dedicated chat log search screen now with "from:USER" and "in:CHANNEL" (`fa2d762`)

### Fixes
- fix: survey data calculating incorrect session durations (`274f7cb`)
- fix: survey data overflowing its rightful space (`decec43`)

---
*5 commits since v0.6.4*

## v0.6.4 (2026-04-18)

### Improvements
- impv: making collapsed side panels much more noticable and clear they expand, i hope (`1dbd970`)

### Fixes
- fix: fixing where i broke ingesting skill updates (`39e93c8`)

---
*3 commits since v0.6.3*

## v0.6.3 (2026-04-18)

### Features
- feat(Brewery): merge brewing investigation and helper tools (`b473c25`)
- feat(Brewery): add "Try Next" suggestions for untried ingredient combos (`67f3dbc`)
- feat(Brewery): add effect search panel with material availability (`a2e67ac`)
- feat(Brewery): CSV import for brewing discoveries (`8313ec4`)
- feat(Brewery): add discovery journal from inventory snapshot scanning (`f3575b1`)
- feat(Brewery): add Brewery tab with CDN brewing data extraction (`dc6f21a`)

### Improvements
- impv(Brewery): support effect_desc column in CSV import for tooltip text (`8d750b9`)
- impv(Brewery): discovery table with fixed columns, owned counts, and delete (`e51d83e`)
- impv(Brewery): UI polish, flexible CSV import, export, and format help (`33c7030`)
- impv(Brewery): randomize Try Next suggestions within availability tiers (`2a45a5a`)
- impv(Brewery): remove duplicate text descriptions, add owned counts to ingredients (`d4c7b7c`)
- impv(Brewery): rework effect panel with resolved descriptions and center panel results (`c795bc4`)
- impv(Brewery): human-readable effect pool labels with descriptions (`18716b3`)
- impv(Brewery): resolve TSys power info for human-readable effect descriptions (`dc99fb3`)

### Fixes
- fix: fix up some minor warnings (`96e9449`)
- fix(Brewery): filter by Crafter field + auto-scan on inventory import (`b2d5c37`)

---
*19 commits since v0.6.2*

## v0.6.2 (2026-04-18)

### Features
- feat: add mushroom farming dashboard widget (`75b0e69`)
- feat: add teleport machine codes dashboard widget (`2602bb3`)
- feat: item parser 2.0 + surveys 2.0 (`7edd681`)

### Improvements
- impv: better batching and sleep controls for dual-log replays for development (`1ec25a1`)
- impv: show next moon phase instead of only full moon in mushroom widget (`fce2630`)
- impv: polish teleport codes and mushroom farming widgets (`bfd4a9d`)
- impv: cap zone NPCs widget height to prevent it dominating dashboard (`fb6e759`)
- impv: recipes widget now checks both inventory and storage (`b87a4b0`)
- impv: improve text contrast and bump base font size (`f1ad9f7`)
- impv: auto-expand NPC details panel when selecting an NPC (`2ab182f`)

### Fixes
- fix: forgot one piece of unhooking the tracker (`abdb612`)
- fix: recipe widget properly looks up recipe requirements (`3688959`)
- fix: right sidebar in the survey tracker starts expanded (`31f6dfc`)
- fix: projects now respect the sort rules of that list (`08aae15`)
- fix: removing the 'start tracking' button since it is broke as hell. hardly a fix, but added a todo to investigate and reimplement later (`d226e94`)
- fix: stampede correctly shows with proper ranks in build planner and browser fix: we now rebuild derived tables on version change. (`2fe3aa6`)
- fix: farming session timer handles midnight rollover (`a31399a`)
- fix: pricing calculator now uses market values with value*2 fallback (`bbd6a7c`)
- fix: ability families with mixed monster/player tiers no longer misclassified (`6616540`)
- fix: build planner name dropdown now gets its own line (`54f3fbc`)
- fix: tooltips now flip above/below to stay within viewport (`2bcb1e9`)

---
*24 commits since v0.6.1*

## v0.6.1 (2026-04-14)

### Features
- feat: '?' button now has useful information in it. moved and updated about, too. (`3fd7644`)
- feat: npc pages and widget v2.0 - now actually useful (`bbf33e1`)

### Improvements
- impv: semi-improvement. surveying tracker honesty about its current status now. (`c3454a1`)

### Fixes
- fix: small improvements for inventory tracking (`bf32dd7`)

---
*8 commits since v0.6.0*

## v0.6.0 (2026-04-14)

### Features
- feat(StallTracker): Phase 12 — polish (`0c2d435`)
- feat(StallTracker): Phase 11 — store-level race protection (`8eb7291`)
- feat(StallTracker): Phase 10 — Import / Export / Clear (`cf53f1a`)
- feat(StallTracker): custom DatePicker, replaces native date inputs (`e5ca778`)
- feat(StallTracker): Phase 9 — Shop Log modal (`c068e85`)
- feat(StallTracker): Phase 8 — Inventory tab with last-known-price (`9c8de45`)
- feat(StallTracker): Phase 7 — Revenue pivot tab (`bed2c23`)
- feat(StallTracker): Phase 6 — Sales tab (`33715fb`)
- feat(StallTracker): Phase 5 — Pinia store + parent view (`2d87d05`)
- feat(StallTracker): Phase 4 — Revenue + Inventory aggregations (`0e6900a`)
- feat(StallTracker): Phase 3 — Tauri CRUD commands (`df960e7`)
- feat(StallTracker): Phase 2 — live ingest via coordinator (`6a289e6`)
- feat(StallTracker): Phase 1 — parser, year resolver, migration (`eae0dd8`)
- feat: dev tools screen for testing (`c89e34f`)
- feat: lorebook databrowser (`c12bd40`)

### Fixes
- fix(DatePicker): sync visible month when modelValue changes externally (`65ecb60`)
- fix(StallTracker): clear filterTimer on tab unmount (`8a680dd`)
- fix(StallTracker): clear stale error on no-owner reload across all tabs (`5f1eae6`)
- fix(StallTracker): reset actionInProgress on parent view unmount (`ffc2099`)
- fix(StallTracker): wrap handleImport body in try/finally (`c9218a1`)
- fix(DataBrowser): replace nested buttons in sidebar list items (`f5e2ddc`)
- fix(StallTracker): drop unused backfill_year helper (`d2b6599`)
- fix(StallTracker): make Recently Sold Out table sortable (`9f72fec`)
- fix(StallTracker): render Revenue pivot items via ItemInline (`a60af64`)
- fix(StallTracker): make Shop Log table sortable (`29ec79a`)
- fix(StallTracker): make Inventory In Stock table sortable (`f987968`)
- fix(StallTracker): reverse Revenue period columns to newest-first (`2fafb42`)
- fix(StallTracker): render items via ItemInline for tooltips and navigation (`ba0b03b`)
- fix(StallTracker): extract xN quantity from stacked bought listings (`5c111ad`)
- fix(StallTracker): blur date inputs on outside click (`9c9c33c`)
- fix: fixed multi output recipes moving from level planner to projects fix: slightly improved item tracking feat: known issues listin ? menu (`635fe04`)
- fix: statehelm gift tracker doesn't require you to be on the page fix: better item name matching for gear fix: some tooltip clipping issues (`7f2bad4`)

---
*43 commits since v0.5.1*

## v0.5.1 (2026-04-13)

### Features
- feat: add string-based import/export for build planner sharing (`e0432e2`)
- feat: add ability to clone a build in the build planner (`9fc51ef`)
- feat: add sort order toggle for chat logs (newest/oldest first) (`798c08f`)
- feat: add 'Jump to Present' button in chat logs (`049d269`)

### Fixes
- fix: removing the half-complete test that obviously doesn't pass (`e35907a`)
- fix: updating survey tests - they are still bad, but better bad (`b68c669`)
- fix: stupid arrows and their float value. blame archers (`492b06c`)
- fix: yet another attempt to correct survey reward logging (`f4137c9`)
- fix: prevent survey session double-counting on zone change (`f476d05`)
- fix: if you have no builds, now you can create one. (`617fee7`)

---
*12 commits since v0.5.0*

## v0.5.0 (2026-04-11)

### Features
- feat: Pricing Calculator for Projects (`befd646`)

### Improvements
- impv: improved build planner  functionality with CPs and belts, item selection (`bd4371c`)
- impv: build planner layout rework (`23dd02e`)
- impv: clicking a watchword alert on the dashboard takes you to the watchwords page (`ba9eae5`)

### Fixes
- fix: cleaned up some rust warnings (`934c5e7`)
- fix: unbreaking survey data logging due to ring bonuses (`28f4014`)

---
*11 commits since v0.4.8*

## v0.4.8 (2026-04-09)

### Improvements
- impv: small dashboard layout tweaks (`4d7e2b9`)
- impv: status widget more useful with more info. better layout, too (`4613846`)
- impv: max height for dashboard cards before they get an internal scroll (`a76a740`)
- impv: Character->Skills view now lists what bonus levels you have achieved, has a better layout, and provides more info (`7516f61`)
- impv: Better patch notes organization (`5ffef78`)

### Fixes
- fix: dashboard widget drag and drop (`01d0d33`)
- fix: in specific circumstances we were double-counting bonus levels (`e8b3177`)
- fix: date lines on old chat messages don't cause ugly extra spacing anymore. (`a4d1b93`)
- fix: deletion confirmation not waiting for confirmation (thanks rey!) (`c4939fb`)

---
*18 commits since v0.4.7*

## v0.4.7 (2026-04-07)

### Fixes
- fix: market price list didn't scroll (`1c4dd53`)

---
*5 commits since v0.4.6*

## v0.4.6 (2026-04-07)

### Fixes
- fix: test fix to bypass false failure (`7bd4f26`)

---
*3 commits since v0.4.5*

## v0.4.5 (2026-04-06)

_Pre-convention history — all commits in range:_

- release: v0.4.5 (`e0ccb70`)
- bad text on npc browser (`bcfc6a3`)
- gourmand screen scroll fix (`9ef266a`)
- multi-character fix (`b9fcca7`)

---
*4 commits in v0.4.4..v0.4.5*

## v0.4.4 (2026-04-06)

_Pre-convention history — all commits in range:_

- release: v0.4.4 (`33d02ba`)
- statehelm rep tracker (`fcc3570`)
- survey analytics improvements (`bfec7fd`)
- crafting views now properly support dynamic slots, and valid daynamic slot items can be set by players (`c0e7e86`)
- Merge branch 'main' of https://github.com/danielout/glogger (`18404fe`)
- survey and analytics fixes (`d263a9d`)

---
*6 commits in v0.4.3..v0.4.4*

## v0.4.3 (2026-04-03)

_Pre-convention history — all commits in range:_

- release: v0.4.3 (`e8ed4f0`)
- fixing survey summary historical data (`0552339`)
- documenting our NPC vendor stuff (`37e36db`)
- market prices made the app think it was vendor buyable due to legacy code. aka last week me is dumb (`624053c`)
- Properly displaying recipes with proto items in the crafting skills display. (`2d914a8`)

---
*5 commits in v0.4.2..v0.4.3*

## v0.4.2 (2026-04-03)

_Pre-convention history — all commits in range:_

- release: v0.4.2 (`4bbcb88`)
- fixing silly build error (`a2060fe`)
- more workflow updates. i'm dumb and can't read (`a543909`)

---
*3 commits in v0.4.1..v0.4.2*

## v0.4.1 (2026-04-03)

_Pre-convention history — all commits in range:_

- release: v0.4.1 (`ddb4ed4`)
- Updating workflow from v4 to v5 (`8bcdc62`)
- Tackling assorted TODO list items (`5726f1a`)
- surveying analytics improvements (`ddd00bb`)
- Merge branch 'main' of https://github.com/danielout/glogger (`07902f1`)
- timestamp standardization (`5fdfb9d`)

---
*6 commits in v0.4.0..v0.4.1*

## v0.4.0 (2026-04-02)

_Pre-convention history — all commits in range:_

- release: v0.4.0 (`5af850c`)
- Layout updates (`1bb2367`)
- NEW FEATURE: Crafting Skill Reports (`98ad97f`)
- Layout/UX improvements (`d27b38f`)
- first step towards cross-referenced data browsing (`dab6a58`)
- data browser and layout bugfixing (`8701a75`)
- projtects tab improvements. groups, intermediates. (`b88669b`)
- startup order refinement (`ace780b`)
- titlebar version updating not automagical (`1d0c4d3`)

---
*9 commits in v0.3.1..v0.4.0*

## v0.3.1 (2026-03-31)

_Pre-convention history — all commits in range:_

- release: v0.3.1 (`80a4087`)
- build planner bugs. joy. (`b189594`)

---
*2 commits in v0.3.0..v0.3.1*

## v0.3.0 (2026-03-30)

_Pre-convention history — all commits in range:_

- release: v0.3.0 (`985bc73`)
- NEW FEATURE: Build Planner (`f799992`)
- Fixing a bug where monsters that ate flesh lumps couldn't import their gourmand report, WOGAN. (`3033f36`)
- layout improvement to crafting levelling planning (`31b653d`)
- fixing up our release pipeline so i don't have to nuke my real data for testing (`4d0dcd6`)
- build planner initial work (`53b6f35`)
- dev/realease directory breakout and window -state plugin (`d2c00fe`)
- fixing bug with first craft xp being modified (`463e603`)
- documentation consolidation and organization (`52b4bea`)
- small fix to inventory tracking - updating gamestate from json (`dfebd7f`)
- Merge branch 'main' of https://github.com/danielout/glogger (`1173ebe`)
- fixing the crafting level planner to use the right xp (base level vs total) standardizing how handle level across the app (`62befc4`)
- better organizing and breaking out docs for my sanity. (`8e98b1e`)
- haha let's stop murdering people's databases (`69b431f`)

---
*14 commits in v0.2.2..v0.3.0*

## v0.2.2 (2026-03-29)

_Pre-convention history — all commits in range:_

- release: v0.2.2 (`2ec7401`)
- fixing a bug with Cook's Help Brought To You By ReyDash (`8d3a3c9`)
- moving gourmand doc slight ui tweaks for the crafting levelling tab. (`192413e`)
- Merge branch 'main' of https://github.com/danielout/glogger (`2dd7d25`)
- NPC and Quest screens for character (`abe86f7`)

---
*5 commits in v0.2.1..v0.2.2*

## v0.2.1 (2026-03-28)

_Pre-convention history — all commits in range:_

- release: v0.2.1 (`e681e90`)
- new skills page that hopefully is more useful! (`e647a38`)
- fixed an issue with file position watchers and reactive components (`09bc459`)

---
*3 commits in v0.2.0..v0.2.1*

## v0.2.0 (2026-03-27)

_Pre-convention history — all commits in range:_

- release: v0.2.0 (`efcb3d1`)
- prep for alpha v0.2.0 (`e501377`)
- prototype-v0.1.1 (`f22ef7a`)
- doc shuffle (`9c19c38`)
- prototype-v0.1.0 (`a633b4f`)
- adding player export samples (`add51ed`)
- adding data browser (`1b04bfe`)
- JSON sample files for easier browsing than full files (`74022a1`)
- Breaking out our CDN importing in to specialty files. (`09c0a00`)
- basic survey setup (`d3358ab`)
- breaking out things in to smaller components, refined skill view. (`05bf679`)
- barebones testing functional (`917d75c`)
- initial commit (`98f6efb`)
- Initial Project Scaffold (`1f76811`)

---
*14 commits in v0.2.0*

