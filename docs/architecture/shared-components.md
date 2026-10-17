# Shared Components

Reusable components for displaying game entity references throughout the app. These provide a consistent look and behavior — color-coded names, hover tooltips, and click-to-navigate — so every part of the UI renders entities the same way.

## Entity Colors

Each entity type has a dedicated color token defined in [`theme.css`](../../src/assets/css/theme.css). Use these via Tailwind utilities (`text-entity-item`, `bg-entity-item/10`, `border-entity-item/50`, etc.).

| Entity   | Token            | Color     | Usage                  |
|----------|------------------|-----------|------------------------|
| Item     | `entity-item`    | `#7ec8e3` | Blue — items, loot     |
| Quest    | `entity-quest`   | `#e0c060` | Gold — quests          |
| Skill    | `entity-skill`   | `#5cb85c` | Green — skills         |
| NPC      | `entity-npc`     | `#e0965c` | Orange — NPCs          |
| Ability  | `entity-ability` | `#b07ce0` | Purple — abilities     |
| Recipe   | `entity-recipe`  | `#c8a05c` | Warm gold — recipes    |
| Area     | `entity-area`    | `#6a9fb5` | Teal — zones/areas     |
| Enemy    | `entity-enemy`   | `#d9534f` | Red — enemies/mobs     |

## Component Overview

```
src/components/Shared/
├── GameIcon.vue                 # Reusable icon with loading/fallback
├── EntityTooltipWrapper.vue     # Slot-based tooltip positioning
├── DataTable.vue                # Sortable table with skeletons, empty state, cell slots
├── FilterBar.vue                # Search input with result count and filter slots
├── SkeletonLoader.vue           # Animated placeholder (text/circle/rect variants)
├── DataTableSkeleton.vue        # Table-shaped loading skeleton
├── EmptyState.vue               # Empty state display (panel or compact)
├── TabBar.vue                   # Tab navigation bar (v-model)
├── ModalDialog.vue              # Confirm/prompt modal dialog
├── AccordionSection.vue         # Collapsible section with arrow toggle
├── Timestamp.vue                # Timezone-aware timestamp display
├── ToastContainer.vue           # Toast notification overlay
├── StyledSelect.vue             # Themed dropdown replacement for native <select>
├── SearchableSelect.vue         # StyledSelect + filter input for long option lists
├── DatePicker.vue               # Themed calendar popover (replaces native <input type="date">)
├── ItemCard.vue                 # Survey loot card (count + percentage)
├── SkillCard.vue                # Session skill summary card
├── SkillGrid.vue                # Grid of SkillCard components
├── SkillLevelDisplay.vue        # Inline level with bonus breakdown
├── SourcesPanel.vue             # Entity source listing (CDN, items, quests)
├── SourceEntryRow.vue           # Single source entry with inline links
├── PaneLayout.vue               # Two-pane layout container
├── SidePane.vue                 # Resizable/collapsible side pane
├── Item/
│   ├── ItemTooltip.vue          # Tooltip content (presentational)
│   ├── ItemInline.vue           # Inline text reference
│   ├── ItemMinicard.vue         # Compact two-line card (name, prices, owned count)
│   ├── ItemIconOnly.vue         # Icon-only reference
│   └── ItemIconPlus.vue         # Card with icon + name + value + type
├── Quest/
│   ├── QuestTooltip.vue         # Tooltip content
│   └── QuestInline.vue          # Inline text reference
├── Skill/
│   ├── SkillTooltip.vue         # Tooltip content
│   └── SkillInline.vue          # Inline text reference
├── NPC/
│   ├── NpcTooltip.vue           # Tooltip content
│   └── NpcInline.vue            # Inline text reference
├── Recipe/
│   ├── RecipeTooltip.vue        # Tooltip content
│   └── RecipeInline.vue         # Inline text reference
├── Ability/
│   ├── AbilityTooltip.vue       # Tooltip content
│   └── AbilityInline.vue        # Inline text reference
├── Area/
│   ├── AreaInline.vue           # Inline area reference with tooltip and navigation
│   └── AreaTooltip.vue          # Area tooltip (name, NPCs, monsters)
└── Enemy/
    └── EnemyInline.vue          # Inline enemy reference with click navigation
```

