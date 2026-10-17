# UX Standards

This document defines the UX/UI patterns every screen in glogger should follow. The data browser (now a popup overlay rather than a dedicated screen) is our gold standard for search/detail patterns — when in doubt, look at how it does things.

---

## Navigation & Keybinds

Implemented in the `useKeyboard` composable (`src/composables/useKeyboard.ts`). Screens opt in to the behaviors they need.

### Mouse
- Scroll wheel always scrolls whatever the mouse is hovering over, not whatever has focus.

### Keyboard — List Navigation
- **Up/Down arrow** (or **W/S**): move selection up/down through items in a list (search results, recipe steps, quest objectives, etc.)
- **Enter**: confirm / open the selected item
- List focus should follow selection — the selected item scrolls into view automatically.

### Keyboard — Pane Navigation
- **Left/Right arrow** (or **A/D**): move focus between horizontal panes on screens that have them (e.g., left list -> center detail).
- Active pane should have a subtle visual indicator so the user knows where keyboard input will land.

### Keyboard — Sub-screen Tabs
- **Shift+Left / Shift+Right** (or **Q/E**): cycle through tabs within a screen (e.g., Crafting -> Quick Calc / Projects / Leveling / History).
- Tab switching wraps around (last -> first, first -> last).

### Keyboard — Global
- Menu bar section hotkeys (1-9, 0) for jumping directly to a screen should be considered as a future addition.
- **Esc**: close any open detail panel, deselect the current selection, or dismiss a toast — in that priority order.

### Implementation Notes
- Keybind state (which pane is active, which list item is selected) lives in the screen's own component state, not in a global store.

---

## Layout

### Desktop-First Design

glogger is a desktop application targeting desktop monitors (1920×1080 and above). All layout decisions should maximize use of the available screen real estate. Do not design as if this were a mobile or narrow-viewport web app.

**Horizontal space principles:**
- **Use the full width.** Tables, grids, and data-heavy views should spread across the available width rather than centering in a narrow column. Avoid `max-w-*` constraints on data layouts unless there's a specific readability reason.
- **Prefer horizontal arrangements over vertical stacking** when content fits. Controls, filters, and metadata that would read naturally side-by-side should be placed horizontally. Only stack vertically when items genuinely need full width or when there are too many to fit in a row.
- **Multi-column layouts for related data.** When displaying groups of related information (e.g., zone breakdowns, category stats), use side-by-side columns or grid layouts instead of stacking everything into a single tall scrolling list.
- **Tables should be information-dense.** Don't add excessive padding or spacing between table rows/columns. Use compact row heights (`py-1` to `py-1.5`), tight column gaps, and let data breathe through alignment rather than whitespace.
- **Inline metadata over dedicated rows.** Stats, badges, and secondary info should sit alongside their parent element (inline or in adjacent columns) rather than occupying their own full-width row when possible.

**Vertical space principles:**
- **Minimize vertical stacking of single-row elements.** A header, then a toggle, then a filter bar, each on their own full-width row, wastes vertical space. Combine toolbar elements into a single row where possible.
- **Accordions and collapsibles are good** for secondary content, but primary data should be visible without expanding anything.
- **Scrollable regions over pagination.** Desktop users have scroll wheels and large viewports — prefer continuous scrolling within panes over paginated views.

**Anti-patterns to avoid:**
- Centering a narrow content column in a wide viewport (mobile-web pattern).
- Full-width cards containing a single line of text or a small table.
- Stacking items vertically that could sit side-by-side at desktop widths.
- Excessive padding/margins that push content below the fold unnecessarily.
- Summary stats displayed as a single horizontal row of cards when there's room to integrate them more compactly.
- **`w-full` on data tables** when the table doesn't need to fill the container width. This causes the first column (usually item/entity names) to absorb all remaining space, pushing data columns to the far right with a huge empty gap. Prefer auto-width tables that shrink-wrap to their content. Only use `w-full` when you intentionally want a column to stretch (e.g., a description field that benefits from wrapping).

### Menu Bar
- Always sticky at the top of the frame.
- **Left block ("identity"):**
  - "glogger" title text in accent-gold.
  - Log status indicators (player.log tailing, chat logging) to the right of the title.
  - Server name / character name below the title to show the active context.
- **Center:** Section navigation links, evenly spaced between the identity block and the settings icon.
- **Right:** Settings gear icon.

### Sub-screen Tabs
- Sticky, directly below the menu bar.
- Use the `TabBar` shared component (`src/components/Shared/TabBar.vue`) or the `.tab` / `.tab-active` component classes.
- Tabs should not scroll — if a screen has too many tabs, that's a sign the screen is doing too much.

