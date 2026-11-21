# Chat — Channels

## Overview

Browse public and custom chat channels (Global, Trade, Help, LFG, etc.) with per-channel message history and search.

## How It Works

- **Channel sidebar** — lists all non-JSON channels with message counts
- **Select a channel** to view its messages in the main panel
- **Search** within the selected channel by message text
- **Paginated loading** — 100 messages per page with "Load More"
- Messages rendered via `ChatMessageList` in standard layout with timestamps and sender names
- Item references in messages automatically detected and rendered as `ItemInline` components
