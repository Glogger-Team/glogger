# Trip Routing

## Overview

A reusable multi-zone route planner for Project: Gorgon. Given a set of stops (zone + purpose), produces an ordered trip plan that accounts for the player's available travel methods (walking, bind recall, mushroom circle recall, TP machine).

The solver is a backend service — multiple frontend features feed stops into it. Currently no UI exposes routing directly; it's infrastructure for work order fulfillment, crafting project pickups, delivery quests, and inventory consolidation.

## Architecture

```
Frontend feature           Tauri command       Rust solver
(WO tab, Projects, etc.) → plan_trip() ───→ trip_router::plan_route()
                                                  │
                                            zone_graph::ZoneGraph
                                            (static adjacency + BFS)
```

**Key files:**

| File | Purpose |
|------|---------|
| [src-tauri/src/zone_graph.rs](../../src-tauri/src/zone_graph.rs) | Zone connectivity graph, BFS distance matrix, sub-zone resolution, friendly name resolution |
| [src-tauri/src/trip_router.rs](../../src-tauri/src/trip_router.rs) | Route solver, travel method selection, Tauri command |
| [src-tauri/src/db/migrations.rs](../../src-tauri/src/db/migrations.rs) | v39: `game_state_teleportation` table |
| [src-tauri/src/coordinator.rs](../../src-tauri/src/coordinator.rs) | `ingest_teleportation_binds()` — parses bind locations from SkillReport books |
| [src-tauri/src/db/game_state_commands.rs](../../src-tauri/src/db/game_state_commands.rs) | `get_teleportation_binds`, `set_mushroom_circles` commands |

## Zone Connectivity Graph

The CDN `areas.json` only provides zone names — no connectivity. The zone adjacency graph is **hardcoded** in `zone_graph.rs` as a static edge list, sourced from:
- https://wiki.projectgorgon.com/wiki/Zones
- https://wiki.projectgorgon.com/wiki/Dungeons

### Overworld Zones (15 nodes)

```
Anagoge Island ─── Serbule ─── Serbule Hills
                      │
                   Eltibule ─── Red Wing Casino ─┬─ Rahu
                      │                          └─ Statehelm (moonphase-dependent)
                 Kur Mountains ──── Ilmari ──────── Rahu
                   │       │
                Gazluk   Sun Vale ─── Winter Nexus ─── Fae Realm
                   │
                 Povus ─── Rahu
                   │
                Vidaria ─── Statehelm
```

The graph is defined in `OVERWORLD_EDGES` — each entry is a bidirectional `(zone_a, zone_b)` tuple using CDN area key constants (e.g., `AREA_SERBULE = "AreaSerbule"`).

### Sub-Zone Resolution

Dungeons and caves are mapped to their parent overworld zone via `SUBZONE_PARENTS`. When a player is in "Caves Beneath Gazluk" (`AreaGazlukCaves`), the router treats them as being in Gazluk (`AreaGazluk`).

This means routing doesn't care about dungeon-to-dungeon paths — it only routes between the ~15 overworld zones.

### Casino Portal (Moonphase-Dependent)

The Red Wing Casino has two portal exits. Which one is active depends on the moon phase:

| Moon Phase | Portal Destination |
|-----------|-------------------|
| New Moon, Quarter Moon, Full Moon, Last Quarter | Rahu |
| Waxing Crescent, Waning Crescent, Waxing Gibbous, Waning Gibbous | Statehelm |

Both edges (Casino↔Rahu and Casino↔Statehelm) are in the static graph. `plan_trip` filters edges by the current moon phase via `TravelConfig.casino_portal` (matched case-insensitively; anything unrecognized keeps both edges, optimistic): `"rahu"` excludes the Casino↔Statehelm edge, `"statehelm"` excludes Casino↔Rahu. The Trip Planner widget maps `get_current_moon_phase` to the portal destination using the same phase table as `ContextBar.vue`, and the graph is built with `ZoneGraph::new_excluding()` to remove the inactive edge before BFS runs.

### Friendly Name Resolution

