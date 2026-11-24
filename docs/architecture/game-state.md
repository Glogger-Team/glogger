# Game State System

Centralized, persisted representation of the player's current game state. Merges data from two sources — **character/inventory JSON exports** (complete reports, manually triggered) and **chat status events** (real-time but partial) — into a single source of truth that any feature can query.

## Design Principles

### Persist Derived State, Not Raw Events

We maintain a **live state model** — a set of `game_state_*` tables representing "last known value" per domain. Events update this model in place; we don't store raw events or replay logs.

### Freshness Tracking

Every value has a `last_confirmed_at` timestamp. Features can use this to display freshness indicators or prompt for a fresh export.

### Deconfliction: Most Recent Timestamp Wins

When a report import and a log event conflict, the **most recent timestamp wins**. Report imports use `ON CONFLICT DO UPDATE ... WHERE excluded.last_confirmed_at > existing.last_confirmed_at` to avoid overwriting fresher log data.

### Session Awareness

On `CharacterLogin`, the game dumps full state across many events (items, skills, attributes, equipment, recipes, abilities, active skills, effects). This naturally refreshes nearly all domains. The `GameStateManager` clears transient state (inventory, equipment, combat, mount, effects) on login and resets favor deltas.

## Architecture

### Data Flow

```text
Chat.log → ChatLogWatcher → chat_status_dispatch::dispatch_chat_message → writes game_state_* tables
                                                                            ↓
                                                                  emit("game-state-updated", [domain names])
                                                                            ↓
                                                                  gameStateStore
                                                               (refreshDomain per domain)

Report Import → seed_game_state_from_snapshot() → same DB tables
```

### Key Files

| File | Role |
|---|---|
| `src-tauri/src/game_state.rs` | `GameStateManager` — active character/server + session singleton; clears transient state on live login |
| `src-tauri/src/db/game_state_commands.rs` | Tauri query commands (one per domain) |
| `src-tauri/src/db/migrations.rs` | All `game_state_*` table definitions (V1 schema) |
| `src-tauri/src/db/character_commands.rs` | Report seeding for skills, attributes, recipes, equipment, favor, currencies, storage |
| `src-tauri/src/db/inventory_commands.rs` | Report seeding for inventory |
| `src-tauri/src/coordinator.rs` | Wires `GameStateManager` into the chat dispatch loop, emits `game-state-updated` |
| `src/stores/gameStateStore.ts` | Frontend Pinia store — single source of truth for all game state |
| `src/types/gameState.ts` | TypeScript interfaces matching Rust response structs |

## Database Schema

The game-state tables (the `game_state_*` and `character_*` families) live in the V1 unified migration and use `last_confirmed_at TEXT` for freshness. Ledger/event tables added later (`milking_timers`, `player_milking_log`, `player_pooping_log`, `item_transactions`) are their own migrations and carry their own timestamps instead.

| Table | PK | Key Columns |
|---|---|---|
| `game_state_session` | `id CHECK (id = 1)` | character_name, server_name, last_login_at |
| `game_state_skills` | (character_name, skill_name) | level, bonus_levels, xp, tnl, max_level, source |
| `game_state_active_skills` | character_name | skill1, skill2 |
| `game_state_attributes` | (character_name, attribute_name) | value |
| `game_state_weather` | `id CHECK (id = 1)` | weather_name, is_active |
| `game_state_combat` | character_name | in_combat |
| `game_state_mount` | character_name | is_mounted |
| `game_state_inventory` | (character_name, instance_id) | item_name, item_type_id, stack_size, slot_index, source |
| `game_state_recipes` | (character_name, recipe_id) | completion_count, source |
| `game_state_equipment` | (character_name, slot) | appearance_key |
| `game_state_favor` | (character_name, server_name, npc_key) | npc_key, npc_name, favor_tier, last_confirmed_at, source, cumulative_delta (legacy, V91-zeroed) — `session_delta` is NOT a column; computed at read from `favor_events` |
| `game_state_currencies` | (character_name, currency_name) | amount, source |
| `game_state_effects` | (character_name, effect_instance_id) | effect_name, source_entity_id |
| `game_state_storage` | (character_name, server_name, vault_key, instance_id) | item_name, item_type_id, stack_size, slot_index, source |
| `game_state_area` | (character_name, server_name) | area_name |
| `game_state_moon` | `id CHECK (id = 1)` | phase |
| `game_state_guild` | (character_name, server_name) | guild_id, guild_name, motd |
| `game_state_directed_goals` | (character_name, server_name, goal_id) | — |
| `game_state_strings` | (character_name, server_name, key) | value (NOTEPAD, FRIEND_STATUS, PUBLIC_STATUS, etc.) |
| `game_state_books` | (character_name, server_name, book_type, title) UNIQUE | content (raw HTML) |
| `character_report_stats` | (character_name, server_name, category, stat_name) | stat_value (parsed from PlayerAge/Behavior reports) |
| `milking_timers` | (character_name, server_name, cow_name, zone) | last_milked_at (+ `cooldown_unknown`: the stamp is a bound, not a milk time) |
| `player_pooping_log` | id (auto) | character/server/target + `pooped_at` (local wall-clock), UNIQUE per event — "Poop on X!" Combat lines (Dungcrafting count) |
| `player_milking_log` | id (auto) | character/server/other_player + `direction` (`milked`/`milked_by`/`self_milked`) + `milked_at` (RFC3339 UTC event time), UNIQUE per event (v72) |
| `item_transactions` | id (auto) | timestamp, item_name, internal_name, quantity, context, source, instance_id, vault_key |

