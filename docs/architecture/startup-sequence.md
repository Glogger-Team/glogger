# Startup Sequence

Documents the full initialization flow from app launch to interactive state, covering both the Rust backend and Vue frontend.

## Timeline Overview

```
T+0.000s  Rust setup() begins
          ├── Settings loaded from disk
          ├── Prototype version check (nuke DB if changed)
          ├── Database pool initialized (migrations run)
          ├── DataIngestCoordinator created (game state seeded)
          └── Managed state registered → frontend can now call commands

T+0.070s  CDN game data load spawned (background async task)
          └── Frontend webview begins rendering

T+0.070s  Frontend: App.vue mounts → startup.initialize()
          ├── settingsStore.initialize() — loads settings from backend
          ├── Server list loaded
          └── Phase decision:
              ├── First run → setup wizard (setup-path → setup-watchers → setup-character)
              ├── Auto-load enabled → loading phase (immediate)
              └── Otherwise → character select screen

T+0.070s  Loading phase: runStartupTasks() — BLOCKS until all complete
          ├── Task 1: Wait for game data (CDN parse) ........... ~2s
          ├── Task 2: Start log watchers + catch-up ............ ~2-5s
          │   ├── Register event listeners
          │   ├── Start Player.log tailing (if auto-enabled)
          │   ├── Start chat log tailing (if auto-enabled)
          │   ├── Run initial poll (blocks until catch-up complete)
          │   │   ├── Player.log replayed from saved position
          │   │   ├── Chat log replayed from saved position
          │   │   ├── Active character resolved from log content
          │   │   └── Game state seeded from log events
          │   ├── Start periodic polling (1500ms interval)
          │   └── Reload settings (catch-up may have updated character/server)
          ├── Task 3: Load character data from reports .......... ~0.5s
          │   ├── Auto-import latest character report
          │   ├── Auto-import latest inventory report
          │   ├── Seed game state (skills, favor, recipes, currencies, storage)
          │   └── Load character reports
          └── Task 4: Load full game state from DB .............. ~0.1s
              ├── loadAll() — all 11 game state domains
              ├── Load storage vault CDN metadata
              └── Load market values

T+5.0s    Phase becomes "ready" → main UI renders
          ├── startupComplete flag set on gameStateStore
          ├── "App is interactive" logged
          └── character-login events now trigger live reload

T+6.0s    CDN data persisted to database (background)
```

Timings are approximate and vary by machine, data size, and network conditions.

## Backend Sequence (Rust)

All backend initialization happens synchronously inside `tauri::Builder::setup()` in `src-tauri/src/lib.rs`.

### Step 1: Settings (`src-tauri/src/settings.rs`)
- `SettingsManager::new()` loads `settings.json` from the app data directory
- This is **blocking** — everything else depends on settings

### Step 2: Version Check & Database Nuke
- Compares `last_app_version` in settings against current app version
- If changed: deletes the database file + WAL/SHM sidecars (prototype-phase behavior)
- Persists current version to settings

### Step 3: Database (`src-tauri/src/db/mod.rs`)
- `db::init_pool()` creates the SQLite connection pool
- Runs the unified v1 migration schema
- Sets WAL mode, foreign keys, busy timeout pragmas
- **Blocking** — coordinator needs the pool

### Step 4: Coordinator (`src-tauri/src/coordinator.rs`)
- `DataIngestCoordinator::new()` creates the coordinator with:
  - DB pool, settings, app handle, game data state (empty at this point)
- Seeds `GameStateManager` with persisted character/server names from settings
- Player and chat watchers are `None` — not started yet
- **Blocking** — must be registered as managed state

### Step 5: State Registration
- `settings_manager`, `db_pool`, and `coordinator` registered as Tauri managed state
- Frontend commands can now be invoked

### Step 6: Game Data Load (Background)
- `tauri::async_runtime::spawn()` kicks off `init_game_data()`
- **Non-blocking** — setup() returns immediately after spawning
- On success: emits `game-data-ready` event, then persists CDN data to database
- On failure: emits `game-data-error` event
- Takes ~2 seconds (10K+ items, 23K+ effects, etc.)

## Frontend Sequence (Vue)

### Phase: `splash`
- Initial state when `App.vue` mounts
- Shows `StartupSplash` component
- `startup.initialize()` is called in `onMounted`

### Phase Decision (`src/stores/startupStore.ts`)
After loading settings and server list:

| Condition | Next Phase |
|-----------|------------|
| `!setupCompleted \|\| !gameDataPath` | `setup-path` (first-time wizard) |
| `autoLoadLastCharacter && activeCharacterName` | `loading` |
| Otherwise | `select-character` |

### Phase: `loading` (`runStartupTasks`)