### Content Area
- Fills remaining viewport height below menu bar + tabs: `h-[calc(100vh-<offset>px)]`.
- No page-level scrollbar. Each pane manages its own overflow independently.

### Pane Layout
- Screens use a **1-, 2-, or 3-pane layout** as needed.
  - **Center pane** is always the main content area.
  - **Left pane** is for navigation/selection (lists, filters, search). Prefer left + center when only two panes are needed (this is what the data browser does).
  - **Right pane** is for contextual detail or secondary info.
- Left and right panes should be **collapsible** via a toggle. Collapsed state is remembered via `useViewPrefs`.
- Side panes have a fixed width (e.g., `w-80` or similar). Center pane is `flex-1`.
- Not all screens need side panes — a single full-width center pane is fine for simpler views (e.g., Dashboard).

---

## Patterns to Follow (from the Data Browser)

The data browser's individual browser components establish the patterns below. Other screens should match these as closely as makes sense. (Note: the data browser itself is a popup overlay opened via Ctrl+D or the nav bar button, not a dedicated screen.)

### Search & Filtering
- Text search uses a **250ms debounce** before executing.
- Search inputs get `autofocus` when they're the primary interaction on a screen.
- Filters live in the left pane, above or beside the result list.
- Advanced/secondary filters should be collapsible so they don't overwhelm the default view.
- Show a **result count** inline when results exist (e.g., "48 results").
- Show an **active filter indicator** when non-default filters are applied, with a "clear filters" action.

### List Items
- Consistent styling across all list-based screens:
  - Hover: `hover:bg-[#1e1e1e]`
  - Selected: `bg-[#1a1a2e]` + `border-l-2 border-l-accent-gold`
  - Text: `text-xs`, monospace
  - Padding: `px-2 py-1`
  - Separator: `border-b border-surface-dark`
- Lists scroll independently from the detail pane.

### Detail Panel
- When nothing is selected, show a centered italic placeholder: *"Select a [thing] to view details"* in `text-border-default`.
- Detail content uses a consistent section header style:
  ```
  text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5
  ```
- Key-value grids: `grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-1.5`
- Close/deselect button (x) in the top-right of the detail panel header.

### Async Data Loading
- Load related/secondary data **on selection**, not eagerly for every item.
- Show an inline spinner (`text-accent-gold animate-spin` with a reload icon) while loading.
- Handle errors gracefully — log to console, show a brief inline message, don't crash the view.

---

## State Persistence

Implemented via the `useViewPrefs` composable (`src/composables/useViewPrefs.ts`), backed by the settings store.

### Screen State (across navigation)
- When you navigate away from a screen and come back, it should be **exactly how you left it**: selected item, scroll position, expanded sections, filter values.
- Implementation: use `v-show` instead of `v-if` for top-level screen switching so component state is preserved in the DOM. Where that's not possible (e.g., route-based), cache state in a Pinia store keyed by screen name.

### View Preferences (across sessions)
- Sort order, group-by selection, column visibility, collapsed pane state, and similar per-screen preferences should persist across app restarts.
- Each screen gets its own namespace via `useViewPrefs(screenKey)`.

### What NOT to Persist
- Transient selections (which item is highlighted in a list) — reset on app restart.
- Scroll positions — reset on app restart. Trying to restore these is fragile and usually more annoying than helpful.
- Toast/notification state — always starts clean.

---

## Empty States

Implemented via the `EmptyState` shared component (`src/components/Shared/EmptyState.vue`). Every screen and panel that can have "no data" needs an intentional empty state.

### Patterns

**No data yet (action required):**
- Primary: `text-sm text-text-secondary`
- Secondary: `text-xs text-text-muted`
- Center the block vertically and horizontally in the available space.
- Example: *"No skill updates yet."* / *"Start playing to see XP gains here."*

**No results (search/filter):**
- `text-sm text-text-secondary italic`
- If filters are active, suggest clearing them.

**Awaiting selection:**
- `text-border-default italic`, centered in the panel.

**Error state:**
- Use the `.error-box` component class or inline `text-accent-red`.

**Loading state:**
- `text-accent-gold text-sm` with `animate-spin` on the icon (for inline) or `animate-pulse` on the text (for full-panel loading).

### Guidelines
- Always tell the user **why** the area is empty and **what they can do** about it.
- Don't use generic messages like "Nothing to show" — be specific about what's missing.
- Empty states in side panes can be more compact (single line). Empty states in main content areas should be more prominent (centered block).

---

