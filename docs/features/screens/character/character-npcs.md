# Character NPCs Screen

## Overview

The NPCs screen shows all NPCs a character has interacted with, their favor relationships, and detailed information about each NPC's services, gift preferences, and favor progression. It answers the core questions players ask: "What does this NPC like?", "What do they buy?", "How close am I to the next favor tier?", and "What do I unlock next?"

## Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  CharacterName · ServerName      Known NPCs: 87  Above Neutral: 34│
├──────────────────────────────────┬──────────────────────────────┤
│  Filter: [________]              │                              │
│  [By Favor ▾] [Favor ▾]         │  NPC DETAIL PANEL            │
│  ☐ Hide neutral                  │                              │
│                          42 NPCs │  Agrashab                    │
│                                  │  Sun Vale                    │
│  ▼ Close Friends (3)             │  "He looks at you..."        │
│    Agrashab    Sun Vale  CloseFr │                              │
│    Joeh        Serbule   CloseFr │  ── Favor ──                 │
│    Sie Antry   Eltibule  CloseFr │  [Close Friends]             │
│                                  │  ◆ Soul Mates    1000 favor  │
│  ▼ Friends (8)                   │  ◆ Like Family    800 favor  │
│    Mushroom Jack  Serbule Friend │  ◆ Best Friends   600 favor  │
│    ...                           │  ◇ Close Friends  ████ 45%   │
│                                  │  ✓ Friends                   │
│  ▼ Comfortable (12)              │  ✓ Comfortable               │
│    ...                           │  ✓ Neutral                   │
│                                  │                              │
│  ▼ Neutral (19)                  │  ── Services ──              │
│    ...                           │  $ Vendor (Despised+)        │
│                                  │    Despised   5,000  Armor.. │
│                                  │    Friends   10,000  Armor.. │
│                                  │    CloseFr   20,000  Armor.. │
│                                  │    BestFr    40,000  Armor.. │
│                                  │                              │
│                                  │  ◆ Training (Comfortable+)   │
│                                  │    Hammer                    │
│                                  │                              │
│                                  │  ── Gift Preferences ──      │
│                                  │  [Love] Fairy Wings      +3.5│
│                                  │  [Love] Magic Clubs      +3  │
│                                  │  [Love] Ancient Coins    +2  │
│                                  │  [Hate] Troll Flesh      -2  │
└──────────────────────────────────┴──────────────────────────────┘
```

## Data Sources

The screen merges three data sources into a unified NPC list:

| Source | When Available | What It Provides |
|--------|---------------|-------------------|
| `characterStore.npcFavor` (report) | After character report import | All known NPCs with favor tier |
| `gameStateStore.favorByNpc` (live) | After favor change events in session | Live favor deltas, updated tier |
| `gameDataStore.npcsByKey` (CDN) | Always (after CDN load) | Name, area, description, services, preferences |

**Merge logic:**
- Start with all NPCs from the report (these are NPCs the player has met)
- Add any game-state-only NPCs (discovered this session, not yet in a report)
- Enrich each with CDN data from `gameDataStore.npcsByKey`
- Effective favor tier: prefer game state tier over report tier

## Components

### NpcsScreen (`src/components/Character/NpcsScreen.vue`)

Top-level screen component. Manages selected NPC state and provides the summary bar + two-panel layout. Handles empty state when no character data is loaded.

### NpcListPanel (`src/components/Character/NpcListPanel.vue`)

Left sidebar (w-80) with the full NPC list.

**Grouping options:**
- **By Favor Tier** (default) — groups from Soul Mates down to Despised
- **By Area** — groups by NPC location (alphabetical)
- **None** — flat list

**Sorting options:**
- **Favor** (default, descending) — highest favor first
- **Name** (alphabetical)

**Filters:**
- Text filter on NPC name, area, or favor tier
- Hide neutral — hides NPCs at Neutral favor

**Each row shows:**
- NPC display name
- Area (small, truncated)
- Live data indicator dot (gold) if game state favor differs from the report
- Colored favor tier badge

Groups are collapsible with sticky headers showing group name and count.

### NpcDetailPanel (`src/components/Character/NpcDetailPanel.vue`)

Right panel composing the header and section sub-components. Shows header with NPC name, area (via `AreaInline`), and description. Falls back gracefully when CDN data is unavailable (shows only favor info).

Sections displayed in order:
1. Header (name, area, description)
2. NpcFavorSection (tier ladder + progress)
3. NpcVendorSection (gold status, timer, last sell) — only if NPC has a store service
4. NpcStorageSection (slot usage, stored items) — only if NPC has a storage service
5. NpcInventoryGiftsSection (inventory items matching NPC preferences)
6. NpcServicesSection (vendor caps, training, barter, etc.)
7. NpcPreferencesSection (gift preferences)
8. NpcQuestsSection (associated quests with favor rewards)
9. Training fallback (if not covered by services section)

### NpcFavorSection (`src/components/Character/NpcDetailSections/NpcFavorSection.vue`)

Visual favor tier ladder showing all tiers from Soul Mates to Despised:
- Current tier highlighted with gold accent
- Tiers at or below current shown as unlocked (check mark)
- Tiers above shown as locked (dimmed, with points required to reach)
- If cumulative favor delta is available, shows a progress bar within the current tier
- Report vs game state comparison when they differ

### NpcServicesSection (`src/components/Character/NpcDetailSections/NpcServicesSection.vue`)

Parses the raw CDN `services` JSON array into typed structures and renders a card per service type:

- **Vendor (Store)** — Table of favor tier → max gold cap → item types they buy. Current tier row is bright, locked tiers are dimmed. The `CapIncreases` string format `"FavorTier:MaxGold:ItemType1,ItemType2"` is parsed into structured data.
- **Training** — Skills listed via `SkillInline`, with favor tiers that unlock additional training
- **Barter** — Available at minimum favor tier, with additional unlock tiers
- **Consignment** — Item types accepted, with unlock tiers for expanded consignment
- **Storage** — Favor tiers that increase storage space
- **Other** (Stables, AnimalHusbandry, InstallAugments, GuildQuests) — Type name + required favor tier

Service types are defined in `src/types/npcServices.ts` as a discriminated union with a `parseServices()` function.

### NpcPreferencesSection (`src/components/Character/NpcDetailSections/NpcPreferencesSection.vue`)

Gift preferences sorted by preference value (highest first). Each row shows:
- Desire badge (Love = red, Like = green, Hate = red/dark)
- Item name or keyword description
- Numeric preference value (+/-)

Also shows which favor tiers unlock gifting (from `gift_favor_tiers` array).

### NpcVendorSection (`src/components/Character/NpcDetailSections/NpcVendorSection.vue`)

Shows vendor gold status for NPCs with a store service:
- Gold available vs max with color coding (green when high, yellow when low, red when empty)
- Estimated reset timer (168h from `vendor_gold_timer_start`, labeled as estimated)
- Last sell relative timestamp
- **Manual edit**: Edit button allows users to manually set gold available and gold max values — useful when vendors are stuck on "unknown" because no sell events were logged. Upserts into `game_state_npc_vendor` via `set_manual_vendor_gold` Tauri command.
- Falls back to "No vendor data yet" when no sell events have been tracked (with a prompt to use the Edit button)

### NpcStorageSection (`src/components/Character/NpcDetailSections/NpcStorageSection.vue`)

Shows storage usage for NPCs with a storage service:
- Slots used / unlocked at current favor tier with percentage
- Maximum possible slots at Soul Mates tier
- List of stored items with ItemInline components and stack counts
- Uses `storageVaultsByKey` and `storageByVault` from gameStateStore

### NpcInventoryGiftsSection (`src/components/Character/NpcDetailSections/NpcInventoryGiftsSection.vue`)

Cross-references player inventory with NPC gift preferences:
- Lists items in inventory matching Love/Like preferences
- Shows quantity owned, desire badge, and preference value per item
- Estimated total favor from gifting all matching items
- Falls back to "No matching gifts in inventory"

### NpcQuestsSection (`src/components/Character/NpcDetailSections/NpcQuestsSection.vue`)

Shows quests associated with the NPC via `gameDataStore.getQuestsForNpc()`:
- Quest names via QuestInline component
- Repeatable indicator for quests with `ReuseTime_Minutes` or `ReuseTime_Days`
- Favor reward amounts
- Sorted: non-repeatable first, then repeatable; within each group by favor reward descending

This component is shared between the Character NPC screen and the Data Browser NPC detail panel.

## Shared Utilities

### useFavorTiers (`src/composables/useFavorTiers.ts`)

Extracted favor tier logic shared across all NPC components:
- `FAVOR_TIERS` — ordered array (Soul Mates → Despised)
- `favorColor(tier)` — text color class
- `favorBadgeClasses(tier)` — bg + border + text classes for badges
- `isTierAtOrAbove(playerTier, requiredTier)` — comparison
- `pointsToNextTier(tier)` — favor points needed to advance (from wiki data)
- `tierDisplayName(tier)` — adds spaces (e.g., "CloseFriends" → "Close Friends")

### NPC Service Types (`src/types/npcServices.ts`)

TypeScript interfaces for parsed NPC services: `StoreService`, `TrainingService`, `BarterService`, `ConsignmentService`, `StorageService`, `GenericService`. Union type `NpcService` with `parseServices()` parser function.

## Favor Point Thresholds

From the Project Gorgon wiki, the favor points required to advance between tiers:

| From → To | Points |
|-----------|--------|
| Neutral → Comfortable | 100 |
| Comfortable → Friends | 200 |
| Friends → Close Friends | 300 |
| Close Friends → Best Friends | 600 |
| Best Friends → Like Family | 800 |
| Like Family → Soul Mates | 1000 |
| **Total (Neutral → Soul Mates)** | **3,000** |

These are hardcoded in `useFavorTiers.ts` and used for progress estimation in the favor section.

## Data We Cannot Show

Some player questions can't be answered with currently available CDN/log data:

- **What do NPCs sell?** — Available in the Data Browser via vendor_prices CDN table, but not yet shown in the Character NPC detail panel
- **Specific barter items** — Barter service exists but item specifics aren't in CDN
- **Hangout activities** — Not in current CDN NPC data
- **Exact favor values** — We track cumulative deltas and tier but not absolute numerical favor

Previously missing data that is now available:
- **Council pool / reset timing** — Vendor gold tracked via `game_state_vendor` table; estimated 168h reset timer shown in NpcVendorSection. Users can also manually set vendor gold via the Edit button.
- **Quest/task associations** — Precomputed NPC-quest index in gameDataStore; shown in NpcQuestsSection
- **Storage contents** — Tracked in `game_state_storage`; shown in NpcStorageSection

## NPC Card Features

The `NpcCard` component (`src/components/Shared/NPC/NpcCard.vue`) displays a compact summary of an NPC in the card grid:
- NPC name (via `NpcInline`) with pin-to-shelf button and favor badge
- **Area/zone** — displays the NPC's `area_friendly_name` via `AreaInline` (clickable)
- Gift tracking dots (when enabled)
- Vendor gold status with reset timer
- Storage usage
- Gift preferences (top N)
- Service summary (training skills, vendor caps, storage tiers)
