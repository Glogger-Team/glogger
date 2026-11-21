# Widget: Trip Planner

**ID:** `trip-planner` | **Default size:** Medium | **Config:** Yes | **Component:** `widgets/TripPlannerWidget.vue`

Interactive zone-to-zone route planner. Pick a start and end zone, optionally enable teleportation shortcuts, and get a step-by-step travel plan.

## Features

- **Zone dropdowns** — select from all 15 overworld zones for start and destination
- **"Current" button** — sets the start zone to the player's current zone (from live game state)
- **"Home" button** — sets the start zone to the player's configured home zone (quick shortcut next to "Current")
- **Teleport toggle** — enable/disable bind recall, mushroom circle, and TP machine shortcuts
- **TP Machine toggle** — separately enable TP machine routing (only visible when teleports are on)
- **Bind summary** — shows configured bind pads and mushroom circles at a glance
- **Route display** — ordered step list with hop count, travel steps styled differently from action steps

## Config Panel

Accessed via the gear icon on the widget card. Allows manual configuration of:

- **Primary Bind** — teleport pad bind location (dropdown of all zones + key sub-zones like Caves Beneath Gazluk)
- **Secondary Bind** — second pad bind (unlocked later in game)
- **Mushroom Circle 1** — attuned circle location (dropdown of zones known to have circles)
- **Mushroom Circle 2** — second circle attunement

- **Home Zone** — per-character home zone for quick start zone selection via the "Home" button

Bind pad locations are also auto-populated from the database when available — the coordinator parses them from the live JSON `Book` arm (and the Books-watcher backfill) when the player opens their Teleportation skill info in-game. Manual config takes priority over auto-detected values only if the manual value is already set; otherwise the DB value fills in the blanks.

Config is persisted to `localStorage` under key `tripPlannerWidget.config`.

## Data Sources

### Current Zone

Read from `gameStateStore.world.area?.area_name`. The game state area name is a friendly name (e.g., "Serbule") which is mapped to CDN area keys via a lookup table in the widget.

### Route Planning

Calls the `plan_trip` Tauri command (defined in `trip_router.rs`). The command builds a `ZoneGraph`, resolves friendly names in the travel config, and runs the greedy nearest-neighbor solver. See [trip-routing.md](../../trip-routing.md) for full solver documentation.

### Casino Portal (Moon Phase)

On mount, the widget fetches the current moon phase via `get_current_moon_phase` and maps it to the casino portal destination (Rahu or Statehelm) using the same phase table as the Status widget. This is passed as `casinoPortal` in every `plan_trip` call, so the solver excludes the unavailable Casino edge from the zone graph.

### Teleportation Binds

On mount, the widget calls `get_teleportation_binds` to load auto-detected bind locations from `game_state_teleportation`. These fill in any blanks in the localStorage config.

## Zone List

The widget maintains a static list of 15 overworld zones matching the zone graph in `zone_graph.rs`:

| CDN Key | Display Name |
|---------|-------------|
| AreaNewbieIsland | Anagoge Island |
| AreaSerbule | Serbule |
| AreaSerbule2 | Serbule Hills |
| AreaEltibule | Eltibule |
| AreaSunVale | Sun Vale |
| AreaKurMountains | Kur Mountains |
| AreaCasino | Red Wing Casino |
| AreaDesert1 | Ilmari |
| AreaRahu | Rahu |
| AreaGazluk | Gazluk |
| AreaFaeRealm1 | Fae Realm |
| AreaPovus | Povus |
| AreaVidaria | Vidaria |
| AreaStatehelm | Statehelm |
| AreaPlanes | Winter Nexus |

The config panel's bind pad dropdown also includes sub-zones that appear as bind locations (Caves Beneath Gazluk, Caves Under Serbule, Rahu Sewers, Statehelm Undercity). The mushroom circle dropdown is limited to zones known to have circles.

**When a new zone is added to the game**, update the `ZONES` array in the widget, the `ALL_LOCATIONS`/`CIRCLE_LOCATIONS` arrays in the config component, and the zone graph in `zone_graph.rs`.
