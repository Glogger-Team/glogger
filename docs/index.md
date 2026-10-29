# Documentation Index

## [TODO.md](TODO.md)
Small tasks and notes that don't belong in a dedicated plan.

## [flatpak-build.md](flatpak-build.md)
Building and running glogger as a Linux Flatpak: offline source generation, the manifest fixes that got it rendering, and runtime notes.

---

## Architecture

Core structure, patterns, and standards used across the app.

- [architecture-summary.md](architecture/architecture-summary.md) — High-level overview of the Rust/Tauri + Vue architecture and data flow.
- [live-event-streams.md](architecture/live-event-streams.md) — Unified reference for all live game events: Player.log and Chat.log data flow, all event streams, how features subscribe, timestamp handling, and how to extend both parsers.
- [player-event-parser.md](architecture/player-event-parser.md) — PlayerEventParser: event types, internal state, pending delete buffer, encoded value decoding, frontend listening, and how to extend.
- [player-log-events.md](architecture/player-log-events.md) — Complete reference for all Player.log event types: item, skill, NPC, vendor, storage, and screen events with encoding formats and practical patterns.
- [survey-mechanics.md](architecture/survey-mechanics.md) — Behavioral catalog of the three survey-map kinds (Basic, Motherlode, Multihit): use mechanics, loot patterns, map lists per area, and parser disambiguation rules.
- [game-state.md](architecture/game-state.md) — Centralized game state system: design principles, database schema, GameStateManager, frontend store API, and domain catalog.
- [cdn-data-parsing.md](architecture/cdn-data-parsing.md) — How CDN JSON files are deserialized, typed, and stored with raw JSON preservation.
- [cdn-field-schemas.json](architecture/cdn-field-schemas.json) — Machine-readable field inventory for all 27 CDN data types.
- [cdn-gap-analysis.json](architecture/cdn-gap-analysis.json) — Per-file comparison of CDN fields vs what our Rust parsers currently capture.
- [settings-file.md](architecture/settings-file.md) — How app configuration is stored and managed via the Rust settings system.
- [user-data-management.md](architecture/user-data-management.md) — Multi-character/multi-server data scoping, server auto-detection, character management, market values, and aggregate views.
- [shared-components.md](architecture/shared-components.md) — Reusable entity inline/tooltip components (ItemInline, NpcInline, etc.) and color tokens.
- [implementation-checklist.md](architecture/implementation-checklist.md) — Step-by-step checklists for common dev tasks (new parsers, DB tables, CDN fields, commands).
- [standards-persistence-naming.md](architecture/standards-persistence-naming.md) — Naming conventions, data persistence patterns, store/command/migration/type standards.
- [build-channels.md](architecture/build-channels.md) — Build channels (Dev, Release, Experimental): identifiers, data dirs, CI workflows, and data seeding behavior.
- [startup-sequence.md](architecture/startup-sequence.md) — Full startup sequence: backend init, frontend phases, game data loading, and readiness audit.
- [styling.md](architecture/styling.md) — CSS architecture using Tailwind v4, theme tokens, and component classes.
- [color-standards.md](architecture/color-standards.md) — Color usage audit: current state, inconsistencies, proposed semantic token palette, migration plan.
- [layout-patterns.md](architecture/layout-patterns.md) — Layout system: v-show navigation, TabBar, EmptyState, PaneLayout/SidePane, pane layout patterns.
- [ux-standards.md](architecture/ux-standards.md) — UX/UI standards: desktop-first design principles, keyboard navigation, layout rules, state persistence, empty states, toasts, visual consistency.
- [ux-composables.md](architecture/ux-composables.md) — UX composables: useKeyboard (nav/hotkeys), useToast (notifications), useViewPrefs (persistent screen preferences).
- [toast-system.md](architecture/toast-system.md) — Toast notification system: store, composable, container component, types, and usage guidelines.
- [time.md](architecture/time.md) — Time & timestamp standards: UTC storage, timezone offset detection, display modes, formatting API, and rules.
- [time-handling-audit.md](architecture/time-handling-audit.md) — Time handling audit: codebase-wide consistency check against time standards, findings and recommendations.
- [pipeline-structure.md](architecture/pipeline-structure.md) — Data pipeline architecture: CDN ingestion, player data flow, 4-bucket game state scoping, coordinator design.
- [capture-analysis-results.md](architecture/capture-analysis-results.md) — Devtools capture analysis: observed log events, parseable data formats, and feature opportunities identified from live gameplay captures.
- [widget-sizing-audit.md](architecture/widget-sizing-audit.md) — Dashboard widget sizing audit: per-widget height inventory, grid system analysis, inconsistencies, and proposed standards.

