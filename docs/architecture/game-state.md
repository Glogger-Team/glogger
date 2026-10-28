# Game State System

Centralized, persisted representation of the player's current game state. Merges data from two sources — **character/inventory JSON exports** (complete reports, manually triggered) and **Player.log events** (real-time but partial) — into a single source of truth that any feature can query.

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

```
Player.log → LogWatcher → PlayerEventParser → LogEvent::PlayerEventParsed
                                                        ↓
                                              DataIngestCoordinator
                                                   ↓              ↓
                                        GameStateManager     accumulate player events
                                         (writes to DB)       + domain names into batch
                                                ↓
                                    flush → emit("player-events-batch")
                                          + emit("game-state-updated", [domain names])
                                                ↓
                                          gameStateStore
                                       (refreshDomain per domain)

Report Import → seed_game_state_from_snapshot() → same DB tables
```

### Key Files

| File | Role |
|---|---|
| `src-tauri/src/game_state.rs` | `GameStateManager` — processes `PlayerEvent`s into DB writes |
| `src-tauri/src/db/game_state_commands.rs` | Tauri query commands (one per domain) |
| `src-tauri/src/db/migrations.rs` | All `game_state_*` table definitions (V1 schema) |
| `src-tauri/src/db/character_commands.rs` | Report seeding for skills, attributes, recipes, equipment, favor, currencies, storage |
| `src-tauri/src/db/inventory_commands.rs` | Report seeding for inventory |
| `src-tauri/src/coordinator.rs` | Wires `GameStateManager` into event loop, emits `game-state-updated` |
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
| `game_state_favor` | (character_name, npc_name) | npc_id, cumulative_delta, favor_tier, source |
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

### Chat-Based Stack Correction

`GameStateManager::correct_stack_from_chat()` fixes the `stack_size=1` problem in `game_state_inventory` and `game_state_storage`. Player.log `ProcessAddItem` always records `stack_size=1`; the real quantity comes from chat status `"Item x5 added to inventory."`. The correction:

1. Resolves the chat display name to a Player.log internal name via CDN `resolve_item()`
2. Finds the most recent row with `stack_size=1` matching that item name
3. Updates the stack size to the chat quantity
4. Returns which domains were corrected so the coordinator can emit `"game-state-updated"`

## GameStateManager (Rust)

Lightweight struct with `active_character: Option<String>` and `active_server: Option<String>`. Receives `&DbPool` per call.

### Event → Domain Mapping

| PlayerEvent | Table | Operation | Domain |
|---|---|---|---|
| SkillsLoaded | game_state_skills | DELETE all + bulk INSERT | `skills` |
| ActiveSkillsChanged | game_state_active_skills | UPSERT | `active_skills` |
| AttributesChanged | game_state_attributes | Batch UPSERT (transaction) | `attributes` |
| WeatherChanged | game_state_weather | UPSERT singleton | `weather` |
| CombatStateChanged | game_state_combat | UPSERT | `combat` |
| MountStateChanged | game_state_mount | UPSERT | `mount` |
| ItemAdded | game_state_inventory | INSERT | `inventory` |
| ItemStackChanged | game_state_inventory | UPDATE | `inventory` |
| ItemDeleted | game_state_inventory | DELETE | `inventory` |
| RecipeUpdated | game_state_recipes | UPSERT | `recipes` |
| EquipmentChanged | game_state_equipment | DELETE all + bulk INSERT | `equipment` |
| FavorChanged | game_state_favor | UPSERT (increments cumulative_delta) | `favor` |
| EffectsAdded | game_state_effects | Clear on login batch + bulk INSERT | `effects` |
| EffectsRemoved | *(signal only)* | Emits domain update | `effects` |
| EffectNameUpdated | game_state_effects | UPDATE effect_name | `effects` |
| StorageDeposit | game_state_storage | UPSERT | `storage` |
| StorageWithdrawal | game_state_storage | DELETE | `storage` |
| *(AreaTransition — handled in coordinator)* | game_state_area | UPSERT | `area` |
| MoonPhaseChanged | game_state_moon | UPSERT singleton | `moon` |
| GuildInfoLoaded | game_state_guild | UPSERT | `guild` |
| DirectedGoalsLoaded | game_state_directed_goals | DELETE all + bulk INSERT | `directed_goals` |
| PlayerStringUpdated | game_state_strings | UPSERT per key | `strings` |
| *(BookOpened — handled in coordinator)* | game_state_books | UPSERT | `books` |
| *(BookOpened + HelpScreen/PlayerAge — coordinator)* | character_report_stats | UPSERT | `report_stats` |
| *(Cow milk detection — coordinator)* | milking_timers | UPSERT | `milking` |
| *(Combat "Poop on X!" line — coordinator + Chat.log backfill)* | player_pooping_log | INSERT OR IGNORE | `report_stats` |

### Timestamp Handling

Log events have `"HH:MM:SS"` timestamps. `GameStateManager` normalizes these to `"YYYY-MM-DD HH:MM:SS"` using `chrono::Local::now()`. Game-state timestamps are stored UTC; the one deliberate exception is `player_pooping_log.pooped_at`, which keeps the chat log's **local** wall clock so the live tail and the Chat.log backfill derive the identical dedup key (`MIN`/`MAX`/`substr(...,1,10)` in `get_pooping_stats` operate on that local value). The frontend's `useTimestamp` composable converts UTC to the user's preferred display timezone (local, server, or UTC) based on the `timestamp_display_mode` setting.

### Login Behavior

On `CharacterLogin`, `set_active_character()`:
- Updates `game_state_session`
- Clears transient state: inventory, equipment, combat, mount
- Resets favor `cumulative_delta` to 0 (fresh session baseline)
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
| Live inventory | `liveItemMap`, `liveEventLog` | Real-time inventory tracking from `player-event` |

### Event Listeners

- `game-state-updated` → `refreshDomain()` per domain in payload
- `player-event` → `handleInventoryEvent()` for live inventory tracking
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
| NPC Favor | `game_state_favor` | `FavorChanged` | Character report (tier) |
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

Log events only provide **deltas** (`ProcessDeltaFavor`), not absolute values. `cumulative_delta` tracks the running sum since last report. `favor_tier` comes from reports (e.g., `"BestFriends"`). Delta resets to 0 on login.

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