Bind locations from the game use friendly names ("Red Wing Casino", "Caves Beneath Gazluk", "Dungeons Beneath Eltibule") while the graph uses CDN area keys. `ZoneGraph::resolve_friendly_name()` handles the mapping. Supported names include all overworld zones (including Phantom Ilmari Desert / `AreaDesert2`) plus common sub-zone names that appear in teleportation status reports.

Unrecognized names in `TravelConfig` are kept as-is and logged (`eprintln!`) at resolution; the solver then fails the plan with an "Unknown zone" error naming the zone rather than silently dropping the stop.
## How to Modify the Zone Graph

### Adding a new zone

1. Add a CDN key constant in `zone_graph.rs`:
   ```rust
   pub const AREA_NEW_ZONE: &str = "AreaNewZone";
   ```
2. Add edges to `OVERWORLD_EDGES`:
   ```rust
   (AREA_EXISTING_ZONE, AREA_NEW_ZONE),
   ```
3. Add a friendly name mapping in `resolve_friendly_name()`:
   ```rust
   "New Zone" | "New Zone Name" => AREA_NEW_ZONE,
   ```
4. Add a display name in `trip_router.rs` `friendly_zone_name()`:
   ```rust
   "AreaNewZone" => "New Zone",
   ```
5. Update the `test_graph_builds` assertion for the new zone count.

### Adding a new dungeon / sub-zone

Add an entry to `SUBZONE_PARENTS`:
```rust
(AREA_NEW_DUNGEON, AREA_PARENT_ZONE),
```

No edge needed — dungeons route through their parent.

### Changing an edge

Edit or remove entries in `OVERWORLD_EDGES`. All edges are bidirectional. One-way edges would require changes to `ZoneGraph::new()` (currently adds both directions).

## Travel Methods

The solver evaluates four travel methods for each zone-to-zone transition and picks the cheapest:

| Method | Cost (hops) | When available |
|--------|-------------|----------------|
| **Walk** | BFS distance (1+ hops) | Always |
| **Bind recall** | 1 | Player has a bind pad at the destination zone |
| **Mushroom circle** | 1 | Player has an attuned circle at the destination zone |
| **TP machine** | 2 | Player has a bind at Gazluk Caves + `use_tp_machine` enabled |

Selection logic is in `best_travel_method()`. Walking is only used when it's cheaper than any available teleport option.

### Teleportation Data Sources

**Bind pad locations** are auto-parsed from Player.log when the player opens their Teleportation skill info page in-game:

```
ProcessBook("Skill Info", "Teleportation Status:\n\n\n
Primary Bind Location: Red Wing Casino\n
Secondary Bind Location: Caves Beneath Gazluk\n...", "SkillReport", ...)
```

The coordinator intercepts this `BookOpened` event, extracts "Primary Bind Location" and "Secondary Bind Location", and persists them to the `game_state_teleportation` table.

**Mushroom circle attunements** cannot be parsed from logs. They must be manually configured by the user via the `set_mushroom_circles` command. The table supports two circle slots.

**TP machine access** is inferred: if either bind location resolves to `AreaGazlukCaves` (Caves Beneath Gazluk), the player has TP machine access. The `use_tp_machine` flag in `TravelConfig` lets the user/feature toggle this.

## Solver Algorithm

1. **Resolve zones** — all stop zones and the start zone are resolved to overworld parents via `resolve_overworld()`.
2. **Group stops by zone** — stops in the same overworld zone are bucketed together.
3. **Greedy nearest-neighbor** — from the current zone, pick the unvisited zone group with the lowest effective travel cost (considering teleports). Repeat until all groups are ordered.
4. **Within-zone ordering** — stops in the same zone are sorted by purpose priority: Pickup (0) → VendorBuy (1) → Craft (2) → TurnIn (3) → Deposit (4).
5. **Travel step insertion** — between zone transitions, insert the appropriate travel step(s) based on the selected method (one step for walk/recall, two steps for TP machine).