## Features

Cross-cutting feature documentation not tied to a single screen.

- [chat-parser.md](features/chat-parser.md) — Chat log parser: file format, line parsing, channel exclusion, item link extraction, watch rules.
- [chat-item-linking.md](features/chat-item-linking.md) — Detecting and linking item references in chat messages to CDN data.
- [advanced-settings.md](features/advanced-settings.md) — Advanced Settings tab: log reparsing, database statistics, and diagnostics.
- [dev-mode.md](features/dev-mode.md) — Dev Mode: toggle, dev panel window (game state, component showcase, testing helpers), hidden settings.
- [update-notifications.md](features/update-notifications.md) — Update notifications: GitHub release check, bottom bar indicator, toast alert.
- [trip-routing.md](features/trip-routing.md) — Trip routing: multi-zone route planner with zone graph, teleport-aware solver, bind location parsing.
- [statehelm-gift-backfill.md](features/statehelm-gift-backfill.md) — Statehelm weekly gift-status backfill from the game's own logs: reconcile rule, bounded rotated-log archive, session dating.

*Feature docs that were specific to a single screen have been merged into the corresponding screen docs below (character import → [character-stats](features/screens/character/character-stats.md), inventory import → [inventory-snapshots](features/screens/inventory/inventory-snapshots.md), farming → [economics-farming](features/screens/economics/economics-farming.md), surveying → [economics-surveying](features/screens/economics/economics-surveying.md), storage vaults → [inventory-vaults](features/screens/inventory/inventory-vaults.md), data browser → [data-browser](features/screens/data-browser.md), dashboard → [dashboard](features/screens/dashboard.md), gourmand → [character-gourmand](features/screens/character/character-gourmand.md)).*

## Screens

Per-screen documentation organized by view.

### Dashboard
- [dashboard.md](features/screens/dashboard.md) — Dashboard screen: widget registry, 6-column grid, drag-to-reorder, settings pane.
  - [widget-status.md](features/screens/dashboard/widget-status.md) — Status widget: weather, combat/mount, effects, currencies.
  - [widget-skill-tracking.md](features/screens/dashboard/widget-skill-tracking.md) — Live Skill Tracking widget: session XP gains.
  - [widget-items-incoming.md](features/screens/dashboard/widget-items-incoming.md) — Items Incoming widget: loot, crafts, summoned.
  - [widget-items-outgoing.md](features/screens/dashboard/widget-items-outgoing.md) — Items Outgoing widget: sold, stored, consumed.
  - [widget-councils.md](features/screens/dashboard/widget-councils.md) — Councils widget: gold currency changes.
  - [widget-current-zone.md](features/screens/dashboard/widget-current-zone.md) — Current Zone widget: area + NPCs with favor.
  - [widget-favor-changes.md](features/screens/dashboard/widget-favor-changes.md) — Favor Changes widget: NPC favor deltas.
  - [widget-notes.md](features/screens/dashboard/widget-notes.md) — Notes widget: personal checklist.

  - [widget-critical-resources.md](features/screens/dashboard/widget-critical-resources.md) — Critical Resources widget: tracked item quantities.
  - [widget-statehelm-summary.md](features/screens/dashboard/widget-statehelm-summary.md) — Statehelm Gifting widget: weekly gift progress.
  - [widget-watchword-alerts.md](features/screens/dashboard/widget-watchword-alerts.md) — Watchword Alerts widget: recent match feed.
  - [widget-death-tracker.md](features/screens/dashboard/widget-death-tracker.md) — Death Tracker widget: recent deaths, rez tracking, top rezzers.
  - [widget-words-of-power.md](features/screens/dashboard/widget-words-of-power.md) — Words of Power widget: auto-captured crafted words with age counters and click-to-copy.
  - [widget-trip-planner.md](features/screens/dashboard/widget-trip-planner.md) — Trip Planner widget: zone-to-zone route planner with teleport-aware solver.