### Item Transaction Ledger (`item_transactions`)

Added in V15 migration. Records every item gain/loss event from both Player.log and chat status for historical analysis. Unlike the `game_state_inventory` table (which tracks current state), this is an append-only audit trail.

- **Player.log sources:** `ItemAdded` (new items only, not login reloads), `ItemDeleted` (with context: vendor_sell, storage_deposit, consumed, unknown), `StorageDeposit`, `StorageWithdrawal`
- **Chat status sources:** `ItemGained`, `Summoned`
- **`source` column:** `'player_log'` or `'chat_status'`
- **`context` column:** `'loot'`, `'vendor_sell'`, `'storage_deposit'`, `'storage_withdraw'`, `'consumed'`, `'summoned'`, `'unknown'`
- **`quantity`:** positive for gains, negative for losses

### Chat-Based Stack Correction (retired)

Removed: Player.log `ProcessAddItem` always recorded `stack_size = 1` and chat status carried the real quantity, so the coordinator patched placeholders in place. The Player.log writer is gone and report imports delete-then-reinsert authoritative snapshot quantities, leaving no placeholders to patch — the correction path and its tests were removed.

## GameStateManager (Rust)

Lightweight struct with `active_character: Option<String>` and `active_server: Option<String>`. Receives `&DbPool` per call.

### Domain Write Sources

| Source | Table | Operation | Domain |
|---|---|---|---|
| `chat_status_dispatch` level-up XP | game_state_skills | UPSERT (xp/level) | `skills` |
| `chat_status_dispatch` favor/gift | game_state_favor, game_state_gift_log | UPSERT / INSERT | `favor` |
| `chat_status_dispatch` recipe discovery | game_state_recipes | UPSERT (completion_count) | `recipes` |
| `chat_status_dispatch` book content | game_state_books | UPSERT | `books` |
| `chat_status_dispatch` pigeon/entity report | player_messages | INSERT (NOT EXISTS guard) | `player_messages` |
| `chat_status_dispatch` player death | character_deaths (+ death_damage_sources) | INSERT | `deaths` |
| `chat_status_dispatch` item gain/loss | item_transactions | INSERT | `item_transactions` |
| `chat_status_dispatch` milking/poop | player_milking_log, player_pooping_log, milking_timers | INSERT/UPSERT | `milking`, `report_stats` |
| coordinator `AreaTransition` | game_state_area | UPSERT | `area` |
| coordinator `set_active_character` (live) | game_state_session | UPSERT; clears inventory/equipment/combat/mount | session |
| report import (`seed_game_state_from_snapshot`) | all game_state_* | bulk seed | all |

The deleted Player.log → `PlayerEventParser` → `PlayerEvent` pipeline no longer writes these tables.

### Timestamp Handling

Log events have `"HH:MM:SS"` timestamps. `GameStateManager` normalizes these to `"YYYY-MM-DD HH:MM:SS"` using `chrono::Local::now()`. Game-state timestamps are stored UTC; the one deliberate exception is `player_pooping_log.pooped_at`, which keeps the chat log's **local** wall clock so the live tail and the Chat.log backfill derive the identical dedup key (`MIN`/`MAX`/`substr(...,1,10)` in `get_pooping_stats` operate on that local value). The frontend's `useTimestamp` composable converts UTC to the user's preferred display timezone (local, server, or UTC) based on the `timestamp_display_mode` setting.

### Login Behavior

On `CharacterLogin`, `set_active_character()`:

