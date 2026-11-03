# NPC Tracking Improvements

## Overview

Goal is to make our NPC data much more useful and easier to interact with for players. There are three main avenues to attack this problem: dashboard widgets, tooltip improvements, and NPC screen improvements. All three rely on some better foundational game state tracking.

Some notes:
- NPCs reset their purchase gold every 168 hours. However, this counter only starts when a player sells them something and the counter is not yet running. So if a player sells something to VendorA on day0, then their available gold will decrease and start the timer to reset it in 168 hours. If they come back on day4 the timer isn't up yet, so any more sales to the vendor will continue to decrease their available gold and not impact when the cap resets. The vendor gold available will reset on day7. If they player doesn't sell items to the NPC again until day10, that's when the next 168 hour timer will begin.
- If a player increases their favor rank with an NPC during a reset timer period, the difference in NewGoldCap - OldGoldCap will be added to their purchase pool. This does not change the timer.

All names of fields/variables/etc in this doc are to be taken as placeholders and we should obviously use whatever ones make the most sense.

## GameState Updates

NPC data in the game state needs to track:
- NPC ID, obviously, so we know what NPC we're dealing with.
- Last interaction: when did we see the user last interact with this NPC? Can come from Player.log parsing.
- Last sell: when did we last see the user sell to this NPC? Also detectable in player.log.
- Favor level: what is the current level of favor with this npc? (Friends, Comfortable, Like Family, etc)
- Favor value: what is the numerical favor value with this npc? this lets us determine how close to the next rank they are.
- StorageSlotsUsed: how many slots of storage is the player using on this NPC, if applicable. this + know the total lets us calculate % full.
- GoldAvailable: how much gold does the vendor _currently_ have available? we should be able to get this from player.log parsing when they sell.

### Current State Assessment

**Already tracked in the database:**
- `game_state_favor` table: `npc_key`, `npc_name`, `cumulative_delta` (session favor changes), `favor_tier`, `last_confirmed_at`, `source` (log/snapshot/both)
- `game_state_gift_log` table: individual gift events with `gifted_at` timestamp and `favor_delta`
- `game_state_storage` table: per-vault item tracking with `vault_key`, `instance_id`, `item_name`, `stack_size`, `slot_index`
- `character_npc_favor` table: snapshot-derived favor levels from `/outputcharacter` imports

**Already parsed from Player.log but NOT persisted:**
- `InteractionStarted` / `InteractionEnded` — only used for internal parser context (vault key enrichment). These could feed "Last Interaction" tracking.
- `VendorSold` / `VendorStackUpdated` — parsed and available in activity feed, but not stored in a dedicated table. These could feed "Last Sell" tracking.
- `VendorGoldChanged` — parsed from `ProcessVendorUpdateAvailableGold(currentGold, serverId, maxGold)` but **completely unused on the frontend** (dead code after the type definition). This is exactly the event we need for GoldAvailable tracking.

**New tracking needed:**
- `last_interaction_at` — persist from `InteractionStarted` events (currently only held transiently in the parser)
- `last_sell_at` — persist from `VendorSold` events
- `vendor_gold_available` / `vendor_gold_max` — persist from `VendorGoldChanged` events (already parsed, just need storage)
- `vendor_gold_timer_start` — derived: set to current time when we first see a `VendorGoldChanged` where gold decreased, reset when gold returns to max. This is our best-guess 168h timer.
- `favor_value` (numerical) — we track `cumulative_delta` (session changes) but not absolute value. Snapshots give us the tier but not the exact number. We'd need either a log event for absolute favor or to calculate from snapshot tier + cumulative deltas.

### Implementation Approach

A new `game_state_npc_status` table (or extending `game_state_favor`) would consolidate per-NPC tracking:
```
character_name, server_name, npc_key,
last_interaction_at,
last_sell_at,
vendor_gold_available,
vendor_gold_max,
vendor_gold_timer_start,  -- our best-guess 168h timer anchor
```
This separates vendor/interaction state from favor tracking, which has different update patterns.

---