## Composables

Shared logic lives in [`src/composables/`](../../src/composables/):

- **`useTooltip(options?)`** — Tooltip show/hide with configurable delay. Options: `delay`, `interactive`, `onHover`. Returns `showTooltip`, `onMouseEnter`, `onMouseLeave`, `onTooltipMouseEnter`, `onTooltipMouseLeave`, `cleanup`. When `interactive: true`, mouseleave has a 150ms grace period so the user can move their cursor into the tooltip body.
- **`useGameIcon()`** — Loads icon via `gameDataStore.getIconPath()` + `convertFileSrc()`. Returns `iconSrc`, `iconLoading`, `loadIcon(iconId)`. Memoizes per icon ID.
- **`useEntityNavigation()`** — Provides/injects a `navigateToEntity(target)` function for click-to-browse. The provider in [`App.vue`](../../src/App.vue) opens the Data Browser overlay and passes a nav target to [`DataBrowserOverlay.vue`](../../src/components/DataBrowser/DataBrowserOverlay.vue).
- **`useToast()`** — Wrapper for `toastStore` with convenience methods: `success()`, `info()`, `warn()`, `error()`.
- **`usePaneResize(options)`** — Drag-to-resize logic for `SidePane`. Options: `side` (`"left"` | `"right"`), `minWidth`, `maxWidth`, `initialWidth`, `defaultWidth`, `onWidthChange`, `onResizeEnd`. Returns `isResizing`, `startResize(e)`, `resetWidth()`. Side-aware (left pane drag-right = wider, right pane drag-left = wider).
- **`useViewPrefs<T>(screenKey, defaults)`** — Persists view preferences (pane widths, collapsed state) to settings store with 500ms debounce. Returns `prefs` (Ref\<T\>), `update(partial)`.
- **`useKeyboard()`** — Keyboard navigation for list views. Supports arrow keys, W/S (list), Q/E (tabs), A/D (panes), Escape. Context-aware: suppresses nav keys when typing in inputs. Auto-scrolls selected items into view.
- **`useTimestamp()`** — Timezone-aware timestamp formatting utilities. All functions read the `timestampDisplayMode` setting (local/server/utc) and format accordingly. Exports: `formatTimeShort()` ("14:30"), `formatTimeFull()` ("14:30:00"), `formatDateShort()` ("Mar 26"), `formatDate()` ("2026-03-26"), `formatDateTimeShort()` ("Mar 26, 14:30"), `formatDateTimeFull()` ("2026-03-26 14:30:00"), `formatRelative()` ("2m ago"), `formatSmart()` (time if today, datetime-short otherwise), `formatDuration(seconds, opts?)` ("2m 5s", "1h 2m"), `formatStaleness(timestamp)` ("today", "3 days ago"), `getTimezoneSuffix()`, `parseUtc()`. See [time.md](time.md) for the full API reference.
- **`useFavorTiers()`** — Favor tier constants, colors, and utilities. Tier order: SoulMates > LikeFamily > BestFriends > CloseFriends > Friends > Comfortable > Neutral > Despised. Exports: `tierIndex()`, `isTierAtOrAbove()`, `favorColor()`, `favorBadgeClasses()`, `pointsToNextTier()`, `tierDisplayName()`.
- **`useQuestRequirements()`** — Quest eligibility evaluation (MinSkillLevel, MinFavorLevel, ActiveCombatSkill, QuestCompleted, MinLevel, Race, Or). Exports: `evaluateRequirement()`, `evaluateQuestEligibility()`, `eligibilityLabel()`, `eligibilityClasses()`, `requirementStatusIcon()`, `requirementStatusColor()`.

## Inline Components