### Character
- [character.md](features/screens/character.md) — Character screen: architecture, component hierarchy, data sources.
  - [character-skills.md](features/screens/character/character-skills.md) — Skills tab: two-panel layout, tracked skills, XP progression, CDN enrichment.
  - [character-stats.md](features/screens/character/character-stats.md) — Stats tab: character report import, report management.
  - [character-npcs.md](features/screens/character/character-npcs.md) — NPCs tab: favor progression, services, gift preferences.
  - [character-quests.md](features/screens/character/character-quests.md) — Quests tab: personalized quest reference with requirement eligibility.
  - [character-gourmand.md](features/screens/character/character-gourmand.md) — Gourmand tab.
  - [character-statehelm.md](features/screens/character/character-statehelm.md) — Statehelm tab: weekly gift tracker with NPC services and filters.
  - [character-buildplanner.md](features/screens/character/character-buildplanner.md) — Build Planner tab: combat build planning with mod/ability/CP management.
  - Account tab: server-wide aggregate view (wealth, inventory, skills across all characters). Uses `AggregateView.vue` from Dashboard components.

### Inventory
- [inventory.md](features/screens/inventory.md) — Inventory screen: architecture, component hierarchy, vault capacity models.
  - [inventory-live.md](features/screens/inventory/inventory-live.md) — Live Inventory tab: real-time tracking from player log.
  - [inventory-snapshots.md](features/screens/inventory/inventory-snapshots.md) — Storage reports tab: point-in-time browsing from Export Storage as JSON (Settings → VIP).
  - [inventory-vaults.md](features/screens/inventory/inventory-vaults.md) — Storage Vault Database tab: area-grouped vault browser with capacity tracking.

### Crafting
- [crafting.md](features/screens/crafting.md) — Crafting screen: architecture, component hierarchy, shared commands, design decisions.
  - [crafting-quickcalc.md](features/screens/crafting/crafting-quickcalc.md) — Quick Calculator tab.
  - [crafting-projects.md](features/screens/crafting/crafting-projects.md) — Projects tab: material breakdown, pickup list, shopping list, live crafting detection.
  - [crafting-price-helper.md](features/screens/crafting/crafting-price-helper.md) — Price Helper: integrated into Projects tab as a pricing mode with customer-provides and fee configuration.
  - [crafting-leveling.md](features/screens/crafting/crafting-leveling.md) — XP Leveling Optimizer tab.
  - [crafting-history.md](features/screens/crafting/crafting-history.md) — Crafting History tab.
  - [crafting-workorders.md](features/screens/crafting/crafting-workorders.md) — Work Orders tab.
  - [crafting-cookshelper.md](features/screens/crafting/crafting-cookshelper.md) — Cook's Helper tab.
  - [crafting-skills.md](features/screens/crafting/crafting-skills.md) — Skills tab: per-skill summaries with charts and recipe lists.
  - [crafting-dynamic-items.md](features/screens/crafting/crafting-dynamic-items.md) — Dynamic Items tab: configure which items are allowed for wildcard ingredient slots.
  - [crafting-brewery.md](features/screens/crafting/crafting-brewery.md) — Brewery tab: brewing discovery journal, per-player effect mapping, ingredient matrix.

### Economics
- [economics.md](features/screens/economics.md) — Economics screen: architecture, component hierarchy, market/farming/surveying.
  - [economics-market.md](features/screens/economics/economics-market.md) — Market Prices tab: player-maintained price database.
  - [economics-farming.md](features/screens/economics/economics-farming.md) — Farming tab: session-based profitability tracking.
  - [economics-surveying.md](features/screens/economics/economics-surveying.md) — Surveying tab (Session / Session History / Analytics): survey tracker on the provenance pipeline, schema, commands, A3 stitching.
  - [economics-stall-tracker.md](features/screens/economics/economics-stall-tracker.md) — Stall Tracker: shop log parsing, sales/revenue/inventory analytics from PlayerShopLog books.

### Chat Logs
- [chat.md](features/screens/chat.md) — Chat Logs screen: architecture, FTS search, item linking, shared components.
  - [chat-channels.md](features/screens/chat/chat-channels.md) — Channels tab: public/custom channel browser.
  - [chat-tells.md](features/screens/chat/chat-tells.md) — Tells tab: direct message conversations.
  - [chat-simple.md](features/screens/chat/chat-simple.md) — Party, Nearby, Guild, System tabs.
  - [chat-all.md](features/screens/chat/chat-all.md) — All Messages tab: global search with advanced filtering.
  - [chat-watchwords.md](features/screens/chat/chat-watchwords.md) — Watchwords tab: rule-based alerts and notifications.

