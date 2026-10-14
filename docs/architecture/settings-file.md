# Settings System Guide

## Overview

Application settings are stored in a local JSON file managed by the Rust backend. The Vue frontend communicates with Rust via Tauri `invoke` commands to read and write settings. Settings are never stored in `localStorage`.

**Key properties:**
- **Portable** - Easy to back up or transfer between machines
- **Inspectable** - View and edit with any text editor
- **Rust-authoritative** - The Rust `SettingsManager` is the single source of truth; the Vue store is a synchronized mirror

## File Location

The settings file is created automatically at:

**Windows:**
```
%APPDATA%\com.glogger.dev\settings.json
```

Typical full path: `C:\Users\YourName\AppData\Roaming\com.glogger.dev\settings.json`

The exact path is displayed at the bottom of the Settings page in the app.

## Fields

**Source:** [`src-tauri/src/settings.rs`](../../src-tauri/src/settings.rs) — `AppSettings` struct

### Core Paths

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `log_file_path` | string | `""` | Path to the Player.log file (legacy, may deprecate) |
| `game_data_path` | string | auto-detected | Root Project Gorgon data folder |
| `db_path` | string or null | `null` | Custom database path (`null` = default in app data dir) |

### Startup & Auto-behavior

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `auto_watch_on_startup` | boolean | `false` | Auto-start watching Player.log on launch (legacy) |
| `auto_tail_chat` | boolean | `false` | Auto-start chat log tailing on startup |
| `auto_tail_player_log` | boolean | `false` | Auto-start Player.log tailing on startup |
| `auto_load_last_character` | boolean | `true` | Restore last active character on startup |
| `auto_watch_reports` | boolean | `true` | Auto-watch Reports folder for new character exports |
| `report_watch_interval_seconds` | number | `5` | How often to check the Reports folder (seconds) |
| `auto_check_game_data` | boolean | `true` | Auto-check for new CDN game data versions |
| `auto_update_game_data` | boolean | `true` | Auto-update CDN data when new version found |
| `setup_completed` | boolean | `false` | Whether first-time setup has been completed |
| `last_app_version` | string or null | `null` | Last app version run (detects prototype upgrades for stale data cleanup) |

### Active Session

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `active_character_name` | string or null | `null` | Currently active character (auto-detected from logs) |
| `active_server_name` | string or null | `null` | Currently active server (auto-detected from chat login) |
| `timezone_offset_seconds` | number or null | `null` | Auto-detected timezone offset from UTC in seconds (from chat login line) |
| `manual_timezone_override` | number or null | `null` | Manual timezone override; takes precedence over auto-detected when set |

### Chat & Notifications

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `excluded_chat_channels` | string[] | `["Error", "Emotes", ...]` | Chat channels excluded from DB persistence (does NOT affect parsing — see [live-event-streams.md](live-event-streams.md)) |
| `watch_rules` | WatchRule[] | `[]` | Chat watch rules for notifications (see [chat-parser.md](../features/chat-parser.md)) |

### Data Management

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `auto_purge_enabled` | boolean | `false` | Enable automatic purging of old player data on startup |
| `auto_purge_days` | number | `90` | Delete player data older than this many days |

### Feature Settings

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `timestamp_display_mode` | string | `"local"` | How to display timestamps: `"local"` (browser timezone), `"server"` (game server timezone), `"utc"` |
| `exclude_max_enchanted_recipes` | boolean | `true` | Exclude "Max-Enchanted" recipes from automated recipe selection |
| `market_price_mode` | string | `"universal"` | `"universal"` (one price per item) or `"per_server"` (price per item per server) |
| `item_valuation_mode` | string | `"highest_market_vendor"` | How to value items for wealth calculations. Options: `"highest_market_vendor"`, `"highest_market_buy_used"`, `"vendor_only"`, `"buy_used_only"`, `"market_only"` |
| `dev_mode_enabled` | boolean | `false` | Enable dev mode (reveals beta features/tools) |

### UI State

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `view_preferences` | JSON or null | `null` | Opaque per-screen UI preferences (frontend-managed, persisted as JSON) |