Inline components are designed to sit naturally within a block of text. They render as `inline-flex` elements with entity-colored text. Icons and text scale with the parent font size (icons use `1em`-based sizing via `GameIcon size="inline"`).

**Behavior:** Data and icons load eagerly on mount (not on hover). Hover shows a rich tooltip after a 500ms delay. Click navigates to the entity's Data Browser tab.

**Common props:** All entity inline components (except AbilityInline) accept a `reference` prop that takes **any known form** of entity reference — numeric ID, display name, internal name, or CDN key. The backend resolver handles disambiguation. All support an optional `bordered` prop for a subtle bordered/card-like look (off by default).

### Item

```vue
<ItemInline reference="Amazing Longsword" />
<ItemInline reference="Amazing Longsword" :show-icon="false" />
<ItemInline reference="Amazing Longsword" bordered />
```

**Props:** `reference: string`, `showIcon?: boolean` (default `true`), `bordered?: boolean` (default `false`)

Data resolved eagerly via `store.resolveItem()` on mount, and re-resolved when `reference` changes.

### Quest

```vue
<QuestInline reference="Quest_SomeName" />
<QuestInline reference="Quest_SomeName" bordered />
```

**Props:** `reference: string`, `bordered?: boolean` (default `false`)

Displays the quest's friendly name once loaded (falls back to the reference). Data resolved eagerly via `store.resolveQuest()` on mount.

### Skill

```vue
<SkillInline reference="Sword" />
<SkillInline reference="Sword" :show-icon="false" />
```

**Props:** `reference: string`, `showIcon?: boolean` (default `true`), `bordered?: boolean` (default `false`)

Data resolved eagerly via `store.resolveSkill()` on mount.

### NPC

```vue
<!-- Reference only (resolves synchronously via resolveNpcSync) -->
<NpcInline reference="Joeh" />

<!-- With pre-loaded data (avoids lookup) -->
<NpcInline reference="Joeh" :npc="npcInfoObject" />
```

**Props:** `reference: string`, `npc?: NpcInfo`, `bordered?: boolean` (default `false`)

The `npc` prop is optional. If provided, it is used directly. If omitted, the component resolves synchronously via `gameData.resolveNpcSync()`. Tooltip is disabled when no NPC data is available.

### Ability

```vue
<AbilityInline :ability="abilityInfoObject" />
```

**Props:** `ability: AbilityInfo`, `showIcon?: boolean` (default `true`), `bordered?: boolean` (default `false`)

Requires the full `AbilityInfo` object because the store only supports `getAbilitiesForSkill()`, not individual lookups. The calling component (which already fetched the list) passes the data directly. Icon loaded eagerly on mount.

### Recipe

```vue
<RecipeInline reference="Brewed Mudbeer" />
<RecipeInline reference="Brewed Mudbeer" :show-icon="false" />
```

**Props:** `reference: string`, `showIcon?: boolean` (default `true`), `bordered?: boolean` (default `false`)

Data resolved eagerly via `store.resolveRecipe()` on mount.

### Area

```vue
<AreaInline reference="Serbule" />
<AreaInline reference="AreaSerbule" bordered />
```

**Props:** `reference: string`, `bordered?: boolean` (default `false`)

Resolves the area reference to a friendly name via `gameDataStore`. Hover shows `AreaTooltip` with area name, short name, notable NPCs (up to 8), and monsters (up to 12). Click navigates to the area in the Data Browser.

### Enemy

```vue
<EnemyInline reference="Feral Cow" />
```

**Props:** `reference: string`

Renders as styled text with entity-enemy color. Click navigates to the enemy in the Data Browser. No tooltip yet.

## Item-Specific Components

### ItemMinicard

A compact two-line card for items. Top line shows the item name, bottom line shows vendor price, market price (clickable "???" to set if missing), and owned count. Optional icon on the left, scaled to card height. Border on by default.

```vue
<ItemMinicard reference="Amazing Longsword" />
<ItemMinicard reference="Amazing Longsword" :show-icon="false" :bordered="false" />
```