Data I think we can derive without needing to store, but should evaluate this based on performance impact of doing so:
- HasStorage: does the npc have storage? gamedata has this.
- StorageSlotsTotal: Should be able to look this up in gamedata based on favor level of the NPC.
- StorageSltosMaximum: what is the most storage slots this NPC could have at max favor? this lets us figure out how many not yet unlocked slots the player has.
- HasShop: does the npc buy/sell items? gamedata has this.
- PurchaseGoldCap: weekly purchase cap of the NPC, based on gamedata + favor level.
- PurchaseGoldMaximum: at max favor, what will their cap be? we can use this to figure out how much weekly gold the player is missing out on from the vendor
- PurchasePreferredItems: what items will they pay fullprice (up to their single item cap) for - this is in gamedata
- PurchaseSingleCap: what's the most they'll buy a single item for? this can be found in gamedata + favor level
- what items they want for favor and how much they give for them.
- what quests they have available - should be able to find their quests in game data, then look at the user's completed and active quests to figure out what open quests they have from this NPC, versus ones completed, versus ones they haven't picked up yet -> this might be slow to lookup real time!
- favor available from quests - should be able to look at quest rewards and figure this out. -> this might be slow to lookup real time!
- gifts available: some npcs give the player gifts upon reaching certain favor levels. this i think is in the game data? might be a slow lookup also.
- what training do they offer
- what barters do they offer
- what items do they sell

### CDN Data Availability Assessment

All of the above "derivable" data is confirmed available in CDN. Here's exactly where each comes from:

| Data Point | CDN Source | Notes |
|---|---|---|
| HasStorage | `StorageVaultInfo.has_associated_npc` from `storagevaults.json` | Already parsed via `game_data/storage_vaults.rs` |
| StorageSlotsTotal | `StorageVaultInfo.levels[favorTier]` | Maps favor tier → slot count. Already in the typed struct. |
| StorageSlotsMaximum | `StorageVaultInfo.levels[SoulMates]` (or highest key) | Max entry in levels map |
| HasShop | NPC services array contains `Type: "Store"` | Services are stored as raw JSON in `NpcInfo.services` |
| PurchaseGoldCap | `StoreService.CapIncreases` entries at player's favor tier | Format: `"FavorLevel:GoldCap:ItemType,ItemType,..."` — need to parse this string format |
| PurchaseGoldMaximum | Highest CapIncreases entry | Same parsing, just take the max tier |
| PurchasePreferredItems | `StoreService.CapIncreases` item type lists | The item types after the gold cap in each entry |
| PurchaseSingleCap | Not directly in CDN? | May need investigation — could be a game constant or derived from gold cap |
| Gift preferences | `NpcInfo.preferences[]` with `desire`, `keywords`, `pref` | Already parsed and displayed in NPC detail panel |
| Gift favor tiers | `NpcInfo.gift_favor_tiers` (ItemGifts in CDN) | e.g., `["CloseFriends", "BestFriends", "LikeFamily", "SoulMates"]` |
| Training | `TrainingService.skills[]` + `unlocks[]` at favor tiers | Already shown in NPC detail services section |
| Barter | `BarterService` with `AdditionalUnlocks[]` | Available in services, limited detail on actual barter items |
| Items they sell | `sources_items.json` Vendor/Barter entries (in-memory `vendor_items_by_npc` index, `value × 1.5`) | NPC sell prices shown in the Data Browser; the old `vendor_prices` SQL table was dead (no readers/writers) and dropped in migration v85 |
| NPC Quests | `QuestData.FavorNpc` field matches NPC key | Format: `"AreaName/NPC_Key"` — cross-reference available |
| Quest favor rewards | `QuestData.Reward_Favor` | Numeric favor reward per quest |
| Repeatable quests | `QuestData.ReuseTime_Minutes` / `ReuseTime_Days` | Fields exist — quests with these are repeatable |
| NPC gifts at favor levels | Likely in raw_json, needs investigation | Not in current typed fields |

**Performance note on quest lookups:** The concern about slow quest lookups is valid. There are potentially hundreds of quests to cross-reference per NPC. Options: (1) precompute NPC→quest mappings at CDN load time and cache them, (2) build an index table `npc_quests(npc_key, quest_key)` during CDN import, or (3) compute on-demand but with a LRU cache. Option 2 is probably best — it's a one-time cost at CDN load.

**Services parsing note:** NPC services are currently stored as `Option<Vec<Value>>` (raw JSON). The TypeScript side has typed interfaces (`StoreService`, `TrainingService`, etc. in `src/types/npcServices.ts`) but parsing happens ad-hoc in components. A shared service-parsing utility (or parsing at CDN load time into typed structs) would make all the downstream features cleaner.

## NPC Screen:
- Our statehelm vendor cards are awesome. Not perfect, but really quite good. They contain loads of info. Our NPC screen needs to use something like this. We should just make a shared, configurable vendor card we can use. Statehelm is currently the only place with limited numbers of gifts per week, so that kind of tracking is only relevant there, but still.
- Left panel: npc list/filter/search/etc - need more/better controls here, but this is a decent start.
- Main panel: show the NPC cards that match the filters.
- Right panel: click a card shows expanded details for that npc. We should improve the NPC view from the data browser to show all the relevant NPC information both contained in game data _and_ unique to the player to make one big unified detail view for NPCs. We can show this page in the databrowser and in this side panel here.
- our right panel will need to default to fairly wide for this to work and make sense. but that shouldn't be a problem. we can experiement with this until we get a good default.
- NPC Details:
  - all our relevant game state information
  - list of items in the inventory that match what the NPC likes as gifts.
  - estimated total favor obtained from donating all gifts, and what favor level that would put the NPC at.
  - gifting calculator for that npc: use where they are (current level and value) and what their desired target favor level is, then let them select an item to gift (not just owned items, could be anything) and we can calculate a _rough_ guess at how many of that item they'll need.
  - show what items we (think) that npc is storing
  - timer to gold reset if applicable, etc.
  - icon to pin the npc tooltip to the bottom (people love this feature)