The file uses **snake_case** field names. The Vue frontend uses **camelCase** equivalents (see [Rust/Vue Communication](#rustvue-communication) below).

---

## Rust/Vue Communication

### Architecture

```
settings.json <--> SettingsManager (Rust) <--> Tauri Commands <--> settingsStore (Vue)
                    [source of truth]          [IPC bridge]        [reactive mirror]
```

### Rust Side (`src-tauri/src/settings.rs`)

The `SettingsManager` struct owns the settings lifecycle:

- **`SettingsManager::new(app_data_dir)`** - Loads `settings.json` from disk on startup, or creates defaults if the file doesn't exist. The `AppSettings` struct is held in an `Arc<RwLock<>>` for thread-safe access.
- **`SettingsManager::get()`** - Returns a clone of the current in-memory settings.
- **`SettingsManager::update(settings)`** - Writes new settings to both memory and disk atomically.
- **`SettingsManager::get_db_path()`** - Returns the custom `db_path` if set, otherwise `{app_data_dir}/glogger.db`.
- **`SettingsManager::get_player_log_path()`** - Derives `Player.log` path from `game_data_path`.
- **`SettingsManager::get_chat_logs_dir()`** - Derives `ChatLogs/` directory from `game_data_path`.

Three Tauri commands are exposed to the frontend:

| Tauri Command | Rust Function | Purpose |
|---------------|---------------|---------|
| `load_settings` | `load_settings()` | Returns the current `AppSettings` to the frontend |
| `save_settings` | `save_settings(settings)` | Accepts an `AppSettings` object, updates memory + writes to disk |
| `get_settings_file_path` | `get_settings_file_path()` | Returns the absolute path to `settings.json` for display |

### Vue Side (`src/stores/settingsStore.ts`)

The Pinia `settingsStore` mirrors the Rust settings as reactive state:

- **`initialize()`** - Called once on app startup. Invokes `load_settings` to hydrate the store, and `get_settings_file_path` to display the file location.
- **Individual updaters** (`updateLogFilePath`, `updateGameDataPath`, etc.) - Each updates the local reactive state and immediately calls `save_settings` to persist via Rust.
- **`updateSettings(partial)`** - Merges a partial settings object and saves.

### Case Conversion

The JSON file and Rust struct use **snake_case**. The Vue `AppSettings` interface uses **camelCase**. Two private functions in `settingsStore.ts` handle conversion:

- `toBackendSettings(settings)` - camelCase -> snake_case (before sending to Rust)
- `fromBackendSettings(settings)` - snake_case -> camelCase (after receiving from Rust)

When adding a new setting, you must update all three places:
1. `AppSettings` struct in `settings.rs` (with `#[serde(default)]` for backwards compatibility)
2. `BackendSettings` interface + `AppSettings` interface in `settingsStore.ts`
3. Both conversion functions (`toBackendSettings` / `fromBackendSettings`)

---

## Settings Page Structure

The Settings page (`src/components/Settings.vue`) is a tabbed container. The settings file path is displayed at the bottom of the page regardless of which tab is active.

### General Tab

Contains day-to-day configuration:

- **Game Data Directory** — text input + Browse for the Project Gorgon data folder
- **Startup Behavior** — checkboxes for auto-tailing Player.log and Chat.log
- **Timestamp Display** — choose between Local Time, Server Time, or UTC for all timestamp displays
- **Game Data (CDN)** — shows cached vs. remote CDN version, update status, force refresh button

### User Data Tab

`UserDataSettings.vue` — the data-management surface (record counts, backups, purges):

- **User Data Storage** — record counts + compact database
- **Backup & Export** — export survey/market/sales history to a backup file
- **Auto-Purge** — checkbox + days input for automatic *database* cleanup on startup (off by default)
- **Manual Purge** — purge records by age or purge all
- **Reports Import** — reserved for the reports auto-import

### Advanced Tab

`AdvancedSettings.vue` — developer/diagnostics surface:

- **Developer Mode** — reveals beta features, experimental tools, and extra debugging info (off by default)
- **Data Browser** — raw-JSON panel, unobtainable-item visibility, history size
- **Character Report Watching** — auto-ingest new character exports (chat detection + fallback polling)
- **Player.log Upload** / **Dual-Log Replay** — dev-mode-only manual parse and paired replay tools

(Record counts, database compaction, and manual purges live in the **User Data** tab —
`UserDataSettings.vue`; CDN table rebuild lives in the **Game Data** tab — `GameDataSettings.vue`.)

### About Tab

Displays app version and build information.

### Component Conventions

- Path inputs save on blur or Enter key, not on every keystroke
- Local `ref()` values shadow store values to avoid saving on every character typed; `watch()` syncs if the store changes externally
- Destructive operations (purge all, CDN rebuild) use visual warning/danger styling and require explicit user action

---

## Manual Editing

You can edit `settings.json` while the app is closed:

1. Close the application completely
2. Open `settings.json` in any text editor
3. Make changes (use double backslashes `\\` for Windows paths)
4. Save the file
5. Restart the application

New fields added in newer app versions use `#[serde(default)]` in Rust, so older settings files missing those fields will load fine with defaults filled in.

## Backup & Restore

Copy the `settings.json` file to back up. Replace it to restore. Restart the app after restoring.

When transferring to another machine, update `game_data_path` to match the new machine's directory structure.

## Troubleshooting

### Settings Not Saving
- Check file permissions on the app data directory
- Check disk space
- Check antivirus (some block writes to AppData)
- View console logs for error messages from `save_settings`

### Settings Reset on Startup
- File may be deleted by antivirus
- File may have JSON syntax errors (Rust will fail to parse and fall back to defaults)
- File permissions may prevent reading

### Can't Find Settings File
- Open the app, go to Settings — the full path is displayed at the bottom
- Or open Windows Run (Win+R) and type: `%APPDATA%\com.glogger.dev`

### Invalid JSON Errors
- Validate at [JSONLint](https://jsonlint.com/)
- Or delete the file entirely to reset to defaults (the app will recreate it)
