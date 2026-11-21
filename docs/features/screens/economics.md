# Economics Screen

## Overview

The economics screen consolidates tools for tracking wealth, market prices, farming profitability, and surveying analytics. Four tabs cover different aspects of the game's economy.

## Architecture

### Files

**Backend (Rust):**

- `src-tauri/src/db/market_commands.rs` — market value CRUD, import/export
- `src-tauri/src/db/farming_commands.rs` — farming session persistence
- `src-tauri/src/db/survey_commands.rs` — survey type CDN reference data (read)
- `src-tauri/src/survey/` — survey tracker module (aggregator, persistence, commands) on the provenance pipeline

**Frontend (Vue/TS):**

- `src/components/Economics/EconomicsView.vue` — 4-tab container
- `src/components/Market/MarketView.vue` — market price management
- `src/components/Economics/EconomicsFarmingView.vue` — farming session wrapper
- `src/components/Farming/` — farming session components
- `src/components/Economics/EconomicsSurveyView.vue` — surveying 3-tab wrapper
- `src/components/Surveying/` — SurveyTrackerView, HistoricalTab, AnalyticsTab

**Stores:**

- `marketStore` — market value CRUD, import/export, valuation modes
- `farmingStore` — farming session lifecycle, live event tracking
- `surveyTrackerStore` — read-through cache over the backend survey tracker

### Component Hierarchy

```text
EconomicsView.vue                   — 4-tab container
├── MarketView.vue                  — market price management
├── EconomicsFarmingView.vue        — farming wrapper
│   ├── FarmingSessionCard.vue      — active session with live tracking
│   └── HistoricalTab.vue           — past session browser
├── EconomicsSurveyView.vue         — surveying 3-tab wrapper
│   ├── SurveyTrackerView.vue       — Session: live tracker over backend state
│   ├── HistoricalTab.vue           — Session History: expandable session list
│   └── AnalyticsTab.vue            — Zones / Survey Types / Items views
└── StallTrackerView.vue            — stall tracker (shop log analytics)
```

## Per-Tab Documentation

- [economics-market.md](economics/economics-market.md) — Market Prices
- [economics-farming.md](economics/economics-farming.md) — Farming Sessions
- [economics-surveying.md](economics/economics-surveying.md) — Surveying Sessions
- Stall Tracker — not yet implemented

## Database Tables

- **`market_values`** — item prices keyed by (server_name, item_type_id), with timestamps
- **`farming_sessions`** — session headers (name, notes, start/end, elapsed, vendor gold, attributing character_name/server_name, NULL on legacy rows)
- **`farming_session_skills`** — XP/level data per skill per session
- **`farming_session_items`** — net item quantities per session
- **`farming_session_favors`** — NPC favor deltas per session
- **`survey_types`** — CDN reference table for survey costs, XP, and recipes

## Key Design Decisions

- **Universal vs per-server prices** — market values can be scoped to a specific server or marked universal (`server="*"`), controlled by a settings toggle.
- **Valuation modes** — six options for how items are valued (highest of market/vendor, market-only, vendor-only, etc.), affecting profit calculations across farming and surveying.
- **Session-based tracking** — both farming and surveying use explicit start/pause/end session lifecycles rather than always-on tracking, to keep data meaningful.
- **Market store as shared dependency** — survey profit calculations reactively update when market prices change.
