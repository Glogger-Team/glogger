# Chat — JSON

## Overview

Raw `JSON:*` lines from `Chat-*.log`, isolated in their own tab. These are the
machine-readable game events (loot receipts, XP awards, book bodies, favor
deltas, pigeon mail, tips) that the typed parsers consume. The tab shows the
lines exactly as logged, newest first, with the channel badge naming the Type.

## How It Works

- **`jsonScope: 'only'`** — the tab queries `get_chat_messages` with the JSON
  scope set to only, so it sees `channel LIKE 'JSON:%'` rows and nothing else.
- **Day navigation and text search** work like every other tab. Full Search
  (the Search tab) also matches JSON lines; every other chat view excludes
  them, including Watchwords and the Channels sidebar counts.
- **No item-link filters** — JSON lines carry no `chat_item_links` rows, so the
  tab offers only the text search box.

## Use Cases

- Checking exactly what the game logged for a loot, XP, or favor event.
- Copying a real line as a new parser fixture.
- Confirming a new `JSON:Type` appeared before wiring an arm for it.
