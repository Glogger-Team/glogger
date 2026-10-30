# Persistence, Data Access & Naming Standards

Conventions and patterns used across the codebase for naming, data storage, and state management.

## Naming Conventions

### Vue Components

- **PascalCase** filenames and tag names: `VaultDatabaseTab.vue`, `ContextBar.vue` (the Dashboard Status widget; the archived copy also referenced as `StatusWidget.vue` is removed).
- Organized by feature in `src/components/` subdirectories: `Dashboard/`, `Character/`, `Crafting/`, etc.
- Shared/reusable components live in `src/components/Shared/` — entity inlines, layout primitives, form controls.
- Widget components follow the pattern `{Name}Widget.vue` inside `Dashboard/widgets/`. The Status widget is the exception: `ContextBar.vue` + `ContextBarConfig.vue` live directly under `Dashboard/` as `context-bar` (`useMoonPhase.ts` holds the shared casino-portal/dark-chapel tables; `TripPlannerWidget.vue` reuses the portal table).
- Tab components follow `{Name}Tab.vue` when they represent a tab within a screen.
- Screen-level components use `{Name}View.vue` or `{Name}Screen.vue`.

### Pinia Stores

- **camelCase** filenames with `Store` suffix: `gameStateStore.ts`, `farmingStore.ts`.
- Store IDs are short camelCase strings: `defineStore("farming", ...)`, `defineStore("deaths", ...)`.
- Export the composable as `use{Name}Store`: `useGameStateStore`, `useFarmingStore`.
- All stores use the **composition API** style (`defineStore("id", () => { ... })`), not the options style.

### Tauri Commands (Rust)

- **snake_case** function names: `get_character_deaths`, `save_farming_session`.
- Grouped by domain in `src-tauri/src/db/{domain}_commands.rs` files (e.g., `death_commands.rs`, `farming_commands.rs`, `crafting_commands.rs`).
- Non-DB commands live in top-level modules: `commands.rs`, `coordinator.rs`, `cdn_commands.rs`.
- Naming patterns by operation:
  - **CRUD**: `get_*`, `create_*`, `update_*`, `delete_*` (e.g., `get_crafting_projects`, `create_build_preset`, `delete_farming_session`)
  - **Batch reads**: `get_all_*`, `search_*` (e.g., `get_all_npcs`, `search_items`)
  - **Entity resolution**: `resolve_*` (e.g., `resolve_item`, `resolve_skill`)
  - **Imports**: `import_*` (e.g., `import_character_report`, `import_market_values`)
  - **Actions**: verb phrase (e.g., `poll_watchers`, `force_refresh_cdn`, `tail_chat_log`)
- Every command is registered in the `invoke_handler` block in [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs), organized by domain with comments.

### Database Tables

- **snake_case** table names.
- **Game state tables** use `game_state_` prefix: `game_state_skills`, `game_state_inventory`, `game_state_favor`.
- **CDN/reference tables** are plain nouns: `items`, `skills`, `recipes`, `npcs`, `abilities`, `areas`.
- **Feature tables** use the feature domain: `farming_sessions`, `crafting_projects`, `survey_sessions`, `stall_events`.
- **Junction/child tables** include the parent concept: `build_preset_mods`, `crafting_project_entries`, `death_damage_sources`.
- **Indexes** follow `idx_{table}_{column(s)}`: `idx_gs_skills_char`, `idx_item_tx_time`.
- Timestamps are stored as ISO 8601 text in UTC. See [`time.md`](time.md) for full standards.

### TypeScript Types

- **PascalCase** interface and type names: `GameStateSkill`, `CraftingProject`, `CharacterDeath`.
- Types organized in `src/types/` by domain:
  - `gameData/` — CDN entity types, one file per entity (`items.ts`, `skills.ts`, `recipes.ts`, etc.) with a barrel `index.ts`.
  - `gameState.ts` — game state query response types (skills, attributes, inventory, etc.).
  - `playerEvents.ts` — player log event types.
  - `database.ts` — admin/utility types (stats, purge options).
  - `{feature}.ts` — feature-specific types (`crafting.ts`, `farming.ts`, `stallTracker.ts`).
- Types that mirror Rust structs use the same field names (snake_case) to match serde serialization.
- Frontend-only types use camelCase fields.

### Composables

- **camelCase** filenames with `use` prefix: `useViewPrefs.ts`, `useKeyboard.ts`, `useToast.ts`.
- Located in `src/composables/`.
- Export a single function matching the filename: `useViewPrefs()`, `useKeyboard()`.
- Used for reusable reactive logic that doesn't warrant a global store.