Shows `StartupProgress` with a 4-task checklist. Each task must complete before the next begins. All tasks are orchestrated by `startupStore.runStartupTasks()`.

**Task 1: Loading game data**
- Waits for `gameDataStore.status` to become `"ready"` (watches the reactive ref)
- The Rust backend is already loading CDN data in a background task spawned during setup()
- If status becomes `"error"`, startup halts and shows an error message
- This is the only **fatal** task — all others are resilient

**Task 2: Catching up on logs**
- Registers Tauri event listeners (`skill-update`, `survey-event`, `survey-session-ended`, `player-event`)
- Starts Player.log tailing if `autoTailPlayerLog` enabled
- Starts chat log tailing if `autoTailChat` enabled
- Runs one synchronous `poll_watchers` call that **blocks until all historical log content is parsed**
  - This resolves the active character from log history
  - Seeds game state tables from log events (inventory, skills, equipment, etc.)
  - On resume from saved position, the watcher is seeded with the last-known character name from `log_file_positions`
- Starts periodic polling (1500ms interval) for live updates
- Reloads settings from backend (catch-up may have updated active character/server)
- Non-fatal — partial failures log warnings but don't block startup

**Task 3: Loading character data**
- Calls `characterStore.initForActiveCharacter()` which:
  - Auto-imports the latest character report from the Reports folder
  - Seeds game state from the report (skills, favor, recipes, currencies)
  - Loads character list and reports, auto-selects most recent
  - Auto-imports latest inventory report (seeds storage)
  - Background-imports reports for all other characters on the server (for aggregate views)
- Now uses the **correct** active character resolved by the catch-up in Task 2
- Non-fatal — if no reports exist, continues with empty character data

**Task 4: Preparing game state**
- Calls `gameStateStore.loadAll()` — loads all 11 game state domains from the database:
  - skills, attributes, active_skills, world, inventory, recipes, equipment, favor, currencies, effects, storage
- Loads storage vault CDN metadata (`gameStateStore.loadStorageVaults()`)
- Loads market values (`marketStore.loadAll()`)
- This pulls in everything seeded by Tasks 2 and 3 plus any data already in the DB from prior sessions

### Phase: `ready`
- All startup tasks completed successfully
- `gameState.startupComplete` set to `true` — enables live `character-login` handling
- `"App is interactive"` logged to backend
- Main UI renders (MenuBar, view panels, ToastContainer)
- Game data, game state, and character data are all available
- Log watchers are already running and fully caught up

### Character Identity During Startup

During the startup catch-up, `character-login` events are emitted by the backend as it replays historical log content. The `gameStateStore` listener updates `settingsStore.settings.activeCharacterName` reactively but **skips** the heavy reload work (resetting session state, calling `loadAll()`, calling `initForActiveCharacter()`) until `startupComplete` is true. This prevents redundant reloads during catch-up — the startup sequence handles loading character and game state explicitly after catch-up finishes.

Once the app is `ready`, `character-login` events from live log tailing trigger the full reload flow: reset session skills, clear live inventory, reload all game state, and reinitialize character data.

### Ongoing: Post-Ready Background Work
After the app is interactive:
- **Report polling**: If `autoWatchReports` is enabled, periodically checks for new character/inventory reports.
- **CDN persistence**: CDN data is written to the database after game-data-ready (~2s after data load).

## Error Handling

| Failure | Behavior |
|---------|----------|
| Game data load fails | Startup halts on Task 1. Error shown in StartupProgress with red X and message. |
| Log watcher start fails | Task 2 completes with warning. App continues but no live data flows. |
| Character report import fails | Task 3 completes with "Partial" note. App continues with empty character data. |
| Game state load fails | Task 4 shows error. App continues — some features may show empty state. |

## Key Files

**Startup orchestration:**
- `src/stores/startupStore.ts` — Phase management and `runStartupTasks()` sequencing
- `src/App.vue` — Phase-based rendering, mounts startup flow
- `src/components/Startup/StartupProgress.vue` — Loading screen with task progress

**Backend initialization:**
- `src-tauri/src/lib.rs` — Rust `run()` function, setup handler, CDN background task
- `src-tauri/src/coordinator.rs` — DataIngestCoordinator, log watcher management
- `src-tauri/src/game_state.rs` — GameStateManager, event processing, report seeding

**Data stores populated during startup:**
- `src/stores/gameDataStore.ts` — CDN data (items, skills, recipes, NPCs, effects)
- `src/stores/gameStateStore.ts` — Character game state (11 domains), startupComplete flag
- `src/stores/characterStore.ts` — Character reports, report imports
- `src/stores/coordinatorStore.ts` — Log watcher status, polling
- `src/stores/marketStore.ts` — Market values
