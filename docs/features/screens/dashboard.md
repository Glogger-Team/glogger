# Dashboard Screen

## Overview

The dashboard is the landing screen — an at-a-glance view of the player's current session. It renders a configurable grid of widget cards that can be reordered via drag-and-drop and shown/hidden via a settings pane.

The **Aggregate View** (server-wide analytics across all characters) has moved to the **Character > Account** tab.

## Architecture

### Widget System

The dashboard is built on a registry-driven widget system:

- **Widget registry** (`src/components/Dashboard/dashboardWidgets.ts`) — array of `{ id, name, component, defaultSize }` objects. All widgets are registered here. Adding a new widget means creating the component and adding a registry entry.
- **DashboardCard** (`src/components/Dashboard/DashboardCard.vue`) — shared wrapper providing card chrome (background, border, rounded corners), a compact title bar (drag handle), and an optional gear icon with config popover slot. The gear icon only appears when the widget provides a `config` slot.
- **Responsive CSS grid** — uses `repeat(auto-fill, minmax(280px, 1fr))` so column count adapts to screen width. Wider monitors fit more widgets per row; narrower monitors fewer. No fixed column count.
- **Widget sizes**: Small (1 column, ~280px), Medium (2 columns, span 2), Large (4 columns, span 4 = 2x medium).
- **Drag-to-reorder** — via `vuedraggable` (SortableJS wrapper). Cards are dragged by their title bar. New order is persisted.
- **Settings pane** — right-side pane ("Widgets") with checkboxes to show/hide each widget. Defaults to open.

### Persistence

Widget order and visibility are stored in `viewPreferences['dashboard']` via `useViewPrefs`:
- `cardOrder: string[]` — widget IDs in display order
- `hiddenCards: string[]` — widget IDs that are hidden

New widgets (added in code updates) automatically append to the end of the card order.

### Files

**Core:**
- `src/components/Dashboard/DashboardView.vue` — main container with registry-driven grid
- `src/components/Dashboard/DashboardCard.vue` — shared card wrapper (title bar, config popover)
- `src/components/Dashboard/DashboardSettingsPane.vue` — right pane with widget visibility toggles
- `src/components/Dashboard/dashboardWidgets.ts` — widget registry and size class definitions

**Shared components used by widgets:**
- `src/components/Dashboard/ActivityFeed.vue` — reusable activity feed (used by 4 feed widgets)
- `src/components/Dashboard/ContextBar.vue` — time, moon phase, weather, currencies
- `src/components/Dashboard/ContextBarConfig.vue` — config panel for Status widget section toggles
- `src/components/Dashboard/widgets/ZoneNpcsWidget.vue` — current area with friendly NPCs, services, and configurable filters
- `src/components/Dashboard/widgets/ZoneNpcsWidgetConfig.vue` — config panel for Zone NPCs widget
- `src/components/Dashboard/PlayerNotes.vue` — localStorage-backed checklist

**Widget wrappers** (`src/components/Dashboard/widgets/`):
Self-contained components that bind store data to shared components. Each is a thin wrapper.

**Stores:**
- `gameStateStore` — live session data (skills, inventory, activity feeds, world state, currencies)
- `settingsStore` — watch rules (for Watchword Alerts widget)
- `useStatehelmTracker` composable — gift tracking (for Statehelm Gifting widget)

### Component Hierarchy

```
DashboardView.vue
├── PaneLayout (screen-key="dashboard", right pane: "Widgets")
│   ├── Center: responsive CSS grid
│   │   └── draggable wrapper (vuedraggable)
│   │       └── DashboardCard (v-for widget in orderedWidgets)
│   │           └── <component :is="widget.component" />
│   └── Right pane: DashboardSettingsPane
│       └── Checkbox list of all registered widgets
```

## Widgets

Each widget is documented in its own file under `dashboard/`:

| Widget | Size | Description |
|--------|------|-------------|
| [Status](dashboard/widget-status.md) | Small | Server/game time, moon phase, weather, currencies |
| [Live Skill Tracking](dashboard/widget-skill-tracking.md) | Large | Session XP gains per skill |
| [Items Incoming](dashboard/widget-items-incoming.md) | Medium | Loot, crafts, summoned items |
| [Items Outgoing](dashboard/widget-items-outgoing.md) | Medium | Sold, stored, consumed items |
| [Councils](dashboard/widget-councils.md) | Medium | Gold/council currency changes |
| [Current Zone](dashboard/widget-current-zone.md) | Medium | Area + NPCs with favor ranks |
| [Favor Changes](dashboard/widget-favor-changes.md) | Medium | NPC favor deltas |
| [Notes](dashboard/widget-notes.md) | Medium | Personal checklist (localStorage) |

| [Critical Resources](dashboard/widget-critical-resources.md) | Medium | Tracked item quantities |
| [Statehelm Gifting](dashboard/widget-statehelm-summary.md) | Medium | Weekly gift progress summary |
| [Watchword Alerts](dashboard/widget-watchword-alerts.md) | Medium | Recent watchword match feed |
| [Recipe Items](dashboard/widget-recipe-items.md) | Medium | Recipe scrolls/skill books in inventory, classified by learnability |
| Gift Watcher | Medium | Monitors inventory for items matching watched NPCs' gift preferences |
| Teleport Machine Codes | Medium | Searchable lookup for ~190 teleportation machine codes across 14 zones, grouped by zone/destination |
| [Milking Timers](dashboard/widget-milking-timers.md) | Small | NPC cow milking cooldown tracker — cows grouped by zone with 1h countdown timers, current zone floated to top. Includes "Players Milked", "Milked By", and "Self Milked" tabs for player-to-player and self-milking tracking. |
| Mushroom Farming | Large | Moon-phase-aware mushroom reference table with level, grow time, substrates, and extra/reduced yield highlighting |
| Stat Tracker | Small | Configurable display of player attributes and computed stats. Gear icon opens config to search and add/remove tracked stats. Defaults: Total Skill Levels, Combat XP Earned Mod, Crafting XP Earned Mod, Max Inventory Size. Updates live as attributes change. |
| [Words of Power](dashboard/widget-words-of-power.md) | Medium | Auto-captures words of power when crafted, grouped by power name with age counters, click-to-copy, and manual add/remove. |
| Timers | Small | Countdown timers with add/pause/resume/restart/remove. Flexible duration input, preset quick-start buttons with configurable presets. Persisted via SQLite (`user_timers` table) with localStorage write-through cache. Timers scoped per character/server. Expired timers pulse red. |
| Recurring Events | Small | Calendar-based recurring event timers with countdowns. Supports daily, weekly, biweekly (with anchor date), and monthly recurrence. Events auto-advance and sort by next occurrence. Imminent events (<1h) highlighted. Persisted via useViewPrefs. |
| Garden Almanac | Small | Parses GardeningAlmanac book content, extracts current and upcoming crop bonus events. Shows ItemInline/AreaInline display, countdown timer, upcoming events list. Data stored in `garden_almanac` table. |
| Vendor Councils | Medium | Vendor gold summary across all tracked vendors, grouped by item type category (Armor, Weapons, Jewelry, Skins, Potions, Food). Shows per-vendor gold remaining with color-coded ratios, reset timers, and category/grand totals. Toggle between active character and cross-character aggregate view. |

## Adding a New Widget

1. Create a component in `src/components/Dashboard/widgets/`. It should be self-contained — import its own stores and bind its own data. No props from the parent.
2. Add a registry entry to `dashboardWidgets.ts`:
   ```ts
   { id: 'my-widget', name: 'My Widget', component: MyWidget, defaultSize: 'medium' }
   ```
