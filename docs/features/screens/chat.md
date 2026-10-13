# Chat Logs Screen

## Overview

A full chat log browser that imports, indexes, and displays all in-game chat with full-text search, per-channel filtering, item link detection, and configurable watchword alerts. Chat data is parsed from the game's chat log files and stored in SQLite with FTS indexing.

The screen also hosts a **Poems** tab — a browsable, searchable collection of poems recited by other players at poetry podiums. Unlike the other tabs (which come from `Chat-*.log`), poems are sourced from `Player.log` `ProcessTalkScreen` events and stored globally (not per-character).

## Architecture

### Files

**Backend (Rust):**
- `src-tauri/src/chat_parser.rs` — chat log file parsing
- `src-tauri/src/chat_status_parser.rs` — status channel event parsing
- `src-tauri/src/chat_commands.rs` — Tauri command handlers
- `src-tauri/src/db/chat_commands.rs` — database query layer

**Frontend (Vue/TS):**
- `src/components/Chat/ChatView.vue` — 10-tab container
- `src/components/Chat/ChatMessageList.vue` — shared paginated message renderer
- `src/components/Chat/ChatSearchView.vue` — unified search; results open in the day's real log
- `src/utils/displayDay.ts` — display-timezone day bucketing (offset → day / UTC day bounds) shared by the day picker, windows and headers
- `src/components/Chat/ChatMessage.vue` — individual message display
- `src/components/Chat/MessageWithItemLinks.vue` — item link detection and rendering
- `src/utils/parseSearchQuery.ts` — search query parser for `from:`/`in:` operators
- Channel views: `ChannelView`, `TellsView`, `PartyView`, `NearbyView`, `GuildView`, `SystemView`, `AllMessagesView`, `WatchwordsView`
- `src/components/Chat/PoemsView.vue` — recorded-poems browser (list + full-poem viewer with prev/next)

**Poems pipeline (Rust):**
- `src-tauri/src/player_event_parser.rs` — `handle_talk_screen` detects `Poem by X` talk screens and emits `PlayerEvent::PoemRecorded` (`parse_poem_body` strips the intro/outro review blurbs and pulls out the bold title)
- `src-tauri/src/game_state.rs` — persists `PoemRecorded` into the `poems` table (`INSERT OR IGNORE` for dedup)
- `src-tauri/src/db/poem_commands.rs` — `get_poems` query command

**Stores:**
- `chatStore` — tailing state management
- `settingsStore` — watchword rule persistence

### Component Hierarchy

```
ChatView.vue                        — 9-tab container
├── ChatSearchView.vue              — unified search (default tab)
├── ChannelView.vue                 — public/custom channels with sidebar
├── TellsView.vue                   — direct messages with conversation list
├── PartyView.vue                   — party channel
├── NearbyView.vue                  — nearby/local chat
├── GuildView.vue                   — guild chat
├── SystemView.vue                  — system/status messages
├── AllMessagesView.vue             — global search across all channels
├── WatchwordsView.vue              — rule-based filtering and alerts
└── PoemsView.vue                   — recorded poems (self-contained; list + viewer)

Shared:
├── ChatMessageList.vue             — paginated message list (standard + bubble layouts)
└── MessageWithItemLinks.vue        — parses and links item references
```

## Per-Tab Documentation

- [chat-channels.md](chat/chat-channels.md) — Channels
- [chat-tells.md](chat/chat-tells.md) — Tells (Direct Messages)
- [chat-simple.md](chat/chat-simple.md) — Party, Nearby, Guild, System
- [chat-all.md](chat/chat-all.md) — All Messages (Global Search)
- [chat-watchwords.md](chat/chat-watchwords.md) — Watchwords (Alert Rules)

## Shared Components

### ChatMessageList