**Props:** `reference: string`, `showIcon?: boolean` (default `true`), `bordered?: boolean` (default `true`), `width?: "fixed" | "min" | "max"` (default `"fixed"`)

Width modes control the card's sizing behavior (all based on `11rem`/`w-44`):
- `"fixed"` — exact width (default, cards are uniform)
- `"min"` — at least that width, can grow
- `"max"` — at most that width, can shrink

Hover shows full item tooltip (interactive, so market editor in tooltip works). Click navigates to the item in the Data Browser. The "???" button opens an inline market value editor popup.

### ItemIconOnly

Just an icon. Hover shows the full item tooltip. Click navigates to the item in the Data Browser.

```vue
<ItemIconOnly name="Amazing Longsword" />
<ItemIconOnly name="Amazing Longsword" size="lg" />
```

**Props:** `name: string`, `size?: "xs" | "sm" | "md" | "lg"` (default `"sm"`)

### ItemIconPlus

A rectangular card showing a large icon alongside the item's name, vendor value, and type keyword.

```vue
<ItemIconPlus name="Amazing Longsword" />
```

**Props:** `name: string`

### ItemCard

A survey loot card showing item icon, count, and drop percentage. Used by the Surveying session view. Hover shows item tooltip.

```vue
<ItemCard item-name="Iron Ore" :count="15" :percentage="42.3" />
```

**Props:** `itemName: string`, `count: number`, `percentage: number`

## Skill Display Components

### SkillCard

A session summary card for a single skill. Shows current level (with bonus breakdown), XP gained, XP/hour, levels gained, time-to-next-level, and a progress bar.

```vue
<SkillCard :skill="skillSessionData" />
```

**Props:** `skill: SkillSessionData`

### SkillGrid

Renders a flex-wrap grid of `SkillCard` components for all skills in the current session. Reads directly from `gameStateStore.sessionSkillList`.

```vue
<SkillGrid />
```

**Props:** *(none — reads from store)*

### SkillLevelDisplay

Inline level display that shows bonus breakdown when bonus levels are present (e.g., "10 (8+2)"). Includes a title tooltip with the full breakdown.

```vue
<SkillLevelDisplay :skill="{ level: 10, base_level: 8, bonus_levels: 2 }" />
```

**Props:** `skill: { level: number; base_level: number; bonus_levels: number }`

## Source Components

### SourcesPanel

Displays all known sources for an entity — CDN-defined sources (training, vendor, barter, etc.), items that bestow it, and quests that reward it. Shows loading state and "no known sources" fallback.

```vue
<SourcesPanel :sources="entitySources" :loading="isLoading" />
```

**Props:** `sources: EntitySources | null`, `loading?: boolean`

### SourceEntryRow

A single source entry row with contextual inline entity links. Handles source types: Skill, Training, Vendor, Barter, NpcGift, HangOut, Quest, QuestObjectiveMacGuffin, Effect, Item.

```vue
<SourceEntryRow :entry="cdnSourceEntry" />
```

**Props:** `entry: CdnSourceEntry`

## Layout Components

### PaneLayout

A two-pane layout container that manages optional left and right `SidePane` components with a flexible center content area.

```vue
<PaneLayout
  screen-key="npcs"
  :left-pane="{ title: 'NPC List', defaultWidth: 350 }"
  :right-pane="{ title: 'Details', defaultWidth: 400 }">
  <template #left>Left pane content</template>
  <template #right>Right pane content</template>
  Main content
</PaneLayout>
```

**Props:** `screenKey: string`, `leftPane?: PaneConfig`, `rightPane?: PaneConfig`

**PaneConfig:** `{ title: string; defaultWidth?: number; minWidth?: number; maxWidth?: number }`

### SidePane

A resizable, collapsible side pane with drag handle and persisted state. When collapsed, shows a vertical text label that can be clicked to expand. Double-click the drag handle to reset width. Uses `usePaneResize` for drag logic and `useViewPrefs` to persist width/collapsed state per screen.

```vue
<SidePane side="left" title="NPC List" screen-key="npcs" :default-width="350" />
```