### Current NPC Screen Architecture

The NPC screen (`NpcsScreen.vue`) currently uses a **two-panel** layout:
- **Left panel** (`NpcListPanel.vue`): NPC list with filter by name/area/tier, grouping (area/favor/none), sorting (favor/name), toggle to hide neutral NPCs. Shows gold dot indicator for live gamestate data vs snapshot-only.
- **Right panel** (`NpcDetailPanel.vue`): Selected NPC detail with sections for favor (`NpcFavorSection.vue`), services (`NpcServicesSection.vue`), and preferences (`NpcPreferencesSection.vue`).

**To get to the proposed 3-panel layout**, we'd shift to: left list panel | center card grid | right detail panel. This is a natural evolution of the existing PaneLayout usage. The center panel replaces the current right panel as the "browsing" view, and the right panel becomes the deep-dive.

### Shared NPC Card Component

The Statehelm cards (`StatehelmView.vue`) currently show per-NPC:
- Name + favor tier badge
- Gift progress: X/5 with +/- buttons and dot indicators
- Top preferences (sorted by pref value)
- Service summary (training skills, storage tiers, vendor caps + item types)

A shared `NpcCard.vue` component could accept config props to toggle sections:
- `showGiftTracking` (Statehelm weekly limits — only relevant there)
- `showVendorGold` (current gold / max gold + timer)
- `showStorageUsage` (slots used / total)
- `showFavorProgress` (tier + progress bar to next)
- `showPreferences` (top N gift preferences)
- `showServices` (training/barter/storage/vendor summary)

### NPC Detail Panel Expansion

The existing `NpcDetailPanel.vue` already has favor, services, and preferences sections. New sections needed:
- **Inventory Gifts section**: cross-reference player inventory with NPC preferences to show giftable items. This requires the inventory tracking system (already exists in `game_state_inventory`).
- **Gifting Calculator section**: target favor tier selector + item selector → estimated quantity. Uses `POINTS_TO_NEXT` from `useFavorTiers.ts` and preference `pref` values.
- **Storage Contents section**: pull from `game_state_storage` for the NPC's vault key. Already have this data.
- **Vendor Status section**: gold available/max, estimated timer, last sell time. Needs the new `game_state_npc_status` persistence.
- **Quest section**: NPC's quests cross-referenced with player completion state.

The Data Browser's `NpcBrowser.vue` has its own detail panel — these should share components or use the same detail component to avoid divergence.

## Dashboard widgets

- Zone NPCs widget improvements:
  - Show favor level, purchasing gold available, storage full %, and time until purchase gold reset
- Gift watcher:
  - can select an npc (or multiple?) you want to gift items to, and the gift watcher looks for desired gifts to enter the inventory and tracks them when they do. option for toasts as well?

### Implementation Notes

**Zone NPCs widget**: Would need to know which zone the player is currently in (we have this from area change events), then show NPC cards for that zone filtered to ones with interesting data (vendor gold, storage, etc.). The CDN `NpcInfo.area_name` / `area_friendly_name` gives us zone membership.

**Gift watcher widget**: This is essentially a filter on inventory change events crossed with NPC preferences. When an `ItemAdded` event fires, check if the item matches any preference keywords for watched NPCs. The `NpcPreference.keywords` field contains item keywords, item names, skill prereqs, equipment slots, etc. — matching logic needs to handle all these keyword types. Could store watched NPCs in a simple user preferences table.

## Tooltips

