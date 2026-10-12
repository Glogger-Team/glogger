# glogger — Session Handoff

**Date:** 2026-10-11 (Session 37 — release pipeline: Windows + macOS + Linux + Flatpak)
**Machine:** Windows 11 (primary dev box)
**Branch:** `main` — committed + pushed (`924f609`, `be03489`, `520271e`, `0424340`, `cf776c5`, `44cbc0d`, `26ab6f3`, `d8cee16` + docs). No Rust/frontend behavior change.
**Status:** ✅ Published. Run **38186878765** (tag `v1.1.0` re-pointed at the fixed `main`) went fully green: validate → 3 platform builds → publish → flatpak attach; release v1.1.0 is Live/`Latest` with 9 assets and the channel manifest lists all three platforms. Earlier dry runs: **38180252724** (Release, `publish=false` — `artifacts-windows` 8.3 MB / `artifacts-macos` 25.0 MB / `artifacts-linux` 98.8 MB) and **38181296027** (Flatpak, `glogger.flatpak` 8.90 MB). Locally: both workflows parse, every `run:` block `bash -n` clean, and the collect / version-probe / latest.json / release-notes blocks were executed against fake bundle trees under `set -euo pipefail`. `npm run build` clean after the npm dep bump.

## TL;DR — Session 37

**User question:** "can we make at least all the ones upstream glogger ships?" — Answer: we weren't. The fork's
workflow shipped **one** artifact (Windows NSIS `.exe`), a regression against upstream (`danielout/glogger`
v0.8.7 and `crisp-oddio/glogger-oddio` v0.12.18 both ship Windows + macOS `dmg`/`app.tar.gz` + Linux
`deb`/`AppImage` + `glogger.flatpak`, with `latest.json` carrying three platform keys). Now we ship the same set.

### Blockers found on the way (all fixed)

1. **`v1.1.0` was never released.** Run 38178378091 died in *Build release installer*: tauri-cli's version gate
   refuses to build when the npm packages and the Rust crates differ in major.minor (`tauri` 2.12.1 vs
   `@tauri-apps/api` 2.11.1, updater 2.13.1 vs 2.10.1, …) — a side effect of `16697fe` bumping the cargo side only.
   `gh release list` still shows v1.0.3 as latest. Fixed by aligning the six npm packages (`be03489`);
   `npx tauri info` now reports no mismatch. **That tag's tree could not be rebuilt** — it predates the fix —
   so the tag itself was re-pointed at the fixed commit (see *Outcome* below).
2. **Runner `pipefail`.** `find … | head -40` SIGPIPEs under the runner's `bash -eo pipefail` and killed the linux
   collect step (38178973878). Replaced with `find -print -quit` / `|| true`; the `sed | head -1` version probe
   became an exiting `awk` (`520271e`).
3. **Flatpak, three iterations:** actions/checkout leaves a tag as a detached commit with **no local `refs/tags`
   entry**, and `git` inside the gnome-50 builder container cannot reach `origin` at all ("origin does not appear
   to be a git repository") → the tag check became a shape check, with the `v<semver>` rule enforced only on the
   publishing path so a `publish=false` dispatch can build any ref (`cf776c5`, `26ab6f3`). Then the real failure:
   the GNOME SDK exports `CFLAGS` containing `-O2`, which cc-rs appends **after** aws-lc-sys's own `-O0`, so
   `jitterentropy-base.c` hard-`#error`s ("must not be compiled with optimizations") — cleared
   `CFLAGS`/`CXXFLAGS` for the flatpak module (`d8cee16`).

### What the pipeline does now (`.github/workflows/release.yml`)

- `validate` (ubuntu): `npm ci` → `npm run build` → resolve version and **fail when a pushed tag ≠
  `tauri.conf.json` version**.
- `build` matrix, each job running `cargo test --lib` on its own OS before bundling:
  `windows-latest` (nsis) · `macos-latest --target aarch64-apple-darwin` (dmg + app.tar.gz) ·
  `ubuntu-latest` (deb + AppImage). Per-platform collect asserts every `.sig` exists (an unsigned/partial
  release cannot be published).
- `publish` (skipped on a dry run): jq-generated `latest.json` with `windows-x86_64` + `darwin-aarch64` +
  `linux-x86_64`, GitHub Release with a download table, then the `updater-latest` pointer refresh.
- `flatpak`: `workflow_call` into `.github/workflows/flatpak.yml` after publish (no `push: tags` trigger, so the
  tag pushed by `npm run release` cannot race the release workflow); the bundle is attached to the release.
- Dry run: `gh workflow run release.yml -f ref=<tag|main> -f publish=false`.

### Not touched / known limits

- macOS builds are **unsigned** (no Apple certificate, same as upstream) → right-click → Open; **Apple silicon
  only** (Intel Macs have no artifact, upstream parity). Flatpak has no updater entry. Windows ARM64 unsupported.
- No Rust or frontend behavior changed, so the `cargo test --lib` gate on this box was not re-run for code reasons —
  the three CI jobs ran it on all three OSes and passed.
- `%APPDATA%\glogger.Release` untouched; the local Windows installer path (`npm run tauri:build`) is unchanged.

### Outcome — v1.1.0 published (2026-10-12)

Per the user's call, the **v1.1.0 tag was re-pointed** from `36d0117` (unbuildable: predates the npm/crate
alignment) to the fixed `main` (`13998ed`) via `git push origin :refs/tags/v1.1.0` + `git push origin
refs/tags/v1.1.0` — no version bump, so `tag == tauri.conf.json` still passes the `validate` gate. The tag push
triggered run **38186878765**, which went fully green (validate → 3 platform builds → publish → flatpak
`workflow_call`).

Release <https://github.com/cprivitere/glogger-twinkleoftoes/releases/tag/v1.1.0> (marked Latest) now carries
upstream's full asset set: `windows-setup.exe`(+`.sig`) 8.28 MB · `macos.dmg` 13.1 MB · `macos.app.tar.gz`
(+`.sig`) 12.1 MB · `linux.deb` 12.5 MB · `linux.AppImage`(+`.sig`) 87.1 MB · `glogger.flatpak` 8.89 MB. The
`updater-latest` channel's `latest.json` reports version `1.1.0` with `windows-x86_64`, `darwin-aarch64` and
`linux-x86_64` entries (real 404/416/420-byte signatures), so installed builds auto-update to the new release.

A rebuilt tag is not covered by CI's `validate` job (it only compares the tag name to the config version), so
re-pointing a tag whose build failed should be done deliberately — the old run 38178378091 stays in history
against the stale commit.

---

# glogger — Session Handoff