### CSS / Tailwind

- Tailwind v4 utility classes for all styling — no custom CSS except theme tokens.
- Component-scoped classes use kebab-case when needed.
- Theme tokens defined in the app's Tailwind config. See [`styling.md`](styling.md) for details.

## Data Persistence Patterns

### SQLite (Backend State & User Data)

**Use for**: Any data that must survive app restarts, component unmounting, and navigation. All tracked game activity, historical records, user configurations.

- **Game state**: Last-known values per character+server in `game_state_*` tables (skills, inventory, attributes, favor, etc.). Single-row-per-entity pattern with `last_confirmed_at` timestamp.
- **Historical records**: Append-only ledgers like `item_transactions`, `character_deaths`, `chat_messages`, `stall_events`.
- **User projects**: Structured user content like `crafting_projects`, `build_presets`, `farming_sessions`.
- **CDN reference data**: Wiped and reloaded on each CDN update — `items`, `skills`, `recipes`, `npcs`, etc. No migration care needed for these.

All DB access goes through Tauri commands. Frontend never touches SQLite directly. See the [migration patterns](#migration-patterns) section below.

### Settings File (App Configuration)

**Use for**: User-configurable app behavior — file paths, toggles, retention policies, display preferences.

- Settings are stored in a JSON file managed by `SettingsManager` on the Rust side.
- The `settingsStore` provides frontend access via `load_settings` / `save_settings` Tauri commands.
- Rust uses snake_case field names; the store converts to/from camelCase for frontend use.
- The `viewPreferences` field within settings stores per-screen UI prefs (see below).
- See [`settings-file.md`](settings-file.md) for the full settings architecture.

### useViewPrefs (Per-Screen UI State)

**Use for**: View-level preferences that should persist across navigation and restarts — sort orders, filter selections, toggle states, expanded/collapsed sections.

- Backed by the `viewPreferences` map in settings (serialized to the settings JSON file).
- Each screen gets a unique `screenKey` string.
- Call `useViewPrefs(screenKey, defaults)` to get a reactive `prefs` ref and an `update()` function.
- Updates are debounced (500ms) to avoid write storms.
- Shared across components using the same key — multiple components can read/write the same prefs.

Example usage patterns from the codebase:
- Toggle state: `useViewPrefs("vaultDatabase", { showTotals: false })` in [`VaultDatabaseTab.vue`](../../src/components/Inventory/VaultDatabaseTab.vue)
- Sort/filter: `useViewPrefs('survey-session-list', { filterZone: null, sortBy: 'date', sortDir: 'desc' })` in [`SurveyTrackerView.vue`](../../src/components/Surveying/SurveyTrackerView.vue)
- Expand/collapse: `useViewPrefs('gst-launcher', { expanded: true })` in [`GstLauncher.vue`](../../src/components/Surveying/GstLauncher.vue)

See [`ux-composables.md`](ux-composables.md) for the full composable API.

### Pinia Stores (Reactive Runtime State)

**Use for**: In-memory reactive state that components need to share — live game data, session accumulators, UI coordination. Stores are the bridge between Tauri events/commands and Vue components.

- Stores **read** from SQLite (via Tauri commands) and **listen** to Tauri events for live updates.
- Stores **do not own** persistent game data — they are thin reactive caches over backend state.
- Component-local `ref()` is fine for ephemeral UI state (open dropdowns, scroll positions, draft inputs).
- If state must survive component unmount, it belongs in a store or the database, not in component refs.

### When to Use What

| Data kind | Storage | Example |
|---|---|---|
| Tracked game activity | SQLite | Deaths, item transactions, survey uses |
| Last-known game values | SQLite (`game_state_*`) | Skill levels, inventory, favor |
| Historical records | SQLite | Chat messages, farming sessions, stall events |
| User projects/content | SQLite | Crafting projects, build presets |
| CDN reference data | SQLite (wiped on update) | Items, skills, recipes, NPCs |
| App configuration | Settings file | File paths, toggles, retention days |
| Per-screen UI prefs | Settings file (viewPreferences) | Sort order, filter, collapse state |
| Live reactive state | Pinia store | Current session data, event feeds |
| Ephemeral UI state | Component `ref()` | Dropdown open, scroll position, draft text |

## Store Patterns

All stores use the Pinia composition API. The standard structure:

1. **State**: `ref()` for reactive values, typed interfaces for complex shapes.
2. **Computed**: `computed()` for derived values.
3. **Actions**: `async function` for Tauri command calls, plain functions for state mutations.
4. **Event listeners**: `listen()` for Tauri event subscriptions (typically in an `init` or `subscribe` function).
5. **Return**: Explicit return of public API — state refs, computed properties, and action functions.

Reference examples:
- Simple DB-backed store: [`deathStore.ts`](../../src/stores/deathStore.ts) — loads from DB, handles live events, caches on demand.
- Settings store: [`settingsStore.ts`](../../src/stores/settingsStore.ts) — bidirectional sync with backend settings file.
- Complex live state: [`gameStateStore.ts`](../../src/stores/gameStateStore.ts) — multiple entity types, event listeners, session tracking.
- Session-based: [`farmingStore.ts`](../../src/stores/farmingStore.ts) — start/stop lifecycle, event accumulation, timer management.

### Stores vs Composables

- **Store**: Global singleton, shared across all components. Use when multiple unrelated components need the same state.
- **Composable**: Per-call instance (unless explicitly cached like `useViewPrefs`). Use for reusable reactive logic that's scoped to a component or a small group of related components.

## Tauri Command Patterns

### Adding a New Command

1. **Define** the function in the appropriate `src-tauri/src/db/{domain}_commands.rs` file (or create a new one for a new domain).
2. **Annotate** with `#[tauri::command]`.
3. **Accept** managed state via `State<'_, T>` parameters (e.g., `db: State<'_, DbPool>`, `game_data: State<'_, GameDataState>`).
4. **Return** `Result<T, String>` — map errors with `.map_err(|e| e.to_string())`.
5. **Import** the command in [`lib.rs`](../../src-tauri/src/lib.rs) via `use` statement.
6. **Register** in the `invoke_handler(tauri::generate_handler![...])` block, placed in the appropriate domain section.
7. **Invoke** from frontend: `await invoke<ReturnType>("command_name", { paramName: value })`.

Note: Rust commands use snake_case parameters. When invoking from TypeScript, parameter names must match the Rust parameter names (Tauri uses serde for deserialization).

See [`implementation-checklist.md`](implementation-checklist.md) for the full step-by-step checklist.

## Migration Patterns

### Rules

- The **v1 migration** in [`migrations.rs`](../../src-tauri/src/db/migrations.rs) is the frozen baseline schema. Never modify it.
- All new schema changes are **incremental migrations** (v2, v3, ..., currently up to v40).
- Each migration runs exactly once, guarded by `if current_version < N`.
- CDN/reference tables (items, skills, recipes, etc.) are wiped and reloaded on each CDN update — no migration needed for those.
- Player data tables must always be migrated **non-destructively** (add columns, create new tables, backfill data).

### Adding a New Table

1. Write a `migration_vN_{description}` function in [`migrations.rs`](../../src-tauri/src/db/migrations.rs).
2. Add the `if current_version < N` block in `run_migrations()`.
3. Call `super::record_migration(conn, N)` after success.
4. Follow existing table naming conventions (snake_case, domain prefix for game state tables).
5. Add indexes for columns used in WHERE/JOIN/ORDER BY clauses.

### Adding Columns to Existing Tables

- Use `ALTER TABLE ... ADD COLUMN` with a default value so existing rows don't break.
- If a CHECK constraint change is needed (SQLite doesn't support ALTER CHECK), recreate the table: create new, copy data, drop old, rename. See `migration_v38_self_milking` for an example.

## Type Patterns

### Organization

Types live in `src/types/`, organized by domain:

- **`gameData/`** — One file per CDN entity type, barrel-exported through `index.ts`. These mirror the Rust CDN structs.
- **`gameState.ts`** — Types for game state query responses. Fields use snake_case to match Rust serde output.
- **`playerEvents.ts`** — Player log event discriminated union types.
- **`{feature}.ts`** — Feature-specific types (crafting, farming, stallTracker, etc.).
- **`database.ts`** — Admin/utility types (database stats, purge options).

### Conventions

- Interfaces that mirror Rust structs keep **snake_case** field names (they deserialize directly from Tauri command responses).
- Frontend-only interfaces use **camelCase** field names.
- Use **discriminated unions** with a `kind` field for event types (matches Rust `#[serde(tag = "kind")]`).
- Export types from the file where they're defined. Stores may define small supporting interfaces locally (e.g., `LiveInventoryItem` in `gameStateStore.ts`) when they're only used by that store.
- Game data types are re-exported through `src/types/gameData/index.ts` for convenient imports.
