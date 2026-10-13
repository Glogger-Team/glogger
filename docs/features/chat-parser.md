# Chat Parser — Feature Spec

## Purpose

The chat parser is the core engine that reads Project Gorgon chat log files, extracts structured data from each line, and prepares it for database storage. It must handle all known chat line formats, support incremental file reading, and integrate with the settings-driven architecture.

---

## Chat Log File Format

### File Naming
- Pattern: `Chat-YY-MM-DD.log`
- Location: `{game_data_path}/ChatLogs/`
- One file per day, may contain multiple login/logout sessions

### File Structure
Each file begins with a login banner, followed by timestamped chat lines:

```
**************************************** Logged In As PlayerName. Server Dreva. Timezone Offset -07:00:00.
26-03-09 05:00:17	[Status] You have 4 friends online.
26-03-09 05:00:18	[Global] SomePlayer: hello everyone
26-03-11 12:46:01	[Tell] You->AnotherPlayer: you need?
[Item: Cow: Moo of Calm 7]
26-03-11 12:46:21	[Tell] AnotherPlayer->You: I JUST traded for it today haha
...
******************** Entering Area: Casino
...
**************************************** Logged Out.
```

### Line Format
Every content line follows: `YY-MM-DD HH:MM:SS<TAB>content`

The content portion has several variants:
1. **Channel + Sender**: `[Channel] Sender: message text`
2. **Channel only (system)**: `[Channel] system message text`
3. **No channel (system)**: `raw system text` (login banners, area transitions, etc.)
4. **Tell messages**: `[Tell] Sender->Recipient: message text`
5. **Combat messages**: `[Combat] EntityName #ID: combat info`

---

## Features

### 1. Line Parsing (`parse_chat_line`)

Parse a single chat log line into a structured `ChatMessage`.

**Input:** A raw string line from the log file.

**Output:** `Option<ChatMessage>` — `None` for blank lines.

**ChatMessage fields:**
| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | `NaiveDateTime` | Parsed from `YY-MM-DD HH:MM:SS` |
| `channel` | `Option<String>` | Channel name from `[Channel]` bracket, or `None` for system lines |
| `sender` | `Option<String>` | Player name if present, `None` for system messages |
| `message` | `String` | The message body |
| `is_system` | `bool` | `true` for non-player messages |
| `from_player` | `Option<bool>` | Tell direction: `true` = outgoing, `false` = incoming |
| `item_links` | `Vec<ItemLink>` | Extracted `[Item: ...]` references |

**Parsing rules:**
- Split line on first `\t` to get timestamp + content
- If content starts with `[`, extract channel name from brackets
- For Tell channel: parse `Sender->Recipient` arrow format, normalize so `sender` is always the conversation partner
- For other channels: detect system vs player messages (system indicators: starts with `-`, starts with `You `, contains `#`)
- For non-bracketed content: always system messages (banners, area transitions)
- Chat messages can include linebreaks.

**Gotcha — the sender is already split off `[Channel] Sender: text` lines.** `message` therefore does **not** contain `"Sender: "`, so any downstream check that compares `message` against a whole raw line can never match. Two per-ability hooks live in the coordinator's Combat branch (`DataIngestCoordinator::process_chat_events`) for exactly this reason, both driven by `chat_combat_parser::parse_self_ability_line`:

| Line | Effect |
|---|---|
| `[Combat] Name: Collect Milk on Name!` | `player_milking_log` row, direction `self_milked` |
| `[Combat] Name: Poop on Name!` | `player_pooping_log` row (Dungcrafting count) |

The `Collect Milk` check compared `message` against `"Name: Collect Milk on Name!"` until 2026-10, which could never match a sender-split message: the archive's self-milk lines therefore recorded **zero** rows before the fix (the count is only knowable from the logs, which is what `backfill_ability_history` measures).

### 2. Channel Exclusion

**Important:** Channel exclusion controls what gets **persisted to the database**, not what gets parsed. The parser itself parses ALL channels — exclusion is applied at the `insert_chat_messages()` call in the coordinator. This ensures that structured parsers (like `ChatStatusParser` for `[Status]` messages) always receive their data regardless of the user's exclusion settings.

**Configurable** via `settings.excluded_chat_channels`. Default exclusions:
- Error
- Emotes
- Action Emotes
- NPC Chatter
- System
- Status
- Combat

See [live-event-streams.md](../architecture/live-event-streams.md) for how Status and other excluded channels are still routed to structured parsers.

### 3. Item Link Extraction (`extract_item_links`)

Detect and parse `[Item: ...]` patterns within message text. Sometimes the item pattern is on a newline following the text. 

**Format:**
- `[Item: ItemName]` 

**ItemLink fields:**
| Field | Type | Description |
|-------|------|-------------|
| `raw_text` | `String` | Full bracket text, e.g. `[Item: Leatherworking: Great Evasion Shirt]` |
| `item_name` | `String` | Just the item name portion |


### 4. Session Detection

Detect login/logout boundaries within a single daily log file.