3. The widget automatically appears on new users' dashboards. Existing users see it appended to their card order.
4. To add per-widget configuration, use the `config` slot on `DashboardCard` — the gear icon only appears when this slot is provided.

## Data Sources

| Data | Source | Persistence |
|------|--------|-------------|
| Server time, game time | `gameStateStore.serverTime` / `gameTime` | Session (computed, 1s tick) |
| Weather, combat, mount | `gameStateStore.world` | Session (in-memory) |
| Currencies | `gameStateStore.currencies` | Session (in-memory) |
| Skill tracking | `gameStateStore.sessionSkillList` | Session (in-memory) |
| Items incoming/outgoing | `gameStateStore.itemsIncoming` / `itemsOutgoing` | Session (last 30, in-memory) |
| Council changes | `gameStateStore.councilChanges` | Session (last 30, in-memory) |
| Favor changes | `gameStateStore.favorChanges` | Session (last 30, in-memory) |
| Current area | `gameStateStore.world.area` | Persistent (database) |
| Area NPCs | `gameDataStore.getNpcsInArea()` | CDN cache |
| NPC favor ranks | `gameStateStore.favorByNpc` | Persistent (database) |
| Inventory counts | `gameStateStore.ownedItemCounts` | Persistent + live (merged) |
| Gift tracking | `useStatehelmTracker` composable | Persistent (database) |
| Watch rule matches | `get_watch_rule_messages` Tauri command | Persistent (database) |
| Moon phase | `useMoonPhase` composable (calculated) | N/A (derived) |
| Teleport codes | Hardcoded static data in widget | N/A (static reference) |
| Mushroom data | Hardcoded static data + `useMoonPhase` | N/A (static + derived) |
| Player notes | `localStorage` | Persistent (browser) |
| Widget order/visibility | `viewPreferences['dashboard']` | Persistent (settings file) |

## Key Design Decisions

- **Widget registry** — data-driven rendering instead of hardcoded template. Adding a widget is just a component + a registry entry.
- **Self-contained widgets** — each widget component owns its store bindings. No props threading from the parent. This makes widgets independently testable and easy to add/remove.
- **Responsive grid** — `repeat(auto-fill, minmax(280px, 1fr))` adapts column count to available width. No fixed column count prevents stretched/ugly cards on ultrawide monitors.
- **Split activity feeds** — five focused feed cards instead of one mixed transaction log, making it easy to scan specific categories at a glance.
- **Single source per feed** — each feed uses exactly one event source to avoid double-counting.
- **Conditional gear icon** — the config popover gear only renders when a widget provides a `config` slot, keeping the title bar clean for widgets without options.
- **Drag-to-reorder** — vuedraggable with `v-model` on a writable computed. Title bar as handle. Order persisted via useViewPrefs.
- **Notes in localStorage** — player notes are intentionally not in the database since they're personal scratch-pad items.

## Future Work

### New Widget Ideas

- **Daily Quest Tracker** — track daily quest availability/completion. Needs investigation into log events for daily quest accept/complete, whether CDN data marks quests as daily, and reset schedules. **Effort: Medium | Impact: Medium**
- **Rez Timer** — detect death events in PlayerEventParser, track cooldown, display countdown. Research needed into death/rez log lines. **Effort: Large | Impact: Medium**
- **Long-Cooldown Timers** — Resuscitate, portals, Hoplology, etc. Needs cooldown tracking infrastructure, parser work, and possibly manual duration config. Generalized version of rez timer. **Effort: Large | Impact: Medium-High**
- **"What Should I Do Next"** — suggestion engine using skill levels, recipe data, crafting history. Start simple (random tips), get smarter over time. **Effort: Large | Impact: Medium**

### Widget System Enhancements

- **Size override from settings pane** — allow changing a widget's width class (small/medium/large) from the Widgets pane
- **Status widget polish** — grouped/categorized currencies, icons or color coding, richer header with character name, level range, current area, session duration