**Props:** `side: "left" | "right"`, `title: string`, `screenKey: string`, `defaultWidth?: number` (default `320`), `minWidth?: number` (default `200`), `maxWidth?: number` (default `700`)

## Timestamp Component

Displays a formatted timestamp that respects the user's `timestampDisplayMode` setting (local, server, or UTC). Hover shows the full datetime with timezone suffix.

```vue
<Timestamp value="2026-03-26 14:30:00" />
<Timestamp value="2026-03-26 14:30:00" granularity="time-full" />
<Timestamp value="2026-03-26 14:30:00" granularity="datetime-short" />
```

**Props:** `value: string` (UTC timestamp from DB), `granularity?: TimestampGranularity` (default `"smart"`)

**Granularity options:** `time-short` ("14:30"), `time-full` ("14:30:00"), `date-short` ("Mar 26"), `date-full` ("2026-03-26"), `datetime-short` ("Mar 26, 14:30"), `datetime-full` ("2026-03-26 14:30:00"), `relative` ("2m ago"), `smart` (time if today, datetime-short otherwise)

Renders as a `<time>` element with `datetime` attribute for accessibility. Use the component when timestamps appear as standalone display elements. Use the composable functions directly when timestamps are embedded in strings (e.g., `<option>` tags, interpolated text).

## Data Display Components

### DataTable

A sortable data table with built-in loading skeletons, empty state, and custom cell rendering via scoped slots.

```vue
<DataTable
  :columns="columns"
  :rows="filteredRows"
  :sort-key="sortKey"
  :sort-dir="sortDir"
  :loading="isLoading"
  empty-text="No items found"
  @sort="onSort"
>
  <template #cell-name="{ row }">
    <ItemInline :reference="row.name" />
  </template>
</DataTable>
```

**Props:** `columns: ColumnDef[]`, `rows: Record<string, unknown>[]`, `sortKey?: string`, `sortDir?: 'asc' | 'desc'` (default `'asc'`), `loading?: boolean` (default `false`), `emptyText?: string` (default `"No data"`), `emptyHint?: string`, `compact?: boolean` (default `false` — tighter cell padding `px-2 py-1` and `text-[11px]` instead of `text-xs`), `hoverable?: boolean` (default `true`), `stickyHeader?: boolean` (default `true`), `skeletonRows?: number` (default `5`), `rowClickable?: boolean` (default `false` — adds `cursor-pointer` to rows), `rowClass?: (row, index) => string`

**ColumnDef:** `{ key: string; label: string; sortable?: boolean; align?: 'left' | 'center' | 'right'; width?: string; numeric?: boolean }`

**Slots:** `header-{key}` (custom header), `cell-{key}` (custom cell, receives `{ row, value }`)

**Events:** `sort({ key, dir })`, `rowClick({ row, index })` (fires on any row click; pair with `rowClickable` for click-to-drill/select tables — keep per-row highlight logic in `rowClass`)

### FilterBar

A search input bar with result count display and a slot for additional filter controls.

```vue
<FilterBar v-model="search" placeholder="Search items..." :result-count="filtered.length">
  <StyledSelect v-model="category" :options="categoryOptions" />
</FilterBar>
```

**Props:** `modelValue: string` (v-model), `placeholder?: string` (default `"Search..."`), `resultCount?: number`, `resultLabel?: string` (default `"results"`)

**Slots:** default — filter buttons or dropdowns rendered alongside the search input.

### SkeletonLoader

Animated placeholder for loading states. Three variants for different content shapes.

```vue
<SkeletonLoader variant="text" :lines="3" />
<SkeletonLoader variant="circle" />
<SkeletonLoader variant="rect" width="w-full" height="h-32" />
```

**Props:** `variant?: 'text' | 'circle' | 'rect'` (default `'text'`), `lines?: number` (default `1`, text variant only), `width?: string`, `height?: string`

### DataTableSkeleton