Generic message list renderer used by all tabs:
- **Standard layout** — timestamp, channel badge (optional), sender name, message body
- **Tell/bubble layout** — chat bubbles with player messages on right, others on left
- **Day toolbar** — day dropdown (display-timezone days + counts), date picker, ◀/▶ day stepping, Back to Live, and the ascending/descending sort toggle (kept in the toolbar so it stays reachable once a day runs several pages)
- **Pagination** — infinite scroll in both directions (auto-loads within 200px of either edge), 100 messages per page. The continuation affordances are **pinned to the list edges**, because the in-flow button/spinner sat a whole page below the fold and gave no sign that a page was coming: the top edge shows "Load newer|older messages · N above" while rows exist above, and the bottom edge reads "More messages below", switches to "Loading more messages…" while a page is in flight, and "End of the loaded window" once the window is exhausted. Callers that know the query's size pass `windowStart`/`windowTotal` for a "300 of 4,656" readout. Prepends re-anchor `scrollTop` so the visible row does not move — the container carries `overflow-anchor: none` so that adjustment is the only thing compensating for the inserted rows (Chromium's own scroll anchoring would otherwise double it and land the viewport near the bottom). The outer element is `flex-1 min-h-0`, not `h-full`, so the list cannot overflow the view header above it (with `h-full` its bottom ~75px — where the bottom pill pins — was clipped). Race-condition guarded (emit guards in the scroll handler + loading check in parent). The pinned top pill also stands in for "Jump to Present" while newer rows exist above: that button means "scroll to the newest end", which is only true when the window's top *is* the newest end (and reaching the top edge would trigger a newer-page load anyway).
- **Keyboard** — when the list has focus, PageUp/PageDown page the window (holding them streams pages through the edge loaders) and Home/End jump to the ends; the parent view forwards the page keys while its search box holds focus
- **Clickable messages** — optional `clickable` prop enables click-to-open; `highlightId` prop marks a specific message with a gold border and scrolls it into view (instant, so the intermediate scroll events can't trip the edge loaders)
- **Timestamps** — short format for today, full format for older messages; day headers appear only when the loaded window spans more than one display day

### MessageWithItemLinks

Parses message text to detect `[Item: ItemName]` patterns and renders them as `ItemInline` components with hover tooltips and click-to-navigate behavior.

## Database Schema

| Table | Purpose |
|-------|---------|
| `chat_messages` | Core message storage (timestamp, channel, sender, message, flags) |
| `chat_item_links` | Item references found in messages (raw_text, item_name, item_id) |
| `chat_messages_fts` | Full-text search index on message content |
| `poems` | Recorded poems (author, title, content, recorded_at). Global, not character-scoped; deduped via `UNIQUE(author, title, content)` |

## Tauri Commands

### Import & Tailing
- `scan_chat_logs(path) → ScanResult` — bulk import all chat logs from directory
- `scan_chat_log_file(path) → ScanResult` — import single file
- `tail_chat_log(chat_log_file) → Vec<ChatMessage>` — continuous import of active log

### Query
- `get_chat_messages(ChatFilter) → Vec<ChatMessage>` — filtered message query (day windows are ordinary `startTime`/`endTime` bounds on this query)
- `count_chat_messages(ChatFilter) → number` — count matching the same filter semantics
- `get_chat_days(offsetSeconds?) → Vec<ChatDayRow>` — every day that has messages with its count, newest first, bucketed in the viewer's display timezone
- `get_chat_channels() → Vec<String>` — list all channels
- `get_chat_channel_stats() → Vec<ChannelStat>` — per-channel message counts
- `get_tell_conversations() → Vec<ChannelStat>` — list conversation partners
- `get_watch_rule_messages(rule_id, limit, offset) → Vec<ChatMessage>` — messages matching a watchword rule
- `get_chat_stats() → ChatStats` — overall statistics
- `get_poems() → Vec<PoemRow>` — all recorded poems (global, newest first); searching/filtering is done on the frontend
- `scan_player_log_for_poems() → usize` — one-time backfill: scans the entire current Player.log for past `Poem by X` recitals and inserts them (deduped). Exposed as the "Scan Player.log" button on the Poems tab, since the live watcher only sees poems recited after it starts tailing.

### Maintenance
- `purge_chat_messages(days) → usize` — delete messages older than N days
- `delete_all_chat_messages() → usize` — wipe all chat data

### ChatFilter

```typescript
interface ChatFilter {
  channel?: string;
  sender?: string;
  searchText?: string;
  startTime?: string;
  endTime?: string;
  hasItemLinks?: boolean;
  itemName?: string;
  tellPartner?: string;
  limit?: number;
  offset?: number;
  sortOrder?: 'asc' | 'desc';
}
```

## Key Design Decisions

- **FTS indexing** — full-text search via SQLite FTS5 for fast text queries across potentially millions of messages.
- **Item link extraction at parse time** — item references are detected during import and stored in a separate table, enabling "item links only" filtering without re-scanning messages.
- **Watchword rules in settings** — rules persist in `settingsStore` (app settings file) rather than the database, keeping them lightweight and portable.
- **Deduplication** — `INSERT OR IGNORE` prevents duplicate messages when re-importing or tailing overlapping ranges.
- **Offset pagination** — simple offset/limit pagination rather than cursor-based, sufficient for chat browsing patterns. Continuation works in both directions: appending below the window and (after a mid-day jump) prepending above it, with the scroll position re-anchored so the visible row stays put.
- **Display-timezone day buckets** — stored timestamps are UTC but every rendered clock time is shifted into the viewer's display timezone (Local/Server/UTC), so the day picker, the day windows it opens and the day headers inside the list are all bucketed with that same shift (`src/utils/displayDay.ts`). Day bounds are handed to the query as UTC `startTime`/`endTime`.
- **Structured search syntax** — `from:player` and `in:channel` operators parsed on the frontend and mapped to existing backend filter fields. Remaining text goes to FTS5.
- **Results open in the day's real log** — clicking a search result selects that message's day and loads a window centred on it (row position derived from two `count_chat_messages` calls), so the entry sits among its real neighbours with channel colours and pagination that keeps going, instead of a fixed ±N context sample. The query box stays live above it ("← Back to results" restores the list).