- Updates `game_state_session`
- Clears transient state: inventory, equipment, combat, mount
- Resets favor session baseline: V91 zeroes the cumulative_delta integral; session_delta is now summed from favor_events via session_favor_sums (fresh session baseline)
- Does NOT clear storage (storage persists across sessions)

## Frontend Store (`gameStateStore`)

Pinia composition API store. All game state flows through here.

### Persisted State (from DB)

Loaded via parallel `invoke()` calls in `loadAll()`, refreshed per-domain via `refreshDomain(domain)`.

| Ref | Type | Tauri Command |
|---|---|---|
| `skills` | `GameStateSkill[]` | `get_game_state_skills` |
| `attributes` | `GameStateAttribute[]` | `get_game_state_attributes` |
| `activeSkills` | `GameStateActiveSkills \| null` | `get_game_state_active_skills` |
| `world` | `GameStateWorld` | `get_game_state_world` |
| `inventory` | `GameStateInventoryItem[]` | `get_game_state_inventory` |
| `recipes` | `GameStateRecipe[]` | `get_game_state_recipes` |
| `equipment` | `GameStateEquipmentSlot[]` | `get_game_state_equipment` |
| `favor` | `GameStateFavor[]` | `get_game_state_favor` |
| `currencies` | `GameStateCurrency[]` | `get_game_state_currencies` |
| `effects` | `GameStateEffect[]` | `get_game_state_effects` |
| `storage` | `GameStateStorageItem[]` | `get_game_state_storage` |
| `storageVaults` | `StorageVaultDetail[]` | `get_storage_vault_metadata` |

### Computed Lookups

| Computed | Type | Description |
|---|---|---|
| `skillsByName` | `Record<string, GameStateSkill>` | O(1) skill lookup |
| `attributesByName` | `Record<string, number>` | O(1) attribute lookup |
| `inventoryItemCounts` | `Record<string, number>` | Items in inventory (DB + live), excludes storage |
| `heldItemCounts` | `Record<string, number>` | Items on the player (inventory + saddlebag when tracked) |
| `ownedItemCounts` | `Record<string, number>` | Total owned across inventory + storage |
| `recipesById` | `Record<number, GameStateRecipe>` | O(1) recipe lookup |
| `recipeCompletions` | `Record<string, number>` | Keyed by `"Recipe_{id}"` for CDN compat |
| `knownRecipeKeys` | `Set<string>` | Recipe keys with count > 0 |
| `favorByNpc` | `Record<string, GameStateFavor>` | O(1) NPC favor lookup |
| `currenciesByName` | `Record<string, GameStateCurrency>` | O(1) currency lookup |
| `effectsById` | `Record<number, GameStateEffect>` | O(1) effect lookup |
| `namedEffects` | `GameStateEffect[]` | Effects with resolved display names |
| `storageByVault` | `Record<string, GameStateStorageItem[]>` | Items grouped by vault_key |
| `storageVaultsByKey` | `Record<string, StorageVaultDetail>` | O(1) vault metadata lookup |

### Session Tracking (In-Memory)

These are not persisted to DB — they reset on login/session restart.

| Feature | State | Description |
|---|---|---|
| Session skills | `sessionSkills` | XP deltas, levels gained, XP/hour per skill since session start |
| Live inventory | `liveItemMap`, `liveEventLog` | Real-time inventory tracking from chat status events |

### Event Listeners

- `game-state-updated` → `refreshDomain()` per domain in payload
- `character-login` → `resetSessionSkills()`, `clearLiveInventory()`, `loadAll()`

## Data Domains

### Implemented

| Domain | DB Table | Log Events | Report Seeding |
|---|---|---|---|
| Skills | `game_state_skills` | `SkillsLoaded` | Character report |
| Active Skills | `game_state_active_skills` | `ActiveSkillsChanged` | — |
| Attributes | `game_state_attributes` | `AttributesChanged` | — |
| Weather | `game_state_weather` | `WeatherChanged` | — |
| Combat | `game_state_combat` | `CombatStateChanged` | — |
| Mount | `game_state_mount` | `MountStateChanged` | — |
| Inventory | `game_state_inventory` | `ItemAdded`/`Changed`/`Deleted` | Storage report |
| Recipes | `game_state_recipes` | `RecipeUpdated` | Character report |
| Equipment | `game_state_equipment` | `EquipmentChanged` | — |
| NPC Favor | `game_state_favor` | `game-state-updated` (`favor` domain) | Character report (tier) |
| Currencies | `game_state_currencies` | *(none — report only)* | Character report |
| Effects | `game_state_effects` | `EffectsAdded`/`Removed`/`NameUpdated` | — |
| Storage | `game_state_storage` | `StorageDeposit`/`Withdrawal` | Storage report (vault items) |
| Area | `game_state_area` | `AreaTransition` (via coordinator) | — |