**Patterns to detect:**
| Pattern | Event |
|---------|-------|
| `Logged In As {Name}. Server {Server}.` | Session start — extract player name and server |
| `Logged Out.` | Session end |
| `Entering Area: {AreaName}` | Area transition (within a session) |

**Extracted session metadata:**
- Player name (from login banner)
- Server name (from login banner)
- Timezone offset (from login banner)
- Area transitions with timestamps

### 5. File Discovery (`get_chat_log_files`)

Scan the ChatLogs directory for log files.

**Behavior:**
- Find all files matching `Chat-YY-MM-DD.log`
- Parse date from filename
- Return sorted list (newest first)
- Gracefully handle missing directory

### 6. Incremental Reading (`read_chat_log`)

Read a log file starting from a byte offset (for tailing/incremental imports).

**Behavior:**
- Seek to `start_position` byte offset
- Read remaining content
- Parse all lines
- Return `(Vec<ChatMessage>, new_position)` where `new_position` = file size after read
- Return empty vec if `start_position >= file_size`

### 7. Player Name Extraction (`extract_player_name`)

Read the login banner from the beginning of a chat log to get the player's character name.

**Pattern:** `Logged In As {PlayerName}. Server {ServerName}.`

### 8. Unread Notifications

Track unread message counts per channel and surface notifications to the user.

**Behavior:**
- Each channel maintains an independent unread count
- Count increments when new messages arrive via tailing and the user is NOT viewing that channel
- Count resets to 0 when the user opens/views that channel
- Unread badges shown on channel tabs in the Chat UI

**Configuration (per channel):**
- `enabled` — whether to show unread badge for this channel (default: `true` for player channels, `false` for system channels)
- Stored in settings as `chat_notification_channels: Vec<String>` — list of channels with notifications enabled
- Users toggle per-channel in the Chat settings or channel tab context menu

**Implementation notes:**
- Unread state is frontend-only (ephemeral, not persisted to DB) — resets on app restart
- The coordinator emits events with channel info when new messages arrive; the frontend store tracks counts
- Tell channel shows per-conversation unread counts (not just one total)

### 9. Watchwords & Item Watches

Alert the user when incoming chat messages match configurable watch rules. Designed for trade sniping, monitoring specific topics, or tracking mentions of your name.

**Watch Rule structure:**
| Field | Type | Description |
|-------|------|-------------|
| `id` | `u64` | Unique rule ID |
| `name` | `String` | User-defined label, e.g. "Flamestrike deals" |
| `enabled` | `bool` | Toggle without deleting |
| `channels` | `Option<Vec<String>>` | Channels to match (`None` = all channels) |
| `conditions` | `Vec<WatchCondition>` | ALL conditions must match (AND logic) |
| `notify` | `WatchNotifyConfig` | How to alert the user |

**WatchCondition variants:**
| Condition | Example | Description |
|-----------|---------|-------------|
| `ContainsText(String)` | `"WTS"` | Case-insensitive substring match on message body |
| `ContainsItemLink(String)` | `"Strange Dirt"` | Match on item name within `[Item: ...]` links |
| `FromSender(String)` | `"TraderJoe"` | Match on sender name |

All conditions in a rule use **AND** logic — every condition must match for the rule to fire. Users create multiple rules for OR logic.

**Examples:**
- "WTS + Flamestrike": `conditions: [ContainsText("WTS"), ContainsText("Flamestrike")]` in Trade channel
- "Strange Dirt in any channel": `conditions: [ContainsItemLink("Strange Dirt")]`, `channels: None`
- "Messages from a friend": `conditions: [FromSender("BestFriend")]`, `channels: None`
- "Guild mentions crafting": `conditions: [ContainsText("craft")]`, `channels: ["Guild"]`

**WatchNotifyConfig:**
| Field | Type | Description |
|-------|------|-------------|
| `sound` | `bool` | Play a notification sound |
| `toast` | `bool` | Show a system/in-app toast notification |
| `highlight` | `bool` | Highlight the matching message in the chat view |

**Storage:** Watch rules are persisted in `settings.json` under `watch_rules: Vec<WatchRule>`. They are evaluated in Rust on the backend as messages are parsed during tailing, so alerts fire even if the chat UI isn't open.

**Evaluation flow:**
1. New message parsed during tailing
2. For each enabled watch rule:
   - Check if message channel matches rule's channel filter
   - Evaluate all conditions against the message
   - If all match → emit a `watch-rule-triggered` event to the frontend with rule ID + message
3. Frontend displays notification per the rule's `WatchNotifyConfig`

### 10. Chat Search

Search historical chat messages with flexible filters. Serves both the "search old chats" UI and programmatic queries.

**Search filters (all optional, combinable):**
| Filter | Type | Description |
|--------|------|-------------|
| `query` | `String` | Full-text substring search on message body (case-insensitive) |
| `channel` | `String` | Exact channel match |
| `sender` | `String` | Exact sender match |
| `from_date` | `NaiveDate` | Messages on or after this date |
| `to_date` | `NaiveDate` | Messages on or before this date |
| `has_item_links` | `bool` | Only messages containing item links |
| `item_name` | `String` | Messages containing a specific item link |
| `tell_partner` | `String` | For Tell channel: messages to/from a specific player |