### Data Browser
- [data-browser.md](features/screens/data-browser.md) — Data Browser overlay: architecture, layout, sidebar tabs (History/Favorites/Pinned), shared patterns, search/filter summary.
  - [data-browser-items.md](features/screens/data-browser/data-browser-items.md) — Items tab.
  - [data-browser-skills.md](features/screens/data-browser/data-browser-skills.md) — Skills tab.
  - [data-browser-abilities.md](features/screens/data-browser/data-browser-abilities.md) — Abilities tab.
  - [data-browser-recipes.md](features/screens/data-browser/data-browser-recipes.md) — Recipes tab.
  - [data-browser-quests.md](features/screens/data-browser/data-browser-quests.md) — Quests tab.
  - [data-browser-npcs.md](features/screens/data-browser/data-browser-npcs.md) — NPCs tab.
  - [data-browser-enemies.md](features/screens/data-browser/data-browser-enemies.md) — Enemies tab (translation strings + AI data).
  - [data-browser-areas.md](features/screens/data-browser/data-browser-areas.md) — Areas tab (NPCs, monsters, storage vaults).
  - [data-browser-effects.md](features/screens/data-browser/data-browser-effects.md) — Effects tab.
  - [data-browser-lorebooks.md](features/screens/data-browser/data-browser-lorebooks.md) — Lorebooks tab (book reader).
  - [data-browser-titles.md](features/screens/data-browser/data-browser-titles.md) — Titles tab.

### Search
- [search.md](features/screens/search.md) — Unified search: Scryfall-inspired structured query syntax, quick search overlay (Ctrl+F), dedicated search page, 14 searchable categories, relevance scoring.

### Settings
- [settings.md](features/screens/settings.md) — Settings screen: tabbed configuration for general, chat logs, user data, game data, advanced, and dev tools.

### Help
- [help.md](features/screens/help.md) — Help screen: tabbed reference hub with About, setup guide, changelog (GitHub releases), known issues, and PG News.

## Plans

Active implementation plans and feature designs.

- [brewing-helper.md](plans/brewing-helper.md) — Brewery tab: phases 1-4 implemented, phases 5-7 (aging, live tracking, session summary) remaining.
- [quest-tracking.md](plans/quest-tracking.md) — Quest tracking system: quest event parsing, repeatable cooldowns, work orders, Statehelm quests, active quest browser.
- [interactive-maps.md](plans/interactive-maps.md) — Interactive zone maps with NPC locations, landmarks, and live game state overlays.
- [owned-quantity-tracking.md](plans/owned-quantity-tracking.md) — Track item/currency quantity changes over time with historical trend data.

## Archive

Completed plans are moved to `archive/plans/` for reference. These features are fully implemented with corresponding feature/screen docs.

- [stall-tracker-implementation.md](archive/plans/stall-tracker-implementation.md)
- [survey-tracker-rewrite.md](archive/plans/survey-tracker-rewrite.md)
- [npc-tracking-improvements.md](archive/plans/npc-tracking-improvements.md)
- [item-provenance-overhaul.md](archive/plans/item-provenance-overhaul.md)
- [stats-improvements.md](archive/plans/stats-improvements.md)
- [projects-performance.md](archive/plans/projects-performance.md) — top 3 perf fixes landed; remaining UX items moved to TODO.md.

## Scripts

Utility scripts in `scripts/`.

- `analyze_capture.py` — Analyze a debug capture JSON file: ProcessXxx type inventory, noise classification, combat summary, chat extraction, state diff. Run `python scripts/analyze_capture.py --help` for options.
- `extract_cdn_schemas.py` — Extract field schemas from CDN JSON files.
- `bump-version.sh` — Bump app version across Cargo.toml, tauri.conf.json, tauri.experimental.conf.json, and package.json.
- `minimize-player-log.sh` — Strip noise lines from a raw Player.log for smaller test fixtures.

## Samples

Sample data files for development and testing (gitignored).

- `samples/CDN-full-examples/` — Complete CDN JSON samples.
- `samples/character-export-samples/` — Character export JSON examples.
- `samples/player-log-samples/` — Player.log excerpts for parser testing.
- `samples/devtolsCaptures/` — Debug capture JSON files for analysis and feature discovery.