A table-shaped skeleton for use as a loading state in contexts where `DataTable`'s built-in `loading` prop isn't available (e.g., before the table component mounts).

```vue
<DataTableSkeleton :columns="4" :rows="8" />
```

**Props:** `columns?: number` (default `4`), `rows?: number` (default `5`), `showHeader?: boolean` (default `true`)

## UI Utility Components

### EmptyState

Flexible empty state display with two variants.

```vue
<!-- Centered in a panel -->
<EmptyState primary="No items found" secondary="Try adjusting your filters" />

<!-- Compact inline -->
<EmptyState primary="No results" variant="compact" />
```

**Props:** `primary: string`, `secondary?: string`, `variant?: "panel" | "compact"` (default `"panel"`)

### TabBar

Tab navigation bar with v-model binding.

```vue
<TabBar v-model="activeTab" :tabs="[{ id: 'items', label: 'Items' }, { id: 'skills', label: 'Skills' }]" />
```

**Props:** `tabs: Tab[]`, `modelValue: string`

**Tab:** `{ id: string; label: string }`

### ModalDialog

A modal dialog for confirm/prompt scenarios. Teleported to body with backdrop dismiss and keyboard handling (Enter to confirm, Escape to cancel).

```vue
<ModalDialog v-model:show="showDialog" title="Rename" @confirm="onConfirm" @cancel="onCancel" />
<ModalDialog v-model:show="showConfirm" title="Delete?" type="confirm" message="Are you sure?" danger />
```

**Props:** `show: boolean`, `title: string`, `type?: "prompt" | "confirm"` (default `"prompt"`), `message?: string`, `placeholder?: string`, `initialValue?: string`, `confirmLabel?: string` (default `"OK"`), `danger?: boolean` (default `false`)

**Events:** `update:show`, `confirm(value: string)`, `cancel`

### AccordionSection

A collapsible section with an arrow toggle. Supports named slots for title content and an optional badge.

```vue
<AccordionSection :default-open="true">
  <template #title>Section Title</template>
  <template #badge><span class="text-xs">3</span></template>
  Section content here
</AccordionSection>
```

**Props:** `defaultOpen?: boolean` (default `true`)

**Slots:** `title`, `badge`, default

### ToastContainer

Toast notification overlay. Reads from `toastStore` and auto-dismisses notifications. Pauses timer on hover. Place once in the app root.

```vue
<ToastContainer />
```

**Props:** *(none — reads from store)*

Use via the `useToast()` composable: `toast.success("Saved!")`, `toast.error("Failed")`.

### StyledSelect

A themed dropdown replacement for the native `<select>` element. Teleports its option panel to `body` so it can escape parent `overflow: hidden` clipping, and matches the dark theme without per-page CSS overrides.

```vue
<StyledSelect
  v-model="value"
  :options="[{ value: 'a', label: 'Option A' }, { value: 'b', label: 'Option B' }]"
  size="sm" />
```

**Props:** `options: SelectOption[]`, `modelValue: string`, `placeholder?: string` (default `"Select..."`), `size?: "xs" | "sm" | "md"` (default `"sm"`), `colorClass?: string`, `fullWidth?: boolean` (default `false`)

**SelectOption:** `{ value: string; label: string }`

Keyboard: Up/Down arrow keys cycle through options, Enter/Space toggles the dropdown, Escape closes it. The option panel is positioned with `Teleport`, flips above the trigger if there's no room below, and is dismissed by clicking the full-screen click-catcher backdrop.

### SearchableSelect

`StyledSelect`'s sibling for long option lists where typing-to-filter beats arrow-key scrubbing. The dropdown contains a focused search input at the top and an "All" sentinel option (selecting it sets the model to `null`).

```vue
<SearchableSelect
  v-model="filterBuyer"
  :options="store.filterOptions.buyers"
  all-label="All buyers" />
```

**Props:** `options: string[]`, `modelValue: string | null`, `allLabel?: string` (default `"All"`), `fullWidth?: boolean` (default `false`)