The greedy algorithm is not globally optimal (it doesn't look ahead), but for 3-8 stops across a 15-node graph it produces good-enough results.

## Tauri Commands

### `plan_trip`

Plan a multi-zone route.

```typescript
invoke('plan_trip', {
  startZone: 'AreaSerbule',         // CDN area key or friendly name
  stops: [
    { zone: 'AreaRahu', purpose: 'pickup', details: 'Get Iron Filament x20 from vault' },
    { zone: 'AreaEltibule', purpose: 'craft', details: 'Craft Iron Sword x5' },
    { zone: 'AreaRahu', purpose: 'turn_in', details: 'Turn in WO to Himhi' },
  ],
  travelConfig: {                    // optional — omit for walking-only
    primaryBind: 'Red Wing Casino',  // friendly name or CDN key
    secondaryBind: 'Caves Beneath Gazluk',
    mushroomCircle1: null,
    mushroomCircle2: null,
    useTpMachine: true,
  }
})
```

**Response:**
```typescript
{
  steps: [
    { zone: 'AreaEltibule', action: 'travel', details: 'Walk to Eltibule' },
    { zone: 'AreaEltibule', action: 'craft', details: 'Craft Iron Sword x5' },
    { zone: 'AreaCasino', action: 'travel', details: 'Walk to Red Wing Casino' },
    { zone: 'AreaRahu', action: 'travel', details: 'Walk to Rahu' },
    { zone: 'AreaRahu', action: 'pickup', details: 'Get Iron Filament x20 from vault' },
    { zone: 'AreaRahu', action: 'turn_in', details: 'Turn in WO to Himhi' },
  ],
  totalHops: 3
}
```

### `get_teleportation_binds`

Query the player's known bind locations.

```typescript
invoke('get_teleportation_binds', { character: 'MyChar', server: 'Dreva' })
// → { primaryBind, secondaryBind, mushroomCircle1, mushroomCircle2, lastUpdated }
```

### `set_mushroom_circles`

Manually set mushroom circle attunements (can't be auto-detected).

```typescript
invoke('set_mushroom_circles', {
  character: 'MyChar',
  server: 'Dreva',
  circle1: 'Serbule',
  circle2: 'Rahu'
})
```

## Database

### `game_state_teleportation` (migration v39)

| Column | Type | Description |
|--------|------|-------------|
| `character_name` | TEXT | PK (with server_name) |
| `server_name` | TEXT | PK (with character_name) |
| `primary_bind` | TEXT | Friendly name of primary bind location |
| `secondary_bind` | TEXT | Friendly name of secondary bind location |
| `mushroom_circle_1` | TEXT | Manually configured circle zone |
| `mushroom_circle_2` | TEXT | Manually configured circle zone |
| `last_updated` | TEXT | ISO 8601 timestamp |

Bind locations are upserted when the coordinator sees a Teleportation SkillReport. Mushroom circles are only written via the manual `set_mushroom_circles` command.

## Tests

28 tests across the two modules:

**zone_graph (13 tests):** graph construction, zone count, self-distance, adjacent distance, multi-hop distance, sub-zone resolution, sub-zone distance, shortest path, unknown zones, symmetry, full reachability, neighbors, casino↔statehelm edge, friendly name resolution.

**trip_router (15 tests):** empty stops, single stop same/different zone, within-zone ordering, multi-zone route, sub-zone stops, unknown zones, greedy nearest-neighbor, bind recall shortcut, mushroom circle recall, TP machine shortcut, TP machine not used when walking is shorter, recall reduces total hops, friendly name resolution in config.

## Future Work

### Phase 3: Feature Integration

Wire the solver into consumer features that generate stop lists:

- **Work order fulfillment** — stops for vault pickups, vendor buys, craft stations, NPC turn-ins. Requires eligibility filtering (recipe known? skill high enough? materials available?) before generating stops.
- **Crafting project pickup lists** — route-order the existing pickup list output by zone.
- **Delivery quest routing** — pickup + delivery NPC as two stops.
- **Inventory duplicate consolidation** — vaults with duplicate items + target vault.

### Crafting Station Awareness

Routing needs to know which zones have which crafting stations to direct "craft X" stops correctly. The CDN has **no structured station field** on recipes — requirements appear only in `Description` text using two patterns:
- `"Requires a <station>"` — mostly Cooking recipes
- `"Must be near a <station>"` — Armorsmithing, Leatherworking, etc.

**Stations explicitly mentioned in CDN descriptions (11 types):**

| Station | Skills Using It |
|---------|----------------|
| Forge | Armorsmithing, Blacksmithing, Toolcrafting |
| Forge consecrated to Norala | Armorsmithing |
| Forge consecrated to Tast | Armorsmithing |
| Forge consecrated to Umrad | Armorsmithing |
| Forge or kiln | Artistry, Glassblowing |
| Stove or fire pit | Cooking, Cheesemaking, Butchering, FireMagic, IceMagic, WeatherWitching |
| Tofu press | Cheesemaking |
| Tanning rack | Leatherworking, Tanning, Tailoring, Toolcrafting |
| Cotton gin | Textiles |
| Tack bench | Saddlery |
| Blood well | Vampirism |
| Teleportation platform | Teleportation |

**Skills with implicit station requirements (not mentioned in CDN descriptions):**

| Skill | Recipes | Likely Station | Notes |
|-------|---------|---------------|-------|
| Bladesmithing | 130 | Forge | Almost certainly needs a forge |
| CandleMaking | 78 | Candle making station | Confirmed by player |
| Carpentry | 202 | Workbench? | Needs verification |
| Brewing | 52 | Brewing equipment | Needs verification |
| Alchemy | 203 | Alchemy table? | Needs verification |
| SushiPreparation | 25 | Stove/fire pit? | Needs verification |

**Implementation strategy — hybrid approach, parse what we can, hardcode the rest:**

1. **At CDN import time:** Regex-extract station from recipe descriptions. Two patterns cover all cases:
   - `/[Rr]equires a ([^.)]+)/`
   - `/[Mm]ust be near a ([^.)]+)/`
   Store as a `station_required` field on the recipe record.

2. **Fallback skill→station table:** Small static map for skills where CDN descriptions are silent. Maintained manually, rarely changes.

3. **Zone→station lookup:** Which zones have which stations. This is a small static table — most zones have stoves, but specialized stations (consecrated forges, blood wells, tofu presses) are in specific locations. Needs to be manually curated from wiki/player knowledge.

### Edge Cost Weighting

Currently all zone transitions cost 1 hop. Some transitions are much longer in-game (Gazluk → Povus is a long run vs Serbule → Serbule Hills which is short). The solver structure supports weighted costs but they all default to 1.

### ~~Moonphase-Filtered Casino Edge~~ (Implemented)

The `casino_portal` field in `TravelConfig` controls which Casino edge is active. When set to `"rahu"`, the Casino↔Statehelm edge is excluded from the graph (and vice versa). The Trip Planner widget fetches the current moon phase via `get_current_moon_phase` and maps it to the portal destination using the same phase table as `ContextBar.vue`. The graph is built with `ZoneGraph::new_excluding()` to remove the inactive edge before BFS runs.

### Within-Zone Routing

Currently the solver treats a zone as a single point — "you're in Serbule, do these 4 things." If full map coordinate data becomes available in the future, we could order within-zone stops by NPC-to-NPC proximity. Not worth modeling until we have map data.

### Mushroom Circle Auto-Detection

Circle attunements can't currently be parsed from logs. If a parseable source is found in the future, the coordinator can auto-populate the `mushroom_circle_*` fields. Until then, users configure circles manually via `set_mushroom_circles`.

### Other Travel Methods Not Yet Modeled

- **Gazluk one-way teleport to Fae Realm** — niche, most players don't use it.
- **Skill-based teleports** (Druid grove recall, etc.) — too niche for the general solver.
- **NPC transport** (boatman in Serbule to Sun Vale/Anagoge) — already modeled as normal edges in the zone graph, so they work automatically.
