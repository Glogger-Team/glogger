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

## Stall Price Observations — removed (batch-1, 2026-10-30)

The experimental stall-price surface (capture panel, auto-detected purchase
sentinels, OCR scan, `get/record/update/delete_stall_price_observation`
commands, tooltip "Last seen" line) was stripped from the live app. Table
`stall_price_observations` (migration V68) and existing rows are retained.
Re-introduction (capture panel + OCR + purchase-sentinel fill-in) stays a
backlog item pending a reliable stall-browsing signal.