**Events:** `update:modelValue (value: string | null)` — `null` represents the "All" choice.

Behavior:

- Search query lives only while the dropdown is open and is reset on close.
- Enter inside the search input picks the first filtered match.
- The dropdown is teleported to `body` at `z-[70]` so it layers above any modal at `z-[60]` (the Stall Tracker Shop Log modal is the current consumer).
- Position-flips above the trigger if there's no room below.
- Click outside closes without applying.

### DatePicker

A themed calendar popover that replaces native `<input type="date">`. The motivation: WebView2's native date picker has inconsistent outside-click dismissal — the popup stays open until the input loses focus, and there's no clean DOM workaround that doesn't break in-picker interactions like month navigation. A custom popover gives full control and matches the rest of the app's dropdown styling.

<img src="../screenshots/economics/stall-tracker/date-picker.png" alt="DatePicker — month grid, Today highlight, Today/Clear shortcuts" width="320" />

```vue
<DatePicker v-model="filterDateFrom" placeholder="From date" />
```

**Props:** `modelValue: string` (ISO `"YYYY-MM-DD"` or `""` for unset), `placeholder?: string` (default `"Select date"`)

**Events:** `update:modelValue (value: string)` — emits the empty string when the user clicks "Clear".

Behavior:

- **Trigger button**: shows the formatted date (`"Apr 13, 2026"`) when set, the placeholder otherwise. Calendar icon, matches `SearchableSelect`'s button shape.
- **Calendar grid**: 7×6 grid (Mon-first), 42 cells with leading/trailing days from neighbor months rendered dimmer. Today is marked with a subtle ring; the selected day with `accent-gold/20` background.
- **Footer shortcuts**: "Today" link (always visible), "Clear" link (only when a value is set).
- **Keyboard**: Arrow keys move by 1 day / 1 week from the selected day (or from today if nothing's selected). Escape closes. The popover root has `tabindex="-1"` and is focused on open so the keydown handler actually fires.
- **Local-time consistent**: ISO strings are constructed from local-time `getFullYear/getMonth/getDate`, matching the backend's `event_at` storage format. No UTC drift, no off-by-one across timezones.
- **Layering**: `z-[70]` (same as `SearchableSelect`) so it works inside a `z-[60]` modal.
- **Repositions on resize and scroll** while open so the popover follows its trigger.

The component is intentionally minimal — single-month only, no range mode, no time-of-day. Two `<DatePicker>`s side-by-side cover the from-to filter pattern.

## Tooltip Components

Tooltip components are **presentational** — they receive already-loaded data as props and render the tooltip body. They are not used standalone; they are placed inside the `#tooltip` slot of `EntityTooltipWrapper`.

| Component          | Props                                |
|--------------------|--------------------------------------|
| `ItemTooltip`      | `item: ItemInfo`, `iconSrc: string \| null` |
| `QuestTooltip`     | `quest: QuestInfo`                   |
| `SkillTooltip`     | `skill: SkillInfo`, `iconSrc: string \| null` |
| `NpcTooltip`       | `npc: NpcInfo`                       |
| `AbilityTooltip`   | `ability: AbilityInfo`, `iconSrc: string \| null` |
| `RecipeTooltip`    | `recipe: RecipeInfo`, `iconSrc: string \| null` |

If you need a custom card or display that still wants the standard tooltip on hover, use `EntityTooltipWrapper` directly:

```vue
<EntityTooltipWrapper border-class="border-entity-item/50" @hover="loadMyData">
  <!-- Your custom trigger content -->
  <div>My custom item display</div>

  <template #tooltip>
    <ItemTooltip v-if="myItem" :item="myItem" :icon-src="myIconSrc" />
  </template>
</EntityTooltipWrapper>
```

## Base Components

### GameIcon

Renders an icon by `icon_id` with loading spinner and `?` fallback.

```vue
<GameIcon :icon-id="item.icon_id" alt="Item name" size="md" />
```

**Props:** `iconId: number | null | undefined`, `alt?: string`, `size?: "xs" | "sm" | "md" | "lg" | "inline"`

Sizes: `xs` = 16px, `sm` = 20px (default), `md` = 32px, `lg` = 48px, `inline` = 1.1em (scales with parent text). The `inline` size is used by entity inline components so icons match the surrounding font size. It also omits the background/border treatment used by fixed sizes.

### EntityTooltipWrapper

Wraps any content with tooltip-on-hover behavior. Handles delay, positioning, and show/hide state.

```vue
<EntityTooltipWrapper
  :delay="500"
  border-class="border-entity-skill/50"
  @hover="onFirstHover"
>
  <slot />              <!-- trigger content -->
  <template #tooltip>
    <slot name="tooltip" />  <!-- tooltip body -->
  </template>
</EntityTooltipWrapper>
```

**Props:** `delay?: number` (default 500), `disabled?: boolean`, `borderClass?: string`, `interactive?: boolean`

- `interactive` — when `true`, the tooltip stays open while the user's mouse is inside it (allows clicking links/buttons in the tooltip). When `false` (default), the tooltip has `pointer-events-none` and closes immediately on mouseleave. `ItemInline` uses `interactive: true` so the market value editor in `ItemTooltip` is clickable.

**Events:** `hover` — emitted once on first mouseenter, use for lazy data loading.

## Charting — vue-data-ui

The app uses [`vue-data-ui`](https://vue-data-ui.graphixy.net/) for chart visualizations. Its CSS is imported globally in `main.ts`. Currently used by the Crafting Skills tab (`VueUiDonut`), but available for any screen.

**Available components:** `VueUiDonut`, `VueUiBar`, `VueUiLine`, `VueUiSparkline`, `VueUiRadar`, and many more — see the library docs for the full catalog. All components accept a `dataset` array and a `config` object.

**Usage pattern:**

```vue
<script setup lang="ts">
import { VueUiDonut } from "vue-data-ui";
import type { VueUiDonutConfig, VueUiDonutDatasetItem } from "vue-data-ui";

const dataset: VueUiDonutDatasetItem[] = [
  { name: "Iron Ore", color: "#6366f1", values: [150] },
  { name: "Wood",     color: "#f59e0b", values: [90] },
];

const config: VueUiDonutConfig = {
  responsive: true,
  style: {
    chart: {
      backgroundColor: "transparent",
      color: "#a1a1aa",
      legend: { show: false },
      tooltip: {
        backgroundColor: "#27272a",
        color: "#d4d4d8",
        borderColor: "#3f3f46",
      },
    },
  },
  userOptions: { show: false },
  table: { show: false },
};
</script>

<template>
  <VueUiDonut :dataset="dataset" :config="config" />
</template>
```

**Tips:**
- Set `responsive: true` so charts fill their container
- Use `userOptions: { show: false }` and `table: { show: false }` to hide the built-in toolbar/table
- Match tooltip colors to the app theme tokens (`surface-card`, `text-primary`, `border-default`)
- Use a muted chart palette that works on dark backgrounds (see `SkillsTab.vue` for a tested palette)

## Navigation

Inline components call `navigateToEntity({ type, id })` on click, which is provided via Vue's provide/inject from [`App.vue`](../../src/App.vue). This opens the Data Browser overlay (via `dataBrowserStore.open()`) and activates the correct browser type tab.

The mapping from entity type to Data Browser tab:

| Entity Type | Browser Tab |
|-------------|-------------|
| `item`      | Items       |
| `skill`     | Skills      |
| `ability`   | Abilities   |
| `recipe`    | Recipes     |
| `quest`     | Quests      |
| `npc`       | NPCs        |
| `area`      | Areas       |
| `enemy`     | Enemies     |

To use navigation in a custom component:

```vue
<script setup lang="ts">
import { useEntityNavigation } from "../../composables/useEntityNavigation";
const { navigateToEntity } = useEntityNavigation();

function handleClick() {
  navigateToEntity({ type: "item", id: "Amazing Longsword" });
}
</script>
```