### Not Yet Implemented

| Domain | Status | Notes |
|---|---|---|
| Stats | Covered by attributes | `MAX_HEALTH` etc. are attributes — no separate table needed |
| Quests | Needs parser work | `ProcessLoadQuests`, `ProcessAddQuest`, `ProcessUpdateQuest`, `ProcessCompleteQuest` |
| Map Data | Needs parser work | `ProcessMapFog`, `ProcessMapFx` |
| Character Identity | Partial | Name/login tracked via `game_state_session`; race/guild not yet |

## Domain-Specific Notes

### NPC Favor

Log events only provide **deltas** (`ProcessDeltaFavor`), not absolute values. `session_delta` is computed at read: `SUM(delta)` over `favor_events` since `last_login_at` (`session_favor_sums` in `game_state_commands.rs`). `favor_tier` comes from reports (e.g., `"BestFriends"`). V91 zeroed the old stored `cumulative_delta` integral once at migration; nothing resets per login.

### Currencies

Attribute-based currencies (e.g., `CUR_COMBAT_WISDOM`) flow through `game_state_attributes`. Report-only currencies (Gold, Councils) live in `game_state_currencies`. `VendorGoldChanged` events track vendor gold, not player gold.

### Effects

`ProcessRemoveEffects` prints opaque `System.Int32[]` (C# ToString), so we can't determine which effects were removed. We emit a signal-only `EffectsRemoved` event. Stale entries are cleaned up on next login batch (which clears all effects and re-adds active ones).

### Storage

Storage persists across sessions — it is NOT cleared on `CharacterLogin` (unlike inventory). Items are only removed by explicit `StorageWithdrawal` events or full report replacement. Vault keys come from the parser's `current_interaction.npc_name` (e.g., `"NPC_Joe"`), not the game entity ID in the log line. Slot capacity is computed on the frontend by cross-referencing `storagevaults.json` metadata with favor tiers and attributes. See [Storage Tracker](../features/storage-tracker.md) for full details.

### Stats → Attributes Mapping

Report stat names map to attribute keys: `"MaxHealth"` → `"MAX_HEALTH"`, `"MaxPower"` → `"MAX_POWER"`, `"MaxArmor"` → `"MAX_ARMOR"`.

## Feature Integrations

### Crafting Material Availability

`check_material_availability()` in `crafting_commands.rs` queries two sources:

1. **`game_state_inventory`** — persisted log-driven inventory, aggregated by `item_type_id`
2. **`character_snapshot_items`** (storage vaults only) — from the latest Export Storage as JSON (Settings → VIP) export

The frontend's `craftingStore.checkMaterialAvailability()` calls this backend function and builds `MaterialNeed[]` with `inventory_have` + `storage_have` + `vault_breakdown`.

### Skill Synergy Bonuses

`game_state_skills.bonus_levels` is populated from `SkillsLoaded` login events (the `bonus` field in each full skill list entry, `SkillSnapshot`). `SkillCard.vue` displays the effective level as `level + bonus_levels` with a breakdown showing both values.

### Cook's Helper

Uses `gameStateStore.knownRecipeKeys` to filter recipes the player has learned, and `gameStateStore.ownedItemCounts` to show how many of each dish the player already has in inventory/storage.

## Decisions

- **Attribute storage:** Persist ALL attributes, not a curated subset. Storage is cheap, avoids revisiting schema.
- **Favor thresholds:** Build the favor-level-to-tier mapping from wiki data. `ProcessVendorScreen` provides tier enum for validation.
- **Opaque C# types:** `TransitionalQuestState`, `AbilityBarContents[]`, `StableSlot[]` are unparseable (Unity `.ToString()` outputs type name only). Extract what we can around them.
- **Effect ID mapping:** CDN effects JSON exists but mapping to in-game IDs is unclear. Rely on `ProcessUpdateEffectName` for display names.
- **Weather values:** No CDN/wiki reference for weather strings. Collect organically from `ProcessSetWeather` events.

## Open Questions

- **UI freshness patterns:** How to display data staleness varies per feature. Defer until specific features consume game state.
- **Weather bool flag:** The `ProcessSetWeather` bool parameter's meaning is unknown (indoor/outdoor? active/clearing?). Needs more samples.
