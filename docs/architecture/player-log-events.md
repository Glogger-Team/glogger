# Player.log Event Reference

How the game client communicates game state through Player.log, and how to decode the events.

## Event Types

### ProcessAddItem — New item enters inventory

```
[HH:MM:SS] LocalPlayer: ProcessAddItem(InternalName(instanceId), slotIndex, isNew)
```

| Field | Type | Meaning |
|---|---|---|
| `InternalName` | string | CDN internal name (e.g., `MetalSlab2`, `UnrefinedSilverOre`) |
| `instanceId` | u64 | Unique instance identifier for this specific stack/item |
| `slotIndex` | i32 | Inventory slot; see interpretation below |
| `isNew` | bool | True if newly acquired (loot, craft, storage withdrawal), False if loading inventory |

**When it fires:**
- Login (all inventory items, `isNew=False`, `slotIndex=-1`)
- Looting items from the ground or containers (`isNew=True`, `slotIndex=-1`)
- Crafting results (`isNew=True`, `slotIndex=-1`)
- Receiving items from NPCs/quests (`isNew=True`, `slotIndex=-1`)
- Storage vault withdrawal (`isNew=True`, `slotIndex >= 0` = target inventory slot)
- Item entering inventory that creates a **new stack** (item you didn't already have a stack of)

**Interpreting `slotIndex`:**

| slotIndex | isNew | Meaning |
|---|---|---|
| `-1` | `False` | Session-start inventory load |
| `-1` | `True` | Genuine new acquisition (loot, craft, vendor purchase) |
| `>= 0` | `True` | **Storage vault withdrawal** — always paired with `ProcessRemoveFromStorageVault` |

**Key behavior:** At login, every inventory item fires a ProcessAddItem with `isNew=False`. This is how we build the **instance ID → item name mapping**. Items acquired during gameplay fire with `isNew=True`.

**Stack seeding:** For genuine new items (`slotIndex=-1, isNew=True`), the parser seeds the stack to 1. For storage withdrawals (`slotIndex>=0, isNew=True`), the parser defers seeding until `ProcessRemoveFromStorageVault` provides the authoritative quantity. For session-load items (`isNew=False`), no seeding occurs — the first `ProcessUpdateItemCode` establishes the baseline.

### ProcessUpdateItemCode — Existing stack updated

```
[HH:MM:SS] LocalPlayer: ProcessUpdateItemCode(instanceId, encodedValue, fromServer)
```

| Field | Type | Meaning |
|---|---|---|
| `instanceId` | u64 | Instance identifier (same as from ProcessAddItem) |
| `encodedValue` | u32 | Packed value: see decoding below |
| `fromServer` | bool | True = authoritative server update, False = client-side (e.g., moving between storage) |

#### Decoding `encodedValue`

The second argument packs two values into a single integer. **The stack size is 0-based** — the actual count is the encoded value plus one.

```
encodedValue = ((stackSize - 1) << 16) | itemTypeId
```

| Bits | Extraction | Value |
|---|---|---|
| High 16 bits | `(value >> 16) + 1` | **Stack size** (actual quantity after the update) |
| Low 16 bits | `value & 0xFFFF` | **Item type ID** (maps to CDN `items.id`) |

**Example:**
```
ProcessUpdateItemCode(136937342, 1642723, True)

  (1642723 >> 16) + 1 = 26    → actual stack size is 26
  1642723 & 0xFFFF    = 4323   → item type ID 4323 (MetalSlab3)
```

> **Verified:** The 0-based encoding was confirmed by cross-referencing `ProcessUpdateItemCode` values against the game's JSON inventory export (`StackSize` field), which uses 1-based counts. Every data point shows the encoded value is exactly 1 less than the JSON value.

**When it fires:**
- Adding items to an existing stack (quantity increases)
- Consuming items from a stack (crafting, using consumables — quantity decreases)
- Moving items between inventory and storage (typically `fromServer=False`)

**Tracking deltas:** By remembering the previous stack size for each instance ID, the delta between old and new stack size tells you how many items were added or removed:
```
delta = newStackSize - previousStackSize
  > 0 → items gained
  < 0 → items consumed/moved
```

### ProcessDeleteItem — Item removed from inventory

```
[HH:MM:SS] LocalPlayer: ProcessDeleteItem(instanceId)
```

| Field | Type | Meaning |
|---|---|---|
| `instanceId` | u64 | Instance identifier being removed |

**When it fires:**
- Stack fully consumed (last item used)
- Item moved to storage (paired with `ProcessAddToStorageVault`)
- Item destroyed or quest-consumed
- Motherlode map consumed on successful find

**Important:** DeleteItem fires for both "real" deletion (item consumed/destroyed) and storage transfers. To distinguish, check if a `ProcessAddToStorageVault` follows immediately — if so, the item was stowed, not destroyed.

### ProcessLoadSkills — Full skill snapshot

```
[HH:MM:SS] LocalPlayer: ProcessLoadSkills({type=SkillName,raw=R,bonus=B,xp=X,tnl=T,max=M}, ...)
```

A single line containing every skill the player has, each as a comma-separated struct:

| Field | Type | Meaning |
|---|---|---|
| `type` | string | Internal skill name (e.g., `Hammer`, `Mentalism`, `Anatomy_Cats`) |
| `raw` | u32 | Base skill level (without bonus) |
| `bonus` | u32 | Bonus levels from gear/buffs |
| `xp` | u32 | Current XP within the level |
| `tnl` | i32 | XP required to reach next level (-1 = cannot level further) |
| `max` | u32 | Maximum achievable level for this skill |

**When it fires:**
- Login — appears once alongside the ProcessAddItem inventory load
- Zone changes / reloads — fires again with a fresh snapshot

**Key behavior:** Every skill the player has ever touched is included, even at level 0. Meta-skills like `Anatomy` and `Phrenology` appear with `raw=0` and a `bonus` equal to the highest sub-skill (they are virtual roll-up skills, not directly trainable). Skills at their cap have `tnl=-1` (e.g., `Compassion` at 50/50).

**Example (abbreviated):**
```
[16:00:53] LocalPlayer: ProcessLoadSkills(
  {type=Hammer,raw=70,bonus=5,xp=0,tnl=1153715,max=70},
  {type=Mentalism,raw=76,bonus=0,xp=2353127,tnl=2502977,max=80},
  {type=Gourmand,raw=49,bonus=0,xp=835,tnl=2500,max=100},
  {type=Surveying,raw=60,bonus=3,xp=0,tnl=4000,max=60},
  ...
)
```

**Effective level** = `raw + bonus`. For Hammer above: effective level 75 (70 + 5).

### ProcessAddToStorageVault — Item moved to storage

```
[HH:MM:SS] LocalPlayer: ProcessAddToStorageVault(npcId, -1, slot, InternalName(instanceId))
```

When this follows a ProcessDeleteItem with the same instanceId, the item was **moved to storage**, not consumed.

## NPC Interaction Events

### ProcessStartInteraction — Begin interacting with an entity

```
[HH:MM:SS] LocalPlayer: ProcessStartInteraction(entityId, interactionType, distance, canInteract, "NPC_Name")
```

| Field | Type | Meaning |
|---|---|---|
| `entityId` | u32 | Entity ID (NPC, player saddlebag, interactable object) |
| `interactionType` | u32 | Type of interaction: `7` = talk/vendor, `3` = saddlebag/personal storage |
| `distance` | f32 | Distance to entity when interaction started (0 for self-interactions) |
| `canInteract` | bool | Whether the interaction is valid |
| `NPC_Name` | string | Internal NPC identifier (e.g., `"NPC_Yetta"`, `"NPC_Kalaba"`), empty string for non-NPC interactions |

**When it fires:** Player clicks on or otherwise initiates interaction with an NPC or interactable entity.

**Interaction types observed:**
- `7` — NPC talk/vendor (includes storage NPCs, hired vendors)
- `3` — Saddlebag / personal storage (entityId = player entity, NPC_Name = `""`)

### ProcessWaitInteraction — Interaction delay

```
[HH:MM:SS] LocalPlayer: ProcessWaitInteraction(entityId, delay, "", "")
```

| Field | Type | Meaning |
|---|---|---|
| `entityId` | u32 | Entity being interacted with |
| `delay` | u32 | Wait time in milliseconds (e.g., `500`) |

**When it fires:** Brief server-side delay during an NPC interaction, typically before a screen transition (e.g., opening shop logs, switching vendor tabs).

### ProcessPreTalkScreen — NPC talk screen preamble

```
[HH:MM:SS] LocalPlayer: ProcessPreTalkScreen(npcId, PreTalkScreenInfo)
```

Fires before each talk/prompt screen. Appears repeatedly during a single NPC conversation as the player navigates between screens (gift, vendor, dialogue).

### ProcessTalkScreen — NPC dialogue

```
[HH:MM:SS] LocalPlayer: ProcessTalkScreen(npcId, "", "dialogue text", "", System.Int32[], System.String[], 0, Generic)
```

| Field | Type | Meaning |
|---|---|---|
| `npcId` | u32 | NPC entity ID |
| `dialogue text` | string | The NPC's spoken text |
| `Generic` | enum | Dialogue category |

**When it fires:** NPC greets the player or responds during conversation.

### ProcessPromptForItem — NPC requests an item (gift giving)

```
[HH:MM:SS] LocalPlayer: ProcessPromptForItem(npcId, "Give Gift", "dialogue", "prompt", null, System.Int32[], System.String[], -1301, "", Error, 0, ForNpc, "NPC_Name")
```

| Field | Type | Meaning |
|---|---|---|
| `npcId` | u32 | NPC entity ID |
| `"Give Gift"` | string | Action type |
| `dialogue` | string | NPC's response text (e.g., `"A gift? For me?"`, `"Oh, thanks!"`) |
| `prompt` | string | UI prompt text (`"Choose gift"` or `"Choose another gift"`) |
| `ForNpc` | enum | Indicates this is an NPC-directed gift |
| `NPC_Name` | string | Internal NPC identifier |

**When it fires:** NPC opens the gift-giving UI. Fires once initially, then again after each gift is given (with updated dialogue like `"Choose another gift"`).

### ProcessDeltaFavor — NPC favor change

```
[HH:MM:SS] LocalPlayer: ProcessDeltaFavor(npcId, "NPC_Name", delta, isGift)
```

| Field | Type | Meaning |
|---|---|---|
| `npcId` | u32 | NPC entity ID |
| `NPC_Name` | string | Internal NPC identifier |
| `delta` | f32 | Favor amount gained (e.g., `2.8476`, `1.582`) |
| `isGift` | bool | True when favor is from a gift |

**When it fires:** After giving a gift to an NPC. The delta varies per item — different gifts are worth different amounts of favor.

### ProcessSetAttributes — Player attribute update

```
[HH:MM:SS] LocalPlayer: ProcessSetAttributes(entityId, "[KEY1, KEY2, ...], [val1, val2, ...]")
```

| Field | Type | Meaning |
|---|---|---|
| `entityId` | u32 | Player entity ID |
| `keys` | string[] | Parallel array of attribute names |
| `values` | f32[] | Parallel array of attribute values (matching order to keys) |

**Format:** Two parallel arrays inside a single string argument — attribute names and their values. A single event can set 1 to hundreds of attributes at once.

**When it fires:**
- **Login** — two massive dumps (hundreds of attributes each) covering all character state
- **Mount/dismount** — re-dumps ~44 attributes (stats change when mounted)
- **Skill bar swap** — re-dumps stats affected by active skills
- **During play** — incremental single or small-batch updates (e.g., `[IS_MOUNTED], [1]`, `[CUR_HEALTH, MAX_HEALTH, ...], [667, 667, ...]`)

**Known attribute categories:**
- **Vitals:** `CUR_HEALTH`, `MAX_HEALTH`, `CUR_POWER`, `MAX_POWER`, `CUR_ARMOR`, `MAX_ARMOR`, `CUR_METABOLISM`, `MAX_METABOLISM`
- **Regen:** `COMBAT_REGEN_HEALTH_DELTA`, `NONCOMBAT_REGEN_HEALTH_DELTA`, `COMBAT_REGEN_POWER_DELTA`, `NONCOMBAT_REGEN_POWER_DELTA`, `COMBAT_REGEN_ARMOR_DELTA`, `NONCOMBAT_REGEN_ARMOR_DELTA`
- **Movement:** `MOVEMENT_SPEED`, `SPRINT_BOOST`, `NONCOMBAT_SPRINT_BOOST`, `GRAVITY`, `JUMP_BURST`, `CLIMB_SPEED`
- **Combat modifiers:** `VULN_*` (per damage type × direct/indirect/elite), `MOD_*` (per damage type), `MITIGATION_*`
- **Ability modifiers:** `MOD_ABILITY_*`, `ABILITY_COST_MOD_*`, `ABILITY_RAGE_MOD_*`, `ABILITY_TAUNT_MOD_*`
- **Skill modifiers:** `MOD_SKILL_*`, `BOOST_SKILL_*`
- **NPC interaction:** `NPC_MOD_TRAININGCOST`, `NPC_MOD_MAXSALESVALUE`, `NPC_MOD_FAVORFROMGIFTS`, `NPC_MOD_FAVORFROMHANGOUTS`
- **Social:** `CUR_COMMUNITY`, `MAX_COMMUNITY`, `CUR_PEACEABLENESS`, `MAX_PEACEABLENESS`, `CUR_CLEANLINESS`, `MAX_CLEANLINESS`
- **Crafting:** `MAX_ACTIVE_WORKORDERS`, `WORKORDER_COIN_REWARD_MOD`, `CRAFTING_XP_EARNED_MOD`
- **Mount:** `IS_MOUNTED`, `MAX_MOUNT_ANXIETY`, `MOUNTED_TOP_SPEED_LAND`, `MOUNTED_TURN_SPEED_LAND`, `MOUNTED_ACCELERATION_LAND`, `MAX_SADDLEBAG_VAULT_SIZE`, `MOUNT_RESILIENCE`, etc.
- **Equipment:** `EQUIPMENT_LEVEL_CAP`, `EQUIPMENT_CAP_MASK`
- **Inventory:** `MAX_INVENTORY_SIZE`, `BONUS_STABLE_SLOTS`, `MAX_SADDLEBAG_VAULT_SIZE`
- **XP modifiers:** `COMBAT_XP_EARNED_MOD`, `CRAFTING_XP_EARNED_MOD`, `ANATOMY_XP_EARNED_MOD`, `SKINNING_XP_EARNED_MOD`, `ANGLING_XP_EARNED_MOD`, etc.
- **Misc:** `FOOD_LEVEL`, `ACTIVE_TITLE`, `RACIAL_LEVEL`, `AUTOLOOT_RADIUS`, `PVP`, `IS_CORPSE_INTACT`, `CUR_COMBAT_WISDOM`, `CUR_HYDRATION`, `MAX_HYDRATION`

**Examples:**
```
# Login dump (abbreviated — real line has hundreds of attributes)
[23:32:47] LocalPlayer: ProcessSetAttributes(11921435, "[MAX_HEALTH, CUR_HEALTH, MAX_POWER, CUR_POWER, ...], [667, 667, 442, 442, ...]")

# Mount
[23:33:25] LocalPlayer: ProcessSetAttributes(11921978, "[IS_MOUNTED], [1]")

# Health/combat update
[23:33:33] LocalPlayer: ProcessSetAttributes(11921978, "[CUR_HEALTH, MAX_HEALTH, CUR_POWER, MAX_POWER, CUR_ARMOR, MAX_ARMOR, CUR_METABOLISM, MAX_METABOLISM], [667, 667, 442, 442, 766, 766, 140, 140]")

# Single modifier change
[23:32:47] LocalPlayer: ProcessSetAttributes(11921435, "[WORKORDER_COIN_REWARD_MOD], [1.36]")
```

### ProcessUpdateQuest — Quest state change

```
[HH:MM:SS] LocalPlayer: ProcessUpdateQuest(entityId, TransitionalQuestState)
```

**When it fires:** A quest objective was completed or quest state changed. In the gift-giving context, this fires after a favor threshold is crossed (e.g., giving enough gifts unlocks a quest step).

**NOT YET PARSED.**

### ProcessSetWeather — Weather change

```
[HH:MM:SS] LocalPlayer: ProcessSetWeather("WeatherName", boolFlag)
```

| Field | Type | Meaning |
|---|---|---|
| `WeatherName` | string | Weather condition name (e.g., `"Clear Sky"`, `"Cloudy 3"`) |
| `boolFlag` | bool | Possibly indicates outdoor area (always True in samples) |

**When it fires:** On login and when the weather changes (zone transitions, weather cycle updates).

Relevant for features that depend on weather conditions (e.g., some Fletching recipes require clear weather).

**Examples:**
```
[23:32:47] LocalPlayer: ProcessSetWeather("Clear Sky", True)
[16:06:32] LocalPlayer: ProcessSetWeather("Cloudy 3", True)
```

### ProcessSetCelestialInfo — Moon phase

```
[HH:MM:SS] LocalPlayer: ProcessSetCelestialInfo(moonPhase)
```

| Field | Type | Meaning |
|---|---|---|
| `moonPhase` | string | Moon phase name (e.g., `WaxingCrescentMoon`, `FullMoon`, `WaningGibbousMoon`) |

**When it fires:** On login and area transitions. Provides the server's authoritative moon phase.

**Parsed** → `MoonPhaseChanged { phase }`. Persisted to `game_state_moon` singleton.

### ProcessAddEffects — Effects/buffs applied

```
[HH:MM:SS] LocalPlayer: ProcessAddEffects(entityId, sourceEntityId, "[effectId1, effectId2, ...]", boolFlag)
```

| Field | Type | Meaning |
|---|---|---|
| `entityId` | u32 | Target entity (player) |
| `sourceEntityId` | u32 | Source of effects (0 = system/login, self = self-applied buff) |
| `effectIds` | u32[] | List of numeric effect IDs |
| `boolFlag` | bool | False on login batch, True during gameplay |

**When it fires:**
- **Login** — large batch of all active effects with `sourceEntityId=0`, `boolFlag=False`
- **During play** — smaller batches when buffs are applied, with `sourceEntityId=self`, `boolFlag=True`

**Parsed** → `PlayerEvent::EffectsAdded`. Effect IDs are numeric — `ProcessUpdateEffectName` provides display names.

**Examples:**
```
# Login batch (many effect IDs, source=0)
[23:32:46] LocalPlayer: ProcessAddEffects(11921435, 0, "[302, 303, 13330, 26297, 26142, 26304, 44086019, ...]", False)

# In-play buff application (source=self)
[23:32:47] LocalPlayer: ProcessAddEffects(11921435, 11921435, "[13304, ]", True)
[23:32:47] LocalPlayer: ProcessAddEffects(11921435, 11921435, "[9024, ]", True)
```

### ProcessRemoveEffects — Effects/buffs removed

```
[HH:MM:SS] LocalPlayer: ProcessRemoveEffects(entityId, System.Int32[])
```

**When it fires:** When buffs expire or are dispelled. Fires on dismount and other state changes.

**Parsed** → `PlayerEvent::EffectsRemoved` (signal-only — the `System.Int32[]` is C#'s opaque ToString(), so individual effect IDs cannot be extracted).

### ProcessUpdateEffectName — Effect display name

```
[HH:MM:SS] LocalPlayer: ProcessUpdateEffectName(entityId, effectInstanceId, "Effect Name, Level N")
```

| Field | Type | Meaning |
|---|---|---|
| `entityId` | u32 | Player entity ID |
| `effectInstanceId` | u32 | Instance ID of the effect |
| `displayName` | string | Human-readable name with level (e.g., `"Performance Appreciation, Level 0"`) |

**When it fires:** After an effect is applied, providing its display name.

**Parsed** → `PlayerEvent::EffectNameUpdated`.

### ProcessPlayerMount — Mount/dismount

```
[HH:MM:SS] LocalPlayer: ProcessPlayerMount(entityId, isMounting)
```

| Field | Type | Meaning |
|---|---|---|
| `entityId` | u32 | Player entity ID |
| `isMounting` | bool | True = mounting, False = dismounting |

**When it fires:** Player mounts or dismounts. Followed by `ProcessSetAttributes([IS_MOUNTED], [1/0])`, `ProcessSetActiveSkills`, and `ProcessSetEquippedItems` with updated appearance.

**Examples:**
```
[23:33:25] LocalPlayer: ProcessPlayerMount(11921978, True)
[23:33:31] LocalPlayer: ProcessPlayerMount(11921978, False)
```

### ProcessSetActiveSkills — Active skill bar

```
[HH:MM:SS] LocalPlayer: ProcessSetActiveSkills(Skill1, Skill2)
```

| Field | Type | Meaning |
|---|---|---|
| `Skill1` | string | Primary active skill (e.g., `Hammer`, `Riding`) |
| `Skill2` | string | Secondary active skill (e.g., `Mentalism`) |

**When it fires:** On login, mount/dismount (swaps to Riding), and manual skill bar changes.

**Examples:**
```
[23:33:22] LocalPlayer: ProcessSetActiveSkills(Riding, Mentalism)
[23:33:31] LocalPlayer: ProcessSetActiveSkills(Hammer, Mentalism)
```

### ProcessSetEquippedItems — Equipment state

```
[HH:MM:SS] LocalPlayer: ProcessSetEquippedItems(System.Int32[], System.Int32[], System.Int32[], "appearanceString", entityId)
```

| Field | Type | Meaning |
|---|---|---|
| `int[] (x3)` | arrays | Item ID arrays (serialized as C# type names in log) |
| `appearanceString` | string | Full appearance/equipment string with slot assignments |
| `entityId` | u32 | Player entity ID |

**When it fires:** On login, mount/dismount, and equipment changes. The appearance string contains slot-keyed equipment data.

**Parsed** → `EquipmentChanged`. Extracts `entity_id`, full `appearance` string, and structured `equipment` slots. The three `System.Int32[]` arrays are opaque C# types (Unity prints type name only, not contents).

**Slot keys found in appearance string:**
- `@Chest`, `@Head`, `@Legs`, `@Feet`, `@Hands` — armor slots
- `@MainHand`, `MainHandEquip=Hammer` — main weapon + type
- `@OffHandShield`, `OffHandEquip=Shield` — off-hand + type
- `@Racial` — racial equipment slot
- `Mount=@Horse1(...)` — mount appearance with sub-slots (`@Saddle`, `@Saddlebag`, `@Reins`, `@MountCosmetic`)

### ProcessMountXpStatus — Mount XP eligibility

```
[HH:MM:SS] LocalPlayer: ProcessMountXpStatus(status)
```

| Field | Type | Meaning |
|---|---|---|
| `status` | enum | XP eligibility (e.g., `NotEarnedInThisArea`, `AlreadyMaxLevel`) |

**When it fires:** After mounting, indicates whether mount XP can be earned in the current area.

**NOT YET PARSED.**

### ProcessLoadAbilities — Ability loadout on login

```
[HH:MM:SS] LocalPlayer: ProcessLoadAbilities(System.Int32[], Skill1, Skill2, AbilityBarContents[])
```

**When it fires:** On login. Contains ability IDs, active skill pair, and ability bar layout.

**Parsed** → `AbilitiesLoaded`. Extracts `skill1` and `skill2` (the active skill pair). The `System.Int32[]` and `AbilityBarContents[]` arguments are opaque C# types — Unity prints only the type name, not array contents.

### ProcessLoadRecipes — Recipe knowledge on login

```
[HH:MM:SS] LocalPlayer: ProcessLoadRecipes(System.Int32[], System.Int32[])
```

**When it fires:** On login. Contains known recipe IDs and completion counts.

**Parsed** → `RecipesLoaded`. Signal event (timestamp only). Both `System.Int32[]` arguments are opaque C# types — Unity prints only the type name, not array contents. Individual recipe updates are tracked via `ProcessUpdateRecipe`.

### ProcessUpdateRecipe — Recipe learned/completed

```
[HH:MM:SS] LocalPlayer: ProcessUpdateRecipe(recipeId, completionCount)
```

| Field | Type | Meaning |
|---|---|---|
| `recipeId` | u32 | Recipe ID (maps to CDN recipe data) |
| `completionCount` | u32 | Total times this recipe has been completed |

**When it fires:** After completing a recipe during crafting.

**Example:**
```
[16:10:13] LocalPlayer: ProcessUpdateRecipe(21052, 151)
```

### ProcessSetStarredRecipes — Favorited recipes

```
[HH:MM:SS] LocalPlayer: ProcessSetStarredRecipes(System.Collections.Generic.HashSet`1[System.Int32])
```

**When it fires:** On login. Contains the set of recipe IDs the player has starred/favorited.

**NOT YET PARSED.**

### ProcessSetRecipeReuseTimers — Recipe cooldowns

```
[HH:MM:SS] LocalPlayer: ProcessSetRecipeReuseTimers(entityId, System.Int32[], System.Single[])
```

**When it fires:** During play when recipe cooldowns are active. Contains recipe IDs and remaining cooldown times.

**NOT YET PARSED.**

### ProcessLoadQuests — Quest state on login

```
[HH:MM:SS] LocalPlayer: ProcessLoadQuests(entityId, TransitionalQuestState[], System.Int32[], System.Int32[])
```

**When it fires:** On login. Full quest state including active quests, completed objectives, etc.

**NOT YET PARSED.**

### ProcessAddQuest — New quest acquired

```
[HH:MM:SS] LocalPlayer: ProcessAddQuest(entityId, TransitionalQuestState)
```

**When it fires:** Player accepts or triggers a new quest.

**NOT YET PARSED.**

### ProcessCompleteQuest — Quest completed

```
[HH:MM:SS] LocalPlayer: ProcessCompleteQuest(entityId, questId)
```

**When it fires:** Player completes a quest objective or turns in a quest.

**NOT YET PARSED.**

**Example:**
```
[16:25:49] LocalPlayer: ProcessCompleteQuest(1145895, 25216)
```

### ProcessSelectQuest — Quest tracking selection

```
[HH:MM:SS] LocalPlayer: ProcessSelectQuest(questId)
```

**When it fires:** Player selects a quest to track in the quest tracker UI.

**NOT YET PARSED.**

### ProcessCombatModeStatus — Combat state

```
[HH:MM:SS] LocalPlayer: ProcessCombatModeStatus(status, System.Int32[])
```

| Field | Type | Meaning |
|---|---|---|
| `status` | enum | `NotInCombat` or `InCombat` |

**When it fires:** When entering or leaving combat.

### ProcessMapFx — Map marker/point of interest

```
[HH:MM:SS] LocalPlayer: ProcessMapFx((x, y, z), radius, type, "label", category, "description")
```

| Field | Type | Meaning |
|---|---|---|
| `position` | (f32, f32, f32) | World coordinates |
| `radius` | u32 | Effect radius |
| `type` | u32 | Marker type |
| `label` | string | Short label (e.g., `"Tsavorite is here"`) |
| `category` | enum | Marker category (e.g., `ImportantInfo`) |
| `description` | string | Detailed text (e.g., `"The Tsavorite is 441m east and 1316m north."`) |

**When it fires:** Survey results, resource discoveries, and other map-pinned events.

**NOT YET PARSED** by `PlayerEventParser`. The legacy survey parser used to consume these from raw lines; the new `SurveySessionAggregator` does not need them (it works off `PlayerEvent::ItemDeleted` / `ItemAdded` with `ItemProvenance::SurveyMapUse`).

### ProcessSetAreaSettings — Area configuration

```
[HH:MM:SS] LocalPlayer: ProcessSetAreaSettings(AreaSettingsFromServer)
```

**When it fires:** On login and zone transitions. Contains area-specific settings.

**NOT YET PARSED.** Serialized C# type — actual data content unknown.

### ProcessAddPlayer — Player appearance on login

```
[HH:MM:SS] LocalPlayer: ProcessAddPlayer(serverId, entityId, "appearanceString", "CharacterName", "description", ...)
```

**When it fires:** On login. Contains the player's full appearance string, name, and description.

**NOT YET PARSED.**

### ProcessGuildGeneralInfo — Guild membership

```
[HH:MM:SS] LocalPlayer: ProcessGuildGeneralInfo(guildId, "GuildName", "motd")
```

**When it fires:** On login. Contains guild ID, name, and message of the day.

**Parsed** → `GuildInfoLoaded { guild_id, guild_name, motd }`. Persisted to `game_state_guild`.

### ProcessErrorMessage — Game error

```
[HH:MM:SS] LocalPlayer: ProcessErrorMessage(errorCode, "message")
```

**When it fires:** Various game errors (e.g., entity no longer exists, can't perform action).

**NOT YET PARSED.**

### ProcessEndInteraction — Interaction ended

```
[HH:MM:SS] LocalPlayer: ProcessEndInteraction(entityId)
```

**When it fires:** Player ends an NPC interaction (closes dialogue, walks away).

### ProcessExtendedItemUseInfo — Extended item use data

```
[HH:MM:SS] LocalPlayer: ProcessExtendedItemUseInfo(SystemName, ActionType, System.Collections.Generic.List`1[System.Int32])
```

**When it fires:** On login. Known systems: `Gourmand` with `Initialize` action — contains list of food item IDs the player has eaten.

**NOT YET PARSED.** Currently consumed by `gourmandStore` via separate mechanism.

### ProcessShowRecipes — Recipe UI opened

```
[HH:MM:SS] LocalPlayer: ProcessShowRecipes(SkillName)
```

**When it fires:** Player opens the crafting recipe list for a specific skill (e.g., `Teleportation`).

**NOT YET PARSED.**

### ProcessSetString — String attribute

```
[HH:MM:SS] LocalPlayer: ProcessSetString(key, "value")
```

**When it fires:** Sets named string values. Known keys: `NOTEPAD`, `NOTEPAD_TAB_1` through `NOTEPAD_TAB_4`, `NOTEPAD_TAB_NAMES`, `FRIEND_STATUS`, `PUBLIC_STATUS`, `HUNTING_GROUP_TITLE`, `MOUNT_APPEARANCE`.

**Parsed** → `PlayerStringUpdated { key, value }` for 9 known useful keys (MOUNT_APPEARANCE skipped — already tracked via equipment). Persisted to `game_state_strings`.

### ProcessTitlesList — Unlocked titles

```
[HH:MM:SS] LocalPlayer: ProcessTitlesList(Initialize, System.Collections.Generic.List`1[System.Int32])
```

**When it fires:** On login. Contains list of title IDs the player has unlocked.

**NOT YET PARSED.**

### ProcessBookList — Known books

```
[HH:MM:SS] LocalPlayer: ProcessBookList(Initialize, System.Collections.Generic.List`1[System.Int32])
```

**When it fires:** On login. Contains list of book IDs the player has read.

**NOT YET PARSED.**

### ProcessPlayerVendorScreen — Player shop inventory

```
[HH:MM:SS] LocalPlayer: ProcessPlayerVendorScreen(npcId, "", System.Collections.Generic.List`1[PlayerVendorItemForSale], slotCount, bool, bool, ...)
```

**When it fires:** Player opens a vendor stall UI. The 5th positional arg is
the manager flag: `True` = own stall (management mode), `False` = someone
else's stall (buy mode).

**Parsed:** yes (`VendorScreenOpened`, emitted only for OTHERS' stalls —
own-stall activity is covered by PlayerShopLog books). The stall-price capture
panel it used to drive was removed (batch-1, 2026-10-30) — the event is
currently emitted but consumed nowhere.
The listing contents/prices are not in the log line itself.

### ProcessPlayerVendorScreenUpdate — Player shop item update

```
[HH:MM:SS] LocalPlayer: ProcessPlayerVendorScreenUpdate(npcId, PlayerVendorItemForSale, bool)
```

**When it fires:** Item added or price changed in player's vendor stall.

**NOT YET PARSED.**

### ProcessPlayerVendorScreenRemove — Player shop item removed

```
[HH:MM:SS] LocalPlayer: ProcessPlayerVendorScreenRemove(npcId, instanceId)
```

**When it fires:** Item removed from a player's vendor stall — a purchase from
another player's stall, or collecting your own listing back.

**Parsed:** yes (`VendorItemTaken`, emitted only when the open screen is
someone ELSE's stall; the item/quantity arrive via the paired `ProcessAddItem`
provenance join).

### ProcessSetDisabledEquipment — Disabled equipment slots

```
[HH:MM:SS] LocalPlayer: ProcessSetDisabledEquipment(System.Int32[])
```

**When it fires:** After equipment changes. Indicates which equipment slots are currently disabled.

**NOT YET PARSED.**

### ProcessSetLockedItems — Locked items

```
[HH:MM:SS] LocalPlayer: ProcessSetLockedItems(System.Int32[])
```

**When it fires:** On login. Items the player has locked/protected from accidental use.

**NOT YET PARSED.**

### ProcessInventoryFolderSettings — Inventory UI state

```
[HH:MM:SS] LocalPlayer: ProcessInventoryFolderSettings(System.Collections.Generic.List`1[InventoryFolderSettings])
```

**When it fires:** On login. Player's inventory folder/tab configuration.

**NOT YET PARSED.**

### ProcessSetExtendedGuiFeatures — GUI feature flags

```
[HH:MM:SS] LocalPlayer: ProcessSetExtendedGuiFeatures(ExtendedGuiFeatures)
```

**When it fires:** On login. GUI feature configuration from server.

**NOT YET PARSED.**

### ProcessCompleteDirectedGoals — Tutorial/directed goal completion

```
[HH:MM:SS] LocalPlayer: ProcessCompleteDirectedGoals(System.Int32[])
```

**When it fires:** On login. List of completed tutorial/directed goals. Format is `[id1,id2,id3,]` (comma-separated integers in brackets, with trailing comma).

**Parsed** → `DirectedGoalsLoaded { goal_ids: Vec<u32> }`. Persisted to `game_state_directed_goals` (full replacement on login).

### ProcessMapFog — Explored map areas

```
[HH:MM:SS] LocalPlayer: ProcessMapFog(System.Collections.Generic.List`1[MapFogHistory])
```

**When it fires:** On login. Map exploration/fog-of-war state.

**NOT YET PARSED.**

### ProcessRedemptionCount — Redemption/loyalty points

```
[HH:MM:SS] LocalPlayer: ProcessRedemptionCount(count)
```

**When it fires:** On login. Current redemption point count.

**NOT YET PARSED.**

### ProcessToolCommandResponse — Tool command result

```
[HH:MM:SS] LocalPlayer: ProcessToolCommandResponse(commandId, success, "message", System.Collections.Generic.Dictionary`2[System.String,System.String])
```

**When it fires:** Response to a tool command (e.g., a VIP export button).

**NOT YET PARSED.**

### ProcessRemoveLoot — Item picked up from loot window

```
[HH:MM:SS] LocalPlayer: ProcessRemoveLoot(instanceId)
```

| Field | Type | Meaning |
|---|---|---|
| `instanceId` | u64 | Instance ID of the item being removed from the corpse/loot window |

**When it fires:** The player picks up an item from a corpse's loot window. One `ProcessRemoveLoot` fires per item picked up.

**Key behavior:**

- The `instanceId` refers to the **corpse-side instance** of the item, not the player's inventory instance.
- When the item creates a **new inventory stack**: `ProcessAddItem(Name(instanceId), ...)` fires first with the **same** instanceId, then `ProcessRemoveLoot(instanceId)` follows. The instance IDs match.
- When the item **merges into an existing stack**: `ProcessUpdateItemCode(existingInstanceId, ...)` fires for the player's existing stack, then `ProcessRemoveLoot(corpseInstanceId)` fires with a **different** instanceId that was never seen in any `ProcessAddItem`. The corpse-side instance ID is orphaned.

**Critical distinction from skinning/butchering:** Items granted by skinning, butchering, anatomy, or mycology do **not** produce `ProcessRemoveLoot` events. They are granted directly via `ProcessAddItem` or `ProcessUpdateItemCode` without going through the loot window. This makes `ProcessRemoveLoot` the only reliable signal that an item was a **corpse drop** rather than a skill-harvesting reward.

**Loot attribution strategy:**

| Scenario | Events (in order) | How to identify the item |
|---|---|---|
| New item (new stack) | `ProcessAddItem(Name(id))` → `ProcessRemoveLoot(id)` | Instance IDs match; `AddItem` gives the item name |
| Stacking item (merges) | `ProcessUpdateItemCode(existingId, encoded)` → `ProcessRemoveLoot(unknownId)` | Decode `encoded & 0xFFFF` for `itemTypeId`, resolve via CDN to get item name. The `RemoveLoot` instanceId is orphaned but the `UpdateItemCode` in the same tick during a `CorpseSearch` context identifies what was picked up |
| Skinning/butchering | `ProcessAddItem` or `ProcessUpdateItemCode` only | **No** `ProcessRemoveLoot` — this is how you distinguish harvesting rewards from actual drops |

**NOT YET PARSED.**

### ProcessAttack — Attack action

```
[HH:MM:SS] LocalPlayer: ProcessAttack(attackType)
```

**When it fires:** Player initiates an attack.

**NOT YET PARSED.**

### ProcessShowStable — Stable UI

```
[HH:MM:SS] LocalPlayer: ProcessShowStable(npcId, StableSlot[], System.Int32[], System.String[], modifier)
```

**When it fires:** Player opens the animal stable UI. Contains stable slot data, animal IDs, and names.

**NOT YET PARSED.**

### ProcessFirstEverInteraction — First interaction with entity

```
[HH:MM:SS] LocalPlayer: ProcessFirstEverInteraction("interactionData")
```

**When it fires:** First time interacting with a specific entity type (portals, etc.). Contains interaction metadata string.

**NOT YET PARSED.**

### ProcessEnableInteractor — Interactable entities

```
[HH:MM:SS] LocalPlayer: ProcessEnableInteractor(System.Int32[], System.Int32[])
```

**When it fires:** On login/zone change. Lists entities that can be interacted with.

**NOT YET PARSED.**

### ProcessBook — Display book or log content

```
[HH:MM:SS] LocalPlayer: ProcessBook("title", "content", "bookType", "", "", False, False, False, False, False, "")
```

| Field | Type | Meaning |
|---|---|---|
| `title` | string | Book/log title (e.g., `"Yesterday's Shop Logs"`, `"Today's Shop Logs"`) |
| `content` | string | Full text content with `\n` line breaks |
| `bookType` | string | Category (e.g., `"PlayerShopLog"`) |

**When it fires:** Player opens a readable book, scroll, or log in-game. For player shops, the hired vendor NPC provides daily shop logs via this event.

**Player shop log content** includes structured entries like:
- `"Toncom bought Thin Mesh Grate at a cost of 350 per 1 = 350"` — customer purchase
- `"Zenith collected 3800 Councils from customer purchases"` — owner collecting gold
- `"Zenith added Guava x100 to shop"` — owner stocking items
- `"Zenith made Guavax100 visible in shop at a cost of 500 per 1"` — owner setting prices
- `"Zenith paid 10900 Councils to hire Mantis Attendant for another 24 hours."` — vendor hire renewal
- `"Fidge sent a note to shop owner"` — customer message
- `"Zenith removed Basic Metal Slab x55 from shop"` — owner pulling items

### ProcessScreenText — On-screen notification

```
[HH:MM:SS] LocalPlayer: ProcessScreenText(category, "message")
```

| Field | Type | Meaning |
|---|---|---|
| `category` | enum | Notification type (e.g., `ImportantInfo`, `CraftingNotice`) |
| `message` | string | Display text |

**When it fires:** Various in-game notifications. Known categories:
- `ImportantInfo` — survey distance hints, loot bonuses (e.g., `"The treasure is 342 meters from here."`, `"Malachite collected! Also found Quartz x3"`)
- `CraftingNotice` — crafting/storage results (e.g., `"Stowed 5 items across 3 storages."`)

## Storage Events

### ProcessShowStorageVault — Open a storage vault tab

```
[HH:MM:SS] LocalPlayer: ProcessShowStorageVault(npcId, vaultId, "Storage", "label", slotCount, System.Collections.Generic.List`1[Item], System.String[], "tabName", System.Int32[], System.String[], modifier)
```

| Field | Type | Meaning |
|---|---|---|
| `npcId` | u32 | NPC entity ID providing storage (or player entity for saddlebag) |
| `vaultId` | u32 | Vault identifier (e.g., `1501`–`1507`, `114` for saddlebag) |
| `"Storage"` | string | Vault type (`"Storage"` for NPC vaults, `"Saddlebag"` for mount storage) |
| `label` | string | Description (e.g., `"Access saddlebag contents here"`, empty for NPC storage) |
| `slotCount` | u32 | Total slots in this vault tab |
| `tabName` | string | Named tab (e.g., `"Gardening and Tools"`, `"Equipment and Ammunition"`, `"Potions and Alchemy Ingredients"`, `"Gems, Crystals, and Ores"`, or empty) |

**When it fires:** Player opens or switches between storage vault tabs. Each tab is a separate vault with its own ID.

### ProcessRefreshStorageVault — Storage vault contents refreshed

```
[HH:MM:SS] LocalPlayer: ProcessRefreshStorageVault(npcId, vaultId, slotCount, System.Collections.Generic.List`1[Item])
```

| Field | Type | Meaning |
|---|---|---|
| `npcId` | u32 | NPC entity ID (0 for bulk stow operations) |
| `vaultId` | u32 | Vault identifier |
| `slotCount` | i32 | Slot count (-1 during bulk operations) |

**When it fires:** After items are added to or removed from storage. During a "stow all" operation, multiple vaults refresh simultaneously (npcId=0, slotCount=-1), followed by a `ProcessScreenText(CraftingNotice, "Stowed N items across M storages.")`.

### ProcessRemoveFromStorageVault — Take item from storage

```
[HH:MM:SS] LocalPlayer: ProcessRemoveFromStorageVault(npcId, -1, instanceId, quantity)
```

| Field | Type | Meaning |
|---|---|---|
| `npcId` | u32 | NPC entity ID (or player entity for saddlebag) |
| `instanceId` | u64 | Instance ID of the item being retrieved |
| `quantity` | u32 | Number of items taken from the stack |

**When it fires:** Player takes an item from storage into inventory. Always paired with a preceding `ProcessAddItem` — the item appears in inventory, then the storage removal is confirmed.

**Key behavior:** The inverse of `ProcessAddToStorageVault`. For AddToStorage, the sequence is `DeleteItem → AddToStorageVault`. For RemoveFromStorage, it's `AddItem → RemoveFromStorageVault`.

## Vendor Events

### ProcessVendorScreen — Open vendor shop

```
[HH:MM:SS] LocalPlayer: ProcessVendorScreen(npcId, favorLevel, currentGold, serverId, maxGold, "greeting", VendorInfo[], VendorInfo[], VendorInfo[], VendorPurchaseCap[], System.Int32[], System.String[], -1601)
```

| Field | Type | Meaning |
|---|---|---|
| `npcId` | u32 | NPC entity ID |
| `favorLevel` | enum | Favor tier with this NPC (e.g., `SoulMates`, `BestFriends`) |
| `currentGold` | u32 | Gold the vendor currently has available to buy your items |
| `serverId` | u64 | Server-side identifier |
| `maxGold` | u32 | Maximum gold the vendor can hold |
| `greeting` | string | Vendor greeting text |
| `VendorInfo[]` | array | Vendor inventory tabs (buy/sell/buyback) |
| `VendorPurchaseCap[]` | array | Per-item purchase limits |

**When it fires:** Player opens the vendor/shop UI on an NPC.

**Key behavior:** `favorLevel` reflects the player's relationship tier with that NPC, which determines available inventory and prices. `currentGold` decreases as you sell items to the vendor.

### ProcessVendorAddItem — Sell item to vendor

```
[HH:MM:SS] LocalPlayer: ProcessVendorAddItem(price, InternalName(instanceId), isFromBuyback)
```

| Field | Type | Meaning |
|---|---|---|
| `price` | u32 | Sale price in gold (councils) |
| `InternalName` | string | Item internal name (e.g., `AmuletOfCrushingMitigation5`) |
| `instanceId` | u64 | Item instance ID (same as from ProcessAddItem/DeleteItem) |
| `isFromBuyback` | bool | False = selling to vendor, True = from buyback tab |

**When it fires:** Player sells an item to the vendor. Always preceded by a `ProcessDeleteItem` with the same instance ID (item leaves player inventory).

### ProcessVendorUpdateItem — Vendor stack updated

```
[HH:MM:SS] LocalPlayer: ProcessVendorUpdateItem(instanceId, encodedValue, price)
```

| Field | Type | Meaning |
|---|---|---|
| `instanceId` | u64 | Instance ID already in vendor inventory |
| `encodedValue` | u32 | Packed value, same 0-based encoding as ProcessUpdateItemCode: `((stackSize-1) << 16) \| itemTypeId` |
| `price` | u32 | Price per unit |

**When it fires:** Selling a stackable item that the vendor already has a stack of. Instead of creating a new entry (`VendorAddItem`), the existing vendor stack is updated.

### ProcessVendorUpdateAvailableGold — Vendor gold balance change

```
[HH:MM:SS] LocalPlayer: ProcessVendorUpdateAvailableGold(currentGold, serverId, maxGold)
```

| Field | Type | Meaning |
|---|---|---|
| `currentGold` | u32 | Vendor's gold after the transaction |
| `serverId` | u64 | Server-side identifier |
| `maxGold` | u32 | Vendor's maximum gold capacity |

**When it fires:** After every vendor buy/sell transaction. `currentGold` decreases when the vendor buys from you (pays you gold), increases when you buy from them.

## Practical Patterns — NPC Interactions

### Selling Items to a Vendor

Each sold item produces a three-event sequence:

```
[16:32:25] ProcessDeleteItem(115259296)                                    ← item leaves inventory
[16:32:25] ProcessVendorAddItem(120, AmuletOfCrushingMitigation5(115259296), False)  ← vendor receives it at 120g
[16:32:25] ProcessVendorUpdateAvailableGold(14880, ..., 15000)             ← vendor gold drops by 120
```

When selling a stackable item that already exists in the vendor's inventory:

```
[16:32:27] ProcessDeleteItem(115271948)
[16:32:27] ProcessVendorUpdateItem(115249145, 200909, 7)     ← existing vendor stack updated
[16:32:27] ProcessVendorUpdateAvailableGold(14776, ..., 15000)
```

**Tracking gold earned:** The difference between consecutive `ProcessVendorUpdateAvailableGold` values gives the sale price, or read it directly from `ProcessVendorAddItem`.

### Gift-Giving to an NPC

```
[16:33:00] ProcessPromptForItem(9618, "Give Gift", ..., ForNpc, "NPC_Kalaba")  ← gift UI opens
[16:33:03] ProcessDeltaFavor(9618, "NPC_Kalaba", 2.8476, True)                ← favor gained
[16:33:03] ProcessPromptForItem(9618, "Give Gift", "Oh, thanks!", ...)         ← ready for next gift
[16:33:03] ProcessDeleteItem(114961794)                                         ← gifted item consumed
[16:33:04] ProcessDeltaFavor(9618, "NPC_Kalaba", 1.582, True)                 ← more favor
[16:33:04] ProcessUpdateQuest(1145895, TransitionalQuestState)                 ← quest threshold crossed
```

**Detection logic:**
- `ProcessPromptForItem` with `ForNpc` → gift interaction started
- `ProcessDeleteItem` during a gift prompt → item was given as a gift (not sold or consumed)
- `ProcessDeltaFavor` → favor reward for the gift
- Different items yield different favor amounts

### Player-Owned Shop (Hired Vendor)

Interacting with a hired vendor (`NPC_HiredVendor`) reveals shop management via dialogue and book events:

```
[13:25:58] ProcessStartInteraction(9506, 7, 0, False, "NPC_HiredVendor")
[13:25:59] ProcessTalkScreen(9506, "", "<b>Hi boss, what do you need?</b>\n\n---\n
    Mantis Attendant is hired by you. Time remaining: 27 hours.\n
    Councils in cash-box: 25000", ...)
[13:26:01] ProcessTalkScreen(9506, "", "<b>You collected 25000 councils.</b>\n\n---\n
    Mantis Attendant is hired by you. Time remaining: 27 hours.\n
    Councils in cash-box: 0", ...)
[13:26:04] ProcessBook("Yesterday's Shop Logs", "...", "PlayerShopLog", ...)
[13:26:07] ProcessBook("Today's Shop Logs", "...", "PlayerShopLog", ...)
```

**Key data extractable from hired vendor dialogue:**
- **Attendant type** and **time remaining** (from `TalkScreen` text: `"Mantis Attendant is hired by you. Time remaining: 27 hours."`)
- **Cash-box balance** before/after collection (`"Councils in cash-box: 25000"` → `"Councils in cash-box: 0"`)
- **Collection amount** (`"You collected 25000 councils."`)

**Key data extractable from shop logs (`ProcessBook` with `PlayerShopLog`):**
- Customer purchases: who bought what, price, and quantity
- Owner stocking/pricing/removal actions
- Gold collection history
- Vendor hire payments and renewals
- Customer messages

### Storage Vault Interaction

Interacting with a storage NPC shows vault tabs and allows item transfers:

```
[13:26:55] ProcessStartInteraction(14804, 7, 1200, True, "NPC_Qatik")
[13:26:58] ProcessShowStorageVault(14804, 1507, "Storage", "", 15, ...)        ← first tab
[13:27:03] ProcessShowStorageVault(14804, 1506, "Storage", "", 48, ...)        ← switch tab
[13:27:07] ProcessDeleteItem(136093889)                                         ← item leaves inventory
[13:27:07] ProcessAddToStorageVault(14804, -1, 40, MapleWood(136093889))       ← into storage
[13:27:07] ProcessDeleteItem(133493941)                                         ← stackable, merged
[13:27:07] ProcessRefreshStorageVault(14804, -1, 48, ...)                      ← vault refreshed
```

Retrieving items from storage is the reverse:
```
[13:28:48] ProcessAddItem(MetalSlab4(132702881), 46, True)                     ← item enters inventory
[13:28:48] ProcessRemoveFromStorageVault(14804, -1, 132702881, 11)             ← removed 11 from storage
```

**Bulk stow** (game auto-distributes items across vaults):
```
[13:27:00] ProcessDeleteItem(136202943)
[13:27:00] ProcessDeleteItem(136202764)
[13:27:00] ProcessDeleteItem(136184812)
[13:27:00] ProcessRefreshStorageVault(0, 1505, -1, ...)    ← npcId=0, slotCount=-1 during bulk
[13:27:00] ProcessRefreshStorageVault(0, 1501, -1, ...)
    ... (multiple vaults refresh)
[13:27:00] ProcessScreenText(CraftingNotice, "Stowed 5 items across 3 storages.")
```

### Saddlebag Access

The player's mount saddlebag is accessed via self-interaction (type `3`):

```
[13:27:20] ProcessStartInteraction(4938644, 3, 0, False, "")                   ← self-interaction
[13:27:20] ProcessShowStorageVault(4938644, 114, "Saddlebag", "Access saddlebag contents here", 62, ...)
[13:27:22] ProcessAddItem(DishingHammer(136195024), 59, True)                  ← take from saddlebag
[13:27:22] ProcessRemoveFromStorageVault(4938644, -1, 136195024, 1)
```

**Detection:** `ProcessStartInteraction` with type `3` and empty NPC name → saddlebag. The `ProcessShowStorageVault` will have type `"Saddlebag"` instead of `"Storage"`.

## Instance ID → Item Identity Mapping

Instance IDs are arbitrary per-session numbers. They do **not** correspond to CDN item IDs. To know what item an instance ID refers to, you must either:

1. **Catch the ProcessAddItem at login** — every inventory item is enumerated with `InternalName(instanceId)` when the player logs in
2. **Read the low 16 bits of ProcessUpdateItemCode** — the `itemTypeId` embedded in the encoded value maps to CDN `items.id`

Both approaches should be used together. The AddItem path gives you the internal name mapping; the UpdateItemCode path gives you the numeric type ID and stack size.

## Practical Patterns

### Corpse Looting — Kill → Search → Loot → Skin/Butcher

A complete kill-and-loot cycle involves events from both Player.log and Chat.log. This is the authoritative reference for attributing specific items to specific kills.

**Event sequence (from devtools capture of killing 5 Bear Groupies):**

```
── Kill (Chat.log) ──────────────────────────────────────────────────
[Combat] Zenith: Rib Shatter 9 on Bear Groupie #6701962! Dmg: 1861 health, 166 armor. (FATALITY!)

── Search Corpse (Player.log) ───────────────────────────────────────
ProcessTalkScreen(6701962, "Search Corpse of Bear Groupie", "<death details>", "", [301,401,701,], ..., 0, Corpse)
    ← buttons [301,401,701] = Skin, Butcher, Loot All (available actions)

── Skinning (Player.log) ───────────────────────────────────────────
ProcessAddItem(Skin3(203413381), -1, True)          ← Crude Animal Skin, new stack
ProcessAddItem(Skin2(203413382), -1, True)          ← Rough Animal Skin, new stack
ProcessUpdateSkill({type=Skinning,...}, True, 40, 0, 0)  ← 40 Skinning XP
ProcessTalkScreen(6701962, ..., [701,], ..., 1, Corpse)
    ← buttons now [701] only (Loot All) — skinning consumed button 301
    ← body text updated: "Zenith skinned the corpse ... obtained Crude Animal Skin x2 plus Rough Animal Skin"
    ★ NO ProcessRemoveLoot for skinning items

── Anatomy/Bury (Player.log) ────────────────────────────────────────
ProcessScreenText(GeneralInfo, "You bury the corpse.")
ProcessTalkScreen(6701962, ..., [], ..., 1, Corpse)
    ← buttons now [] — all actions consumed

── Loot Pickup (Player.log) ────────────────────────────────────────
ProcessAddItem(EnchantedBearClaw(203413356), -1, True)   ← new stack created
ProcessRemoveLoot(203413356)                              ← SAME instanceId = corpse drop confirmed
    ★ ProcessRemoveLoot fires ONLY for actual loot pickups

── Next Corpse — Stacking Example ──────────────────────────────────
ProcessUpdateItemCode(203413381, 213611, True)       ← Crude Animal Skin stack: 1 → 4
ProcessUpdateItemCode(203413382, 82538, True)        ← Rough Animal Skin stack: 1 → 2
    ★ Skinning items via UpdateItemCode, NO RemoveLoot

ProcessAddItem(OakWood(203413346), -1, True)         ← new Oak Wood stack
ProcessRemoveLoot(203413346)                          ← corpse drop confirmed

── Later Corpse — Merge-Into-Existing-Stack ────────────────────────
ProcessUpdateItemCode(203413346, 144173, True)       ← Oak Wood stack grows (itemTypeId=13101)
ProcessRemoveLoot(203413293)                          ← orphaned instanceId (never seen in AddItem)
    ★ Item identity recoverable: decode 144173 & 0xFFFF = 13101 → Oak Wood via CDN
```

**Key discriminator:** `ProcessRemoveLoot` fires **only** for items from the corpse's loot table. Skinning, butchering, anatomy, and mycology rewards are granted directly without going through the loot window. This is the only reliable way to distinguish actual drops from harvesting rewards.

**Attribution algorithm:**

1. Parse `ProcessRemoveLoot(instanceId)` during a `CorpseSearch` context
2. Check if `instanceId` matches a recent `ProcessAddItem` → item identity known directly
3. If no match (stacking case), use the immediately preceding `ProcessUpdateItemCode` (the parser's `last_item_event`) → it contains the `itemTypeId` and item name from the instance registry
4. Consume `last_item_event` after fallback use (`.take()`) so a second orphaned `RemoveLoot` doesn't reuse a stale match
5. Ignore `ProcessAddItem`/`ProcessUpdateItemCode` events during `CorpseSearch` that are **not** followed by `ProcessRemoveLoot` → those are skinning/butchering rewards

**Corpse lifecycle signals:**

| Signal | Meaning | Reliability |
|--------|---------|-------------|
| `ProcessTalkScreen(entityId, "Search Corpse of X", ...)` | Corpse search opened — new CorpseSearch context | Always fires |
| `ProcessTalkScreen` for a **different** entityId | Previous corpse interaction ended | Always fires |
| `ProcessScreenText(GeneralInfo, "You bury the corpse.")` | Anatomy performed — strong signal that corpse interaction is complete | Not always (bosses/elites can't be buried, some players skip it) |
| `ProcessTalkScreen(entityId, ..., [], ...)` (empty buttons) | All actions consumed on this corpse (skinned/butchered/looted) | Reliable but doesn't mean player is done clicking |
| CorpseSearch context timeout (30s) | Auto-cleanup if no explicit close | Fallback only |

Note: neither Player.log nor Chat.log has millisecond timestamps, so events within the same second can't be reliably ordered across logs. Within a single log file, line order is authoritative.

### Motherlode Survey Lifecycle

Motherlode maps are used repeatedly to get distance hints. When the correct location is found, the map is consumed and a mining node spawns:

```
[17:37:00] ProcessDoDelayLoop(1, Unset, "Using Kur Mountains Good Metal Motherlode Map", 5305, ...)
[17:37:01] ProcessScreenText(ImportantInfo, "The treasure is 342 meters from here.")
    ↑ distance hint — map NOT consumed

[17:37:03] ProcessDoDelayLoop(1, Unset, "Using Kur Mountains Good Metal Motherlode Map", 5305, ...)
[17:37:04] ProcessDeleteItem(136969636)
[17:37:05] ProcessStartInteraction(5163814, 7, 0, False, "")
[17:37:05] ProcessDoDelayLoop(6, ChopLumber, "Mining ...", 0, AbortIfAttacked, IsInteractorDelayLoop)
[17:37:11] ProcessUpdateItemCode(136937342, 1642723, True)    ← MetalSlab3, stack now 25
[17:37:11] ProcessUpdateItemCode(136807948, 3167735, True)    ← Tungsten, stack now 48
    ↑ found! map consumed → mine node → rewards via stack updates
```

**Detection logic:**
- `ProcessDoDelayLoop` with `"Motherlode Map"` followed by `ProcessScreenText` with distance → ping (searching)
- `ProcessDoDelayLoop` with `"Motherlode Map"` followed by `ProcessDeleteItem` → found (map consumed)
- `ProcessDoDelayLoop` with `"Mining ..."` shortly after → mining the spawned node
- `ProcessUpdateItemCode` calls immediately after mining completes → rewards

### Crafting Consumption

When crafting consumes materials, you see stack decreases:

```
ProcessUpdateItemCode(109085930, 200710, True)    → AdvancedInk, stack decreased
ProcessUpdateItemCode(136144120, 1574047, True)   → TundraLichen, stack decreased
ProcessUpdateItemCode(111587763, 5247202, True)   → MetalSlab2, stack decreased
```

### Storage Transfers (Not Real Deletion)

See the detailed **Storage Vault Interaction** pattern above for full stow/retrieve/bulk sequences. The short version: `ProcessDeleteItem` followed by `ProcessAddToStorageVault` with the same instance ID is a storage move, not a consumption.

## Architecture Notes

### Core Item Tracker

Item event parsing should be a **core system**, not specific to any feature. It provides:

1. **Instance registry** — maps instance IDs to item names/type IDs, built from login AddItem events
2. **Stack tracking** — current stack size per instance, updated on every UpdateItemCode
3. **Delta events** — emits item gained/lost events with item identity and quantity

Features like surveying, crafting tracking, or loot analysis subscribe to these events rather than parsing item lines themselves.

### Limitations

- **Item type ID is 16-bit** — max value 65535. Items with IDs above this would overflow. Check CDN data to confirm all item IDs fit (they should based on the game's scale).
- **Stack overflow** — when a stack hits max (typically 99), additional items of the same type create a new stack via ProcessAddItem rather than ProcessUpdateItemCode.
- **fromServer flag** — `True` on UpdateItemCode means server-authoritative (real game event). `False` typically means client-side inventory management (storage moves). Filter on `True` for tracking real gains/losses.
- **No item metadata in UpdateItemCode** — only the type ID and stack size. For item names, durability, etc., you need the instance registry or CDN lookup.
