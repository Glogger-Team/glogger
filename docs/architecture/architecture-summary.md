# Architecture Summary

Quick reference for the glogger data architecture (post-refactor).

## Core Principle

**Rust owns the lifecycle, Frontend displays the state.**

## Architecture Stack

```
┌─────────────────────────────────────┐
│         Vue Frontend (Display)       │
│  ┌─────────────┐  ┌──────────────┐  │
│  │ Components  │  │ Pinia Stores │  │
│  └─────────────┘  └──────────────┘  │
│         │                 ▲          │
│         │ invoke()        │ listen() │
│         ▼                 │          │
└─────────────────────────────────────┘
          │                 │
          ▼                 │
┌─────────────────────────────────────┐
│       Tauri Bridge (Events)          │
│    Commands  ◄───────►  Events       │
└─────────────────────────────────────┘
          │                 ▲
          ▼                 │
┌─────────────────────────────────────┐
│      Rust Backend (Lifecycle)        │
│  ┌──────────────────────────────┐   │
│  │  DataIngestCoordinator       │   │
│  │  ┌────────────┐ ┌──────────┐ │   │
│  │  │PlayerLog   │ │ ChatLog  │ │   │
│  │  │Watcher     │ │ Watcher  │ │   │
│  │  └────────────┘ └──────────┘ │   │
│  └──────────────────────────────┘   │
│                │                     │
│                ▼                     │
│         ┌────────────┐               │
│         │  Settings  │               │
│         └────────────┘               │
│                │                     │
│                ▼                     │
│         ┌────────────┐               │
│         │  SQLite DB │               │
│         └────────────┘               │
└─────────────────────────────────────┘
```

## Key Components

### Rust Side

| Component | Purpose | Location |
|-----------|---------|----------|
| **DataIngestCoordinator** | Central orchestrator for log watching | [`coordinator.rs`](../../src-tauri/src/coordinator.rs) |
| **LogFileWatcher** | Trait for unified file watching | [`log_watchers.rs`](../../src-tauri/src/log_watchers.rs) |
| **PlayerLogWatcher** | Watches Player.log (master log) | [`log_watchers.rs`](../../src-tauri/src/log_watchers.rs) |
| **ChatLogWatcher** | Watches daily chat logs | [`log_watchers.rs`](../../src-tauri/src/log_watchers.rs) |
| **SettingsManager** | Early-init settings management | [`settings.rs`](../../src-tauri/src/settings.rs) |
| **Database** | SQLite with deduplication | [`db/migrations.rs`](../../src-tauri/src/db/migrations.rs) |

### Frontend Side

| Component | Purpose | Location |
|-----------|---------|----------|
| **coordinatorStore** | Event-driven coordinator state | [`stores/coordinatorStore.ts`](../../src/stores/coordinatorStore.ts) |
| **chatStore** | Backward-compatible chat wrapper | [`stores/chatStore.ts`](../../src/stores/chatStore.ts) |
| **App.vue** | Initialization and polling setup | [`App.vue`](../../src/App.vue) |

## Event Flow

```
User Action (Frontend)
  │
  ▼
invoke('start_chat_tailing')
  │
  ▼
DataIngestCoordinator.start_chat_log_tailing()
  │
  ├─> Creates ChatLogWatcher
  ├─> Starts watching
  ├─> Sets OperationType::ChatTailing
  └─> Emits 'coordinator-status' event
        │
        ▼
      Frontend listen('coordinator-status')
        │
        ▼
      coordinatorStore updates
        │
        ▼
      Components reactively update
```

## Polling Flow

```
App.vue: coordinator.startPolling(1500)
  │
  ▼
setInterval(() => invoke('poll_watchers'), 1500)
  │
  ▼
DataIngestCoordinator.poll()
  │
  ├─> PlayerLogWatcher.poll()
  │     └─> Returns Vec<LogEvent>
  │
  ├─> ChatLogWatcher.poll()
  │     └─> Returns Vec<LogEvent>
  │
  ├─> process_player_events()
  │     ├─> Handle character login
  │     ├─> Handle area transition
  │     └─> Start/update chat watcher
  │
  └─> process_chat_events()
        ├─> Batch insert to database (INSERT OR IGNORE)
        └─> Emit 'chat-messages-inserted' event
              │
              ▼
            Frontend updates message count
```

## Database Schema (Key Tables)

### log_file_positions
Unified position tracking for all log types.
```sql
file_path (PK) | file_type | last_position | player_name | last_processed
```

### chat_messages
Chat messages with deduplication.
```sql
id (PK) | timestamp | channel | sender | message | is_system | log_file
UNIQUE(timestamp, channel, sender, message)  -- Prevents duplicates
```

### character_snapshots
Character progression from Export Character as JSON (Settings → VIP).
```sql
id (PK) | character_name | server_name | snapshot_timestamp | raw_json
UNIQUE(character_name, server_name, snapshot_timestamp)
```

## Startup Sequence

```
1. Get app data directory
   └─> tauri::PathResolver::app_data_dir()

2. Initialize SettingsManager
   └─> Reads settings.json
   └─> Determines database path

3. Initialize Database
   └─> Creates connection pool
   └─> Runs migrations if needed

4. Initialize DataIngestCoordinator
   └─> Stores db_pool, settings, app_handle

5. Register managed state
   └─> app.manage(settings_manager)
   └─> app.manage(db_pool)
   └─> app.manage(coordinator)

6. Frontend initialization (App.vue)
   └─> Start coordinator polling (1.5s)
   └─> Auto-start player log (if enabled)
   └─> Auto-start chat log (if enabled)
```

## Command Reference