## Toasts & Notifications

Implemented via `useToast` composable (`src/composables/useToast.ts`), `toastStore` (`src/stores/toastStore.ts`), and `ToastContainer` component (`src/components/Shared/ToastContainer.vue`).

### Behavior
- Toasts appear in the **bottom-right** corner of the app, stacked vertically with the newest on top.
- Auto-dismiss after **4 seconds** by default. Errors stick until manually dismissed.
- Hovering a toast pauses its dismiss timer.
- Maximum **3 visible toasts** at a time. If more arrive, the oldest auto-dismiss early.
- Esc dismisses the top toast.

### Visual Design
- Small, compact cards: `text-xs font-mono`, max-width ~350px.
- Types and their left-border accent color:
  - **Success:** `border-l-accent-green`, check prefix
  - **Info:** `border-l-accent-blue`, dot prefix
  - **Warning:** `border-l-accent-warning`, triangle prefix
  - **Error:** `border-l-accent-red`, x prefix
- Background: `bg-surface-elevated`, border: `border border-border-default`.
- Dismiss button on the right side.
- Slide-in animation from the right.

### When to Use Toasts vs Inline Feedback
- **Toast:** action completed successfully, background operation finished, non-critical warnings. Things the user should know but doesn't need to act on immediately.
- **Inline:** validation errors on forms, loading states, empty states, anything the user needs to see *in context* to take their next action.
- Rule of thumb: if the feedback is about *the thing the user is looking at*, put it inline. If it's about *something that happened elsewhere*, use a toast.

---

## Visual Consistency Checklist

These are patterns that should be uniform across every screen. Reference the existing component classes in `src/assets/css/components.css` and theme tokens in `src/assets/css/theme.css`.

- **Buttons:** Use `.btn` + variant (`.btn-primary`, `.btn-secondary`, `.btn-warning`, `.btn-danger`). Don't hand-roll button styles.
- **Inputs:** Use `.input` class. Gold focus ring (`focus:border-accent-gold`).
- **Cards/containers:** Use `.card` class or `bg-surface-card border border-border-default rounded`.
- **Tabs:** Use `.tab` / `.tab-active`. Bottom-border underline style. No pill tabs, no background-fill tabs.
- **Section headers:** `text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5`.
- **Entity references:** Always use the shared inline components (ItemInline, SkillInline, etc.) — never render a plain text name when an entity component exists.
- **Data tables:** A header-row + data-row grid MUST use the shared `DataTable` (`src/components/Shared/DataTable.vue`) — pass `compact` for dense padding, `size="sm"` to keep a grid that used to be `text-sm` at 14px (the default is `text-xs`), `rowClickable` + `@row-click` for click-to-drill rows (never per-cell click wrappers), and `rowClass` for per-row highlight; sorting stays in the parent via `@sort`. The parent owns the scroll container (`overflow-auto` + height constraint) — never wrap the component in an extra `overflow` box, since that becomes the sticky header's scrollport and the header stops pinning. Loading/empty states come from its `loading`/`emptyText`/`emptyHint` props (or `DataTableSkeleton` when the table isn't mounted yet; when `rows` is empty the component hides the header and shows only the empty state). Use a raw `<table>` only when the markup is genuinely not a grid: label/value panels whose group headings live outside the table or in `colspan` section rows, pivots with runtime-generated columns (`ArenaWidget`, `AggregateView`'s skills-across-characters table, `StallRevenueTab`'s `tfoot`), a sub-table nested inside a parent row (`DeathsView`), columns that are the recipe's own `variable_slots` (`BreweryRecipeDetail`), or raw debug dumps (`Settings/GameStateDebug`, dev-panel `GameStateTab`). See `docs/architecture/shared-components.md`.
- **Accent color:** Gold (`accent-gold`) is the primary accent for selections, active states, and primary actions. Other accent colors are for semantic meaning (green=success, red=error, blue=info).
- **Font sizes:** `text-base` for titles, `text-sm` for body/labels, `text-xs` for metadata/secondary, `text-[0.65rem]` for fine print/section headers.
- **Spacing:** Prefer `gap-` utilities over margin. Use `gap-4` between major sections, `gap-2` between related items, `gap-1` or `gap-1.5` for tight groupings.

---

## Startup & Loading Gate

- The app should not show any interactable UI until all essential data is loaded and the app is ready.
- The existing startup flow (splash -> setup wizard -> character select -> loading -> ready) handles this. Don't bypass it.
- If a screen depends on async data that loads *after* startup (e.g., game data store), show the loading empty state pattern within that screen rather than blocking the whole app.
