# Economics — Market Prices

## Overview

A player-maintained price database for tracking market values of items. Used throughout the app for valuation (inventory worth, farming profit, survey economics).

## How It Works

- **Search and browse** existing market values via full-width search with FilterBar, summary stat cards at top
- **Add prices** via autocomplete item picker with ItemInline previews — auto-focus price input, Enter/Escape shortcuts, batch entry with success feedback, duplicate detection with update option. Add form in collapsible AccordionSection.
- **Edit inline** — click any price to modify it with real-time save, Cancel button to discard
- **Delete** prices no longer needed
- **Bulk operations** — multi-select checkboxes on rows, bulk action bar with Set Price, Adjust Prices (percentage), and Delete Selected. Backend `bulk_update`/`bulk_delete` commands. Confirmation dialog for destructive operations.
- **Notes** field per item for context (e.g., "checked 2024-03-15")

## Price Modes

Toggle between two modes (via Settings):
- **Universal** — prices apply across all servers (`server="*"`)
- **Per-Server** — prices scoped to the active server

## Valuation Modes

Six options controlling how item values are calculated app-wide:
1. Highest of market or vendor
2. Market price only
3. Vendor price only
4. Market if available, else vendor
5. Vendor if available, else market
6. Always zero

## Import / Export

- **Export** — copies all market values as JSON to clipboard
- **Import** — paste JSON with conflict resolution strategies:
  - **Newest wins** — keep whichever entry has a more recent timestamp
  - **Overwrite** — imported values always win
  - **Keep existing** — existing values always win

## Tauri Commands

- `get_market_values() → Vec<MarketValue>`
- `get_market_value(item_type_id) → Option<MarketValue>`
- `set_market_value(item_type_id, item_name, market_value, notes)`
- `delete_market_value(item_type_id)`
- `bulk_update_market_values(updates)` — batch update multiple prices
- `bulk_delete_market_values(item_type_ids)` — batch delete multiple entries
- `export_market_values() → String` (JSON)
- `import_market_values(json_data, strategy) → ImportResult`

## Stall Price Observations (Other Players' Stalls) — experimental

> **Experimental.** The capture flow is a work in progress and has not been
> verified against live stalls end to end. The capture panel appears on browse
> events; rows are entered manually (typed lines, or bought-item sentinels you
> price yourself). Observations are a separate table and never overwrite your
> curated `market_values`.

The Market Prices tab also lists **stall price observations** — prices seen at
other players' stalls, distinct from your curated `market_values` (which are
never auto-overwritten).

### How prices get in

- **Capture panel** — browsing another player's stall in game emits
  `stall-browse-started` and opens a bottom-right capture panel on the Market
  tab. Type one item per line: `Item [x<qty>] @ <price>` (e.g. `Carrot x80 @ 5`,
  `21890 @ 2500`). Unresolved item names are reported inline and not saved.
- **Auto-detected purchases** — buying from another stall records a
  `source='purchase'` row with `price_unit = 0` (the game never logs prices).
  Set the price inline in the observations table.
- **OCR scan (Windows, opt-in)** — the panel's "Scan window" button captures
  the game window, runs a Tesseract child process (portable build downloaded
  into appdata on first use), and prefills the textarea from the stall's rows.
  Item names fuzzy-match against the CDN list (trigram Dice ≥ 0.75); unmatched
  OCR text lands in the panel for manual correction. Nothing auto-saves.

### Data model

Table `stall_price_observations` (migration V68), keyed
`UNIQUE(observed_at, item_name, stall_npc_entity_id, price_unit)` —
`INSERT OR IGNORE` makes captures and purchase detection idempotent.
`price_unit = 0` is the purchase sentinel (the game has no zero-price
listings). Purged with the player-data purge on `observed_at`.

### Tauri Commands

- `get_stall_price_observations(server_name?, limit?, item_type_id?) → Vec<StallPriceObservation>`
- `record_stall_prices(entries: Vec<StallPriceEntry>) → usize` (inserted count after dedup)
- `update_stall_price_observation(id, price_unit, stall_label?, owner_name?)`
- `delete_stall_price_observation(id)`
- `scan_stall_window() → StallScanResult` (OCR; Windows only)
- `ocr_check_status() → OcrStatus`, `ocr_download()`, `ocr_launch_setup()`

### Item tooltips

Items with an observation show a "Last seen: 3x 2500g at <stall> (2h ago)"
line above the market-value edit row in the item tooltip.