### Coordinator Commands
```typescript
// Start/stop watchers
await invoke('start_player_tailing')
await invoke('stop_player_tailing')
await invoke('start_chat_tailing')
await invoke('stop_chat_tailing')

// Get status
const status = await invoke('get_coordinator_status')

// Poll for new events (called by setInterval)
await invoke('poll_watchers')
```

### Chat Query Commands
```typescript
// Get messages
const messages = await invoke('get_chat_messages', {
  channel: 'Global',
  limit: 100,
  offset: 0
})

// Get channels
const channels = await invoke('get_chat_channels')

// Scan all logs
const result = await invoke('scan_chat_logs', {
  chatLogsDir: 'path/to/ChatLogs'
})
```

## Event Reference

### Events Emitted by Rust

| Event | Payload | When |
|-------|---------|------|
| `coordinator-status` | `CoordinatorStatus` | Watcher started/stopped, status changed |
| `character-login` | `string` (character name) | Character logs in |
| `area-transition` | `string` (area name) | Player enters new area |
| `chat-messages-inserted` | `number` (count) | New messages added to database |

### Frontend Listeners

```typescript
// In coordinatorStore.ts
listen('coordinator-status', (event) => {
  status.value = event.payload
})

listen('character-login', (event) => {
  console.log('Character logged in:', event.payload)
})

listen('area-transition', (event) => {
  console.log('Area transition:', event.payload)
})

listen('chat-messages-inserted', (event) => {
  newChatMessageCount.value += event.payload
})
```

## Settings

```typescript
interface AppSettings {
  logFilePath: string              // Legacy player log path
  autoWatchOnStartup: boolean      // Legacy auto-watch
  gameDataPath: string             // Root game data dir
  autoPurgeEnabled: boolean        // Auto-delete old data
  autoPurgeDays: number            // Days to keep
  autoTailChat: boolean            // Auto-start chat tailing ✨
  autoTailPlayerLog: boolean       // Auto-start player tailing ✨
  dbPath: string | null            // Custom DB path
}
```

## Directory Structure

```
glogger/
├── src-tauri/src/
│   ├── lib.rs                   # Main entry, initialization
│   ├── coordinator.rs           # DataIngestCoordinator
│   ├── log_watchers.rs          # PlayerLogWatcher, ChatLogWatcher
│   ├── settings.rs              # SettingsManager
│   ├── chat_parser.rs           # Chat message parsing
│   ├── chat_commands.rs         # Chat-related Tauri commands
│   └── db/
│       ├── mod.rs               # Database initialization
│       ├── migrations.rs        # Schema definitions
│       ├── queries.rs           # Query helpers
│       └── chat_commands.rs     # Chat database operations
│
├── src/
│   ├── App.vue                  # Main app, coordinator init
│   ├── stores/
│   │   ├── coordinatorStore.ts  # Event-driven coordinator state
│   │   ├── chatStore.ts         # Backward-compatible wrapper
│   │   └── settingsStore.ts     # Settings management
│   └── components/
│       └── Chat/
│           ├── ChatView.vue
│           └── ChatManagement.vue
│
└── docs/
    ├── reference/
    │   ├── data-architecture.md       # Full architecture docs
    │   ├── architecture-summary.md    # This file
    │   └── implementation-checklist.md
    └── guides/
        └── working-with-data-architecture.md
```

## Quick Start: Using the Architecture

### 1. Start Chat Tailing (Frontend)

```typescript
import { useCoordinatorStore } from '@/stores/coordinatorStore'

const coordinator = useCoordinatorStore()

// Start tailing
await coordinator.startChatTailing()

// Monitor new messages
watch(() => coordinator.newChatMessageCount, (count) => {
  console.log('New messages:', count)
})
```

### 2. Listen to Events

```typescript
import { listen } from '@tauri-apps/api/event'

// Listen for character login
listen('character-login', (event) => {
  console.log('Character:', event.payload)
})
```

### 3. Query Database

```typescript
import { invoke } from '@tauri-apps/api/core'

// Get recent messages
const messages = await invoke('get_chat_messages', {
  limit: 50,
  offset: 0
})
```

## Migration from Old Architecture

### Before (Frontend-Driven)
```typescript
// Polled Rust every 2 seconds
setInterval(async () => {
  const messages = await invoke('tail_chat_log', { chatLogFile })
  // Process messages...
}, 2000)
```

### After (Rust-Managed)
```typescript
// Start once, Rust handles polling
await coordinator.startChatTailing()

// Listen for updates
listen('chat-messages-inserted', (event) => {
  console.log('New messages:', event.payload)
})
```

## Best Practices

✅ **DO:**
- Use coordinator for lifecycle management
- Listen to events for state updates
- Use INSERT OR IGNORE for deduplication
- Drop read locks before acquiring write locks
- Emit status changes after state modifications
- Batch database operations when possible

❌ **DON'T:**
- Poll from multiple components independently
- Assume database operations succeed (use Result)
- Hold locks across async operations
- Forget to update operation status
- Create duplicate polling intervals
- Skip error handling

## Performance Targets

- Polling interval: 1.5 seconds
- Message insert batch: Up to 100 messages
- Database connections: Pool of 5
- Memory usage: < 100 MB for normal operation
- Startup time: < 2 seconds

## Testing

```bash
# Run Rust tests
cd src-tauri
cargo test

# Run in dev mode
cd ..
npm run tauri dev

# Build for production
npm run tauri:build
```

## Next Steps

See [Data Architecture Reference](data-architecture.md) for:
- Detailed component descriptions
- Complete event flow diagrams
- Database schema details
- Future enhancement plans

See [Working with Data Architecture](../guides/working-with-data-architecture.md) for:
- Step-by-step implementation guides
- Common patterns
- Debugging tips
- Performance optimization