- NPC tooltips should probably be pretty similar to our npc cards - need to incorprate a lot of data from the player.
- Item tooltips should include what vendors will buy that item for full price, and what their remaining purchase gold is. (we'll have to check performance on this - might not be feasible)
- Item tooltips should show what NPCs want that item as gifts

### Current Tooltip State

**NPC Tooltip** (`NpcTooltip.vue`) currently shows: name, area, description, top 5 gift preferences, and trained skills. No player-specific data.

**Item Tooltip**: Would need reverse lookups — "given this item, which NPCs want it?" This requires matching item properties against all NPC preference keywords. Two approaches:
1. **Precomputed index**: At CDN load, build an inverted index of `item_keyword → [npc_keys]`. Fast lookups but requires maintaining the index.
2. **On-demand scan**: For each item, scan all NPC preferences. With ~200 NPCs and ~5-10 preferences each, this is ~1000-2000 comparisons — probably fine for a tooltip that appears on hover with a small delay.

The vendor purchase gold display on item tooltips requires the new vendor gold tracking (from `VendorGoldChanged` persistence).

## Open Questions

- NPC hangouts: what data do we have on these? can we tell what hangouts an npc has at different favor levels? what their requirements are? what ones a player has completed? i have no idea.
- We can make a best-guess timer based on looking for sales to an npc, but does the log anywhere give us the _real_ time remaining until their gold resets for selling to them? This would be handy if it does.
- Can we see what price they sell items for?
- Can we see their barter option details?  like how many X you have to trade for a Y?
- can we see what requirements they have for training: favor, costs, skill, etc
- what can we find out in regards to repeatable quests? does the data have when they reset? does our player data contain anything about when they were last completed or timers? what about the json?

### Answers & Research Results

**Hangouts:** No hangout data exists anywhere in the CDN data we load, the parser, or the game state. The CDN NPC entries don't include hangout information. This would need to come from a different CDN file (if one exists) or from player log parsing (if hangout events appear in logs). **Status: Unknown, needs game log investigation.**

**Vendor gold reset timer:** The `VendorGoldChanged` event gives us `current_gold` and `max_gold` but **no timer information**. The log does not appear to expose the actual reset timestamp. Our best-guess approach (recording when gold first drops below max, then adding 168 hours) is likely the only option. **Caveat:** If the player wasn't running glogger when they first sold to the vendor, our timer will be wrong. We should show this as "estimated" with appropriate caveats in the UI.

**Item sell prices:** Yes. NPC sell prices come from `sources_items.json` Vendor/Barter entries (the in-memory `vendor_items_by_npc` index, `value × 1.5`). The Data Browser's `NpcBrowser.vue` already displays a vendor inventory table with item names and sell prices. (The old `vendor_prices` SQL table was dead — no readers or writers — and was dropped in migration v85.)

**Barter option details:** Partially. The `BarterService` type has a `Favor` requirement and `AdditionalUnlocks` (favor tiers where more barter options open up), but **not the actual barter recipes** (what items you trade for what). The specific "trade 5 of X for 1 of Y" details don't appear in the NPC CDN data we currently parse. There may be a separate CDN file for barter recipes, or this data may be embedded in the raw JSON (`NpcInfo.raw_json`) under a field we're not extracting. **Status: Needs CDN raw JSON investigation.**

**Training requirements:** The `TrainingService` has `Favor` (minimum favor tier) and `skills[]` (what skills), plus `unlocks[]` (favor tiers where more skills become available). However, **specific training costs** (gold per level, skill level caps, prerequisites) are not in the NPC CDN data. Training cost data may be in a skills CDN file or a separate training data file. **Status: Needs CDN investigation for training cost tables.**

**Repeatable quests:** Yes, partially. The quest CDN data has `ReuseTime_Minutes` and `ReuseTime_Days` fields — quests with these values are repeatable. We can identify which NPC quests are repeatable and their cooldown period. **However**, we don't have player-side data for when repeatable quests were last completed or when they become available again. The player log may emit quest completion events we could use to track this, but it's not currently implemented. **Status: Quest reset data identifiable from CDN; player completion timing would need new parser events.**

---

## Implementation Plan

**Status: ALL PHASES COMPLETE**

This plan is organized into phases with clear dependencies. Each phase builds on the prior one. Phases can contain multiple independent tasks that could be worked in parallel.

### Phase 1: Foundation — NPC Service Parsing & Data Infrastructure

Everything downstream depends on clean, typed access to NPC service data and persistent vendor/interaction state. This phase has no UI changes — it's all plumbing.

#### 1A: NPC Service Parsing Composable

**Goal:** Extract ad-hoc service parsing from components into a shared composable so every downstream feature has typed access to NPC services.

**What exists:** `parseServices()` in `src/types/npcServices.ts` converts raw JSON to typed `NpcService` union. It's called inline in `StatehelmView.vue`, `NpcServicesSection.vue`, and `NpcDetailPanel.vue` — each doing their own filtering.

**Work:**
- Create `src/composables/useNpcServices.ts` composable that wraps `parseServices()` and provides computed helpers:
  - `storeService(npc)` → `StoreService | null`
  - `storageService(npc)` → `StorageService | null`
  - `trainingService(npc)` → `TrainingService | null`
  - `barterService(npc)` → `BarterService | null`
  - `goldCapAtTier(npc, tier)` → `{ maxGold, itemTypes }` (parses the `"Tier:Gold:Types"` string format)
  - `maxGoldCap(npc)` → highest tier gold cap
  - `storageSlots(npc, tier)` → slot count at tier (cross-ref with `StorageVaultInfo.levels`)
  - `maxStorageSlots(npc)` → slots at highest tier
- Refactor `StatehelmView.vue`, `NpcServicesSection.vue`, `NpcDetailPanel.vue` to use the composable instead of inline parsing.

**Files touched:** New `useNpcServices.ts`; modify `StatehelmView.vue`, `NpcServicesSection.vue`, `NpcDetailPanel.vue`.

#### 1B: NPC-Quest Index

**Goal:** Precompute NPC→quest mappings at CDN load time so quest lookups are instant.

**What exists:** `NpcBrowser.vue` does on-demand async calls to find quests for an NPC. `QuestData.FavorNpc` (format `"AreaName/NPC_Key"`) links quests to NPCs.

**Work:**
- In `gameDataStore`, build a computed `questsByNpc: Record<string, QuestData[]>` index during CDN data load by scanning all quests and grouping by their `FavorNpc` NPC key.
- Expose a `getQuestsForNpc(npcKey)` method that returns from this index.
- Include derived fields: `isRepeatable` (has `ReuseTime_Minutes` or `ReuseTime_Days`), `favorReward` (from `Reward_Favor`).
- Refactor `NpcBrowser.vue` to use this index instead of its async query.

**Files touched:** Modify `gameDataStore.ts`, `NpcBrowser.vue`.

#### 1C: Item-to-NPC Preference Matching Utility

**Goal:** Build the matching logic that answers "which NPCs want this item?" and "which items in inventory match this NPC's preferences?"

**What exists:** Items have `keywords: Vec<String>`. NPC preferences have `keywords: Vec<String>`. No matching logic exists anywhere.

**Work:**
- Create `src/composables/useNpcGiftMatching.ts`:
  - `matchesPreference(item: ItemInfo, pref: NpcPreference)` → boolean. Matches if any item keyword is in the preference's keyword list. Handle special keyword formats: `SkillPrereq:X`, `EquipmentSlot:X`, `MinRarity:X`, `Maximized`, and plain item type/name keywords.
  - `findGiftableItems(npc: NpcInfo, inventory: InventoryItem[])` → items from inventory that match any Love/Like preference, sorted by preference value.
  - `findInterestedNpcs(item: ItemInfo, allNpcs: NpcInfo[])` → NPCs that want this item, with the matched preference info.
- This composable is consumed by: NPC detail panel (gift items section), item tooltips (NPC interest section), and the gift watcher widget.

**Files touched:** New `useNpcGiftMatching.ts`.

#### 1D: Persist Vendor Gold & Interaction State (Backend)

**Goal:** Store the already-parsed `VendorGoldChanged`, `InteractionStarted`, and `VendorSold` events so the frontend can display vendor gold, timers, and last-interaction data.

**What exists:**
- `VendorGoldChanged` is parsed (fields: `current_gold`, `server_id`, `max_gold`) but has no handler in `game_state.rs` — falls through to `_ => {}`.
- `InteractionStarted` is parsed (fields: `entity_id`, `interaction_type`, `npc_name`) but only used for parser-internal context.
- `VendorSold` is parsed but not persisted.
- Latest migration is v23.

**Work:**
- Add migration v24: create `game_state_npc_vendor` table:
  ```sql
  character_name TEXT NOT NULL,
  server_name TEXT NOT NULL,
  npc_key TEXT NOT NULL,
  vendor_gold_available INTEGER,
  vendor_gold_max INTEGER,
  vendor_gold_timer_start TEXT,  -- ISO timestamp, best-guess 168h anchor
  last_interaction_at TEXT,
  last_sell_at TEXT,
  last_confirmed_at TEXT NOT NULL,
  PRIMARY KEY (character_name, server_name, npc_key)
  ```
- Add handlers in `game_state.rs` `process_event_inner()`:
  - `VendorGoldChanged` → resolve NPC from `current_interaction` context → upsert `vendor_gold_available` and `vendor_gold_max`. Timer logic: if gold decreased and no existing timer, set `vendor_gold_timer_start` to now. If gold returned to max, clear timer. Push domain `"vendor"`.
  - `InteractionStarted` → resolve NPC name to key → upsert `last_interaction_at`. Push domain `"vendor"`.
  - `VendorSold` → resolve NPC from `current_interaction` → upsert `last_sell_at`. Push domain `"vendor"`.
- Add Tauri command `get_game_state_vendor(characterName, serverName)` → returns all vendor status rows.
- Add TypeScript type `GameStateVendor` to `src/types/gameState.ts`.
- Wire `"vendor"` domain into `gameStateStore.ts` `refreshDomain()` switch.

**Challenge:** `VendorGoldChanged` doesn't include the NPC name — it has a `server_id`. We need to use `current_interaction` (set by the preceding `InteractionStarted` event) to know which NPC the gold update belongs to. This is the same pattern storage events use for vault key enrichment. If `current_interaction` is None, we can't attribute the gold change — log a warning and skip.

**Files touched:** `migrations.rs`, `game_state.rs`, `game_state_commands.rs`, `gameState.ts` (types), `gameStateStore.ts`.

### Phase 2: Shared NPC Card Component & NPC Screen Overhaul

This phase creates the shared visual components and restructures the NPC screen. Depends on Phase 1A (service composable) and 1D (vendor state).

#### 2A: Shared NPC Card Component

**Goal:** A single `NpcCard.vue` component that replaces the inline card layout in `StatehelmView.vue` and serves as the card view for the NPC screen center panel.

**What exists:** `StatehelmView.vue` has a 311-line implementation with inline card rendering. The NPC list panel shows minimal row data.

**Work:**
- Create `src/components/Shared/NPC/NpcCard.vue` with props:
  - `npc: NpcInfo` (required)
  - `favorTier: string | null`
  - `vendorStatus: GameStateVendor | null` (from Phase 1D)
  - `storageSlotsUsed: number | null`
  - Config object or slots for optional sections:
    - Favor badge + progress bar
    - Vendor gold: current/max + estimated timer countdown
    - Storage: used/total slots with percentage
    - Gift tracking: X/N with dot indicators (Statehelm mode)
    - Top N preferences
    - Service summary chips (Training, Storage, Vendor, Barter icons)
  - `compact: boolean` for dashboard widget use vs full card
- Card click emits `select` event for detail panel navigation.
- Refactor `StatehelmView.vue` to use `NpcCard` with gift tracking config enabled. This should significantly reduce its line count.

**Files touched:** New `NpcCard.vue`; modify `StatehelmView.vue`.

#### 2B: NPC Screen — Three-Panel Layout

**Goal:** Restructure the NPC screen from 2-panel (list | detail) to 3-panel (list | card grid | detail).

**What exists:** `NpcsScreen.vue` (113 lines) with `PaneLayout` holding `NpcListPanel` (left) and `NpcDetailPanel` (right).

**Work:**
- Update `NpcsScreen.vue` to use a 3-pane layout:
  - Left: `NpcListPanel` (filters + compact list, narrower than current)
  - Center: Card grid using `NpcCard` components, filtered by left panel selections
  - Right: `NpcDetailPanel` (expanded, wider default — the deep-dive view), shown when a card is clicked
- `NpcListPanel` becomes primarily a filter/search panel. Its output drives which NPC cards appear in the center grid.
- Center panel: responsive grid (2-4 columns based on available width), scrollable, showing `NpcCard` for each matching NPC.
- Right panel: collapsible or hidden when no NPC selected, opens/slides in on card click.

**Files touched:** `NpcsScreen.vue` (3-pane PaneLayout with card grid center), new `NpcFilterPanel.vue` (filter-only panel extracted from `NpcListPanel.vue`). `NpcListPanel.vue` is preserved for backward compatibility but no longer used by the NPC screen.

#### 2C: Unified NPC Detail Panel

**Goal:** A comprehensive NPC detail view usable in both the NPC screen right panel and the Data Browser, replacing two divergent implementations.

**What exists:**
- `NpcDetailPanel.vue` (89 lines): Favor, Services, Preferences sections via sub-components. Character-specific, no quests or vendor inventory.
- `NpcBrowser.vue` (330 lines): Inline detail with trained skills, preferences table, gift tiers, associated quests, vendor inventory, raw JSON. CDN-only, no player state.

**Work:**
- Expand `NpcDetailPanel.vue` (or create a new `NpcDetailView.vue` shared component) with all sections:
  - **Header:** NPC name, area (AreaInline), description, favorite toggle
  - **Favor section** (existing `NpcFavorSection.vue`): tier ladder, progress, session delta
  - **Vendor status section** (new): gold available/max, estimated timer with "estimated" label, last sell timestamp. Uses Phase 1D data.
  - **Storage section** (new): slots used / total at current favor / max at SoulMates. Item list from `game_state_storage`. Uses Phase 1A `storageSlots()`.
  - **Inventory gifts section** (new): Items in player inventory matching this NPC's preferences. Uses Phase 1C `findGiftableItems()`. Show estimated total favor from gifting all. Show projected favor tier after gifting.
  - **Gifting calculator section** (new): Target tier selector, item selector (any item, not just owned), estimated quantity needed. Uses `POINTS_TO_NEXT` from `useFavorTiers.ts` + preference `pref` values.
  - **Services section** (existing `NpcServicesSection.vue`): enhanced with gold caps at current tier, training skills, barter, consignment, storage tier unlocks.
  - **Preferences section** (existing `NpcPreferencesSection.vue`)
  - **Quests section** (new): Uses Phase 1B `getQuestsForNpc()`. Show quest name (QuestInline), completion state, favor reward, repeatable indicator.
  - **Vendor inventory section**: Items they sell with prices (already in NpcBrowser, move to shared).
  - **Pin to bottom** button (for tooltip pinning behavior)
- `NpcBrowser.vue` switches its right panel to use this shared component (passing CDN data only, player state sections hide gracefully when data is null).

**Files touched:** `NpcDetailPanel.vue` or new `NpcDetailView.vue`, new sub-component files for each section, modify `NpcBrowser.vue`.

### Phase 3: Tooltip Enhancements

Depends on Phase 1C (gift matching) and Phase 1D (vendor state).

#### 3A: NPC Tooltip Overhaul

**Goal:** NPC tooltips show player-relevant data, not just CDN info.

**What exists:** `NpcTooltip.vue` (54 lines): name, area, description, top 5 preferences, trained skills. No player data.

**Work:**
- Add to `NpcTooltip.vue`:
  - Favor tier badge + progress to next tier
  - Vendor gold: current/max (if vendor)
  - Storage: used/total (if storage NPC)
  - Estimated timer to gold reset (if applicable)
- Keep it compact — this is a tooltip, not the detail panel. Show data as single-line summaries.
- Data comes from `gameStateStore` (favor, vendor status, storage counts).

**Files touched:** `NpcTooltip.vue`.

#### 3B: Item Tooltip — NPC Buyer & Gift Info

**Goal:** Item tooltips show which vendors buy this item at full price (and their remaining gold), and which NPCs want it as a gift.

**What exists:** Item tooltip system exists but has no NPC cross-reference.

**Work:**
- Add sections to the item tooltip component:
  - **"Vendors"**: List NPCs whose `StoreService.CapIncreases` item types match this item's type. Show NPC name + remaining gold. Uses Phase 1A `goldCapAtTier()` and Phase 1D vendor gold state. Limit to 3-5 entries.
  - **"Wanted as gift by"**: List NPCs with matching preferences. Uses Phase 1C `findInterestedNpcs()`. Show NPC name + desire (Love/Like) + preference value. Limit to 3-5 entries.
- Performance: Both lookups scan all NPCs (~200). The preference scan is ~2000 keyword comparisons — fine for a debounced tooltip. The vendor type match needs item→type resolution which we have from CDN. Cache results per item if needed.

**Files touched:** Item tooltip component (likely in `src/components/Shared/Item/`).

### Phase 4: Dashboard Widgets

Depends on Phase 1C (gift matching), 1D (vendor state), and 2A (NpcCard).

#### 4A: Zone NPCs Widget Enhancement

**Goal:** The existing zone-based dashboard shows NPCs in the current area with favor, vendor gold, storage %, and gold reset timer.

**What exists:** `CurrentZone` widget exists. Area tracking is in `game_state_area` with live updates. `statehelm-summary` widget exists as a pattern.

**Work:**
- Create or enhance a zone NPC widget that:
  - Reads current area from `gameStateStore` world/area state
  - Filters NPCs by `area_name` matching current zone
  - Renders compact `NpcCard` components (Phase 2A) with `compact: true`
  - Shows: favor badge, vendor gold current/max, storage used/total %, timer countdown
- Register in `dashboardWidgets.ts` widget registry. Size: medium or large depending on NPC count.

**Files touched:** New widget component in `src/components/Dashboard/widgets/`, modify `dashboardWidgets.ts`.

#### 4B: Gift Watcher Widget

**Goal:** Player selects NPCs to watch; widget monitors inventory for items those NPCs want and alerts when matches arrive.

**What exists:** Inventory live tracking (`liveItemMap` in gameStateStore) with real-time item add/remove events. No gift watching logic.

**Work:**
- Create `src/components/Dashboard/widgets/GiftWatcherWidget.vue`:
  - Config panel: multi-select NPC picker for watched NPCs. Store selections in user preferences (settings store or a simple DB table).
  - Main view: list of watched NPCs with matched items from current inventory.
  - Watch `liveItemMap` changes — when a new item appears, run `matchesPreference()` (Phase 1C) against watched NPC preferences.
  - Toast notifications (optional, togglable): fire a toast when a gift-worthy item enters inventory. Use existing toast/notification system if one exists, or the Tauri notification API.
- Register in `dashboardWidgets.ts`. Size: medium.

**Files touched:** New widget component, modify `dashboardWidgets.ts`, possibly new config component.

### Phase 5: Statehelm Integration & Polish

This phase circles back to improve Statehelm with the shared components.

#### 5A: Statehelm Refactor to Shared Components — DONE

**Goal:** `StatehelmView.vue` uses `NpcCard` (Phase 2A) and the shared detail components, reducing its 311 lines significantly.

**Work:**
- ~~Replace inline card rendering with `NpcCard` configured for gift tracking mode.~~ Done in Phase 2A.
- ~~Wire the NPC card click to open the unified detail panel (Phase 2C) in a right panel or modal.~~ Done — PaneLayout with collapsible right pane showing `NpcDetailPanel`. Clicking a card selects it and opens the detail panel; clicking again deselects and hides the panel.
- ~~Preserve Statehelm-specific features: weekly gift counter, reset timer, hide-maxed-gifts filter.~~ All preserved.
- Also added: vendor status data (`vendorByNpc`) now passed to each NpcCard so vendor gold info renders on the cards.
- Root layout migrated from ad-hoc `div` to `PaneLayout` per project conventions.

**Files touched:** `StatehelmView.vue`.

#### 5B: NPC Detail — Pin-to-Bottom

**Goal:** Allow pinning an NPC tooltip/summary to the bottom of the screen, similar to existing pinning behavior users love.

**Work:**
- Add a pin icon to the NPC detail panel header and NPC cards.
- When pinned, render a compact NPC summary bar at the bottom of the screen (similar to how existing pinned content works).
- Show: NPC name, favor tier, vendor gold, storage %, timer.
- Allow multiple pins or single-pin-replaces.
- Check if there's an existing pinning system to hook into.

**Files touched:** NpcCard, NpcDetailView, possibly a new `PinnedNpcBar.vue`, layout integration.

### Dependency Graph

```
Phase 1A (Service Composable) ──────┐
Phase 1B (Quest Index) ─────────────┤
Phase 1C (Gift Matching) ───────────┼──→ Phase 2A (NpcCard) ──────→ Phase 4A (Zone Widget)
Phase 1D (Vendor State Backend) ────┘        │                       Phase 4B (Gift Watcher)
                                             ├──→ Phase 2B (3-Panel Layout)
                                             ├──→ Phase 2C (Unified Detail)
                                             │        │
                                             │        ├──→ Phase 5A (Statehelm Refactor)
                                             │        └──→ Phase 5B (Pin-to-Bottom)
                                             │
                                 Phase 1C ───┼──→ Phase 3B (Item Tooltip NPC Info)
                                 Phase 1D ───┤
                                             └──→ Phase 3A (NPC Tooltip Overhaul)
```

**Parallelism within phases:**
- Phase 1: A, B, C, and D are all independent — can be worked simultaneously.
- Phase 2: A must come before B and C (they use the card). B and C are independent of each other.
- Phase 3: A and B are independent of each other (both depend on Phase 1 only).
- Phase 4: A and B are independent of each other.
- Phase 5: A depends on 2A+2C. B depends on 2C.

### Summary of New Files

| File | Phase | Purpose |
|---|---|---|
| `src/composables/useNpcServices.ts` | 1A | Typed NPC service access + computed helpers |
| `src/composables/useNpcGiftMatching.ts` | 1C | Item↔NPC preference matching logic |
| `src/components/Shared/NPC/NpcCard.vue` | 2A | Shared configurable NPC card |
| `src/components/Character/NpcFilterPanel.vue` | 2B | Filter/search panel for NPC card grid |
| `src/components/Character/NpcDetailSections/NpcVendorSection.vue` | 2C | Vendor gold, timer, last sell |
| `src/components/Character/NpcDetailSections/NpcStorageSection.vue` | 2C | Storage contents + slot usage |
| `src/components/Character/NpcDetailSections/NpcInventoryGiftsSection.vue` | 2C | Giftable items from inventory |
| `src/components/Character/NpcDetailSections/NpcGiftCalculatorSection.vue` | 2C | Gifting calculator |
| `src/components/Character/NpcDetailSections/NpcQuestsSection.vue` | 2C | NPC quests with completion state |
| `src/components/Dashboard/widgets/ZoneNpcsWidget.vue` | 4A | Zone NPC dashboard widget |
| `src/components/Dashboard/widgets/GiftWatcherWidget.vue` | 4B | Gift watching + notifications |

### Migration Summary

| Version | Phase | What |
|---|---|---|
| v24 | 1D | `game_state_npc_vendor` table for vendor gold, timers, interaction/sell timestamps |
