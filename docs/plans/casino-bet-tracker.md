# Casino Arena Bet Tracker

Parse arena fight announcements, bet confirmations, and outcomes. Track bet history with win/loss stats and P&L.

## Status

- **Phase 0 (log capture): DONE.** Real samples confirmed — see "Confirmed Log Events" below.
- **Fighter prediction tracker: SHIPPED** (the "who wins most often" subset the user asked for). Kuzavek's `[NPC Chatter]` intro/result narration is parsed into resolved matches, no manual tips:
  - `arena_parser.rs` — stateful `ArenaTracker` (intro pair → result line), 7-fighter seed roster + auto-discovery.
  - `db/arena_commands.rs` + migration v64 (`arena_matches`) — records + aggregates (per-fighter W/L, directed H2H matrix, recent), chat-log backfill.
  - `coordinator.rs` wiring (live) + startup backfill in `lib.rs`.
  - `ArenaWidget.vue` (`arena` dashboard widget) — rankings, H2H matrix, matchup predictor (H2H ≥3 samples, else overall win rate).
- **Betting/P&L half: SHIPPED.** `arena_parser.rs` — stateful `ArenaBetTracker` resolves the player's own wager from `Player.log` NPC 5712 talk screens (`Confirm Bet`) and correlates the payout with the fight outcome. `db/arena_commands.rs` + migration v65 (`arena_bets`, dedup via `idx_arena_bets_dedup`) persist them with `record_arena_bet` and aggregate them into `ArenaStats.betting` via `get_arena_stats`; startup backfill is `backfill_arena_bets_from_player_logs` (both registered in `lib.rs`). Rendered by `ArenaWidget.vue` — personal betting rate + P&L in the header next to the fighter tracker.

## Confirmed Log Events (captured from real logs)

- **Fighters + matchup** (`Chat.log`, `[NPC Chatter] Kuzavek:`): `Introducing our two competitors! On the far side is ... OTIS!` then `And on the side nearest me is Leo ...!`
- **Result** (`Chat.log`): `<X> wins!` / `<X> loses!` / `<X> has been defeated!` / `... has fallen` / `... is down`. Winner named OR loser named; wording is highly variable, so resolution is scoped to the already-known pending pair.
- **Bet placement + odds** (`Player.log`, NPC 5712 "Kuzavek"): `ProcessTalkScreen(5712, "Confirm Bet", "You are betting <em>7500</em> Councils that Otis defeats Leo... you will receive <em>14250</em> Councils...")` — wager, pick, opponent, payout.
- **Payout**: win → `ProcessScreenText(CombatInfo, "You received N Councils.")` (generic — must gate on expected amount + area + time); loss → no line.

## Other Log Events

Arena captures now exist (see "Confirmed Log Events" above), and the events below are handled by the shipped parsers (`ArenaTracker` / `ArenaBetTracker`); kept here as reference:
- `ProcessScreenText` — fight announcements, bet confirmations, outcomes
- `ProcessTalkScreen` — arena NPC bet placement dialogue
- `[Status]` chat — "You used N councils" (bet) / "You received N Councils" (payout) — already parsed as `CouncilsChanged`
- Area detection — `current_area == "Casino"` provides context guard

## State Machine Design

Follows [survey aggregator](../../src-tauri/src/survey/aggregator.rs) pattern:

```
Idle → FightAnnounced → BetPlaced → FightInProgress → FightResolved → PayoutSettled
```

Cross-source correlation via timestamp proximity (Player.log events + chat CouncilsChanged). Area guard limits processing to Casino zone. Pending state in-memory with timeouts.

## Data Model

- **casino_fights**: fight_id, fighters, odds, winner, timestamps
- **casino_bets**: fight_id FK, bet_on, wager, odds, outcome, payout, net_profit
- **casino_sessions**: grouping with denormalized totals (like survey_sessions)

## UI

New "Casino" tab under Economics view with sub-tabs:
- **Live**: active fight card, recent bets, running P&L
- **History**: bet history table with date/fighter/outcome filters
- **Analytics**: total P&L, win rate, P&L over time chart, streaks, per-fighter breakdown

## Phases

### Phase 0: Log Sample Collection (PREREQUISITE)
- Capture full betting cycle via debug capture in-game
- Document ProcessScreenText, ProcessTalkScreen, and chat messages
- Store in `docs/samples/player-log-samples/casino-arena/`
- **This gates everything else**

### Phase 1: Parser
- `src-tauri/src/casino/parser.rs` — parse arena-specific events
- Extend ChatStatusEvent if needed

### Phase 2: Aggregator
- `src-tauri/src/casino/aggregator.rs` — cross-source state machine
- Wire into coordinator

### Phase 3: Persistence
- DB migration, read/write functions, Tauri commands

### Phase 4: Frontend — basic UI
- CasinoView with History tab under Economics

### Phase 5: Analytics + Polish
- P&L charts, win rate, streaks, live session tracking

## Key Files

- [survey/aggregator.rs](../../src-tauri/src/survey/aggregator.rs) — primary pattern to follow
- [coordinator.rs](../../src-tauri/src/coordinator.rs) — event pipeline wiring
- [chat_status_parser.rs](../../src-tauri/src/chat_status_parser.rs) — CouncilsChanged already parsed
- [EconomicsView.vue](../../src/components/Economics/EconomicsView.vue) — where Casino tab goes