**Date:** 2026-10-11 (Session 36 — Gourmand instant-snacks invisible: CDN category-token drift)
**Machine:** Windows 11 (primary dev box)
**Branch:** `main` — local change, not committed (backend parser + migration v70 + tests + Gourmand unknown-category bucket + docs).
**Status:** ✅ `cargo test --lib` **647 passed** (5 ignored); `npm run build` (vue-tsc + vite) clean. Legacy upgrade smoke-tested through the real `run_migrations` on a copy of the dev DB (schema 69 → 70; 57 `'Instant Snack'` rows → `'Instant-Snack'`; 606/606 foods match the UI's categories). Unknown-category bucket proven by a throwaway SSR render of `FoodCategorySection` (header, hint naming both tokens, raw token per card) — no live GUI session.

## TL;DR — Session 36

**User question:** are foods present in the CDN but missing from glogger's display?

**Answer — yes, 57 of 606.** CDN 486 builds the food category from the
`Food_Effect_Level_InstantSnack` string (`"Level {0} Instant Snack"`, **space**), while
`gourmandStore.ts` filters on the hyphenated `'Instant-Snack'`. So the Instant-Snacks section,
its progress bar, and its favourites were permanently empty — no import or watcher could change
that. The `foods` table itself was complete: all 606 live `food_desc` items insert, 0 unparseable.

### What shipped
- `src-tauri/src/db/cdn_persistence.rs` — `parse_food_desc` routes the category through the new
  `canonical_food_category`: case- and `-`/`_`/space-insensitive, mapping to `Meal` / `Snack` /
  `Instant-Snack`; unknown categories pass through verbatim.
- `src-tauri/src/db/migrations.rs` — **migration v70** rewrites stored instant-snack rows in place
  (deliberately does *not* clear `cdn_version`, unlike v67 — no CDN re-download needed). Idempotent.
- Tests — `cdn_persistence` parse cases (both spellings, unknown passthrough, malformed input, known-set
  round-trip); `migrations` v70 case (space/case variants → `Instant-Snack`, `Meal` + unknown untouched, re-run no-op).
- **Unknown-category bucket** (follow-up to the same class of bug) — `isKnownFoodCategory` +
  `KNOWN_FOOD_CATEGORIES` in `src/types/gourmand.ts`; `unknownCategoryFoods` computed in
  `gourmandStore.ts`; a hidden-when-empty "Unknown Categories" section in `GourmandView.vue` whose hint
  names the offending token(s) (`FoodCategorySection` gained an optional `hint` prop). Ingestion logs
  `foods: unrecognised food category …` for the same signal. `insert_foods`'s known-set check is
  covered by `canonicalisation_covers_every_known_category`.
- Docs — `character-gourmand.md` data flow + schema row + "Unknown-category bucket" UI bullet; `docs/TODO.md` entry corrected; `AGENTS.md` migration head v67 → v70 (was already stale at 69); `run_migrations_is_idempotent_and_atomic` head assertion 69 → 70.

### Gotcha (durable)
- CDN `FoodDesc` category tokens are **not stable across versions** — normalise separators/case
  before comparing, never match a literal, and backfill the DB when the game changes one.
  Name-based joins can't catch this class of bug: every eaten entry still resolves to a `foods`
  row (0 orphans), so only the UI's *filter literal* goes dead. The 2026-10-03 TODO diagnosis
  ("stale import") was wrong for the same reason.
- Deliberately *not* foods: 8 items carry `Edible`/`Food`/`Snack` + `PreparedFood=` + `UseVerb: Eat`
  but no `FoodDesc` — the `_Iocaine` poison variants (ids 5409, 6401–6407, crafted by Mycology
  "Poisoned X" recipes). They give no Gourmand XP so they stay out of `foods`, and their names
  collide with the real dishes, so e.g. an eaten-report line "Glazed Beets" is the normal food.

---

**Date:** 2026-07-11 (Session 35 — leveling planner multi-yield materials fix)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — fix `7b665b7` + merge `c6bf73e` (origin/main v0.11.32 baseline), pushed.
**Status:** ✅ `vue-tsc --noEmit` clean (pre- and post-merge). Frontend-only change, no Rust touched. Verified live in `tauri dev` (user click-tested the leveling tab, "looks good"). Release v0.11.33 dispatched — see below.

## TL;DR — Session 35

**User report:** recipes yielding multiple items per craft understated materials in project planning — e.g. 5 planned crafts of Toasted Spriggan Nuts (2/craft) showed only 3 crafts' worth of materials (6 Spriggan Nuts instead of 10).

**Root cause (units mismatch, one branch only):** `resolveRecipeIngredients` in [craftingStore.ts](src/stores/craftingStore.ts) takes a desired *output-item* quantity and derives crafts via `ceil(desired / (yield × chance))`. The **Leveling tab's materials panel** passed its plan *craft counts* straight into that parameter ([LevelingTab.vue](src/components/Crafting/LevelingTab.vue) `recomputeMaterials`), so multi-yield recipes got divided by yield a second time.

### What shipped (`7b665b7`)
- **[craftingStore.ts](src/stores/craftingStore.ts)** — new optional `quantityIsCrafts` param (7th, positional) on `resolveRecipeIngredients`: uses the count as the craft count directly. Ingredient-tree cache key gets a `c` marker so craft-count and item-count requests can't collide; `desired_quantity` in the result stays in item units (`craftCount × outputPerCraft` in crafts mode).
- **[LevelingTab.vue](src/components/Crafting/LevelingTab.vue)** — `recomputeMaterials` resolves with `quantityIsCrafts=true`.

### Audit — every other resolver caller passes item quantities correctly
Projects tab (single + group resolve), ProjectEntryCard (expand/cost), project-needs index, stock targets (shortfall in items), QuickCalc (explicit crafts↔items round-trip UI), work orders→project (order qty = items), Cook's Helper & Brewery combos (qty 1, benign), live tracker (items target, crafts derived via `output_per_craft`). `createProjectFromLevelingPlan` already multiplied crafts × yield when creating entries — the created project's materials now agree exactly with the leveling panel. Rust backend is pure CRUD, no yield math.

### Gotchas / notes
- Local-only `dev-detached.cmd` (untracked, machine-specific) + `glogger-dev` scheduled task = MSIX-safe detached `tauri dev` launch (see `project_msix_sandbox_gotcha` memory). `schtasks /Run /TN glogger-dev`, log at `dev-detached.log`.
- Another session merged [PR #88](https://github.com/crisp-oddio/glogger-oddio/pull/88) (release PR no longer suppressed by old MERGED PRs) and shipped v0.11.32 earlier today; dev was fast-forwarded to pick that up before this session's merge of main.
- Deleted the dead `origin/release/v0.11.31` branch (Session 34's leftover from the force-push incident; no tag, no open PR). Every remaining `release/*` branch matches a published tag.

### Release v0.11.33 — ✅ PUBLISHED (all installers + Flatpak, marked Latest)
- Dispatched `release.yml --ref dev -f version=patch` from dev at `c6bf73e` (0.11.32 baseline → 0.11.33; no stale `release/v0.11.33` branch existed).
- Release PR **#90 opened automatically** — first release proving the PR #88 fix (no manual `gh pr create` needed). Merged → Release Publish shipped Windows setup.exe, macOS dmg/app.tar.gz, Linux deb/AppImage, `glogger.flatpak`, and `latest.json` in one run (Flatpak is part of release-publish.yml now, no separate tag-triggered wait).

---

**Date:** 2026-07-10 (Session 34 — Words of Power widget: auto-remove spoken words)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — committed `8cf9888` on top of `5cba37a`, pushed.
**Status:** ✅ `cargo test --lib` **504 pass** (5 new: 2 parser + 3 delete-helper), `cargo check --lib` clean, no frontend changes. Not click-tested live (needs an in-game word use); verified offline against real data — see below.

## TL;DR — Session 34

**User request:** using a word of power in-game didn't remove it from the Words of Power widget — dead words lingered forever. Now the widget monitors chat for the player speaking a word and deletes it from the saved set.

**The signal (established from real logs):** speaking a word writes exactly one line to Chat.log:
`[Status] You use the word of power WORD!` — 111 instances across the user's logs, all uppercase, no spaces, no case variance, and **no failure/variant lines exist** (checked). Words are random long strings, so matching by word text alone is collision-free.

### What shipped (`8cf9888`)
- **[chat_status_parser.rs](src-tauri/src/chat_status_parser.rs)** — new `ChatStatusEvent::WordOfPowerUsed { timestamp, word }` + `try_word_of_power_used` (prefix/suffix strip, same style as roulette).
- **[words_of_power_commands.rs](src-tauri/src/db/words_of_power_commands.rs)** — `delete_words_by_word` (`TRIM(word) = TRIM(?1) COLLATE NOCASE` — covers manual lowercase/whitespace entries; deletes across ALL characters/servers since a spoken word is consumed game-wide) + `backfill_used_words_from_chat_logs` (mirrors the roulette backfill, but only reads `Chat-*.log` files dated ≥ earliest saved `discovered_at` − 1 day of TZ slack, and short-circuits to 0 work when no words are saved) + registered `backfill_words_of_power_usage` command wrapper (unused by frontend, matches the roulette/combat-wisdom precedent).
- **[coordinator.rs](src-tauri/src/coordinator.rs)** — `WordOfPowerUsed` arm in the `process_chat_events` status match: delete + emit `game-state-updated ["words_of_power"]` only when rows were actually removed. Idempotent under chat catch-up replay (re-deleting is a no-op).
- **[lib.rs](src-tauri/src/lib.rs)** — Step **5d-iii** startup one-shot backfill (async spawn, like 5d/5d-ii) so words spoken while glogger was closed get removed; emits the same domain event on n>0 since the widget may already be mounted.
- **Widget: zero frontend changes** — it already reloads on `game-state-updated` containing `words_of_power`.
- Docs: [widget-words-of-power.md](docs/features/screens/dashboard/widget-words-of-power.md) updated (auto-removal feature + on-use data flow).

### Verification (offline — strongest available without an in-game word use)
- Parser tested against the exact real line; full `cargo test --lib` 504 pass.
- **Real-data cross-check:** the Release DB (`%APPDATA%\glogger.Release\glogger.db`, the daily-driver build) had **56 saved words, 4 of which were already spoken** (e.g. `KACHZAVPLITQUAYZAOBLIYEAO` / Elemental Immunity, used 26-07-02) — exactly the reported bug. The startup backfill will remove those 4 on the next Release-build launch with this code. The **dev DB has 0 saved words**, so a `tauri dev` run won't visibly demonstrate removal — live confirmation needs the portable/release build or a fresh discovery+use in dev.
- **Remaining manual step:** speak a word in-game with glogger running and watch it vanish from the widget.

### Gotchas hit
- PowerShell 5.1 mangles `git commit -m` here-strings containing double quotes (native-arg re-quoting splits the message into pathspecs) — use `git commit -F <file>`.
- **Force-push incident on main (2026-07-09, discovered this session):** an accidental `git push --force` from a stale clone wiped `main` to June-22 state; PR #83 (superness) restored it to `769b4cc` (v0.11.29). Casualties: PR #79 (release v0.11.30), PR #82 (release v0.11.31), and PR #81 (GNOME-50 flatpak fix) were all "merged" into the doomed main and published nothing — no v0.11.30/31 tags exist. The GNOME-50 fix only survives on dev (`c01c0ff`).
- **Release workflow idempotency bug:** re-dispatching release.yml for a version whose old PR was merged-but-lost silently skips `gh pr create` (`gh pr view` matches MERGED PRs, prints "PR already open"). The branch still gets force-updated; the PR must be opened manually.

### Release v0.11.30 — ✅ PUBLISHED (all installers + Flatpak attached)
- Merged `origin/main` into dev (`69a112a`, clean) — synced v0.11.29 version baseline + kaeus's survey perf fix (`92850dc`); 505 tests + vue-tsc green post-merge.
- Dispatched `release.yml --ref dev -f version=patch` → validate green, branch `release/v0.11.30` regenerated (`671d9c0`). PR creation was skipped (bug above) → opened [PR #85](https://github.com/crisp-oddio/glogger-oddio/pull/85) manually; user merged it → Release Publish tagged v0.11.30 and shipped Windows/macOS/Linux installers.
- **Flatpak job failed on first run (exit 127):** on the `flathub-infra:gnome-50` builder image, `pip install --quiet git+…#subdirectory=node` exits 0 but never creates the `flatpak-node-generator` console script (base-distro script-path difference vs the old `bilelmoussaoui:gnome-47` image; `--quiet` hid it). **Fix (`5994ccb` on dev):** invoke `python -m flatpak_node_generator` (module path is image-independent), split/unsilence the pip installs, and sanity-check the module in the install step.
- Re-dispatched `flatpak.yml --ref dev -f tag=v0.11.30` (dev workflow + tag content) → **GNOME 50 build green in 8m46s, `glogger.flatpak` attached to the release.** Session 32's runtime bump is now CI-verified; no GNOME-49 fallback needed.
- Stale leftover: `origin/release/v0.11.31` branch (dead, from the incident) can be deleted.

---

**Date:** 2026-07-09/10 (Session 33 — 3D Model Viewer REMOVED from glogger; now a standalone app)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — **history rewritten + force-pushed** (user-approved): the three model-viewer commits (`89c207f`, `ce5a7b8`, `0f08746`) were dropped via `rebase --onto`; the Flatpak GNOME-50 fix and Import Equipped were kept. `dev` = `origin/dev` = `8a1ee73`. `main` never had the feature.
**Status:** ✅ glogger tree clean; rewritten source identical to `8f34810` (a released, known-good state).

## TL;DR — Session 33

Per the user's direction, the entire 3D Model Viewer was **extracted into its own app and purged from glogger's history**:

- **New home: [github.com/crisp-oddio/pg-model-viewer](https://github.com/crisp-oddio/pg-model-viewer)** (public), local at `A:\Claude\pg-model-viewer`. Standalone Tauri 2 + Vue 3 app with its own lean PG-CDN item layer; releases **v0.1.1** and **v0.1.2** shipped with Windows/macOS/Linux installers (frozen Python extractor sidecar bundled). See that repo's `HANDOFF.md` + auto-memory `project_model_viewer.md` for everything model-viewer related from now on.
- **glogger:** no `model_assets.rs`, no `ModelViewer/` components, no `three` dep, no `EquipAppearance` fields on `ItemInfo` — the surface is exactly pre-feature. Session 31/32's handoff entries describing the feature were dropped with the commits (this entry is the record). Anyone with an old clone of `dev` must hard-reset to `origin/dev`.
- Removed leftovers: the untracked `src-tauri/binaries/` sidecar dir; the (local-only) recovery tag `archive/model-viewer-20260709` was deleted after the standalone was verified.

**Next session (glogger):** merge PR #81 + release v0.11.30 (the Flatpak GNOME-50 fix) is still the open item from Session 32.

---

**Date:** 2026-07-08 (Session 32 — hotfix: Flatpak GNOME 47 runtime EOL)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` at `9db9bb6` (pushed). Same fix cherry-picked to `hotfix/flatpak-gnome-50` (`78328ef`, off `origin/main`) → **PR #81 open, not yet merged** — this is the release path.
**Status:** ✅ Fix committed + pushed on both branches; **CI-unverified** — the GNOME 50 build is only provable by the Flatpak workflow on the next release run.

## TL;DR — Session 32

A user reported the v0.11.29 Flatpak warning `runtime org.gnome.Platform branch 47 is end-of-life` (EOL'd 2025-10-15; GNOME **48** also died 2026-03-24 — we were two bumps behind; EOL runtimes can even fail to install for new users). Bumped to **GNOME 50** (current, supported ~March 2027; 49 EOLs ~Sept 2026):

- [flatpak/io.github.crisp_oddio.glogger.yml](flatpak/io.github.crisp_oddio.glogger.yml) — `runtime-version: '50'`.
- [.github/workflows/flatpak.yml](.github/workflows/flatpak.yml) — builder image → `ghcr.io/flathub-infra/flatpak-github-actions:gnome-50`; the old `bilelmoussaoui/…` registry path is dead (no new runtime tags) — the rename was mandatory.
- [docs/flatpak-build.md](docs/flatpak-build.md) — local-build runtime `//50`, SDK extensions `//25.08` (freedesktop base under GNOME 49/50), + note on the yearly EOL cadence: next bump must touch all three files together. Manifest `sdk-extensions` are unversioned — flatpak-builder resolves the matching branch automatically.

### Gotchas / open ends
- **v0.11.29 was NOT actually removed from GitHub** — release still exists, marked *Latest*, still serving the GNOME-47 flatpak; tag still on origin. (**v0.11.28** is the one that's missing.) Deliberately left untouched. To remove: `gh release delete v0.11.29 --repo crisp-oddio/glogger-oddio` + `git push origin :refs/tags/v0.11.29`.
- **Re-running the Flatpak workflow against v0.11.29 cannot fix it** — the workflow checks out the *tag*, which contains the old manifest. The fix only ships via a new tag: **merge PR #81 → trigger Release workflow (patch → v0.11.30)**.
- If GNOME 50's webkitgtk misbehaves in CI, the fallback is `runtime-version: '49'` + the `gnome-49` image.

**Next session:** merge #81 + release v0.11.30.

---

**Date:** 2026-07-07 (Session 30 — Quest tabs, uncompleted work-order backlog + board tracking, crafting delete fix)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — committed `6e99705`, pushed to `origin/dev` (on top of `31c23be`).
**Status:** ✅ `vue-tsc --noEmit` clean; `cargo check --lib` + `cargo test --lib character` clean. Verified
live in `npm run tauri dev` (oddio@Arisetsu): the new `completed` category populated **323 rows** in the DB
on re-import (matches the export). `src-tauri/Cargo.toml` LF↔CRLF noise left uncommitted as usual.

## TL;DR — Session 30

Three things, all around the Character → Quests screen and the crafting-project system.

### 1. Completed quests + tabbed Quest screen
The VIP character export (`Reports/Character_*.json`) gained a new top-level **`CompletedQuests`** array
(flat quest internal-names, same shape as `ActiveQuests`). It's **optional** — only newer game clients write
it, so characters not re-exported since the update omit it (parsed with `#[serde(default)]`).
- **Backend** ([character_commands.rs](src-tauri/src/db/character_commands.rs)): `CharacterReport.completed_quests`
  + insert loop storing each with **`category='completed'`**. **No migration** — `character_active_quests.category`
  already existed and `get_snapshot_active_quests` returns every category. Comment updated in
  [migrations.rs](src-tauri/src/db/migrations.rs).
- **Frontend** ([QuestsScreen.vue](src/components/Character/QuestsScreen.vue)): reworked into **5 tabs** —
  Active / Work Orders / **Uncompleted WOs** / Uncompleted / Completed. Quest details now resolve from the
  cached full quest list (`gameData.allQuestsCache`, newly exposed with `loadAllQuests` from
  [gameDataStore.ts](src/stores/gameDataStore.ts)) instead of ~150 per-quest `resolve_quest` invokes.
  - **Uncompleted** = every CDN quest that isn't a work order, isn't active, and isn't permanently completed;
    **repeatable** completed quests re-surface (doable again). ~1,264 rows for oddio.
  - [QuestListPanel.vue](src/components/Character/QuestListPanel.vue) defaults to **By Area (zone)** grouping;
    catch-all groups ("Unknown Area"…) sort last; new `completed`/`available`/`repeatable` badges.
    `SnapshotActiveQuest.category` widened in [database.ts](src/types/database.ts).

### 2. Uncompleted work-order backlog tab (+ board tracking + batch project)
New tab **"Uncompleted WOs"** ([WorkOrdersTodoPanel.vue](src/components/Character/WorkOrdersTodoPanel.vue)):
every CDN work order (`WorkOrderSkill` set, excluding `"Unknown"` = Fitz gathering turn-ins) for a crafting
skill the player HAS (`gameState.skillsByName[skill].level ≥ 1`), minus `CompletedWorkOrders`. ~1,097 rows for
oddio (only 3 CompletedWorkOrders in the export — the completed set is small, so it's really "all WOs for my
skills"). Work-order `Requirements` are all `Industry MinSkillLevel 0` — **no real per-quest level gate
exists** — so eligibility is just "a craftable recipe exists."
- **Selection + batch project**: per-row checkboxes + select-all; **Create Project** builds a crafting project
  from the **selected** set via new **`createProjectFromWorkOrders`** ([craftingStore.ts](src/stores/craftingStore.ts))
  — resolves item→recipe **lazily** (only at click, 40-wide concurrency batches, since the backlog is ~1,000
  rows), merges by recipe, skips non-craftable (gathering WOs have no recipe), reports added/skipped. New
  `WorkOrderTodo` type in [types/crafting.ts](src/types/crafting.ts).
- **Board tracking**: every WO has a **fixed turn-in board (1 of 9)** — Fitz the Boatman/Serbule, Sheyna/Rahu,
  Irkima/Red Wing Casino, Ogamboe/Fae Realm, Thimble Pete/Eltibule, Viedesi/Sun Vale, Gremix/Amulna,
  Laura Neth/Kur Mountains, Korbok/Vidaria. **Not a structured field** — NPC from the Scripted "Deliver to X"
  objective, location parsed from the flavor `Description` via `parseWorkOrderBoard` in QuestsScreen
  (`/\bto (.+?) (?:in|on|at) (.+?)\.(?:\s|$)/`, verified 100% of all 1,299 WOs). Shown inline per row + a board
  `<select>` filter.
- **Perf**: rows use rich `ItemInline` (2 IPC each on mount) only when the filtered list ≤250, else plain text
  — keeps the ~1,000-row backlog snappy.

### 3. fix(crafting): craftingStore HMR staleness broke the Delete Project button
User reported "delete crafting project button isn't working now." Root cause was **not a code bug** —
`craftingStore` lacked `acceptHMRUpdate`, so this session's repeated store edits left the running singleton
stale in `tauri dev`; the disconnected `activeProject` made `deleteProject`'s `if (!store.activeProject) return`
bail silently. Fix: added the `acceptHMRUpdate` block ([craftingStore.ts](src/stores/craftingStore.ts)),
mirroring `buildPlannerStore`. Needs one full app reload (Ctrl+R) to shed the already-stale instance. (Delete
itself is fine — button emits `@delete` → confirm → `delete_crafting_project`, which cascades entries via the
`ON DELETE CASCADE` FK with `foreign_keys=ON`.)

### Notes / next
- Not click-driven through the native window by me (Tauri can't mount in the browser-only preview); verified
  via type/compile checks, a DB read confirming the 323 completed rows imported, and HMR applying cleanly.
  A live click-test of the tabs + Create Project + Delete is the remaining manual step.
- Gathering-skill work orders (Fishing/Mining/Surveying/…) appear in the backlog but have no recipe, so
  they're skipped at project creation (reported) — intentional.

---

**Date:** 2026-07-06 (Session 29 — Fix farming history fragmenting for 12-hour-clock users)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — committed `c5a055d` on top of `8e82a6d`. PR [#70](https://github.com/crisp-oddio/glogger-oddio/pull/70) (`dev → main`). (Session 28's PR #66 already merged.)
**Status:** ✅ `vue-tsc --noEmit` clean; `tsToSeconds` validated against 24h/12h/midnight-edges/bad-input/elapsed-diff. No Rust changes.

## TL;DR — Session 29 (12-hour-clock fragmentation bug)

**User report:** farming **session history** was saving in "odd increments from 1 minute to 2 hours,
nothing consistent" — happening **during a single continuous run** (no crash, no app close), and
**even with an 8-hour cap** set. (Not survey — the user confirmed farming.)

**Root cause:** the farming store's internal timestamps come from `getCurrentTimestamp()` →
`formatTimeFull`, which **honors the user's 12/24-hour display setting**. For 12-hour users those
render as `"2:30:00 PM"`, but [`tsToSeconds`](src/stores/farmingStore.ts) split on `":"` and did
`Number("00 PM")` → **`NaN`**. So `getActiveSeconds()` was `NaN` for every 12-hour-clock user (also
silently breaking the live timer — it showed `—` — and pause math). Session 28's cap rollover guard
`getActiveSeconds() < capMinutes*60` is **`false` when the left side is `NaN`**, so the cap check
**failed open** and rolled the session over on the next active tick — fragmenting history at
irregular points, independent of the cap value. A latent bug that the rollover surfaced.

**Fix ([farmingStore.ts](src/stores/farmingStore.ts), commit `c5a055d`):**
- `tsToSeconds` now parses **both** `"14:30:05"` (24h) and `"2:30:05 PM"` (12h) via regex + AM/PM
  handling (incl. `12:00 AM`→0, `12:00 PM`→43200) — restoring correct elapsed/timer/pause for all
  users, so the rollover fires only at the real cap.
- Rollover guard is now **`NaN`-safe**: `if (!Number.isFinite(activeSeconds) || activeSeconds < capMinutes*60) return false;`.
- Display unchanged (the card passes bare `HH:MM:SS` through `formatAnyTimestamp`). Survey rollover
  unaffected — it uses 24h log timestamps in Rust.

**Investigation dead-ends worth remembering** (ruled out before finding the real cause): app
restart/crash/close (user said none); character-login/zone restarts (no farming restart wired);
autosave duplicate rows (upsert via `currentSessionId` → one row per session); the cap value itself
(fragments below 8h ⇒ cap-independent). The "cap-independent" clue is what pointed at a `NaN`
comparison rather than a timing threshold.

**Possible follow-up:** the deeper smell is that internal duration math consumes a *display-formatted*
string at all. A cleaner refactor would have `getCurrentTimestamp()` emit machine-format 24h and
apply 12/24h only at render — not done here to keep the fix minimal/low-risk.

---

**Date:** 2026-07-05 (Session 28 — Cap runaway survey & farming sessions at 2h)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — committed `bea3cda` (auto-roll) + `a8e3eac` (configurable farming cap) on top of `91f4166`.
**Status:** ✅ `cargo test --lib survey::aggregator::tests` **22 pass** (incl. 2 new rollover tests);
`cargo check` + `vue-tsc --noEmit` clean. Verified live in `tauri dev`: the new App Settings
dropdown renders under **Settings → App Settings → Farming**, defaulting to 2 hours.

## TL;DR — Session 28 (session length cap)

**User request:** users were "accidentally forming dozens of hour-long sessions, causing them to
crash the program." Fix: cap any single survey **or** farming session at 2h, then roll it over to a
fresh one so no one session grows into an enormous record the UI can't handle. Follow-up: make the
farming duration user-configurable.

### Surveys — backend, log-time driven ([survey/aggregator.rs](src-tauri/src/survey/aggregator.rs))
- `SESSION_MAX_DURATION_SECS = 2*60*60`. New **`maybe_roll_over_session`** runs at the **top** of
  `process_event` (before the event is handled): if the active session's span — `started_at` → the
  current event, both log-derived UTC — has reached the cap, it `end_session` +
  `recompute_session_bounds_and_end` and emits **`SessionEnded { reason: "rollover" }`**.
- The **replacement session auto-starts on the next survey craft/use** via the normal path, so the
  triggering event lands in the fresh session and **no empty sessions** are ever created.
- **Gated on `auto_start_enabled`** — the automatic regime is exactly the "sessions forming
  accidentally" case; a manually-managed session is the user's to end, so it's left untouched.
- Pending loot windows are intentionally **not** severed — loot that arrives after the roll keeps
  attributing to its (now-closed) use as before.
- `event_iso` is hoisted to the top of `process_event` and reused by the crafting auto-end sweep.
- Tests: `test_session_rolls_over_after_max_duration`, `test_no_rollover_when_auto_start_disabled`.
- **Survey cap is a fixed constant** (deliberately not tied to the new farming setting).

### Farming — frontend, store-driven ([stores/farmingStore.ts](src/stores/farmingStore.ts))
- **`maybeAutoRollover`** rides the existing 60s autosave ticker (so the cap is enforced ~within a
  minute of the mark, no new timer). Past the cap (`getActiveSeconds()`), it `endSession()` (saves
  the capped row in place) → `reset()` → `startSession(name)` — a fresh session with the **same
  name**; timers + autosave restart.
- Guards: skips paused / already-ended sessions, and **only rolls a non-empty session** (an empty
  one is no burden and shouldn't spawn a blank history row — it'll roll the moment real activity
  pushes it back over the cap). Applies to **manual** sessions too, and is **independent of the
  autosave setting** (this is a safety cap, not a convenience).

### Configurable farming cap (`a8e3eac`)
- New setting **`farming_session_cap_minutes`** (default **120**), full plumbing:
  [settings.rs](src-tauri/src/settings.rs) (serde default so old settings files load) →
  [settingsStore.ts](src/stores/settingsStore.ts) (camelCase + backend interfaces, to/fromBackend,
  defaults) → read in `maybeAutoRollover` **each tick** so a change takes effect with no restart.
  **`0`/invalid ⇒ cap disabled** (defensive — prevents a bad value causing runaway rollovers).
- UI: **"Auto-reset session after"** `<select>` in the Farming section of
  [AppSettingsTab.vue](src/components/Settings/AppSettingsTab.vue) — 1 / 2 / 3 / 4 / 6 / 8 hours,
  mirroring the "Auto-save active session" control right above it.

### Notes / possible follow-ups
- The survey cap could be wired to this same setting (or its own) if desired — currently fixed 2h.
- `src-tauri/Cargo.toml` shows a phantom LF→CRLF-only modification in `git status` (0 insertions/
  deletions); left uncommitted on purpose.

---

**Date:** 2026-07-02 (Session 27 — Surveying tab: only track items actually collected from surveys)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — committed `9a8d59c` (windows) + `ea33aa6` (identity fallback) on top of `610b63c`.
**Status:** ✅ `cargo test --lib` **490 pass**. Both ignored accuracy replays run locally again:
`accuracy_report` (player.log path) **103/103 exact**; the new **`chat_only_accuracy_report`**
captures **100.0%** with zero undercounts (identity fallback adds a small, codified wild-node
overcount — see below). No frontend changes (`vue-tsc` untouched surface).

### ⚠️ Live-test result → item-identity fallback (`ea33aa6`)
The window gating alone **did not attribute loot on the user's live setup** (the windows need
Player.log timing signals — mining delay loops / StartInteraction — which evidently don't reach the
user's live log the way the captured datasets suggest). Per the user's direction, a
**timing-independent identity fallback** now runs after the windows in `attribute_chat_gain`:
while a session is active, gains whose item is identifiable as survey yield attribute to the
session's **latest use** (`latest_use_id_for_session` reinstated). Identity
(`is_survey_loot_item`): CDN keyword **`Ore`** (rhodium, stibnite, silver/gold/copper ore, paladium,
iridium, pyrite, cinnabar, tungsten, orichalcum, molybdenum, gold nugget, …) or **`MetalSlab`**
(every slab tier), the **six color-crystal family keywords** (`BlueCrystal`/`GreenCrystal`/
`WhiteCrystal`/`OrangeCrystal`/`YellowCrystal`/`RedCrystal` — the base crystal types the
color-named mineral surveys yield, incl. Massive/Maximized; Rubywall Crystal is internally
`RedCrystal`; deliberately NOT the bare `Crystal` keyword, which would drag in emotion pearls /
mind gems / gem fragments), plus **Semi-Real Hassium** (keyword) and **Magic Sand / Sulfur /
Saltpeter** (internal names — the dusts carry `MagicDust`, not `Ore`). Extension commit `705813d`.
**Accepted trade-off (user's explicit choice):** wild-node ore/gems mined during a session count as
session loot. The chat-only report's assertions encode the contract: undercount >2 = hard failure
(zero observed), overcount bounded at 2% of expected (observed +15/2400, all wild-node ore in
50x-povus; the five basic datasets stay exact even with the crystal families active).

## TL;DR — Session 27 (survey chat-loot gating)

**User request:** the Surveying tab counted *every* drop during a session — kill loot, foraging,
even freshly-crafted survey maps. Root cause: Session 9's chat-authoritative path
(`attribute_chat_gain`) attributed ANY `[Status] "X added to inventory"` line to the session's
latest use whenever a session was active. Now only items actually collected from surveys count.

### The gate — per-kind collection windows ([survey/aggregator.rs](src-tauri/src/survey/aggregator.rs))
- **Basic** (loot lands same game tick as the map's DeleteItem): chat gains attribute only within
  ±5s (`CHAT_BASIC_WINDOW_SECS`) of a recently-consumed Basic use (`recent_basic_uses` queue).
  Because Chat.log and Player.log are tailed independently, either stream can be processed first —
  `claim_unlinked_chat_loot_near` ([survey/persistence.rs](src-tauri/src/survey/persistence.rs))
  retroactively tags untagged `chat_status` rows in the window at DeleteItem time (live gate covers
  the other ordering). First chat attribution also marks the use Completed (helps crafting-session
  auto-end for chat-only users, whose uses previously lingered `pending_loot`).
- **Motherlode/Multihit** (loot lands at mining-swing completion on the spawned node): a `Mining...`
  delay loop opens/refreshes a **swing window** `[loop start, start + ceil(duration) + 5s]`
  (`ChatMiningState`); gains attribute only inside it. Node identity comes from the most recent
  `InteractionStarted` (≤10s before the loop — mirrors the parser's `pending_interaction`).
  Engagement opens from: a pending use within the existing 60s walk grace (skipping uses already
  chat-adopted — `chat_adopted_uses`), a previously-bound node (`known_survey_nodes`, multihit
  resume after mining something else), or a **"FromSurvey"-named** interaction (grace expired on a
  long walk; motherlodes are often nameless so grace is their main path). A swing on a provably
  different node closes the engagement (+marks the use Completed).
- **Foreign-interaction clip** (the last leak, found via ground truth): corpse loot collected
  seconds after a swing completes fell inside the trailing slack (3 single items leaked in the
  50x-povus capture — Ratkin corpse searches at swing+8..10s). An `InteractionStarted` on a
  different entity while a window is open **clips** it (same-second gains count as the
  interaction's). Real swing loot lands exactly at start+duration, so nothing legitimate is lost.
- **Survey maps are never loot** — crafting a map emits the identical chat line; excluded by
  `lookup_survey_kind` in the live gate and by `internal_name NOT LIKE 'GeologySurvey%'/'MiningSurvey%'`
  in the retro-claim SQL.
- `attribute_chat_gain` signature changed: takes `internal_name: Option<&str>` instead of
  character/server (no longer session-scoped — the window state is the gate);
  [coordinator.rs](src-tauri/src/coordinator.rs) passes the already-resolved internal name.
  `persistence::latest_use_id_for_session` removed (was only used by the old ungated path).

### Validation — new chat-only replay harness ([survey/replay_tests.rs](src-tauri/src/survey/replay_tests.rs))
`chat_only_accuracy_report` (ignored, like the others) replays all six datasets as a **chat-only
user**: non-survey-map `ProcessAddItem` lines dropped (map lines kept — real chat-only users do get
uses recorded; it's loot lines that vanish), every chat gain routed through the gate exactly as the
coordinator wires it (gate → insert tagged/untagged row → retro-claim can pick it up). Result:
**100.0%, zero extra items** — the fix captures exactly the survey loot. Also fixed the stale CDN
fixture path (`docs/samples/CDN-full-examples` → [docs/CDN-full-examples](docs/CDN-full-examples)) —
the Session-9 note "survey-test can't run locally" no longer applies:
`cargo test --lib survey::replay_tests::accuracy_report -- --ignored --nocapture` (and the
`chat_only_` variant) both work on this checkout.

### Notes / caveats
- The **Player.log gain-attribution path is untouched** (Case 0/1/2 in `maybe_inject_survey_use_id`,
  pending-use pops, multihit rows) — chat gating is parallel state, so verbose-logging users keep
  their provenance pipeline; the loot summary's chat-wins-per-use dedup is unchanged.
- Historical sessions keep their previously over-attributed rows (no retroactive cleanup).
- Speed-bonus flags on the chat path remain lost (`bonus_qty = 0`, Session-9 caveat) — unchanged.
- Not yet click-tested in `npm run tauri dev` (needs a live surveying run in-game); the ground-truth
  replays are the strongest available offline verification.
- The `src-tauri/Cargo.toml` LF↔CRLF artifact persists — intentionally left uncommitted, as usual.

---

# glogger — Session Handoff

**Date:** 2026-07-01 → 07-02 (Session 26 — Build Planner: prose damage mods + damage-type conversions)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — committed + pushed on top of `bd348bd` (a parallel session's roulette/brewery/settings
line; see gotchas). Earlier same-day commits `0b1b7f9` (HSD direct-hit fix) + `b5f9e3b`
(health/armor-specific damage components, landed via the parallel session) are ancestors.
**Status:** ✅ `cargo test --lib` **485 pass** (~30 new parser/engine tests this arc), `vue-tsc` clean.
Verified live in `npm run tauri dev` against in-game tooltips: Mindworm (Mentalism), Headcracker + all
crushing attacks with Viper Halberd, the six Vanquisher's Blade sword abilities, Icesnake knife bar.

## TL;DR — Session 26 (effective-stats tooltip: the damage model grew up)

User-driven arc: "Mindworm's direct damage missing" → "other skills not converting" → "crush→slash items
ignored" → item-by-item verification. Everything below is in `cdn_commands.rs` (prose/token resolution) +
`game_data/ability_stats.rs` (pure formula engine) unless noted.

### 1. Mindworm & the three direct-damage shapes (commits `0b1b7f9`, `b5f9e3b`)
Up-front damage has **3 CDN shapes**: `Damage`, `HealthSpecificDamage` (armor-bypassing —
Mentalism/Psychology/Vampirism, ~235 abilities; Mindworm's 369 lives HERE, no `Damage` field at all), and
`ArmorSpecificDamage` (~188). All share the ability's single set of direct-mod arrays. Parser types all
three; engine emits `direct_damages: Vec<DirectDamageBreakdown>` (kind `normal`/`health`/`armor`), each
folded with the same buckets. Frontend renders one line per component.

### 2. Deep-dive audit → prose damage mods (coverage 43% → 65%)
Audited all ~31k damage-relevant mod/item effect lines (scan tsysclientinfo+items EffectDescs, diff granted
tokens/prose against ability arrays ∪ injected tokens — scratchpad scripts, method worth repeating after CDN
updates). tsys tiers have **NO structured attribute data** — EffectDescs (display prose) is all there is, so
prose parsing is the only route. New resolvers:
- **`resolve_direct_add`** — named-ability prose (**the biggest gap, ~4,500 lines**: "Many Cuts and
  Debilitating Blow Damage +39", "Aimed Shot deals +25% damage…", "damage is +45"), family-gated like
  `resolve_dot_add`, fed through the ability's own bucket token (`pick_bucket_token`); folds trailing
  "and Power Cost -N". Plus skill/keyword-wide "X attacks deal +N[%] damage" (Hammer/Bomb/Bite/Kick).
- **`parse_conditional_direct_text`** — "Direct Poison and Acid Damage +9 and Indirect Poison and Acid +5
  (per tick) while Archery skill active" + the Direct-%-list Hammer form → `BOOST/MOD_<TYPE>_DIRECT`
  tokens gated on equipped skills (chained into `resolve_mod_effects`).
- **Engine**: dual-scope bare `BOOST/MOD_<TYPE>` tokens ("X Damage (Direct & Per-Tick)", e.g.
  `{MOD_ELECTRICITY}`) now injected into BOTH direct buckets and matching-type DoT ticks. `Smiting` added
  to `DAMAGE_TYPES`. Keyword groups `Kick`/`BardBlast`/`Melee`/`Bite` added to `ATTACK_CATEGORY_KEYWORDS`.
- **Excluded BY DESIGN (the remaining ~35%)**: conditional/temporal prose — "for N seconds", "when you have
  N% or less Armor", "+N damage to Elite enemies/Elves/…", pet damage. `conditional_tail()` is the
  gatekeeper; the tooltip shows **unconditional** numbers only. Don't "fix" these into the tooltip.

### 3. Damage-type conversions (Viper Halberd, Icesnake, Vanquisher's Blade, …)
`harvest_type_rules` pre-pass collects rules from all mod tiers + equipped items; the converted type feeds
`compute()` so the typed-token injection flips automatically (Crushing gear stops applying, Slashing gear
starts) and the tooltip header badge shows "Slashing *(was Crushing)*" (new `damageTypeOverride` prop on
[AbilityTooltip.vue](src/components/Shared/Ability/AbilityTooltip.vue), passed only by
[AbilityBarSummary.vue](src/components/Character/BuildPlanner/AbilityBarSummary.vue)).
- **Skill-scoped** ("Staff abilities that normally deal Crushing damage instead deal Slashing damage" —
  Viper/Tactician's Halberd, Icesnake, Autofreeze Knife): ⚠️ **applies to EVERY ability of the from-type
  regardless of skill** — user verified against live in-game tooltips that the second combat skill's
  crushing attacks convert too. The prose's skill word just names the weapon's own skill. Deliberate
  deviation from the text; don't re-add the skill gate.
- **Named forms, all 6 data variants**: "damage type becomes X" / "is changed to X", "direct damage becomes
  X damage" (Horns of Hate), "its damage becomes X instead of Y" (Pixie Flare), "Damage becomes X instead
  of Y" (Infuriating Fist), "deals X damage instead of Y" (Embrace of Despair — becomes a *Sonic* ability
  but deals *Nature*), and the **name-list** form "Sword Slash, Riposte, Windstrike, … deal direct Fire
  damage" (Vanquisher's Blade / Flaming Gazluk Sword). ⚠️ The list form is matched by **normalized name
  segments** (`parse_names_deal_direct`, lowercase alphanumerics, tier-stripped) because item text spells
  "Windstrike" while the ability is "Wind Strike" — index-walking stopped at the first unresolvable name
  and silently killed the conversion for the WHOLE list. Possessive names ("Fairy Fire's") handled by
  `'s`-stripping in `extract_leading_abilities_rest`.
- **DoT riders** ("All slashing abilities apply Poison Slice damage for 10 seconds" / "Sword abilities
  apply Psychic scream…"): gate on the **converted** type or skill, synthesize a DoT fed by the granting
  item's own `BOOST_DOT_*` token — "(Per Second)" semantics → one tick/second (`sniff_damage_type` maps
  "Poison Slice"→Poison, "Flaming"→Fire alias).

### 4. Gotchas hit this session (avoid future hiccups)
- **Two glogger windows, identical titles**: the user's "conversion stopped working" was them hovering the
  **portable** build (`A:\PortableApps\GLogger\glogger.exe`) instead of the dev build. Check
  `Get-Process glogger | select Path,StartTime` before debugging "regressions".
- **Parallel session moved the branch under the running `tauri dev`**: dev jumped to `bd348bd` mid-session
  → webview hot-reloaded (frontend = new branch) while the Rust backend stayed the pre-move binary until
  the next watcher rebuild. Uncommitted work survived (git refuses checkouts that would clobber). If
  tooltips look inconsistent after a branch move, touch a `src-tauri` file to force a rebuild.
- The in-game tooltip folds skill-level passives/active buffs too — glogger states this in-tooltip and
  stays gear-only (~2% off is expected, type/mod-applicability should match exactly).

---

# glogger — Session Handoff

**Date:** 2026-06-25 (Session 25 — Farming Database: SQLite share format)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — feature committed (`50343e3`) + this `docs(handoff)` commit; pushed to `origin/dev`
(a fast-forward that also syncs 14 pre-existing release-merge commits, v0.11.1–v0.11.5).
**Status:** ✅ `cargo test --lib kill_tracking_commands` green (11 tests incl. new
`sqlite_export_round_trips`), `cargo build --lib` clean (no warnings), `vue-tsc --noEmit` clean. Not yet
exercised in `tauri dev` — export/import run through native file dialogs + Rust `invoke`, which a
frontend-only preview can't drive; the round-trip is covered by the unit test and the CSV path is unchanged.

## TL;DR — Session 25

### 1. SQLite `.db` as a share format for the drop-rate database
The Farming → **Database** share feature ("Export My Data" / "Import Database" in
[DatabaseTab.vue](src/components/Farming/DatabaseTab.vue)) gained a portable **SQLite** file format
**alongside** the existing CSV / JSON / friend-format raw-event support — nothing removed, and both Tauri
command signatures are unchanged (no other callers affected). Note this is the *share/backup* file format:
the drop-rate data itself was always SQLite (tables `enemy_kills`, `enemy_kill_loot`,
`imported_enemy_kill*_agg`, … in the primary `glogger.db`); it was never CSV-backed.

- **Backend** ([kill_tracking_commands.rs](src-tauri/src/db/kill_tracking_commands.rs)):
  `export_kill_loot_database` now builds the aggregate once (`collect_export_enemies`) and dispatches on the
  **destination extension** — `.db`/`.sqlite`/`.sqlite3` → `write_sqlite_export`, else `write_csv_export`.
  The CSV output is **byte-for-byte unchanged** (same columns, derived `drop_rate`, empty-item placeholder
  row for lootless enemies, most-dropped-first sort).
- **Export schema:** a standalone DB with `meta(key, value)` (`format`/`format_version`/`exported_at`) plus
  `enemies(enemy_name, zone, total_kills)` and `loot(enemy_name, zone, item_name, total_quantity,
  times_dropped)`; `zone` is `NULL` for the unknown-zone bucket. Any existing file at the path is removed
  first so a prior export's tables can't linger.
- **Import** (`import_kill_loot_database`): detects SQLite by the **16-byte file-header magic**
  (`file_is_sqlite`), then `parse_sqlite_drop_data` (read-only open, validates the `enemies` table exists,
  attaches loot per (enemy, zone), errors clearly on a non-glogger DB). Everything else still flows through
  the text path (`parse_drop_data` → CSV aggregated / raw-event / legacy JSON). The idempotent,
  `source_label`-tagged merge into the `imported_*` tables is unchanged.
- **Deliberate asymmetry:** export chooses format by **extension** (we create the file, so the extension is
  the user's intent); import chooses by **header magic** (a received file's name can't be trusted, and
  reading a binary DB as UTF-8 text would fail/garble).
- **Frontend:** a CSV/SQLite `<select>` next to *Export My Data*, persisted in the `database` view prefs
  (`exportFormat`, default `csv`); the choice drives the save-dialog filter and the suggested filename
  extension (`oddio-0001.csv` vs `oddio-0001.db`). The import dialog now also accepts
  `.db`/`.sqlite`/`.sqlite3`.

### 2. Verification
`cargo test --lib kill_tracking_commands` → **11 passed** including the new `sqlite_export_round_trips`
(lossless round-trip through write→read: a lootless enemy, the NULL-zone bucket, and most-dropped-first loot
order). `cargo build --lib` clean. `vue-tsc --noEmit` clean.

### 3. Notes for next session
- This push fast-forwards `origin/dev` and also carries 14 pre-existing **release-sync merge commits**
  (v0.11.1–v0.11.5) that local `dev` was ahead by — expected, not from today's work.
- `src-tauri/Cargo.toml` shows as modified in `git status` but it's **pure LF→CRLF line-ending noise** (no
  content diff under `--ignore-space-at-eol`) — intentionally left uncommitted.
- Possible follow-ups (not done): extend the same SQLite option to the **Brewery** discoveries import and the
  **Words of Power** CSV export; and a live `tauri dev` click-through of an actual export→import.

---

**Date:** 2026-06-24 (Session 24 — Build Planner: effective ability stats + indirect-DoT rules)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` — committed + pushed (`2cd8bfa`, `df944df`, `69a3f34`); **[PR #22 `dev → main`](https://github.com/crisp-oddio/glogger-oddio/pull/22) open**.
**Status:** ✅ `cargo test --lib` green (8 `ability_stats` formula tests), `vue-tsc` clean. Effective-stats
tooltip + slot toggle verified live in `npm run tauri dev`; the indirect-DoT correction is pushed and the
dev build hot-rebuilds — user is verifying the numbers against the in-game tooltip.

## TL;DR — Session 24

### 1. Effective ability stats on hover (Build Planner)
Hovering an equipped ability now shows its full combat data folded with the build's assigned gear mods —
mirroring the in-game tooltip — for **damage, DoTs, heals/special values, and power/rage costs**.
- **Engine (Rust, all formula logic, unit-tested):** new
  [game_data/ability_stats.rs](src-tauri/src/game_data/ability_stats.rs) — `compute(stats, damage_type,
  &[ModEffect]) -> AbilityBuildStats`. PG direct-damage formula:
  `final = base·(1 + ΣModBaseDamage% + ΣModDamage%) + ΣDeltaDamage·(1 + ΣModDamage%)`. Each mod token is
  classified by **exact membership** in the ability's attribute arrays (no fuzzy matching).
- **Typed combat stats:** [game_data/abilities.rs](src-tauri/src/game_data/abilities.rs) now parses
  `DoTs[]` and `SpecialValues[]` into typed `CombatStats.dots` / `.special_values` (added to the known-keys
  list so they leave `extra`).
- **Command:** `compute_ability_build_stats(ability_id, mods, pvp)` in
  [cdn_commands.rs](src-tauri/src/cdn_commands.rs) (registered in [lib.rs](src-tauri/src/lib.rs)) resolves
  each preset mod's `{TOKEN}{VALUE}` effects to raw tokens (factored `parse_token_value` helper) and calls
  the engine.
- **Frontend:** [useAbilityBuildStats.ts](src/composables/useAbilityBuildStats.ts) (cached per ability +
  mods signature), [abilityStats.ts](src/types/abilityStats.ts), and
  [AbilityEffectiveStats.vue](src/components/Character/BuildPlanner/AbilityEffectiveStats.vue) rendered in
  the bar-hover tooltip in
  [AbilityBarSummary.vue](src/components/Character/BuildPlanner/AbilityBarSummary.vue) (replaced the old
  `AbilityModBreakdown.vue`, deleted). Known limit stated in-tooltip: only gear mods are modeled, not
  skill-level passives / active buffs — so it's "base + this build's gear," ~2% off the live number.

### 2. DoT (indirect) damage rule — the important subtlety
DoT damage is **indirect** damage, moved **only** by indirect-damage modifiers — **never** by
base-skill-damage % or the ability's *direct*-damage arrays. (Two earlier wrong cuts: first applied
ability-level direct mods per tick; corrected after the user confirmed against the in-game tooltip.)
Indirect modifiers = the DoT's own per-ability tokens **plus** the generic per-damage-type and universal
indirect attributes, applied globally by the DoT's `DamageType`:
`per-tick = (DamagePerTick + Σ flat-indirect) · (1 + Σ %-indirect)`, where flat = `dot.AttributesThatDelta`
∪ `{BOOST_<TYPE>_INDIRECT, BOOST_UNIVERSAL_INDIRECT}`, % = `dot.AttributesThatMod` ∪
`{MOD_<TYPE>_INDIRECT, MOD_UNIVERSAL_INDIRECT}` (`indirect_tokens()` helper; `<TYPE>` = `DamageType`
upper-cased). **Applies to every DoT representation:** the `DoTs[]` array (covers all 1,188 hybrid
direct-hit + DoT abilities — direct hit uses direct mods, each tick indirect) **and** DoTs encoded as
SpecialValues (reflect-style, e.g. Tough Hoof — gated on an `ABILITYDOT`/`_INDIRECT` token so heals/armor-
over-time stay out; damage type parsed from the effect text).

### 3. Slot deselect toggle + Pinia HMR
- Clicking the already-selected equipment slot now **deselects** it, collapsing the slot detail panel back
  to the global "Search All Mods" catalog (`selectSlot` toggle in
  [buildPlannerStore.ts](src/stores/buildPlannerStore.ts), shown when `selectedSlot` is null in
  [BuildPlannerScreen.vue](src/components/Character/BuildPlanner/BuildPlannerScreen.vue)).
- **Dev-env gotcha fixed:** added `acceptHMRUpdate` to `buildPlannerStore` — without it, editing a store
  action during `tauri dev` left the already-instantiated singleton running stale code (the deselect toggle
  appeared to do nothing until a full reload). Other stores still lack it.

### 4. Build import (gorgonexplorer + PgBuilder) — BUILT, PARKED uncommitted
Per the user's "ditch the import stuff for a moment," this is **complete on disk but intentionally NOT
committed** (kept out of PR #22). Sitting in the working tree:
`src-tauri/src/db/build_planner_commands.rs` (refactored shared `insert_exported_build` + the two import
commands), import hunks in `lib.rs` + `buildPlannerStore.ts`, `PaperDollLayout.vue` (auto-detecting import
+ unmatched report), `ModalDialog.vue` (multiline). Formats (both fully reverse-engineered):
- **PgBuilder** — JSON `{ "Build": {…} }` with **exact internal IDs**: `ability_<id>`, `item_<id>`,
  `power_<id>`+`PowerTier_N` (the `power_<id>` is the tsys `client_info` key → `internal_name`). No fuzzy
  matching. Slots: `Main Hand`→`MainHand`, `Neck`→`Necklace`, `Waist`→`Belt`, `Racial`→skip.
- **gorgonexplorer** — `GET https://api.gorgonexplorer.com/api/builds/{id}` (accept a share URL or bare id).
  `firstSkill`/`secondSkill`, `selectedGear` slot→`item_<id>`, abilities by exact name (incl. tier), and
  `selectedMods` resolved **text** → fuzzy reverse-match to power+tier (skill-narrowed; unmatched reported
  for manual add — user chose "best-effort + report").
**Next:** finish/verify the import path (`cargo test` + a live import of both the gx URL and a PgBuilder
export), then commit when the user wants it in the PR.

---

# glogger — Session Handoff

**Date:** 2026-06-24 (Session 23 — Council-wallet estimator, Tier 1)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (unreleased — committed + pushed, no release dispatched)
**Status:** ✅ Backend `cargo test --lib` **437 pass** (5 new parser tests), `vue-tsc` clean. Migration
v56 is `CREATE TABLE IF NOT EXISTS` (idempotent). **Not yet click-tested in the live app** — needs a
`npm run tauri dev` run + a character export to see the council figure populate (browser preview can't
mount Tauri `invoke()`, the usual limitation).

## TL;DR — Session 23 (live council-balance estimate)

**Problem the user raised:** the dashboard only shows "relevant" data right after a manual character +
storage export; live tailing doesn't keep currency current. **Audit finding (the crux):** Player.log
contains **no** absolute currency value anywhere (`ProcessVendorUpdateAvailableGold` is the *vendor's*
budget; the login `ProcessSetAttributes` dump has no gold field; NPC dialog "costs N councils" is a
quoted price, not a debit). The wallet is one currency — internal **`GOLD`**, shown as "councils" =
"coins". **Gains are well-visible in chat; spends are almost entirely invisible** (vendor buys,
training, vault slots, hiring, trades produce no log line). So the only viable model is **anchor on the
last export + accumulate visible chat deltas**, which **drifts high** until the next export re-anchors.

### What shipped (Tier 1)
1. **Parser** ([chat_status_parser.rs](src-tauri/src/chat_status_parser.rs)) — new `try_councils_misc`
   adds 5 previously-dropped wallet lines, all → signed `CouncilsChanged`: `You receive N coins.` (+,
   ~776 in the user's 61 logs — the big gap), `You recovered N Councils stolen by <mob>.` (+),
   `You retrieve N stolen coins.` (+), `You were given N coins by <player>!` (+),
   `<mob> stole N Councils!` (−). 5 unit tests. Existing `CoinsLooted` (corpse, 6851 events) +
   `CouncilsChanged` (received/used) already covered the rest.
2. **Migration v56** ([migrations.rs](src-tauri/src/db/migrations.rs)) — `currency_estimate` table
   (PK char/server/currency): `anchor_amount`, `anchor_at`, `delta_since`. Estimate = anchor + delta.
3. **Anchor seeding** ([character_commands.rs](src-tauri/src/db/character_commands.rs)
   `seed_game_state_from_snapshot`) — on export import, upsert anchor from `report.currencies["GOLD"]`
   at the export `Timestamp` (Z-stripped to "%Y-%m-%d %H:%M:%S" UTC), **resetting `delta_since=0`**.
   Guarded so an older re-imported export can't clobber a fresher anchor.
4. **Live accrual** ([coordinator.rs](src-tauri/src/coordinator.rs)) — new match arms for `CoinsLooted`
   / `CouncilsChanged` call `accrue_currency_delta(amount, event_ts)`, which adds the signed delta to
   `delta_since` for the active char **only when an anchor exists AND `event_ts > anchor_at`** (so
   replaying pre-export chat during catch-up can't double-count).
5. **Command + store** — `get_currency_estimate` ([game_state_commands.rs](src-tauri/src/db/game_state_commands.rs),
   registered in [lib.rs](src-tauri/src/lib.rs)) returns `{anchor_amount, anchor_at, delta_since,
   estimated, has_anchor}`. [gameStateStore.ts](src/stores/gameStateStore.ts) gains `currencyEstimate`
   + `fetchCurrencyEstimate()` (called in `loadAll`), with **optimistic** `applyWalletDelta()` on live
   `CoinsLooted`/`CouncilsChanged` so the figure ticks without a round-trip.
   [characterStore.ts](src/stores/characterStore.ts) re-fetches the estimate after a fresh export
   re-anchors it.
6. **UI** ([CouncilsWidget.vue](src/components/Dashboard/widgets/CouncilsWidget.vue)) — header shows
   `estimated.toLocaleString()` councils + "≈ est. · anchored Nh ago" (or "no export yet — import to
   anchor"), with a tooltip stating income is tracked but spends drift it high. Activity feed unchanged.

### Validated on real data
Ran the 5 patterns over a busy real log (`Chat-26-04-08.log`): net **+27,436** councils for the day
(29,273 gains across 220 events, −1,837 from 5 mugger thefts). The 12 `receive coins` that day were
previously dropped.

### Known limitation / Tier 2 (deferred, by design)
Spends are invisible in both logs, so the estimate **overestimates** between exports — fine for a
farmer (gains ≫ spends), drifts fast for a merchant/crafter. Re-anchoring on each export bounds the
error. **Tier 2** (not built): infer purchases by correlating `ProcessVendorScreen` item prices with
the matching `ProcessAddItem(…, True)` — fragile, separate follow-up. Also: the optimistic client
counter isn't idempotent against a chat re-scan (same caveat as `item_transactions`); a re-export fixes
it. **Next:** click-test in `npm run tauri dev` — export a character, confirm the figure anchors and
ticks on coin loot.

---

# glogger — Session Handoff

**Date:** 2026-06-23 (Session 22 — Build Planner UX batch → v0.11.1)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (reconciled onto v0.11.0; releasing **v0.11.1**)
**Status:** ✅ Build-planner enhancements, verified live in `npm run tauri dev` (HMR through the whole
session) + `vue-tsc` clean. Committed, pushed, release dispatched.

## TL;DR — Session 22 (Build Planner UX)

All in the Character → Build Planner. v0.11.0 (PR #20) merged first; this batch ships as v0.11.1.

### 1. Ability hover tooltip — applied mods + effects
Hovering an ability in the bars beneath the equipment now shows, below the base ability info, an
**"Applied mods (N)"** section listing each mod that targets that ability and its effects.
- **New** [useBuildModEffects.ts](src/composables/useBuildModEffects.ts) — module-level singleton
  (detached `effectScope` + watch on `presetMods`) that resolves each mod's structured effects and
  the TSys↔Ability map **once**, exposing `effectsForAbility(id)` / `modsForAbility(id)`.
- **New** [AbilityModBreakdown.vue](src/components/Character/BuildPlanner/AbilityModBreakdown.vue) —
  the tooltip section. Added into the existing `EntityTooltipWrapper` in
  [AbilityBarSummary.vue](src/components/Character/BuildPlanner/AbilityBarSummary.vue).
- [BuildSummary.vue](src/components/Character/BuildPlanner/BuildSummary.vue) refactored to consume the
  composable (its "By Ability" view is now a reactive computed off the same source — no dup logic).

### 2. Collapsible left-pane sections (persisted)
[PaperDollLayout.vue](src/components/Character/BuildPlanner/PaperDollLayout.vue) — **Equipment** and
**Abilities** got chevron show/hide headers matching the existing **Set Defaults** one. All three
states persist via `useViewPrefs('build-planner', { showDefaults, showEquipment, showAbilities })`
(distinct from `SidePane`'s `build-planner.pane.*` keys). Also added Skill 1 / Skill 2 default
dropdowns to Set Defaults earlier in the day (v0.11.0).

### 3. "Search All Mods" → full catalog search + apply
[GlobalModSearch.vue](src/components/Character/BuildPlanner/GlobalModSearch.vue) — was only filtering
mods already in the build; now searches the **entire mod catalog**:
- Backed by the existing `search_tsys` command; shown whenever **no slot is selected**
  ([BuildPlannerScreen.vue](src/components/Character/BuildPlanner/BuildPlannerScreen.vue) dropped the
  `presetMods.length > 0` gate). Grouped by skill (Generic last), debounced, capped 300.
- **Inline effects:** each result shows its highest-tier effect text, batch-resolved in one
  `get_tsys_power_info_batch` call per search.
- **Click-to-apply:** each result's slots are clickable `+ Slot` buttons. Clicking applies the mod to
  that slot via new store action
  **`addCatalogModToSlot(slotId, internalName)`** ([buildPlannerStore.ts](src/stores/buildPlannerStore.ts))
  — loads that slot's level-appropriate powers, enforces capacity/dup/skill-rarity constraints
  (`computeSlotConstraints`), picks the tier, and adds **without** switching away from the search.
  Transient success/failure message shown.

### Verification / release
`vue-tsc` clean throughout; exercised live via the running dev build (HMR). Released via the two-phase
flow: reconciled `dev` onto `main` (v0.11.0), dispatched `release.yml --ref dev -f version=0.11.1`.

---

# glogger — Session Handoff

**Date:** 2026-06-23 (Session 21 — four-part feature batch; committing per step)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (reconciled to v0.10.1 base; **v0.11.0 release PR open**)
**Status:** ✅ All four tasks done, each committed + pushed individually. `vue-tsc` + `cargo check`
green after every step; farming upsert covered by 2 passing unit tests; CI **validate** (build +
`cargo test`) green. UI changes (planner dropdowns, dashboard resize) still need a `npm run tauri
dev` click-test — computer-use couldn't drive the dev window (grant resolves to the *portable* exe
path, masking the dev build), and the browser preview can't mount the app (needs Tauri `invoke()`).

## Release — v0.11.0 (Session 21)

Repo uses a **two-phase release flow** now (CLAUDE.md's old "push tag" note is stale):
1. **Release** workflow (`release.yml`, workflow_dispatch) → validates, bumps version on a
   `release/vX` branch, opens a **PR to `main`** (main is protected, no direct push).
2. Merging that PR → **Release Publish** (`release-publish.yml`) tags + builds all installers +
   GitHub Release, then Flatpak attaches.

This session: `dev` had diverged from `main` — `main` was at **v0.10.1** (a version-only release,
PR #19, *not* containing our features) while `dev` was at 0.10.0 + the 5 feature/test commits.
Merged `origin/main` into `dev` (clean — our commits never touch version files, so no conflict;
version baseline became 0.10.1), pushed `dev` (`e6b43d2`), then dispatched `release.yml --ref dev
-f version=minor`. Validate + open-release-pr both green →
**[release: v0.11.0 — PR #20](https://github.com/crisp-oddio/glogger-oddio/pull/20)** is OPEN.
**Next action (user):** approve + merge PR #20 to publish v0.11.0.

## TL;DR — Session 21 (in progress)

A batch of four user-requested features. Each is committed & pushed individually. All four complete.

### 1. ✅ Periodic farming-session auto-save (crash protection)
Previously an active farming session lived only in the frontend Pinia store
([farmingStore.ts](src/stores/farmingStore.ts)) and was persisted **only** when the user clicked
"End Session" (`save_farming_session` INSERT). A crash/power-loss mid-session lost everything.
- **Backend** ([farming_commands.rs](src-tauri/src/db/farming_commands.rs)) — `SaveFarmingSessionInput`
  gained an optional `session_id`. `save_farming_session` is now an **upsert**: with `session_id`
  set it UPDATEs that row and **replaces** its child rows (skills/items/favors/kills) from the
  fresh snapshot (no double-count); without it, INSERTs as before. Returns the row id either way.
- **Frontend** ([farmingStore.ts](src/stores/farmingStore.ts)) — extracted the snapshot builder into
  `buildSessionInput(end)` shared by auto-save and `endSession`. New `currentSessionId` ref tracks
  the in-progress row. A 60s ticker (`maybeAutoSave`) checks `settings.farmingAutosaveMinutes`
  (so changing the setting takes effect live) and persists every N minutes; empty sessions are
  skipped until they have data. `endSession` now updates the same row in place (no duplicate);
  `startSession`/`reset` clear the id.
- **Setting** — `farming_autosave_minutes: u32` (default **5**; 0 = off) in
  [settings.rs](src-tauri/src/settings.rs) + [settingsStore.ts](src/stores/settingsStore.ts).
  New **Farming** section in [AppSettingsTab.vue](src/components/Settings/AppSettingsTab.vue) with an
  Off / 5 / 10 / 30 min dropdown.
- **Recovery behaviour:** after a crash the partial session is already a row in the DB (end_time
  NULL) and shows in Session History. The store does not auto-resume the session on next launch —
  the *data* is preserved, which was the requirement. Verified via `vue-tsc` + `cargo check` clean.

### 2. ✅ Serialized export filenames (`charname-NNNN.csv`)
The Farming → Database **Export** previously defaulted to `glogger-drop-rates-<YYYY-MM-DD>.csv`.
Now suggests `<charname>-<NNNN>.csv` — e.g. `oddio-0001`, `oddio-0002`, … — in
[DatabaseTab.vue](src/components/Farming/DatabaseTab.vue).
- Char name = `settings.activeCharacterName` (sanitized to `[a-z0-9_-]`, lowercased), falling back to
  `glogger` when no character is loaded.
- `NNNN` is a **global** 4-digit zero-padded serial persisted via `useViewPrefs("database",
  { exportSerial })`. The next serial is offered as the default filename and only **advances after a
  successful export** (cancelling the dialog or a failed export does not consume a number).
- **Design note:** the counter is global (total exports), not per-character — matches "how many
  times we've exported." Frontend-only change; `vue-tsc` clean.

### 3. ✅ Build Planner — default combat-skill dropdowns
The build planner already had `skill_primary`/`skill_secondary` on the preset (used as the per-slot
fallback — SlotDetailPanel shows "X (default)"), but they were only settable indirectly. Added two
**Skill 1 / Skill 2** dropdowns to the **Set Defaults** section in
[PaperDollLayout.vue](src/components/Character/BuildPlanner/PaperDollLayout.vue), directly under the
existing Rarity + Level row, so you can blanket-apply the two combat skills for the whole build
while still overriding per item.
- Options are `store.combatSkills` (loaded on mount) keyed by display `name` (matches the per-slot
  picker), plus a **None** entry to clear the default.
- Handlers call `updatePreset({ skill_primary|skill_secondary })` then `onBuildParamsChanged()` to
  refresh available powers for the selected slot. Also added the missing `onBuildParamsChanged()`
  to the rarity handler for consistency.
- **Store fix** ([buildPlannerStore.ts](src/stores/buildPlannerStore.ts)) — `updatePreset` used `??`
  for the nullable skill fields, which would have ignored an explicit `null` (the "None" clear).
  Switched to an `in`-operator check so clearing actually persists. `vue-tsc` clean.

### 4. ✅ Dashboard widget resize now reflows neighbours (column-span, not px)
**Problem (the Session-20 known limitation, now fixed):** the right-edge resize set an explicit
**pixel width** on the card while it still occupied its full grid-column track(s), so shrinking a
widget left an unusable gap — no neighbour could move into the freed space.
**Fix:** resize now changes the card's **grid-column span**, snapping to whole columns, so the grid
genuinely frees tracks and subsequent widgets reflow into them (live, during the drag).
- [DashboardCard.vue](src/components/Dashboard/DashboardCard.vue) — `width` prop → `span` prop;
  applies `style="grid-column: span N"` (overrides the Tailwind `col-span-*` class) when a span is
  set. The handle measures the grid geometry (`gridTemplateColumns` track count + `columnGap`) from
  the card's parent grid, derives the current span from rendered width, and snaps the drag to
  `[1, columnCount]`. Stray-click guard + double-click-to-reset preserved (`resize` 0 = reset).
- [DashboardView.vue](src/components/Dashboard/DashboardView.vue) — `cardWidths` pref →
  **`cardSpans`** (`Record<string, number>`); `setCardSpan` stores the override (0 = delete → revert
  to the widget's default size class). Default `col-span-*` size classes still apply when no override.
- **Migration note:** old per-widget `cardWidths` (px) are no longer read; affected widgets revert
  to their default size until resized again — acceptable, clean break from the px approach.
- **Verification:** `vue-tsc` clean. Not click-tested in-app this step — the browser-only preview
  can't mount the dashboard (startup needs Tauri `invoke()`), same limitation noted in Session 20;
  needs a `npm run tauri dev` drag-test to confirm the reflow visually.

---

# glogger — Session Handoff

**Date:** 2026-06-22 (Session 20 — README refresh + screenshots, dev-sync hook, resizable widgets)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (synced even with `main` at v0.9.24; widget-resize committed on top)
**Status:** ✅ Working, verified live in `npm run tauri dev` (app interactive, auto-loaded oddio@Arisetsu,
no migration/panic errors). `vue-tsc --noEmit` clean.

## TL;DR — Session 20

Three things this session, all small:

### 1. README refresh + real screenshots (on `main`, `aab724f`)
Folded the weekend's features (drop-rate DB CSV/zone/loadout, harvest stats, Combat Wisdom, Vendor
Council earnings, Statehelm favor-falloff, Stall Tracker **Trends** tab, chat-authoritative survey
loot) into [README.md](README.md)'s feature list, and replaced the "Screenshots coming soon"
placeholder with a 5-shot table (Dashboard / Build Planner / Vaults / Crafting Projects / Farming
Database) saved under [docs/screenshots/readme/](docs/screenshots/readme/) as `.webp`.
- **Repo-state gotcha discovered:** by this session `origin/dev` and the old PR branch had been
  **deleted** on the remote (the repo moved to a release-PR flow — there's now a `ci/release-pr-flow`
  branch + `release/v0.9.24`). `main` had advanced 46 commits (all the weekend work merged via the
  v0.9.24 release) and someone had hand-edited `README.md` on `main` (the new "community fork by
  oddio" About block). So the README edits were **re-applied on top of current `main`** to preserve
  that About block, then pushed straight to `main`. PR #16 turned out already-merged into the
  (now-deleted) `dev`. Recreated local→`origin/dev` by fast-forwarding `dev` to `main` and pushing.

### 2. Auto-sync hook: dev follows main on every push
Added a **PostToolUse/Bash hook** (filtered `if: Bash(git push*)`) in
[.claude/settings.local.json](.claude/settings.local.json) that, after any successful `git push`,
fast-forwards local `dev` to `main` **without switching the checked-out branch**: if `dev` is
checked out it runs `git merge --ff-only main`, else `git fetch . main:dev`. Only ever
fast-forwards (no force), emits a "local dev synced to main" system message. Personal/machine-local
(local settings file), not committed. NOTE: if it doesn't fire, open `/hooks` once to reload config.

### 3. Resizable widgets — X-axis side drag (committed this session)
The dashboard is a CSS grid (`repeat(auto-fill, minmax(280px, 1fr))`) where every card is `h-100`
(fixed height — so Y was already locked). Added a **right-edge drag handle** to
[DashboardCard.vue](src/components/Dashboard/DashboardCard.vue):
- Thin `cursor-ew-resize` strip on the right edge (no corner handle), highlights on hover/drag.
- Drags **width only**; `dragWidth` ref gives live feedback, persists once on pointerup (debounced).
- Clamped **min 200px** → **max = parent grid-cell width** (`parentElement.clientWidth`).
- **Double-click the handle = reset** (emits `resize` 0 → parent deletes the key → back to stretch).
- Per-widget widths stored in new `cardWidths: Record<string,number>` under the `dashboard` view
  prefs ([DashboardView.vue](src/components/Dashboard/DashboardView.vue) `setCardWidth`); survive reload.
- **Design note / open choice:** because it's a uniform grid, shrinking a card narrower than its
  cell leaves a **gap to the right** (no neighbor reflow) — the card just gets narrower in place.
  Alternative not taken: snap between column spans (1→2→4) so neighbors reflow. User is happy with
  the in-place shrink for now ("looks great").
- **Verification caveat:** the browser-only preview (`npm run dev`) **can't run Tauri `invoke()`**
  (documented limitation — `get_cache_status … reading 'invoke'`), so it stalls on the splash and the
  dashboard never mounts. Verified instead via `vue-tsc` clean + the **native `npm run tauri dev`**
  build, which the user click-tested live and approved.

---

# glogger — Session Handoff

**Date:** 2026-06-21 (Session 19 — Drop database: CSV, zones, combat-loadout segmentation)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (base v0.9.20; these commits are **unreleased** — opened a `dev → main` PR)
**Status:** ✅ Working, verified live in `npm run tauri dev`. `vue-tsc` + `cargo check` + targeted
`cargo test` green. Not yet released — next release bumps from 0.9.20.

## TL;DR — Session 19 (the kill/loot "drop database" grew up)

Turned the drop-rate database into a sharable, zone- and build-aware dataset. Four layers, each
its own commit:

### 1. CSV import/export + friend-format raw-event import (`554e7e4`)
[kill_tracking_commands.rs](src-tauri/src/db/kill_tracking_commands.rs) — replaced the JSON
bundle with **CSV** (added the `csv` crate). Export writes `enemy_name,total_kills,item_name,
total_quantity,times_dropped,drop_rate` (+ zone/combat cols, below); opens cleanly in a sheet.
Import **auto-detects** two layouts via header sniffing (`parse_drop_data` → `parse_csv_drop_data`):
- **Aggregated** (our export, or any sheet with `total_kills`/`total_quantity`/`times_dropped`).
- **Raw loot-event log** (one row per looted item, with a per-corpse id like `enemy_id`):
  `parse_csv_raw_events` aggregates it — distinct corpse ids per enemy = kills, distinct corpse
  ids per (enemy,item) = times dropped, `Item_Amount` summed = qty. This is the format a
  community member already collects (validated against his 4,966-row file: 183 enemies, sane
  rates). Legacy JSON still imports (sniffed by leading `{`). Caveat: raw logs omit empty corpse
  searches, so their denominators (and rates) run slightly high.

### 2. Zone-aware drops — migration **v54** (`554e7e4`)
Same monster in different zones has different loot, so zone is part of the key now. `enemy_kills`
+ both `imported_*_agg` tables gained `zone`; the coordinator stamps the current area on each
corpse-search kill; stats/search/export/import group by `(enemy, zone[, item])`. The friend's
`Zone` column maps straight in.

### 3. Combat-loadout segmentation — migration **v55** (`ba4389b`) + this session's selector
PG drops vary by the **equipped combat-skill pair** (Sword/Shield vs Fire/Ice…), which we already
parse (`SetActiveSkills`). `enemy_kills` + imported aggregates gained `combat_skills` (normalized
`skillA+skillB`, blank = unattributed). Backend filters drop stats by loadout with the agreed
policy: **strict loadout match + an "unattributed" baseline** (so legacy/imported data — which
has no loadout — stays visible).
- **Equipped Skills selector** in the Database tab header (between scope toggle and Export): two
  combat-skill dropdowns ("Combat" skills from `get_all_skills` where `combat===true`), defaulting
  to the live in-game loadout and editable on the fly to preview other builds' tables.
- **Live-reactivity fix (key bug):** first cut read the loadout once via a one-shot `invoke` and the
  ↺ button reused stale captured values, so it never followed in-game changes. Rewired to bind to
  `gameStateStore.activeSkills` — a reactive ref already refreshed by the `active_skills`
  `game-state-updated` event — so the selector now **auto-follows** skill swaps/zone reloads unless
  the user manually overrides (↺ clears the override and re-syncs to live). The
  `[searchTarget, scope, selectedLoadout]` watch drives reloads.

### 4. Harvest Stats on Data Browser > Enemies (`49c7544` section, `1208f02` %)
The enemy detail pane shows what a monster skins/butchers into (`get_enemy_harvest_stats`), now
with a colored **Rate** column (each item's share of harvest pulls), mirroring Kill Stats.

### Also shipped earlier this session (already in v0.9.20): app **Scale** setting (CSS zoom 50–200%,
slider + editable %), permanent imports (migration v53 dropped the FK cascade), list-first DB tab.

### Next up (queued, not started)
"Capture every skill" per kill — a deduped full-skill-snapshot table — for richer future analysis
beyond the combat-loadout segmentation. Schema would hang a `skill_snapshot_id` off `enemy_kills`.

---

# glogger — Session Handoff

**Date:** 2026-06-21 (Session 18 — Drop-rate DB UX overhaul, harvest stats, app scale → v0.9.20)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (merged `origin/main` v0.9.19 back in first, then released **v0.9.20**)
**Status:** ✅ **Shipped.** Three user-facing features across the Farming Database tab, the Data
Browser enemy detail, and Settings. `vue-tsc` + `cargo check` + targeted `cargo test` all green;
verified live in `npm run tauri dev` (app reached "interactive", zero errors). Released v0.9.20
via the Release workflow.

## TL;DR — Session 18

### 1. Farming → Database tab: list-first, permanent imports, harvested box
[DatabaseTab.vue](src/components/Farming/DatabaseTab.vue) +
[kill_tracking_commands.rs](src-tauri/src/db/kill_tracking_commands.rs)
- **Lists everything up front.** Clicking the **Monsters / Items / Harvested** box loads the
  full list for the active scope (empty query → all, no limit). No search required.
- **Search is now a client-side filter** — isolates rows whose name contains the text, hides
  the rest, instantly (no debounce/round-trip). Added a result count + ✕ clear button.
- **Imports merge permanently.** **Migration v53** rebuilds `imported_enemy_kills_agg` /
  `imported_enemy_kill_loot_agg` **without** the old `ON DELETE CASCADE`, so removing an entry
  from the Imported Sources list (`delete_imported_source`, now non-destructive) keeps the
  merged data forever. Re-import still replaces by `source_label` → no double-count. (User chose
  "keep the list, Remove = hide only".)
- **New "Harvested" box** (3rd toggle): lists skinning/butchering yields from `corpse_extracts`
  via new `search_database_harvested`; each row expands to the existing
  [ExtractDetailTable.vue](src/components/Farming/ExtractDetailTable.vue). Harvested is local-only
  → the My Data / Imported / Combined scope toggle auto-disables for it.
- **Scroll fix:** tab root switched to `overflow-hidden` so the result list (`flex-1 min-h-0
  overflow-y-auto`) is the sole scroll region; toolbar/search/imported-sources stay pinned.
- `search_database_enemies` rewritten from N+1 per-enemy queries to set-based aggregation; both
  search commands take an optional `limit` (`None` = all) and an empty `query` = list all.

### 2. Data Browser → Enemies: "Harvest Stats" section
[EnemyBrowser.vue](src/components/DataBrowser/EnemyBrowser.vue) + `get_enemy_harvest_stats`
- Under **Kill Stats**, a new **Harvest Stats** table (Item · Skill · Qty · Pulls) shows what a
  monster skins/butchers into. `corpse_extracts.corpse_name` == `enemy_kills.enemy_name` (both
  from the "Search Corpse of X" context), so it's queried by the monster's display name.
  Local-only; shows "No harvest data yet" when none.

### 3. Settings → Appearance: Application Scale
[AppSettingsTab.vue](src/components/Settings/AppSettingsTab.vue) +
[settingsStore.ts](src/stores/settingsStore.ts) + [settings.rs](src-tauri/src/settings.rs)
- New `ui_scale` setting (percent, 50–200, default 100). Applied via **CSS `zoom`** on
  `document.documentElement` (`applyUiScale`) so it scales the **whole** app uniformly —
  distinct from the existing font-size "Interface Size" which only scales rem-based text.
- Slider previews live while dragging + an **editable number box** (type an exact %, clamps on
  commit). Persists on release; re-applied on startup in the store's `initialize()`. Built for
  the 4K "everything is massive" case.

### Backend summary
- **Migration v53** (`migration_v53_persist_imported_drop_data`) — table rebuild, drops the FK
  cascade on the two imported-aggregate tables. Applied cleanly through the full chain.
- New commands: `search_database_harvested`, `get_enemy_harvest_stats` (both registered in
  [lib.rs](src-tauri/src/lib.rs)). `ui_scale` added to `AppSettings`.
- **Tests:** 2 regression tests in `kill_tracking_commands.rs::tests` — removing an imported
  source keeps its merged data, and re-import-after-removal replaces (no double-count).

### Release / branch note
`origin/main` was 1 commit ahead of `dev` (the `release: v0.9.19` version bump never merged
back). Merged `origin/main` into `dev` **before** releasing so the Release workflow's
`git push origin main` fast-forwards. Released as **v0.9.20** (`--ref dev -f version=patch`).
Next session: as usual `dev` will lag `main` by the v0.9.20 release commit — merge it back before
the next release.

---

# glogger — Session Handoff

**Date:** 2026-06-20 (Session 17 — Linux WebKit DMABUF crash: broadened fix + v0.9.19)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (was @ `origin/main` v0.9.18 `c6b43a2`; now ahead with the broaden fix)
**Status:** ✅ **Shipped.** The NVIDIA-gated DMABUF workaround from v0.9.18 left **Mesa**
(AMD/Intel) users still crashing. Broadened it to apply on **all Linux** and released
**v0.9.19** via the Release workflow.

## TL;DR — Session 17 (WebKitGTK DMABUF crash, broadened)

**Trigger:** A user couldn't launch glogger on Linux. First screenshot was **AppImage v0.8.7**
(Fedora 43), then a retest on **v0.9.18** (Nobara 43 / KDE) **still crashed** — both
`WebKitWebProcess` aborting with **SIGABRT** before any window appeared.

**Diagnosis:**
1. **Flatpak CI is healthy** — not the problem. The crash is the native AppImage/.deb, and
   it's a **runtime** WebKitGTK **DMABUF-renderer abort**, not a build failure.
2. **v0.9.17/v0.9.18 already had a fix** (`c6b43a2`, parallel session on machine `gcfbrian`):
   `apply_nvidia_webkit_workaround()` set `WEBKIT_DISABLE_DMABUF_RENDERER=1` — **but only when
   proprietary NVIDIA was detected**.
3. **The reporter is on Mesa, not NVIDIA.** v0.9.18's coredump showed `libgbm` from
   `mesa-26.1.0` + `libglvnd`, no NVIDIA nodes → the gate never fired → still crashed. The
   crash is **not** NVIDIA-specific; bleeding-edge Mesa 26.1 hits it too.

**The fix (this session):** renamed `apply_nvidia_webkit_workaround` →
**`apply_webkit_dmabuf_workaround`** in [lib.rs](src-tauri/src/lib.rs) — now sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` **unconditionally on Linux** (still skips if the user set
the var; opt back in with `…=0`). Dropped the NVIDIA device-node detection entirely. Doc
[flatpak-build.md](docs/flatpak-build.md) updated to match. `cargo check` clean (note: the
`#[cfg(target_os="linux")]` body isn't compiled on the Windows dev box — real verification is
the Linux CI build + the user re-testing v0.9.19).

**Rationale:** disabling DMABUF compositing has negligible perf cost for this data-tracker UI,
and trying to fingerprint every affected driver/Mesa-version combo is a losing game. Two
distinct stacks (NVIDIA + Mesa 26.1) already crash, so go unconditional.

---

# glogger — Session Handoff

**Date:** 2026-06-20 (Session 16 — Vendor council earnings tracker)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (v0.9.16)
**Status:** ✅ **Shipped.** Second "Earned" tab on the Vendor Councils widget tracks councils
earned per NPC (current period + lifetime). Committed `bb91b0b`, pushed (`dev`→`main`, clean
fast-forward — both at `bb91b0b`). `npm run tauri build` (release) compiled clean (frontend
`vue-tsc` + Rust release build both green).

## TL;DR — Session 16 (Vendor council earnings tracker)

**Problem:** The Vendor Councils widget only showed *currently available* councils per vendor.
When a vendor's weekly gold reset (the 168h timer), you lost all record of how much you'd earned
from that NPC that week. **Fix:** a second **Earned** tab that mirrors the first, showing councils
earned per NPC this period **and** lifetime.

### Data model — two new columns, no new table
**Migration v52** ([migrations.rs](src-tauri/src/db/migrations.rs)) — `ALTER TABLE
game_state_npc_vendor ADD COLUMN councils_earned_current INTEGER DEFAULT 0` +
`councils_earned_lifetime INTEGER DEFAULT 0`. (v51 was Session-15's Combat Wisdom.)

### Backend — accrue on sale, zero `current` on reset
[game_state.rs](src-tauri/src/game_state.rs) `VendorSold` arm now pulls the `price` field (was
`..`-ignored) and does `councils_earned_current = councils_earned_current + price` (same for
lifetime) in the upsert. The `VendorGoldChanged` arm — which already owns the 168h reset detection
via `vendor_gold_timer_start` — now **zeros `councils_earned_current`** in the same `CASE` that
detects an expired timer (both the `current < max` branch, comparing
`datetime(timer_start,'+168 hours') < last_confirmed_at`, and the `>= max` / full-reset branch).
Lifetime is **never** touched by the reset paths.

### Frontend — new tab + new row component
- [VendorCouncilWidget.vue](src/components/Dashboard/widgets/VendorCouncilWidget.vue) — added an
  `activeTab` ref (`'available' | 'earned'`) with a tab toggle at the top. The Available tab is the
  entire prior widget (view-mode + item-type filter hidden on the Earned tab). Earned tab reuses the
  same category-grouping machinery: `earnedVendorEntries` (filters to vendors with any earnings from
  `gameState.vendorByNpc[key].councils_earned_*`), `earnedCategories`, and grand totals
  `grandTotalEarnedCurrent` / `grandTotalEarnedLifetime`. Earned rows render **current / lifetime**.
- New [EarnedVendorRow.vue](src/components/Dashboard/widgets/EarnedVendorRow.vue) — minimal row
  (NpcInline + area + `earnedCurrent.toLocaleString()` / `earnedLifetime`), no quick-edit/aggregate
  machinery (those are Available-tab concerns).
- [types/gameState.ts](src/types/gameState.ts) — `GameStateVendor` gained
  `councils_earned_current: number` + `councils_earned_lifetime: number`.

### ⚠️ Caveats / not-yet-verified
- **Earned tab is active-character only** (reads `gameState.vendorByNpc`, the live per-character
  store). Unlike the Available tab it has **no All-Characters aggregate view** — the
  `get_aggregate_vendor` command doesn't return the new columns. If cross-character lifetime totals
  are wanted, extend `aggregate_commands.rs` + `AggregateVendorEntry`.
- **Earnings accrue only from `VendorSold` (Player.log)** — same source as the widget's existing
  sale tracking. Councils gained any other way aren't counted here (this is "earned *from this
  vendor*", by design).
- **Not exercised live** — Tauri `invoke()` doesn't work under the browser-only preview tool
  (`get_cache_status … Cannot read properties of undefined (reading 'invoke')`), so the Earned tab
  wasn't click-tested. The release build **compiled** clean. Next session: run `npm run tauri dev`
  natively, sell to a vendor, confirm the Earned tab increments and survives a reset. Migration v52
  also hasn't been observed applying on a real DB yet.

---

# glogger — Session Handoff (Session 15)

**Date:** 2026-06-20 (Session 15 — Combat Wisdom tracker widget)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (v0.9.15)
**Status:** ✅ **Shipped.** New Combat Wisdom dashboard widget, verified live by piloting the
dev build, committed/pushed (`dev`→`main`). `cargo test --lib` (410 pass, incl. 9 new) +
`vue-tsc --noEmit` clean.

## TL;DR — Session 15 (Combat Wisdom widget)

New **Combat Wisdom** dashboard widget: tracks wisdom earned this session (which monster, how
much), plus a per-monster reuse-cooldown countdown until each monster can grant wisdom again.

### The log signal (authoritative — from the user's own Chat logs)
Chat.log `[Status]` channel, same path as Prodigy XP / Councils:
```
[Status] You earned 64 Combat Wisdom: Killed the Aktaari Queen
[Status] You earned 73 Combat Wisdom: Defeated Elite Tactician
[Status] You earned 5 Combat Wisdom: Killed The Productivity Expert (Gazluk)
[Status] You earned 1000 Combat Wisdom: Earned a Prodigy Level   ← non-monster
```
Verbs seen: `Killed` (named/boss), `Defeated` (elite, repeats in minutes), rare
`Destroyed`/`Disabled`/`Disconnected`, and `Earned a Prodigy Level` (1000, no monster).

### Cooldown model (user-approved): empirical + wiki fallback, persisted + backfilled
The log never states a monster's class, so per-monster cooldown is **learned** from the shortest
real gap (≥ 60s, to skip duplicate emits) ever observed between that monster's awards, **capped at
the wiki max (24h)** — the observed gap is only an upper bound, so a rarely-killed mob can't show
an absurd multi-week timer, while a real boss still shortens below 24h (~3h) as data accrues.
`Defeated`/elite = no cooldown (always Ready). Fallback before any gap is observed: 24h for
`Killed`, 0 for `Defeated`. History is **persisted in SQLite** and **backfilled from
`ChatLogs/Chat-*.log`** on startup (idempotent) so cooldowns are meaningful immediately.

### Files
- **Parser** [chat_status_parser.rs](src-tauri/src/chat_status_parser.rs): `CombatWisdomEarned`
  variant + `try_combat_wisdom_earned` (wired before `try_xp_gained`; the ` Combat Wisdom: `
  infix disambiguates). 5 unit tests.
- **Migration v51** [migrations.rs](src-tauri/src/db/migrations.rs): `combat_wisdom_earns` table +
  `idx_cw_dedup` unique index `(earned_at, source_name, amount)`. (v50 was already taken by the
  Session-14 corpse-extracts migration.)
- **New** [combat_wisdom_commands.rs](src-tauri/src/db/combat_wisdom_commands.rs):
  `record_combat_wisdom_earn` (skips non-monster/prodigy awards — NULL `source_name` can't dedup;
  they still count live on the frontend), `get_combat_wisdom_monsters` (aggregates per monster:
  last_earned epoch-ms, count, total, `min_gap_secs` ≥ 60), `aggregate_monsters` helper (tested),
  `backfill_from_chat_logs`. 4 unit tests. Registered in [db/mod.rs](src-tauri/src/db/mod.rs).
- **Coordinator** [coordinator.rs](src-tauri/src/coordinator.rs): persists monster awards using
  the **local** timestamp captured *before* the UTC conversion — must match the backfill (which
  doesn't convert) so live + backfill dedup on the same day. Event still emits via the existing
  `chat-status-event` for the live frontend.
- **lib.rs**: registers the 2 commands + runs a one-shot startup backfill (idempotent).
- **Frontend** [gameStateStore.ts](src/stores/gameStateStore.ts): `CombatWisdomEarned` in the
  event union, `combatWisdomSession` accumulator (resets on login), `combatWisdomMonsters` +
  `fetchCombatWisdomMonsters`. New [CombatWisdomWidget.vue](src/components/Dashboard/widgets/CombatWisdomWidget.vue)
  (registered in [dashboardWidgets.ts](src/components/Dashboard/dashboardWidgets.ts), `medium`,
  next to XP Rate / Prodigy): session total, earned-this-session list, live cooldown countdowns
  (soonest-ready first, green **Ready** when elapsed).

### Verified live (piloted the dev build via computer-use)
Startup backfill ingested **559 historical awards**; cooldown list rendered populated and ticking.
Caught + fixed the multi-week-countdown bug live (the 24h cap above) — Mega-Spider 1168h → 18h.
The only thing NOT exercised: a live wisdom kill bumping "Earned this session" (no wisdom-granting
mob was available during the session) — worth a click-through next time you kill one.

### Gotcha (computer-use, recurring)
Two glogger installs: `request_access("glogger")` resolved to the **portable** exe
(`a:\portableapps\glogger\glogger.exe`) and masked the dev window. Fix: grant the exact basename
`"glogger.exe"` → resolves to `…\target\debug\glogger.exe`. Also had to free port 1420 from a
stale Vite (`npm run tauri dev` aborts if 1420 is taken).

---

## TL;DR — three things landed this session

### 1. Corpse-search drop-rate model (finished the Session-13 pivot)
The DB now logs **every lootable kill** (each `Search Corpse of X` you have permission to loot,
even if it dropped nothing), and loot dedup is by item **instance_id** so two separate single-stacks
of the same item off one corpse stay as two rows.

- **Parser** ([player_event_parser.rs](src-tauri/src/player_event_parser.rs)) — new
  `PlayerEvent::CorpseSearched { timestamp, corpse_entity_id, corpse_name, has_permission }`,
  emitted in `handle_talk_screen` for `Search Corpse of X` titles. `has_permission =
  !line.contains("You do not have permission to loot this corpse")`.
- **Migration v49** ([migrations.rs](src-tauri/src/db/migrations.rs)) — wipes the old near-empty
  `enemy_kills`/`enemy_kill_loot`/`player_prev_ingests` rows, adds `enemy_kill_loot.instance_id`
  (signed-32-bit stored bit-preserved as INTEGER) + UNIQUE index `(kill_id, instance_id)`.
- **Coordinator** ([coordinator.rs](src-tauri/src/coordinator.rs)) — chat-FATALITY `EnemyKilled`
  no longer persists a kill (still **emits `enemy-killed`** for the live farming UI). `recent_kills`
  re-keyed `u32 corpse_entity_id → (kill_id, Instant)`. `process_corpse_search` inserts one kill row
  per permission corpse (FIRST-search timestamp, `INSERT OR IGNORE` on the dedup index), caches
  entity_id→kill_id. `attribute_loot_to_kills` handles both `CorpseSearched` and `LootPickedUp` in
  batch order, inserting loot with `instance_id` via `INSERT OR IGNORE`.
- **Backfill** ([replay.rs](src-tauri/src/replay.rs)) — `ingest_kill_loot_from_logs` rewritten to
  **Player.log-only** (no Chat.log pairing); character + UTC base date from the `Logged in as
  character … Time UTC=… Timezone Offset …` line. Server absent in Player.log → `"Unknown"` fallback.
  `IngestResult` dropped `kills_skipped_no_window`; `ingest_player_log` lost its `chat_log_path` arg.
- **Verified live**: user's "crypt test" farm produced a session with 38 kills / 25 items; rows land.

### 2. Drop-rate hover on the Session History tab
[HistoricalTab.vue](src/components/Farming/HistoricalTab.vue) — each expanded session's item rows now
have a hover tooltip (on the **quantity** block, `cursor-help`) showing the **lifetime drop-rate
breakdown** via the existing [ItemDropBreakdownTable.vue](src/components/Farming/ItemDropBreakdownTable.vue)
(`get_item_drop_sources`, scope `combined`). Attached to the qty, **not** the item name, so
`ItemInline`'s own hover + click-to-detail (which the user likes) are untouched — no nested tooltips.
Caveat: a saved session persists only per-item `net_quantity` + per-enemy `kill_count`, not per-item→
per-enemy attribution, so this shows **lifetime** rates, not session-scoped X/Y kills.

### 3. Butchering/skinning harvest detail in the item hovers
For skinning/butchering yields, the hover now shows **skill level at harvest, equipment bonus, and
your anatomy level for that monster type** — all parsed from Player.log.

- **Parser** — `PlayerEventParser` keeps a running `skill_levels` table (from `ProcessLoadSkills` +
  `ProcessUpdateSkill`) and tracks the most-recent `Anatomy_<Family>` reading (`last_anatomy`).
  `CorpseExtract` gained `skill_level`, `equipment_bonus`, `anatomy_family`, `anatomy_level`.
  `skill_level` = the `raw` on the triggering Skinning/Butchering update line; `equipment_bonus` =
  the corpse body's `(with a +N skill bonus from equipment)` (parsed in `handle_talk_screen` via
  `parse_equipment_bonus`); anatomy = `last_anatomy` (family comes free from the co-occurring
  `Anatomy_<Family>` XP update — no monster→family table needed).
- **Migration v50** — new `corpse_extracts` table (one row per extract: character/server, corpse,
  item, qty, skill, skill_level, equipment_bonus, anatomy_family, anatomy_level, extracted_at).
  Coordinator persists `CorpseExtract` to it; new Tauri command `get_corpse_extract_details(item_name)`
  ([kill_tracking_commands.rs](src-tauri/src/db/kill_tracking_commands.rs)) aggregates per-corpse
  (MAX skill/anatomy level = current, since those only rise).
- **Frontend** — Active Session: [ItemDropBreakdown.vue](src/components/Farming/ItemDropBreakdown.vue)
  extract mode shows chips (`Skinning 62`, `+12 equip`, `Anatomy: Canines 44`), carried through
  `FarmingExtractLoot`/store. History: new
  [ExtractDetailTable.vue](src/components/Farming/ExtractDetailTable.vue) queries the command and
  renders a "Harvested" section (empty/hidden when the item isn't an extract).
- **Limits** (told to user): only captures when Skinning/Butchering grants XP (a maxed skill emits no
  update → no extract detected, pre-existing); anatomy uses the most-recent `Anatomy_<Family>` update
  so a rapid multi-corpse autopsy burst could mis-attribute; "equipment" is the `+N` bonus, not a tool
  name (the log carries no tool name at harvest time — user chose the `+N` bonus).

### Next session / open items
- `enemy_kills.killing_ability`/`health_damage`/`armor_damage` are always empty/0 in the corpse-search
  model — fine for drop rates; killer/damage data would come from the `ProcessTalkScreen` corpse body
  (see the Python reference's `damage_log`) if ever wanted.
- History drop-rate hover is lifetime-only; to make it session-scoped, persist per-item→per-enemy loot
  attribution when a farming session is saved.

---

## (Session 13 — superseded by the above; kept for context)

**Date:** 2026-06-20 (Session 13 — Historical log backfill + drop-rate model PIVOT)
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (now at v0.9.14, even with `main`)
**Status:** ⚠️ **WORK IN PROGRESS — uncommitted, mid-pivot. Do NOT commit as-is.**

## TL;DR for next session

Goal this session: let players backfill the lifetime drop-rate DB from old Player.log files
(e.g. sessions played without glogger running). I built a first version, then a verification +
the user's reference parser revealed the **whole drop-rate attribution model is wrong**, and the
user confirmed a **pivot**. The pivot is **designed and decided but only ~10% coded**.

### What's on disk right now (uncommitted, `git status`)
`coordinator.rs, replay.rs, migrations.rs, lib.rs, settings.rs, DatabaseTab.vue,
GeneralSettings.vue, settingsStore.ts, types/farming.ts` (+ `Cargo.toml` = line-endings only, ignore).
This is the **first (chat-FATALITY-based) version** — it compiles (cargo check + vue-tsc clean)
but is being **replaced** by the pivot. Decide: keep useful scaffolding (watcher, settings,
idempotency, Database scan button) and rewrite the attribution core.

### The critical finding
Checked the live dev DB: **119 kills but only 2 loot rows.** Live loot attribution matches the
**chat-FATALITY kill's entity_id** against the **Player.log corpse entity_id** — they don't match,
so almost no loot is ever attributed. The whole drop-rate DB has been silently near-empty.

### The reference parser (the user's, authoritative model)
`C:\Users\bwfre\Downloads\parser_slim_loot.py` — READ THIS FIRST next session. Key lessons:
1. **Denominator = Player.log `Search Corpse of X` events** (lootable corpses), keyed by the
   corpse `entity_id` + enemy name. **NOT chat FATALITY.** Within Player.log the corpse entity_id
   is consistent for both the denominator and the loot, so attribution is reliable (this is the
   fix for 2/119).
2. **No-permission corpses** (`(You do not have permission to loot this corpse.)`) are **excluded**
   from the denominator — not your kill, can't observe drops.
3. **Loot dedup uses a per-event index** (their `item_index`) so two separate single-stacks of the
   same item off one mob stay as two rows. **glogger's analog = the item `instance_id`.** (This is
   the user's "don't let dedupe delete duplicate loot" requirement.)
4. **Player.log is self-sufficient** — character comes from its `Logged in as character … Time
   UTC=mm/dd/yyyy … Timezone Offset …` line. So historical backfill needs **only Player-prev.log,
   NO Chat.log pairing** (the chat-pairing code I wrote can be deleted).

### User decisions (confirmed via AskUserQuestion)
- **Pivot to the corpse-search model: YES.**
- **Clear the existing 119 chat-FATALITY rows on migration: YES.**
- Earlier (still holds): keep live writes + dedupe on ingest; "log every kill" = every *lootable*
  (permission) searched corpse counts (a no-permission corpse isn't your kill, so excluding it
  also satisfies "log every kill").

### Remaining work — the realigned plan (tasks #10–12 superseded by this)
1. **Migration v49**: `DELETE FROM enemy_kill_loot; DELETE FROM enemy_kills;` then
   `ALTER TABLE enemy_kill_loot ADD COLUMN instance_id INTEGER;` +
   `CREATE UNIQUE INDEX idx_kill_loot_dedup ON enemy_kill_loot(kill_id, instance_id);`
   (NOTE: v48 — UNIQUE on `enemy_kills(character,server,enemy_entity_id,killed_at)` + the
   `player_prev_ingests` table — is **already applied** to the dev DB from a test run; keep it.)
2. **Parser** (`player_event_parser.rs`, `handle_talk_screen` ~line 2296 where `Search Corpse of`
   is detected): emit a new `PlayerEvent::CorpseSearched { timestamp, corpse_entity_id,
   corpse_name, has_permission }`. `has_permission = !body_text.contains("You do not have
   permission to loot this corpse")`. (Frontend type + coordinator `_ => {}` arms unaffected.)
3. **Coordinator**: STOP writing chat-FATALITY kills to `enemy_kills` (but KEEP emitting
   `enemy-killed` — the live farming-session UI uses it). Repurpose `recent_kills` as
   `corpse_entity_id → (kill_id, Instant)`. On `CorpseSearched` w/ permission: dedupe by entity_id
   (a corpse fires many `Search Corpse` lines — one kill row per corpse, FIRST-search timestamp),
   `INSERT OR IGNORE` a kill row, cache entity_id→kill_id. `attribute_loot_to_kills`: look up
   `recent_kills[corpse_entity_id]` (now matches LootPickedUp.corpse_entity_id) and insert
   `enemy_kill_loot(kill_id, item_name, quantity, instance_id)` via `INSERT OR IGNORE`.
4. **Historical ingest** (`replay.rs`): rewrite to **Player.log-only** corpse-search model. DELETE
   the chat-pairing helpers (`find_chat_log_for_player_log`, `parse_combat_message` use,
   `parse_searched_corpse_entity`). Parse the `Logged in as character` line for character/server +
   UTC base date. Run lines through `PlayerEventParser`; on `CorpseSearched` insert kill (dedupe by
   entity_id); on `LootPickedUp` attribute by corpse entity_id + insert loot w/ instance_id
   `INSERT OR IGNORE`. Remove `kills_skipped_no_window` from `IngestResult`. Keep the content-hash
   idempotency + `player_prev_ingests`.
5. **Frontend**: update `IngestResult` type (drop `kills_skipped_no_window`) + the DatabaseTab
   scan message.
6. **Add a Rust unit test** for the corpse-search ingest with synthetic Player.log lines —
   **important** because the current real Player.log + Player-prev.log have **no corpse searches**
   (the goblin session rotated out), so there's no live data to verify against right now.

### Gotchas
- **killed_at parity**: live and the Player-prev backfill must produce the *same* `killed_at` for a
  given corpse so the `(char,server,entity_id,killed_at)` unique index catches the live-vs-rotation
  overlap (content-hash only catches re-reading the *same* file). Use the FIRST `Search Corpse`
  line's timestamp per corpse in BOTH paths, with the date derived from the Player.log login line.
- A corpse is searched by **multiple** `Search Corpse of X` lines (one per looted item) — collapse
  to ONE kill row per corpse entity_id.
- `enemy_kills`/`enemy_kill_loot` are used ONLY by the drop-rate feature (coordinator, replay,
  `kill_tracking_commands`, migrations) — safe to repurpose. `get_enemy_kill_stats` /
  `get_item_drop_sources` + the Database tab import/export work unchanged on the repurposed tables.

### Scaffolding already built this session that the pivot KEEPS (mostly)
- `settings.rs`: `auto_ingest_player_prev` (default true) + `get_player_prev_log_path()` +
  `get_auto_ingest_player_prev()`. `settingsStore.ts` + `GeneralSettings.vue` toggle. (Keep.)
- `replay.rs`: `spawn_player_prev_watcher` (mtime poll, 10s delay then every 30s) + content-hash
  idempotency + `ingest_player_log` Tauri command (registered in `lib.rs`). (Keep; rewrite the
  ingest body to Player.log-only corpse-search.)
- `coordinator.rs` `persist_enemy_kill` → `INSERT OR IGNORE` + existing-id lookup. (Superseded —
  this fn stops being called from the chat path; logic moves to a `persist_corpse_search`.)
- `DatabaseTab.vue`: "Scan a log file now" button + `doScan`. (Keep.)

---

**Date:** 2026-06-20
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (version string v0.9.13)
**Outcome:** **Farming UI layout/parity pass + Database tab (community drop-rate sharing).**
Gathered yields split into separate Skinning/Butchering and Mining/Survey boxes (stacked,
independently scrollable). Active/History views unified — per-skill XP is now a hover
popup with a bar chart in both, matching the existing item hover pattern. Fixed a real
cropping bug: `HistoricalTab` had no scroll container of its own under `PaneLayout`'s
`overflow-hidden` content slot. New third **Database** tab: searchable monster/item
lookup with My Data / Imported / Combined scope, plus JSON export/import of lifetime
drop-rate data (tagged by source so re-imports never double-count). Backend (migration
v47, new commands), frontend type-check, and `cargo check` all clean; verified live.

---

## Session 11 — Farming layout split, History/Active parity, Database tab (2026-06-20)

**Outcome:** See banner above. Four-part frontend+backend feature, built off Session 10's
farming work (Looted Items / Skinning & Butchering / Gathered columns).

### 1. Split Gathered into two stacked boxes

- `extracts` (skinning/butchering, keyed by corpse) and a new `gathered` field (mining/
  survey, keyed by node/survey source) on `FarmingSession` — previously both shared one
  `extracts` bucket, conflating two different source types.
  [types/farming.ts](src/types/farming.ts), [farmingStore.ts](src/stores/farmingStore.ts)
  (`recordGathered` now writes to `s.gathered`; new `gatheredItems` computed +
  `sessionSourcesForGathered` helper).
- [FarmingSessionCard.vue](src/components/Farming/FarmingSessionCard.vue): the third grid
  column is now a `flex flex-col` of two independently-scrollable boxes — **Skinning &
  Butchering** (tan, top) and **Mining & Survey** (blue, bottom) — sitting between Looted
  Items and the Activity Log. Each row still hovers into
  [ItemDropBreakdown.vue](src/components/Farming/ItemDropBreakdown.vue), which gained a
  third `mode="gathered"` (session-only, same treatment as `extract` — no enemy entity, no
  lifetime DB data, plain-text source label).

### 2. Active/History UI parity + XP hover popup + cropping fix

- **Root cause of the cropping bug** (screenshot showed "Work Order for…" cut off at the
  window edge): [PaneLayout.vue](src/components/Shared/PaneLayout.vue) wraps its default
  slot in `overflow-hidden`. `HistoricalTab.vue` had no scroll container of its own, so at
  larger `ui_font_size` settings its content silently clipped instead of scrolling. Fixed
  by giving the tab root `h-full overflow-y-auto`.
- New [XpBreakdownChart.vue](src/components/Farming/XpBreakdownChart.vue) — small
  hand-rolled horizontal bar chart (no chart lib, consistent with `ItemDropBreakdown`'s
  style), one bar per skill, used as hover-popup content.
- **Active Session**: the "Total XP" quick-stat is now wrapped in `EntityTooltipWrapper`
  (0.5s delay) with `XpBreakdownChart` in the tooltip slot — same interaction pattern as
  hovering an item. The left Skills panel (per-skill progress bars) was left as-is; this is
  additive, not a replacement.
- **History tab**: the inline always-visible skill-chip row (`+6,968 Shield 24,331/hr…`)
  was the main source of horizontal clutter contributing to the cropping. Replaced with the
  same hover pattern on each session row's `+X XP` summary (`xpSkillsFor(session)` maps DB
  skill rows to the chart's shape). The expanded detail's Items/Favor/Kills sections are now
  boxed (`bg-surface-dark border border-border-default rounded-lg p-3`) in a
  `grid-cols-[repeat(auto-fit,minmax(220px,1fr))]` layout, each capped at `max-h-56
  overflow-y-auto` — matches Active Session's panel styling and scrolls internally instead
  of growing unbounded.

### 3. Backend: community drop-rate database (import/export/search)

- **Design decision (from the user):** rather than picking "merge" vs. "keep separate," the
  Database tab exposes all three as views: **My Data**, **Imported**, **Combined**. This
  requires imported data to be stored completely separately from personal ground truth.
- **Migration v47** ([migrations.rs](src-tauri/src/db/migrations.rs)): three new tables —
  `imported_kill_sources` (label/display_name/imported_at), `imported_enemy_kills_agg`,
  `imported_enemy_kill_loot_agg` (both FK'd to the source label, `ON DELETE CASCADE`). Never
  touches `enemy_kills`/`enemy_kill_loot` (the player's own ground truth).
- [kill_tracking_commands.rs](src-tauri/src/db/kill_tracking_commands.rs) rewritten:
  - `get_enemy_kill_stats` / `get_item_drop_sources` gained a **required** `scope: String`
    (`"mine" | "imported" | "combined"`) param — combines local rows + imported aggregates
    in Rust (`combine_loot_rows`) rather than a SQL UNION, since the two table shapes
    differ. **Breaking signature change** — updated all 3 existing call sites
    ([EnemyBrowser.vue](src/components/DataBrowser/EnemyBrowser.vue),
    [ItemSearch.vue](src/components/DataBrowser/ItemSearch.vue),
    `farmingStore.fetchEnemyStats`) to pass `scope: "combined"` (preserves prior behavior
    exactly, since "combined" == "mine" until something is imported).
  - New `search_database_enemies` / `search_database_items` — `LIKE`-based substring search
    (case-insensitive) across `mine`/`imported`/`combined`, for the Database tab's
    autocomplete-style search box.
  - **Export** (`export_kill_loot_database`): aggregates the player's own `enemy_kills` +
    `enemy_kill_loot` into a JSON bundle (`ExportBundle`) — **personal data only**, never
    re-exports previously-imported data (avoids a pyramiding double-count risk if exports
    get re-shared and re-imported). No character name, server, or per-kill timestamps in
    the bundle — just `{enemy_name, total_kills, loot: [{item_name, total_quantity,
    times_dropped}]}`.
  - **Import** (`import_kill_loot_database`): `source_label` = the imported file's name.
    Re-importing the same filename **deletes and replaces** that label's rows first inside
    a transaction — idempotent, no double-counting on repeat imports of the same file.
  - `list_imported_sources` / `delete_imported_source` for managing what's been imported.
  - All 7 new/changed commands registered in [lib.rs](src-tauri/src/lib.rs).

### 4. Frontend: Database tab

- Third tab added to [EconomicsFarmingView.vue](src/components/Economics/EconomicsFarmingView.vue)
  (`Active Session | Session History | Database`).
- New [DatabaseTab.vue](src/components/Farming/DatabaseTab.vue): scope toggle (My Data /
  Imported / Combined), monster-vs-item search target toggle, debounced (250ms) search box,
  expandable result rows (drill into [EnemyDropTable.vue](src/components/Farming/EnemyDropTable.vue)
  / [ItemDropBreakdownTable.vue](src/components/Farming/ItemDropBreakdownTable.vue) for the
  full per-item/per-enemy drop-rate breakdown), Export/Import buttons via
  `@tauri-apps/plugin-dialog` (`save`/`open`, matching the existing pattern in
  `dev-panel/tabs/DebugCaptureTab.vue`), and a management list of imported sources with
  per-source delete.

## Caveats / known limits

- Export intentionally only ever exports **personal** data ("mine"), never "imported" or
  "combined" — re-sharing someone else's re-shared data was judged a real data-integrity
  risk (no way to detect a chain of re-exports double-counting the same original
  contribution across multiple community files).
- Search is substring `LIKE` over raw internal names (not display names) — works for
  partial-name search but won't catch CDN display-name aliases.
- `EnemyKillStats.loot` was previously returned pre-sorted only by "mine"; combined-scope
  sorting now happens after the in-Rust merge.

## Verification

1. `npx vue-tsc --noEmit` clean; `cargo check` clean.
2. Live dev build: migration v47 applied with no errors (`App version changed (0.9.12 ->
   0.9.13)`, `App is interactive` reached). Confirmed no migration/sqlite errors in the
   startup log.
3. Did not exercise mining/survey gathering or the import/export file dialogs live this
   session (no node/survey access at hand) — logic is covered by type-check + cargo check
   only; flagging for the next session to click through.

---

## Session 10 — Statehelm widget: drop high-favor NPCs (2026-06-20)

**Outcome:** Small follow-up to the Session-8 Statehelm Gifting widget. NPCs now fall
off the widget based on **favor standing** (not just weekly gifts), so you stop seeing
NPCs that no longer benefit from gifting. `vue-tsc` clean; trusted in by the user
without a live re-test (small, localized change).

- **`useStatehelmTracker.ts`** — new `isExcludedByFavor(status)`, applied inside
  `representativeFor()` next to the maxed-gift filter (so an excluded NPC falls off and
  the next-highest skill of that category backfills the slot):
  - **Soul Mates** (top tier) → always excluded.
  - **Like Family** → excluded **unless** the NPC offers storage (storage capacity keeps
    scaling past Like Family, so storage NPCs stay worth gifting). Uses the existing
    `hasStorage()` from `useNpcServices`.
  - Tiers below Like Family are unaffected.
- **`docs/.../widget-statehelm-summary.md`** — falloff section documents the new rules.
- Commit `ffb6344`. Pushing `dev:main` hit the usual release-bump divergence (main had
  `release: v0.9.13` + PR-merge commits dev lacked); resolved per the standing gotcha —
  `git merge origin/main` into `dev` (clean, version strings only), then push. Both
  branches now at `151f14b`.

---

## Session 9 — Survey loot summary from Chat.log `[Status]` (2026-06-19)

**Outcome:** The Economics → Surveying **session loot summary was always empty**
for the user; now it populates live and matches Kaeus' GorgonSurveyTracker ("GST")
summary. Root-caused, fixed, full `cargo test` green (399 + 5 new), **verified live**.

### 🔎 Root cause

`loot_summary_for_session` ([survey/commands.rs](src-tauri/src/survey/commands.rs))
only counts `item_transactions` rows linked via `source_details->>'survey_use_id'`,
and that link was injected **only** on the Player.log path
(`survey::aggregator::process_event`, driven by `ProcessAddItem`/mining context).
PG's verbose Player.log item logging wasn't reaching the user, so the summary stayed
empty even while loot flowed. **Kaeus' GST reads the always-present Chat.log
`[Status]` "added to inventory" lines** — which glogger already parsed and wrote to
`item_transactions` (`source='chat_status'`) but **never linked to the session**
(NULL `source_details`/`item_type_id`). Note: **"GST" == Kaeus' GorgonSurveyTracker
exe**, downloaded/launched by `gst_manager.rs` — not an in-house tracker.

### ✅ The fix (chat-authoritative, native — committed `34160f8`)

- **[survey/persistence.rs](src-tauri/src/survey/persistence.rs)** —
  `latest_use_id_for_session`: most-recent use to attach chat loot to.
- **[survey/aggregator.rs](src-tauri/src/survey/aggregator.rs)** —
  `attribute_chat_gain`: attributes a `[Status]` loot gain to the active session's
  latest survey use, bumps `loot_qty` + loot timestamps, returns the `survey_use_id`.
  (Chat lines carry no node/map id → attribution is at the **use** level, fine for
  the sequential use→collect→use workflow.) +3 unit tests.
- **[coordinator.rs](src-tauri/src/coordinator.rs)** — the chat `ItemGained` handler
  now always populates `internal_name`/`item_type_id` (needed for valuation), calls
  `attribute_chat_gain` for `loot`-context gains, and tags the row
  `source_kind='survey_chat'` + `source_details` `{survey_use_id}`.
- **[survey/commands.rs](src-tauri/src/survey/commands.rs)** —
  `loot_summary_for_session` is **chat-authoritative**: per use, count `survey_chat`
  rows when present; **else fall back to Player.log rows** (`NOT EXISTS` guard) so
  historical/pre-existing sessions still render and no gain is double-counted. +2
  tests. Analytics zone/type totals come from the denormalized `loot_qty` (which the
  chat path bumps), so they populate for free — **no analytics query changes needed.**

### ⚠️ Caveats / known limits

- **Speed-bonus is lost on the chat path** — `[Status]` lines have no "(speed bonus!)"
  marker, so chat-attributed loot has `bonus_qty = 0`. The speed-bonus **analytics**
  (zone/type bonus CTEs) remain Player.log-only; they'll show 0 for chat-only users.
- A hypothetical user whose **Player.log attribution also works** would see
  `loot_qty` (denorm) double-bumped, but the user-visible loot **table** is
  de-duplicated by the `survey_chat`-wins `NOT EXISTS` guard. Real users are
  chat-only (the whole reason for this work), so no double-count in practice.
- The harmless `Cargo.toml` LF↔CRLF artifact reappeared (dev server re-touches it);
  **left uncommitted** — `git checkout -- src-tauri/Cargo.toml` to clear.

### Repo state at end of session

- `dev` @ **`34160f8`**, pushed. `main` @ **`5cab469`** (merge of dev onto the
  v0.9.13 release commit), pushed. Version strings unchanged at **v0.9.13** (feature
  merge, not a release). Working tree clean (modulo the Cargo.toml EOL artifact).
- Memory written: `memory/project_survey_chat_loot.md`.
- `npm run survey-test` (accuracy replay) **couldn't run locally** — it needs
  `docs/samples/CDN-full-examples/items.json`, absent in this checkout (panics before
  replaying; unrelated to this change).

---

## Session 8 — Statehelm widget skill-driven rework + gift-count fixes (2026-06-19)

**Outcome:** The dashboard **Statehelm Gifting** widget (`statehelm-summary`) now
surfaces the NPCs that train the player's most-relevant skills instead of an
alphabetical "fewest gifts first" list, and two real bugs in gift tracking were
root-caused and fixed. All type-checked (`vue-tsc` clean), `cargo test` green (107
parser tests incl. 2 new), and **verified live** by the user.

### ✨ Widget rework — `useStatehelmTracker.ts` + `StatehelmSummaryWidget.vue`

Two labeled sections driven by the player's own skills:
- **Combat:** NPCs for the **2 equipped** combat skills + the player's **top 4**
  combat skills (by **base level**) — up to `4 + equipped` slots, equipped first and
  marked with a gold ✦.
- **Non-Combat:** NPCs for the player's **top 2** non-combat skills.

Design rules (decided with the user):
- **Skill → combat/non-combat** comes from the CDN `combat` flag (`get_all_skills`,
  cached module-level so both StatehelmView + the widget share one fetch).
- **NPC category = "combat wins ties"**: an NPC that trains *any* combat skill is a
  combat NPC; only purely non-combat NPCs are eligible for the non-combat section.
- **Skill → NPC**: when several Statehelm NPCs train a skill (e.g. Geology → 3), the
  one with the **highest current favor standing** represents it (`tierIndex`).
- **Falloff + backfill**: `representativeFor()` skips any NPC already at 5/5 this
  week, so when *any* NPC (equipped included) is maxed it drops off and the
  next-highest skill of that category backfills the slot — the combat section walks
  equipped-first then ranked, capped at `4 + equipped`, deduped by NPC.

### 🐞 Fix 1 — Statehelm Sewers NPCs no longer appear

`statehelmNpcs` filtered on `area.includes('statehelm')`, which also matched
**"Statehelm Sewers"** (`AreaName == AreaStatehelmCaves`) — home to the Pig/Rabbit
animal-form trainers (Hamilton, Fuzzlebun) that have **no** weekly 5-gift cap. Filter
now requires `area_name === 'AreaStatehelm'` exactly (the city proper, 63 NPCs).

### 🐞 Fix 2 — bulk-gifting a stack only counted as 1

Root cause: gifting a **stack** of N identical items emits a **single**
`ProcessDeltaFavor(npcId, "NPC_X", delta, True)` line with N× the per-item favor
(confirmed: Corinth `58.464 = 5 × 11.6928`). The gift log counted one row per favor
event → undercount. **But the game prints the authoritative count** in the gift
dialog: `<Npc> will accept up to <b>5</b> gifts per calendar week and has received
<b>N</b> so far this week` (in `ProcessTalkScreen` post-gift / `ProcessPromptForItem`
pre-gift).

- **`player_event_parser.rs`** — new `PlayerEvent::GiftCountObserved { npc_name,
  received, max }` + `parse_gift_count` (matches the sentence substring, walks back
  from `" will accept up to <b>"` past the last `". "` to capture multi-word names
  like "Sir Brooker"). Dispatched before the `ProcessTalkScreen` branch. 2 unit tests.
- **`game_state.rs`** — `GiftCountObserved` arm **reconciles** that NPC's gift-log
  rows for the current week (Monday 00:00 UTC, new `current_week_start_utc()` helper)
  up/down to the game's exact count, leaving other NPCs and the manual +/- controls
  intact. Coexists with the existing per-`FavorChanged` insert (which still covers
  non-Statehelm gifts): the reconcile is absolute, not additive, so no double count.
  - **Note:** the reconcile only fires when the game re-emits that note (i.e. next
    time you open/complete a gift dialog for that NPC) — it won't retroactively fix a
    gift given before this build without re-opening the NPC's gift dialog.

### Files touched

- `src/composables/useStatehelmTracker.ts` (sewer filter + skill-driven targets)
- `src/components/Dashboard/widgets/StatehelmSummaryWidget.vue` (two-section UI)
- `src-tauri/src/player_event_parser.rs` (`GiftCountObserved` + parser + tests)
- `src-tauri/src/game_state.rs` (reconcile handler + `current_week_start_utc`)
- `docs/features/screens/dashboard/widget-statehelm-summary.md` (behavior doc)

### ⚠️ Gotcha discovered (dev-loop tooling)

Running `cargo test` while `npm run tauri dev` is mid-rebuild **collides on the
`target/` build lock** — it wedged the dev process and the app exited. If you need to
run tests during a live session, stop the dev build first (or expect to relaunch it).
Cleanup used here: kill `cargo`/`glogger`, free port 1420, relaunch `npm run tauri dev`.

---

## Session 7 — Flatpak CI resurrection + v0.9.12 prep (2026-06-20)

**Outcome:** The Flatpak bundle hadn't attached to a release since v0.9.2. Root-
caused it (two separate problems), fixed the real gap, verified the full
pipeline live against two real releases, committed the uncommitted button work,
and synced `dev`→`main` so v0.9.12 picks everything up.

### 🔎 Why the Flatpak "bricked" (two problems, not one)

1. **`gh: command not found` (the visible v0.9.2 failures).** The build always
   *succeeded* — it produced a valid `glogger.flatpak` (~7.8 MB) and uploaded it
   as a workflow artifact. The only failure was the final `gh release upload`
   step, which ran **inside the GNOME-47 flatpak-builder container** where the
   `gh` CLI doesn't exist. This was **already fixed** back in commit `fadbc47`
   (two-job split: build in container, `attach` on a bare runner) — but that fix
   landed *after* the v0.9.2 runs and had never actually been exercised.

2. **It never auto-triggered after v0.9.2 (the real, unfixed gap).** `release.yml`'s
   `prepare` job pushes the tag with `git push origin main --tags` using the
   default `GITHUB_TOKEN`. **GitHub suppresses workflow triggers from tags pushed
   by `GITHUB_TOKEN`** (anti-recursion), so `flatpak.yml`'s `push: tags` trigger
   never fired for v0.9.5/8/9/10/11. v0.9.2 only worked because that tag was
   pushed *manually* from a Linux box (a real user push, which does trigger).

### ✅ The fix (committed `5a92271`, on `main`)

- **`flatpak.yml`** — added a `workflow_call` trigger (+ a required `tag` input on
  `workflow_dispatch`), and a top-level `TARGET_TAG: ${{ inputs.tag || github.ref_name }}`
  resolver that works for all three triggers. The build job now `checkout`s
  `TARGET_TAG`; the `attach` job derives `TAG` from it and runs for any trigger
  (`if: inputs.tag != '' || startsWith(github.ref, 'refs/tags/')`).
- **`release.yml`** — new `flatpak` job (`needs: [prepare, publish]`) that
  **calls** `flatpak.yml` via `uses:` + `with: { tag: <new tag> }`, bypassing the
  anti-recursion block entirely. Every future release now builds + attaches a
  Flatpak with **zero manual steps**.

### ✅ Verified live (real releases, not exp)

- Dispatched the (already-fixed) two-job workflow against **v0.9.10** → built +
  attached `glogger.flatpak` (7.85 MB). Both jobs green.
- Same against **v0.9.11** → attached `glogger.flatpak` (7.84 MB). Both jobs green.
- So v0.9.10 and v0.9.11 now *have* Flatpaks; the build mechanics + `gh`-fix are
  proven end-to-end. The **auto-trigger `workflow_call` path itself** is YAML-valid
  and on `main` but its *first real exercise* will be the v0.9.12 Release run. If
  it hiccups, it'll be in that run's `flatpak` job; fallback is still
  `gh workflow run Flatpak --ref vX.Y.Z` (uses the `tag` dispatch input).

### ✅ Also committed this session

- **`aa26337` feat(menubar): one-click live-tailing toggle** — the Session-6
  uncommitted 📡 button is now committed (vue-tsc clean). No behavior change from
  the Session-6 description.
- Reverted the harmless `Cargo.toml` CRLF-only artifact instead of committing it.

### Repo state at end of session

- `dev` and `main` both at **`9ac3036`**, working tree clean, version strings at
  **v0.9.11** (merged the `release: v0.9.11` bump from main into dev, then
  advanced main to dev's tip — they're identical now).
- **Ready for v0.9.12:** trigger the **Release** workflow as usual; it will now
  also produce the Linux `.flatpak` automatically.

### ⚠️ Gotcha for next time

- A literal `git push origin dev:main` fast-forward was **not** possible because
  `main` carried the bot's `release: v0.9.11` version-bump commit that `dev`
  lacked. Resolution: `git merge origin/main` into `dev` first (absorbs the bump,
  conflict-free — only version strings), then push `dev:main`. Expect this every
  release, since the Release workflow commits the bump to `main` only.

---

## Session 6 — Gourmand live tracking + live-tailing button (2026-06-19)

**Committed/pushed:** Gourmand real-time eaten-food tracking — the open
investigation from Session 5 is **solved**. **In the working tree
(uncommitted):** an "all-in-one" live-tailing toggle button.

### ✅ Gourmand live tracking (committed `b635038`, pushed to origin/dev)

The Session 5 blocker ("no reliable signature for food-eaten in Player.log")
turned out to have a clean answer that needs **no** instance-ID/effect-ID
resolution at all:

```
[23:19:03] LocalPlayer: ProcessDoDelayLoop(1.5, Eat, "Using Human-Style Pizza with Tofu", 5845, AbortIfAttacked)
```

`ProcessDoDelayLoop(1.5, Eat, "Using <Food Name>", …)` fires **exactly once per
food eaten**, with the food name in **plain text**. The parser already emits
this as `PlayerEvent::DelayLoopStarted { action_type: "Eat", label, .. }`.
Confirmed against the user's live log with all 3 foods eaten that day
(Human-Style Pizza with Tofu, Candied Evu Fruit, Ratkin Survival Cheese).

- **`db/gourmand_commands.rs`** — new `record_food_eaten(conn, food_name)`:
  `INSERT … ON CONFLICT(food_name) DO UPDATE SET times_eaten = times_eaten + 1`.
  Leaves `manually_marked` untouched so a manual flag survives a live update.
- **`coordinator.rs`** — new match arm on the `PlayerEvent` side-channel:
  `DelayLoopStarted { action_type, label, .. } if action_type == "Eat"` strips
  the `"Using "` prefix and calls `record_food_eaten`, then emits the existing
  `gourmand-updated` event. The frontend `gourmandStore` already listens for
  that event, so the UI refreshes live with **zero frontend changes**.
- **Coexists with the `/report gourmand` import** (no double-count): report
  import still does `DELETE WHERE manually_marked = 0` + full resync, which
  cleanly reconciles any live increments to the authoritative report numbers.
- `cargo check` clean; all 392 lib tests pass.

### 🚧 All-in-one live-tailing button (UNCOMMITTED in working tree)

Goal: one header button (gold 📡, left of the ⚙ gear) to enable/disable live
tailing of **all** log files at once, and to bake in the old manual workaround
("save a fresh file in-game, then refresh the app"). User confirmed **both**
halves of that workaround were needed.

- **glogger can only automate the "refresh" half** — it reads files; it cannot
  make Project Gorgon flush fresh data to disk. So the button does the
  automatable half and shows a toast reminding the user to do the in-game save.
- **`MenuBar.vue`** — new 📡 button; `isAllTailing` computed (both watchers
  active); `toggleAllTailing()`: if all on → stop both; else start whichever
  aren't running → `startPolling()` → `pollWatchers()` (forced catch-up read,
  the "refresh" half) → `refreshStatus()` → info toast. Wired in `useToast`.
- **`coordinatorStore.ts`** — new `pollWatchers()` action wrapping the existing
  backend `poll_watchers` command (already registered in `lib.rs`).
- `vue-tsc --noEmit` clean. **Not yet verified live** — testing was blocked
  (see gotcha below). Open question for the user: the reminder toast currently
  fires on *every* enable; consider gating it to once-per-session or a
  "don't remind me again" pref if noisy.

### ⚠️ Gotchas discovered this session

- **Two `glogger.exe` installs confuse computer-use.** Start-menu "glogger"
  resolves to a **portable install** at `a:\portableapps\glogger\glogger.exe`,
  which is a release version **behind** the dev build. The dev build runs from
  `src-tauri\target\debug\glogger.exe` (different path) → computer-use masks the
  dev window because the grant locked onto the portable exe. To drive the dev
  build via computer-use, close the portable install or grant the
  `target\debug` exe by exact path/basename. Alternative used here: watch the
  dev build's stdout log for catch-up/poll lines while the user clicks.
- `npm run tauri dev` auto-rebuilds on `Cargo.toml` change (picked up the
  v0.9.10 version bump without a manual restart).

### Repo state at end of session

- `dev` fast-forwarded to **v0.9.10** (origin/main had released it; it was just
  version-string bumps on work dev already had). `origin/dev` now also at
  v0.9.10 after the push.
- `dev` is clean except the **uncommitted button work** (`MenuBar.vue`,
  `coordinatorStore.ts`) + a harmless `Cargo.toml` CRLF artifact (LF↔CRLF only,
  no content change — safe to `git checkout --`).

---

## Session 5 — Words of Power: wiki-sourced categories + CSV export (2026-06-19)

**Outcome:** Widget now groups discovered words by category → level (instead of one flat list per
power name), and can export the full word list to CSV (date/time discovered, word, power name,
category, level). `cargo check`, `cargo test`, and `vue-tsc --noEmit` all clean.

- **New: `src-tauri/src/db/word_of_power_catalog.rs`** — static `(power_name → category, level)`
  lookup table derived from https://wiki.projectgorgon.com/wiki/Words_of_Power (per the user's
  direction to use the wiki as source of truth). The wiki organizes scrolls into six tiers (levels
  0/3/5/7/9/19); several effect names recur across tiers with longer durations at higher tiers
  (e.g. "Super Jumping"). Since the discovery log line only gives the effect name — not which tier
  scroll produced it — each name is pinned to the **lowest** tier it appears at (documented in the
  file's module doc-comment as a deliberate, acknowledged approximation). Unknown names fall back
  to `category: "Unknown", level: None` rather than failing. Two unit tests cover known/unknown
  lookups.
- **`words_of_power_commands.rs`** — `WordOfPower` struct gained `category: String` and
  `level: Option<u32>`; both `get_words_of_power` and `add_word_of_power` now construct rows via a
  new `WordOfPower::with_catalog_lookup(...)` helper that calls into the catalog.
- **`WordsOfPowerWidget.vue`** — regrouped: top-level collapsible **category** sections, each with
  a **Level N** (or "Level unknown") sub-heading, individual words listed underneath (now showing
  `power_name` inline since it's no longer the group header). New **Export to CSV** button next to
  the word list, reusing the existing generic `export_text_file` command + `@tauri-apps/plugin-dialog`
  `save()` pattern (same approach as `craftingStore.ts`/`gourmandStore.ts`).

### Dropped this session: Gourmand real-time tracking investigation

Explored whether "food eaten" could be detected live from Player.log (currently requires a manual/
auto import of the in-game `/report gourmand` text file). Findings, in case this gets picked up
again:
- `DeleteContext::Consumed` in `player_event_parser.rs` is defined but **never constructed** —
  there's no existing signal.
- Found a plausible candidate pattern in a live log window (~21:21–21:24 same-day): an item delete
  immediately followed by `ProcessAddEffects`, occasionally paired with a Gourmand `ProcessUpdateSkill`
  XP gain. But the item's instance ID had no prior `ProcessAddItem` in that session (item was already
  in inventory before tailing started), so it never resolved to a name in `item_transactions`, and the
  corresponding effect ID wasn't in `game_state_effects` either (whose `effect_name` column is mostly
  `NULL` anyway — effect-ID→name resolution is incomplete project-wide).
- Conclusion: no reliable signature is in hand yet. To pin one down, would need either (a) exact
  food name + wall-clock time from the user to search a narrow window, or (b) tail the live log while
  the user eats something with a fresh PlayerLog session (so the item has a resolvable AddItem first).
  **Do not guess at a heuristic from ambiguous data** — risks corrupting the eaten-foods table.

---

# Previous Session

**Date:** 2026-06-19
**Machine:** Windows 11 (primary dev box)
**Branch:** `dev` (created from `main` @ v0.9.9; both synced to v0.9.9)
**Outcome:** **Economics → Farming** session overhaul. The active-session item view is now a
4-column layout — **Skills | Looted Items | Gathered | Activity Log** — where each item is
hover-interactable (0.5s) and pops a per-source drop-rate breakdown. New backend
`CorpseExtract` event distinguishes skinning/butchering yields from loot-table drops. Mining
and survey gains are tracked by source in the Gathered column. Type-checked; 105 parser tests
pass; verified live in the dev build.

---

## TL;DR (this session)

- **Looted Items** column = true corpse loot only (`LootPickedUp`, i.e. `ProcessRemoveLoot`).
  Headline = total quantity looted this session. Hover → [ItemDropBreakdown](src/components/Farming/ItemDropBreakdown.vue):
  per-enemy session drops **+ all-time drop rate & loot-table share** from the DB
  (`get_enemy_kill_stats`).
- **Gathered** column = everything that isn't a loot-table drop, tagged by source skill:
  `SKINNING`/`BUTCHERING` (corpse extracts), `MINING`, `SURVEY`. Hover → session-only per-source
  breakdown (no all-time DB data exists for these).
- Removed the old kill-gate that silently dropped corpse loot when the kill wasn't tracked this
  session — loot now always shows (creates the enemy entry with `count: 0`).

## What is a "looted item" vs not (decided with the user)

- **Looted Items** = `LootPickedUp` only (fires from the corpse loot window; excludes
  skinning/butchering, which grant Butchering/Skinning XP and produce **no** `ProcessRemoveLoot`).
- **Gathered** = skinning/butchering extracts + Mining/SurveyMapUse provenance gains. Filtered by
  **provenance, not item type** — so it captures everything a node/survey yields, not just things
  named "ore/metal." (User accepted this; motherlodes often log no node name → bucket
  "Mining (unknown node)".)
- Vendor buys, storage withdrawals, and craft outputs are excluded from both columns (they still
  appear in the Activity Log via `ItemAdded`/`ItemStackChanged`).

## Implementation (this session)

- **Backend** — [player_event_parser.rs](src-tauri/src/player_event_parser.rs):
  - New `PlayerEvent::CorpseExtract { item_name, item_type_id, quantity, skill, corpse_name }`.
  - `parse_corpse_extract` hooks `ProcessUpdateSkill` lines: when `type=Butchering`/`Skinning`,
    the just-added item (`last_item_event`) is the extract — emit `CorpseExtract` and consume it.
  - `parse_remove_loot` now sets `last_item_event = None` after resolving, so a true loot drop can
    never leak into the extract path.
  - Coordinator needs no change — `PlayerEventParsed` events are batched/forwarded regardless of
    kind (`_ => {}` arms); `CorpseExtract` is **not** persisted to the DB (session-only).
- **Frontend store** — [farmingStore.ts](src/stores/farmingStore.ts):
  - `LootPickedUp` handler de-gated (records even for untracked kills; falls back to
    "Unknown enemy" when no corpse-search context).
  - `CorpseExtract` handler + `recordGathered(s, provenance, item, qty)` helper that routes
    Mining/SurveyMapUse gains into `extracts` keyed by source (node name / survey map).
  - `s.extracts: Record<sourceName, Record<itemName, { quantity, drops, skill }>>` (new session
    field). Computeds: `lootedItems`, `extractedItems`; helpers: `sessionEnemiesForItem`,
    `sessionEnemiesForExtract`, `fetchEnemyStats` (DB cache keyed by enemy name).
  - Loot tally type changed to `{ quantity, drops }` (per-item drop count for drop-rate math).
- **Frontend UI**:
  - [FarmingSessionCard.vue](src/components/Farming/FarmingSessionCard.vue) — grid is now
    `240px 1fr 1fr 280px`; Looted Items + Gathered are separate boxes; each row uses
    `EntityTooltipWrapper` (`:delay="500"`, interactive) with the breakdown in `#tooltip`. Item
    names are resolved to display names via a local `displayName()` (plain text, **not**
    `ItemInline`, to avoid a competing nested tooltip on the hover target).
  - [ItemDropBreakdown.vue](src/components/Farming/ItemDropBreakdown.vue) — `mode: 'loot' | 'extract'`.
    Loot mode fetches all-time DB stats and shows a drop-rate bar + "% of table"; extract mode is
    session-only (source shown as plain text since it may be a node/survey, not an enemy).

## Caveats / known limits

- `CorpseExtract`/Gathered are **session-only** — no all-time history (the `enemy_kill_loot` DB
  table only records `LootPickedUp`).
- `ItemAdded` carries no quantity to the frontend, so new-stack gains count as `+1` (existing
  `itemDeltas` behavior); stack growth via `ItemStackChanged` uses the real delta.
- Survey source names are the raw internal map name (not yet resolved to a display name).
- A stale HMR session object (created before `extracts` existed) once blanked the panel via
  `Object.values(undefined)`; computeds/handlers now guard with `?? {}`. A clean restart clears it.

## Verification (this session)

1. `npx vue-tsc --noEmit` clean; `cargo test --lib player_event_parser` → 105 passed.
2. Confirmed against a real Player.log: corpse loot (`Goblin Calling Card`, `Goblin Skirt`,
   `Health Potion`) → Looted Items; butchering extracts (`Goblin Skull`, `Impressive Goblin
   Skull`, which have Butchering XP and no `RemoveLoot`) → Gathered.
3. App runs interactive in the dev build; Start Session + both columns render.

---

# Previous Session

**Date:** 2026-06-18
**Machine:** Windows 11 (primary dev box)
**Outcome:** Two new dashboard widgets — **XP Rate** (combat vs. prodigy XP/hr + generalized
ETA) and **Prodigy Tracker** (same rates + a user-entered current-XP baseline for a precise
ETA). Backend prodigy-XP parsing + store wiring + widgets, all type-checked and unit-tested.
Verified live in the dev build.

---

## TL;DR

- **XP Rate** widget (`xp-rate`): three divided sections — Combat XP/hr + Combat XP/session,
  Prodigy XP/hr + Prodigy XP/session, and Next prodigy level ETA (generalized: assumes a full
  250M). Combat lines white, prodigy lines gold. Reset button.
- **Prodigy Tracker** widget (`prodigy-tracker`): identical rate sections, plus a persisted
  "Current prodigy XP" input, a progress line + bar (`current / 250M (%)`), and a Next-prodigy-
  level ETA based on *actual remaining* XP. `current = entered baseline + session prodigy XP`.
- New backend parser for prodigy XP, new store accumulator, two new Vue widgets. No coordinator
  change needed (it already broadcasts every `chat-status-event`).

---

## Prodigy XP model (confirmed from a real Chat.log)

- A **maxed combat skill** earns **Prodigy XP** as overflow. Each kill emits a PAIR of
  `[Status]` lines: `You earned N Prodigy XP in <Skill>.` (prodigy, from the maxed skill —
  e.g. "Pig") and `You earned N XP in <Skill>.` (normal XP for the skill being leveled).
- **One prodigy level = 250,000,000 combat XP** (per the Prodigy Potential wiki). The game
  does **not** report current progress within a level, so the ETA assumes a full 250M:
  `ETA = 250,000,000 ÷ prodigy XP/hr`.
- Wiki (https://wiki.projectgorgon.com/wiki/Prodigy_Potential) is sparse; the format above
  came from the user's own log.

## Implementation

- **Backend** — `src-tauri/src/chat_status_parser.rs`: new `ChatStatusEvent::ProdigyXpGained`
  variant + `try_prodigy_xp_gained` (runs before `try_xp_gained`; normal XP lines are unaffected
  because `"N Prodigy"` fails the u32 parse). Two new unit tests. Emitted via the existing
  `chat-status-event` (coordinator `_ => {}` arm already forwards it — no change there).
- **Store** — `src/stores/gameStateStore.ts`: `xpRateSession` accumulator (combat vs prodigy,
  wall-clock keyed so the rate stays live). Combat line is filtered to combat skills only via
  `get_combat_skills` (CDN `Combat: true`), loaded lazily in `loadAll` and cached in
  `combatSkillNames`. Helpers: `accrueXpRate`, `xpRateOf(kind, nowMs)`, `prodigyEta`,
  `resetXpRateSession` (also called from `resetSessionSkills`, i.e. on character login).
  `formatEta` (shared ETA formatter) and `prodigyEta` exported for the widgets.
  Cases added in `handleChatStatusEvent` for `XpGained` (combat-filtered) and `ProdigyXpGained`.
- **UI** — both widgets registered in `dashboardWidgets.ts` (small, right after Live Skill
  Tracking), each with a 1s ticking clock and compact K/M number formatting:
  - `XpRateWidget.vue` (`xp-rate`) — generalized ETA (assumes a full 250M).
  - `ProdigyTrackerWidget.vue` (`prodigy-tracker`) — adds a "Current prodigy XP" input persisted
    via `useViewPrefs('widget.prodigy-tracker', { startXp })`; `currentXp = startXp + session
    prodigy XP` (capped at 250M); ETA = `(250M − currentXp) ÷ prodigy XP/hr`. Shows a progress
    bar + `Ready!` at cap. NOTE: Reset zeroes the shared session accumulator, so after a Reset
    you must re-enter your current in-game prodigy total.

## Design decisions (from the user)

1. **Split:** non-prodigy line = non-maxed **combat** skills only (crafting/tradeskill excluded);
   prodigy line = the maxed-skill overflow.
2. **ETA:** assume a full 250M per level (no current-progress source in the logs).
3. Session totals are shown as their own line items grouped with the matching rate.

## Open items / next steps

- The Prodigy Tracker's current-XP baseline is entered by hand. If a current-prodigy-progress
  value is ever found in the logs (an attribute, UI value, or log line), auto-populate it so the
  XP Rate widget's ETA can also count down from real remaining XP without manual input.
- The rate window is session-start → now (noisy in the first few seconds, settles as you play);
  Reset restarts it. Could add a rolling/“last N minutes” window if the lifetime average feels stale.
- Per the previous handoff, the crafting flat-4×-first-craft / repeat-craft drop-off model still
  wants wiring into **Quick Calc** (`resolveRecipeIngredients` only sets `xp_first_time = base × 3`
  and doesn't model repeat-craft drop-off yet).

## Release / CI note (unchanged)

Always use the **Release** workflow (`release.yml`, workflow_dispatch) — it's the only path that
builds Windows/macOS/Linux and creates the release. A bare tag push only triggers `flatpak.yml`
(this is what left v0.9.2 without a Windows `.exe`). The Flatpak `attach` job waits (up to 30 min,
polling) for the release to exist before `gh release upload`.

---

# Session 2 — Survey investigation + signed-ID fix (2026-06-18, on v0.9.5)

**Outcome:** Found and fixed the root-cause bug behind unreliable survey tracking (signed
instance IDs). Explored a native survey-map/trilateration engine, then **reverted it** at the
user's request — branch is back to clean **v0.9.5 plus the ID fix only**.

## ✅ Kept: signed instance-ID parser fix (`player_event_parser.rs`)

- **Bug:** Project: Gorgon logs item instance IDs as **signed 32-bit** ints (often negative, e.g.
  `ProcessDeleteItem(-1796085135)`). The parser read them as `u64`, so `parse::<u64>()` failed on
  every negative ID and the parser **silently dropped the whole line** — losing ~half of all item
  add/delete/loot events (coin-flip on the sign bit). This is why surveys tracked only
  intermittently: a survey map's craft/consume/loot were dropped whenever its instance ID was
  negative.
- **Fix:** shared `parse_instance_id(&str) -> Option<u64>` that parses `i64` then casts `as u64`
  (bit-preserving, so the same string maps to the same registry key). Routed all 8 item-instance
  parse sites through it (add, updateitemcode, delete, vendor sold/update, storage add/remove,
  remove-loot). Entity/NPC IDs (`u32`) left as-is — separate concern.
- **Test:** `test_negative_instance_id_add_and_delete_resolve` (uses the real Rubywall log lines).
- **Status:** uncommitted in the working tree; 390 lib tests pass, `vue-tsc` clean. Memory note:
  `memory/project_signed_instance_ids.md`.

## 🔎 Findings worth keeping (for if the survey work resumes)

- **glogger's survey tracker is 100% Player.log-driven**, not Chat.log. The aggregator
  (`survey::aggregator`) only sees `ItemAdded`/`ItemDeleted`/`DelayLoopStarted` from the player
  event path (coordinator ~L846). Chat `[Status]` lines never reach it. So integrating
  GorgonSurveyTracker (which reads Chat.log `[Status]` distance msgs) was never going to work —
  different signals, different files.
- **Crystal/mineral (geology) surveys** reveal exact node coords in Player.log via
  `ProcessMapFx((x,y,z), …, "X is here", …, "The X is 395m west and 873m south.")`. **Motherlode
  (mining) surveys** give **distance only** (`"The treasure is N meters from here."`) — the game
  withholds location by design (wiki confirms: no red dot, circle of radius N). So motherlode
  location *requires* trilateration; crystals don't.
- **Player position is never logged continuously.** Only at `SPAWNING LOCAL PLAYER AT (x,y,z)`
  (login/zone-entry, ~4×/session) and derivable from any directional `ProcessMapFx` (node coord +
  bearing → invert for player pos). No movement/heading stream. This is the real blocker for
  passive motherlode trilateration — you can't know where you stood at each distance reading.
- **Bare zone maps:** the cleanest source is the **game client itself** — Unity Addressable
  bundles at `<PG install>/WindowsPlayer_Data/StreamingAssets/aa/StandaloneWindows64/maps_assets_assets_art_maps_map_area<zone>.png_<hash>.bundle`,
  one `Texture2D` each (~1024–2048px, label-free, coordinate-aligned). Extractable with UnityPy
  (Python prototype worked for all 14 outdoor zones). **Key insight:** extract from each user's
  *own* install at runtime → zero redistribution of Elder Game's art. The community wiki maps
  (`*MarkedMap.jpg`) are cluttered and have murky licensing by comparison.

## ↩️ Reverted (not in the tree anymore)

The native survey UI was built and then removed at the user's request: top-level **Survey
Tracking** header (Sessions/Analytics/Map), `SurveyMapView.vue` (pan/zoom + node dots + 2-point
per-zone calibration), `surveyMapStore.ts`, the `ProcessMapFx` → `SurveyNodeLocated` parser/event,
and the staged `zone-maps/` assets. If resumed, the findings above are the starting point; the
extraction + parsing approaches were proven working before removal.

---

# Session 3 — Upstreaming the cprivitere (TwinkleofToes) fork PRs (2026-06-19, on v0.9.5→v0.9.8)

**Context:** TwinkleofToes (GitHub `cprivitere`, repo `cprivitere/glogger`) opened PRs against our
repo to fold in fixes he'd made for the abandoned "zenith" fork, intending to switch to us as his
upstream. Two PRs landed; both reviewed before merge.

## ✅ PR #1 — macOS fixes + UI tweaks (merged via #3)

Cherry-picked his 5 commits onto a branch, **excluded the `GeneralSettings.vue` rework** (extra
path fields/buttons — the author himself flagged the layout as rough) by reverting just that file
to main's version, then merged as **PR #3**. Kept:
- macOS Player.log / game-data **path split** (they live in different dirs on macOS): new
  `get_default_player_log_path()` + `get_default_game_data_path()` in `settings.rs`, wired through
  `setup_commands.rs` / `settingsStore.ts`. Windows path behavior preserved via empty-string fallback.
- Clickable MenuBar status dots (toggle tailing), Start Tailing button on the Inventory warning
  banner, default file-picker paths, and a **Cmd+Q fix** in `useKeyboard.ts` (tab-cycling handler
  no longer swallows ⌘Q). `vue-tsc` + `cargo check` both clean.

## ✅ PR #2 — Node/Action/npm version bumps (merged, then **two bugs fixed on main**)

Merged the Node 22→24, GitHub-Actions, and npm-lockfile bumps — **but the PR was broken** and the
author's "frontend build succeeds" claim missed it because the `validate` CI job only runs
`cargo check`/`cargo test`/`vite build`, none of which invoke `tauri build` (the step that enforces
version parity). Found and fixed both via live Experimental runs:

1. **`tauri-apps/tauri-action@v1` doesn't exist** — the action has no `v1` tag (latest major is
   `v0`, currently → v0.6.2). Every build job failed at action resolution. **Fixed** → pinned back
   to `@v0` in `release.yml` + `experimental.yml`.
2. **Tauri JS/Rust version mismatch** — PR bumped the **npm** packages (`@tauri-apps/api` 2.11.1,
   `plugin-dialog` 2.7.1) but left the **Rust crates** at `tauri` 2.10.3 / `tauri-plugin-dialog`
   2.6.0. `tauri build` requires matching major/minor across both sides → all builds failed.
   **Fixed (user's call: align npm *down* to the known-good Rust crates)** → pinned
   `@tauri-apps/api` to `~2.10.1` and `@tauri-apps/plugin-dialog` to `~2.6.0` (tilde so npm can't
   drift back into 2.11/2.7). All other npm bumps from PR #2 (vue/vite/tailwind/etc.) retained.
   0 npm vulnerabilities.

**Validation:** Experimental workflow run #27839157547 — **all 6 jobs green**, published
`v0.9.8-exp` prerelease with Windows/macOS/Linux installers. Both fixes exercised end-to-end.

## ⚠️ Gotchas discovered (read before touching CI again)

- **The `Experimental` workflow can only be dispatched from `main`.** Its `prepare` job does a
  hardcoded `git push origin main --tags`; dispatched from any other branch it fails instantly with
  `src refspec main does not match any` (before the build even runs). So there's **no branch-based
  dry-run** — you must merge to main first, then test. Each successful (or build-failed-but-prepare-
  succeeded) run **bumps the patch version on main and tags `vX.Y.Z-exp`**. This is why main walked
  0.9.5 → 0.9.8 over three runs; dangling `-exp` tags from failed runs were deleted as cleanup.
- If you want a true dry-run vehicle later, the workflow would need a guard to skip the
  commit/push/tag when dispatched off a non-main ref.

## 📋 State at end of session

- `main` @ **v0.9.8**, all fixes pushed. PRs #1, #2, #3 closed/merged.
- The Session 2 **signed-instance-ID fix** (`player_event_parser.rs`) was finally **committed** this
  session alongside this HANDOFF.md (390 lib tests green) — no longer dangling in the working tree.
- No real (non-exp) release cut this session — next real release via the normal `Release` workflow
  will pick up everything from 0.9.8.

---

# Session 4 — Settings path auto-detect buttons + startup auto-detect (2026-06-19, on v0.9.8)

**Outcome:** Added three path-management buttons and an on-startup auto-detect checkbox to
**Settings → General → Game Data Directory**. All platform-aware (Windows/macOS/Linux), type-checked,
`cargo check` clean, and **verified live end-to-end** in the dev build (including a restart test).

## What was added (UI: `src/components/Settings/GeneralSettings.vue`)

Replaced the old single "Use Default Player.log Location" button with:

- **Auto-Detect Game Path** — sets `gameDataPath` to the OS default via the existing backend command
  `get_default_game_data_path_command`.
- **Auto-Detect Player.log Path** — calls `get_default_player_log_path_command`; that returns an
  explicit path on macOS and **empty on Windows** (logs live inside the game folder), so it falls
  back to `<gameDataPath>/Player.log`. Mixed `\`+`/` separators are cosmetic and work fine on Windows.
- **Reset Paths** — restores both paths to the raw backend defaults (`logFilePath` ends up empty on
  Windows, i.e. derived at read time by `get_player_log_path`).
- **Checkbox "Auto-detect game & Player.log paths on startup"** — persists `autoDetectPathsOnStartup`.

Each button writes an inline status line (green = ok, red = unsupported OS / no default found).
Unsupported OSes (Linux) get a "set it manually" message instead of a blank path.

## Backend wiring

- **`settings.rs`** — new field `auto_detect_paths_on_startup: bool` (`#[serde(default)]`, default
  `false`); added to the `Default` impl. The platform path helpers (`get_default_game_data_path`,
  `get_default_player_log_path`) and their `*_command` Tauri wrappers already existed and are
  registered in `lib.rs`.
- **`lib.rs` — new "Step 2b"** runs right after `Settings loaded` and **before** the coordinator
  starts the log watchers (so refreshed paths take effect that same launch). When the flag is on and
  the default differs, it overwrites `game_data_path`/`log_file_path` and saves. Logs
  `Auto-detected paths on startup (enabled in settings)`. NOTE: `startup_log!` carries a trailing
  semicolon, so its match arms must be wrapped in `{ … }` blocks (bare expression form won't compile).
- **`settingsStore.ts`** — `autoDetectPathsOnStartup` added to both interfaces, both converters
  (`to`/`fromBackendSettings`), and `getDefaultSettings()`.

## Placement note (for the user)

Buttons/checkbox live under the **General** tab's existing "Game Data Directory" section (next to the
path field they act on), **not** the separate "App Settings" tab (which holds Appearance/font/opacity).
This was a deliberate UX call; mentioned to the user, easy to move if they prefer.

## Verification

1. `npx vue-tsc --noEmit` clean; `cargo check` clean.
2. Live: set the path field to a bogus value → each button restored/derived the correct path with a
   status line. Reset Paths restored platform defaults.
3. **Startup test:** enabled the checkbox, set `game_data_path` to `Z:\wrong\on\purpose`, confirmed it
   persisted, **restarted the dev build** → log showed `Auto-detected paths on startup`, `settings.json`
   was corrected back to the real LocalLow path, and Player.log catch-up started normally. (Test wrote
   to the **dev** profile `%APPDATA%\glogger.Dev\settings.json`, separate from the real install.)