**Behavior:**
- Results ordered by timestamp (newest first by default, toggleable)
- Paginated: `limit` + `offset` for infinite scroll
- Returns full `ChatMessageRow` with item links attached
- Search is performed via SQL queries on the `chat_messages` table (indexed on timestamp, channel, sender)

**UI placement:**
- Global search bar in the Chat view header — searches across all channels
- Per-channel filter bar — scoped to current channel
- Tell search — scoped to a specific conversation partner

**Performance considerations:**
- Add SQLite FTS5 index on `message` column if substring search becomes slow at scale
- For now, `LIKE '%query%'` is acceptable for the expected data volume

---

## Integration Points

### Settings Integration
- `excluded_chat_channels` — drives channel filtering (replaces hardcoded `EXCLUDED_CHANNELS`)
- `game_data_path` — determines ChatLogs directory location
- `chat_retention_days`, `tells_retention_days`, `guild_retention_days` — retention policies per channel type
- `chat_notification_channels` — which channels show unread badges
- `watch_rules` — persisted watchword/item watch rule definitions

### Database Integration
- Messages inserted via `insert_chat_messages()` with `INSERT OR IGNORE` deduplication
- Unique constraint on `(timestamp, channel, sender, message)` prevents duplicates
- Item links inserted into `chat_item_links` with automatic item ID resolution
- File positions tracked in `log_positions` table for incremental reads

### Coordinator Integration
- `ChatLogWatcher` calls the parser for tailing operations
- `DataIngestCoordinator` calls the parser for full scans
- Both use byte-position tracking to avoid re-processing

### Frontend Integration
- Parsed messages returned as `ChatMessageRow` with attached `ChatItemLinkRow` data
- Frontend renders item links via [MessageWithItemLinks.vue](../../src/components/Chat/MessageWithItemLinks.vue)

---

## Known Chat Channels

Based on observed log data:

| Channel | Type | Notes |
|---------|------|-------|
| Global | Player chat | Server-wide |
| Trade | Player chat | Buy/sell messages, frequent item links |
| Guild | Player chat | Guild-only, may want longer retention |
| Party | Player chat | Group chat |
| Nearby | Player chat | Proximity-based |
| Tell | Player chat | Private messages, has sender->recipient format |
| Combat | System | Entity combat info, has `#ID` format |
| Status | System | Item gains, XP, economy events — excluded from DB by default but always parsed for `ChatStatusParser` (see [live-event-streams.md](../architecture/live-event-streams.md)) |
| Error | System | Game errors — excluded by default |
| Emotes | System | Player emotes — excluded by default |
| Action Emotes | System | Action-based emotes — excluded by default |
| NPC Chatter | System | NPC dialogue — excluded by default |
| System | System | System notifications — excluded by default |
| Loot | System | Loot notifications |
| Skills | System | Skill-up messages |

---

## Edge Cases & Robustness

### Must Handle
- Empty/blank lines → skip
- Lines without tab separator → skip
- Malformed timestamps → skip line
- Multi-session files (multiple login/logout in one day)
- UTF-8 encoding issues → `from_utf8_lossy`
- Files that shrink (game recreates file) → reset position to 0
- Messages with colons in content (don't split on wrong colon)
- Item links with colons in item names (e.g. `[Item: Cow: Moo of Calm 7]`)
- Item links on continuation lines (no timestamp prefix, just `[Item: ...]`)
- Multiline messages — continuation lines lack the `YY-MM-DD HH:MM:SS\t` prefix
- Tell messages where player name contains special characters

### Resolved Questions
- **Should we store Combat channel messages?** No — separate feature. A future `CombatParser` would follow the same pattern as `ChatStatusParser`.
- **Should we detect and store "Skills" / "Loot" channel messages for tracking progression?** No — Player.log provides this via `PlayerEventParser`.
- **Should Status channel be parsed despite being excluded from DB?** Yes — `ChatStatusParser` receives all Status messages regardless of `excluded_channels`. See [live-event-streams.md](../architecture/live-event-streams.md).
- **Should we use timezone offset from login banner?** Yes — implemented. The offset is parsed, stored in settings, and used for UTC timestamp normalization across the app.
- **Should we extract server name from login banner?** Yes — implemented. Server name is auto-detected and stored in settings.

### Open Questions
- Should area transitions be stored as first-class events (not just system messages) for session timeline features?
- Should we parse quest-related patterns (e.g., `[Quest] You completed...`) if that channel exists?

---

## Implementation Status

Completed:
1. Settings-driven exclusion (applied at DB persistence layer, not parse time)
2. Multiline message handling (continuation lines appended to previous message)
3. Session detection (login/logout/area-change parsed into `LogEvent` variants)
4. Robust edge cases (blank lines, malformed timestamps, UTF-8 lossy)
5. Watchwords & item watches (rule engine evaluated during tailing)
6. Server + timezone extraction from login banner
7. Channel exclusion refactor (parsing sees all channels; filtering only at `insert_chat_messages`)

Remaining:
- Chat search — SQL-backed search with filters
- Unread notifications — per-channel unread counts with configurable badges
