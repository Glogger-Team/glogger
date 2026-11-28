//! Trip routing solver for multi-zone travel planning.
//!
//! Given a set of stops (zone + purpose), produces an ordered trip plan.
//! Phase 1: walking only (zone graph BFS). Phase 2 will add teleportation.
//!
//! Multiple features feed stops into the solver: work order fulfillment,
//! crafting project pickup lists, delivery quests, inventory consolidation.

use crate::cdn_commands::GameDataState;
use crate::game_data::GameData;
use crate::zone_graph::ZoneGraph;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tauri::State;

// ── Input types ─────────────────────────────────────────────────────────────

/// A stop the player needs to make during a trip.
#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct RouteStop {
    /// Area CDN key (e.g. "AreaSerbule") or sub-zone key.
    pub zone: String,
    /// What the player needs to do here.
    pub purpose: StopPurpose,
    /// Human-readable description (e.g. "Pick up Iron Filament x20 from vault").
    pub details: String,
}

/// The type of action at a stop, used for within-zone ordering (see `rank`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StopPurpose {
    /// Pick up items from storage vault — do first.
    Pickup = 0,
    /// Buy items from an NPC vendor.
    VendorBuy = 1,
    /// Craft items at a station.
    Craft = 2,
    /// Turn in items to an NPC (work order, quest).
    TurnIn = 3,
    /// Deposit items into storage vault — do last.
    Deposit = 4,
    /// Gather items from the world (forage, harvest) — like Craft, before TurnIn.
    ///
    /// Appended after Deposit (not sorted between Craft and TurnIn) so adding
    /// it renumbers nothing: execution order comes from `rank`, never from
    /// declaration or discriminant order.
    Gather = 5,
}

impl StopPurpose {
    /// Within-zone execution order: pickup → vendor → craft → gather →
    /// turn-in → deposit. Separate from the discriminant order so a new
    /// variant never renumbers (and thereby never fake-removes) an existing
    /// one in doc-orphan scans.
    fn rank(self) -> u8 {
        match self {
            StopPurpose::Pickup => 0,
            StopPurpose::VendorBuy => 1,
            StopPurpose::Craft => 2,
            StopPurpose::Gather => 3,
            StopPurpose::TurnIn => 4,
            StopPurpose::Deposit => 5,
        }
    }
}

/// Configuration for available travel methods.
#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TravelConfig {
    /// Primary bind pad location (friendly name, e.g. "Red Wing Casino").
    pub primary_bind: Option<String>,
    /// Secondary bind pad location (unlocked later in game).
    pub secondary_bind: Option<String>,
    /// First mushroom circle attunement (zone name or CDN key).
    pub mushroom_circle_1: Option<String>,
    /// Second mushroom circle attunement (unlocked later in game).
    pub mushroom_circle_2: Option<String>,
    /// Whether to consider TP machine shortcuts (requires a bind at Gazluk Caves).
    pub use_tp_machine: bool,
    /// Current casino portal destination: "rahu" or "statehelm".
    /// When set, the solver only uses the matching Casino edge.
    /// When absent, both edges are available (optimistic).
    pub casino_portal: Option<String>,
}

// ── Output types ────────────────────────────────────────────────────────────

/// A planned trip with ordered steps.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PlannedRoute {
    pub steps: Vec<RouteStep>,
    pub total_hops: u32,
}

/// A single step in a planned route.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RouteStep {
    /// Overworld zone CDN key.
    pub zone: String,
    /// What to do at this step.
    pub action: RouteAction,
    /// Human-readable description.
    pub details: String,
}

/// The action type for a route step.
#[derive(Debug, Clone, Serialize, PartialEq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RouteAction {
    /// Travel to a zone (walking).
    Travel,
    /// Pick up items from vault.
    Pickup,
    /// Buy from vendor.
    VendorBuy,
    /// Craft items.
    Craft,
    /// Gather items from the world.
    Gather,
    /// Turn in items.
    TurnIn,
    /// Deposit items.
    Deposit,
}

// ── Travel method resolution ─────────────────────────────────────────────────

/// How to get from one zone to another.
#[derive(Debug, Clone)]
enum TravelMethod {
    /// Walk through the zone graph (may be multiple hops).
    Walk { hops: u32 },
    /// Use primary or secondary bind recall (instant, 1 effective hop).
    BindRecall { bind_name: String },
    /// Use mushroom circle recall (instant, 1 effective hop).
    MushroomRecall { circle_name: String },
    /// Use the TP machine: recall to Gazluk Caves, then TP out (2 effective hops).
    TpMachine,
}

impl TravelMethod {
    fn cost(&self) -> u32 {
        match self {
            TravelMethod::Walk { hops } => *hops,
            TravelMethod::BindRecall { .. } => 1,
            TravelMethod::MushroomRecall { .. } => 1,
            TravelMethod::TpMachine => 2,
        }
    }
}

/// The TP machine zone — "Caves Beneath Gazluk".
const TP_MACHINE_ZONE: &str = "AreaGazlukCaves";

/// Find the best travel method from `from` to `to` given the config.
fn best_travel_method(
    graph: &ZoneGraph,
    from: &str,
    to: &str,
    config: &TravelConfig,
) -> TravelMethod {
    let walk_dist = graph.distance(from, to).unwrap_or(u32::MAX);
    let mut best = TravelMethod::Walk { hops: walk_dist };

    // Check bind recalls — if a bind is in the destination zone, cost is 1.
    let to_resolved = graph.resolve_overworld(to).unwrap_or(to);
    for bind in [&config.primary_bind, &config.secondary_bind]
        .iter()
        .copied()
        .flatten()
    {
        if let Some(bind_zone) = graph.resolve_overworld(bind)
            && bind_zone == to_resolved && best.cost() > 1 {
                best = TravelMethod::BindRecall {
                    bind_name: bind.clone(),
                };
            }
    }

    // Check mushroom circles.
    for circle in [&config.mushroom_circle_1, &config.mushroom_circle_2]
        .iter()
        .copied()
        .flatten()
    {
        if let Some(circle_zone) = graph.resolve_overworld(circle)
            && circle_zone == to_resolved && best.cost() > 1 {
                best = TravelMethod::MushroomRecall {
                    circle_name: circle.clone(),
                };
            }
    }

    // Check TP machine — if the player has a bind at Gazluk Caves, any zone
    // is reachable in 2 hops (recall to caves + teleport out).
    if config.use_tp_machine && best.cost() > 2 {
        let has_tp_bind = [&config.primary_bind, &config.secondary_bind]
            .iter()
            .copied()
            .flatten()
            .any(|b| {
                graph
                    .resolve_overworld(b)
                    .map(|z| z == graph.resolve_overworld(TP_MACHINE_ZONE).unwrap_or(""))
                    .unwrap_or(false)
            });
        if has_tp_bind {
            best = TravelMethod::TpMachine;
        }
    }

    best
}

// ── Solver ───────────────────────────────────────────────────────────────────

/// Plan a route through the given stops, starting from `start_zone`.
///
/// Algorithm:
/// 1. Resolve all stop zones to overworld parents.
/// 2. Group stops by resolved zone.
/// 3. Greedy nearest-neighbor ordering using effective distance (considers recalls/TP).
///    Unreachable groups fail the plan with an error (never silently dropped).
/// 4. Within a zone, order stops by purpose (pickup → vendor → craft → gather → turn-in → deposit).
/// 5. Insert travel steps between zone transitions, picking the best method.
///    Unreachable legs fail the plan before any hop is counted.
pub fn plan_route(
    graph: &ZoneGraph,
    start_zone: &str,
    stops: &[RouteStop],
    travel_config: &TravelConfig,
) -> Result<PlannedRoute, String> {
    if stops.is_empty() {
        return Ok(PlannedRoute {
            steps: Vec::new(),
            total_hops: 0,
        });
    }

    // Resolve start zone.
    let start_resolved = graph
        .resolve_overworld(start_zone)
        .ok_or_else(|| format!("Unknown start zone: {}", start_zone))?;

    // Group stops by resolved overworld zone.
    let mut zone_groups: Vec<(String, Vec<&RouteStop>)> = Vec::new();
    let mut zone_group_map: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for stop in stops {
        let resolved = graph
            .resolve_overworld(&stop.zone)
            .ok_or_else(|| format!("Unknown zone in stop: {}", stop.zone))?;
        let resolved_str = resolved.to_string();

        if let Some(&idx) = zone_group_map.get(&resolved_str) {
            zone_groups[idx].1.push(stop);
        } else {
            let idx = zone_groups.len();
            zone_group_map.insert(resolved_str.clone(), idx);
            zone_groups.push((resolved_str, vec![stop]));
        }
    }

    // Greedy nearest-neighbor ordering using effective distance.
    let mut ordered_zones: Vec<(String, Vec<&RouteStop>)> = Vec::with_capacity(zone_groups.len());
    let mut remaining: Vec<bool> = vec![true; zone_groups.len()];
    let mut current_zone = start_resolved.to_string();

    for _ in 0..zone_groups.len() {
        let mut best_idx = None;
        let mut best_cost = u32::MAX;

        for (i, (zone, _)) in zone_groups.iter().enumerate() {
            if !remaining[i] {
                continue;
            }
            let method = best_travel_method(graph, &current_zone, zone, travel_config);
            let cost = method.cost();
            // Walk legs to unreachable zones report u32::MAX; those must not
            // win the greedy pick (and must not silently satisfy the loop).
            // Reachable legs always cost < MAX (walk hops, 1-hop recalls,
            // 2-hop TP), so `<` plus the reachability gate below keeps prior
            // behavior for all reachable graphs.
            if cost < best_cost && cost != u32::MAX {
                best_cost = cost;
                best_idx = Some(i);
            }
        }

        match best_idx {
            Some(idx) => {
                remaining[idx] = false;
                current_zone = zone_groups[idx].0.clone();
                ordered_zones.push(zone_groups[idx].clone());
            }
            // No remaining group is reachable from the current zone: stop so
            // the length check below reports which zone stranded the route
            // instead of silently returning a partial plan.
            None => break,
        }
    }

    if ordered_zones.len() != zone_groups.len() {
        let missing: Vec<&str> = zone_groups
            .iter()
            .enumerate()
            .filter(|(i, _)| remaining[*i])
            .map(|(_, (zone, _))| zone.as_str())
            .collect();
        return Err(format!(
            "No route to zone group(s): {} ({} of {} groups ordered)",
            missing.join(", "),
            ordered_zones.len(),
            zone_groups.len()
        ));
    }

    // Build the step list.
    let mut steps = Vec::new();
    let mut total_hops: u32 = 0;
    let mut current = start_resolved.to_string();

    for (zone, mut zone_stops) in ordered_zones {
        // Insert travel steps if we need to move zones.
        if zone != current {
            let method = best_travel_method(graph, &current, &zone, travel_config);
            // Reachability gate first: an unreachable leg must fail the
            // plan rather than add u32::MAX to the hop total and emit a
            // truncated step list. Recall/TP methods are reachable by
            // construction (they only win when their bind resolves); Walk
            // legs prove reachability via shortest_path below.
            let cost = method.cost();
            if cost == u32::MAX {
                return Err(format!(
                    "No route from {} to {}",
                    friendly_zone_name(&current),
                    friendly_zone_name(&zone)
                ));
            }

            match method {
                TravelMethod::Walk { hops } => {
                    let Some(path) = graph.shortest_path(&current, &zone) else {
                        return Err(format!(
                            "No route from {} to {}",
                            friendly_zone_name(&current),
                            friendly_zone_name(&zone)
                        ));
                    };
                    total_hops += hops;
                    for &hop_zone in &path[1..] {
                        steps.push(RouteStep {
                            zone: hop_zone.to_string(),
                            action: RouteAction::Travel,
                            details: format!("Walk to {}", friendly_zone_name(hop_zone)),
                        });
                    }
                }
                TravelMethod::BindRecall { ref bind_name } => {
                    total_hops += cost;
                    steps.push(RouteStep {
                        zone: zone.clone(),
                        action: RouteAction::Travel,
                        details: format!(
                            "Bind recall to {} ({})",
                            friendly_zone_name(&zone),
                            bind_name
                        ),
                    });
                }
                TravelMethod::MushroomRecall { ref circle_name } => {
                    total_hops += cost;
                    steps.push(RouteStep {
                        zone: zone.clone(),
                        action: RouteAction::Travel,
                        details: format!(
                            "Mushroom circle recall to {} ({})",
                            friendly_zone_name(&zone),
                            circle_name
                        ),
                    });
                }
                TravelMethod::TpMachine => {
                    total_hops += cost;
                    steps.push(RouteStep {
                        zone: "AreaGazlukCaves".to_string(),
                        action: RouteAction::Travel,
                        details: "Bind recall to Caves Beneath Gazluk (TP Machine)".to_string(),
                    });
                    steps.push(RouteStep {
                        zone: zone.clone(),
                        action: RouteAction::Travel,
                        details: format!("Teleport machine to {}", friendly_zone_name(&zone)),
                    });
                }
            }
            current = zone.clone();
        }

        // Sort stops within zone by purpose.
        // Within-zone execution order comes from StopPurpose::rank, not from
        // declaration order (Gather is appended after Deposit).
        zone_stops.sort_by_key(|s| s.purpose.rank());

        // Add action steps.
        for stop in zone_stops {
            steps.push(RouteStep {
                zone: zone.clone(),
                action: purpose_to_action(stop.purpose),
                details: stop.details.clone(),
            });
        }
    }

    Ok(PlannedRoute { steps, total_hops })
}

fn purpose_to_action(purpose: StopPurpose) -> RouteAction {
    match purpose {
        StopPurpose::Pickup => RouteAction::Pickup,
        StopPurpose::VendorBuy => RouteAction::VendorBuy,
        StopPurpose::Craft => RouteAction::Craft,
        StopPurpose::Gather => RouteAction::Gather,
        StopPurpose::TurnIn => RouteAction::TurnIn,
        StopPurpose::Deposit => RouteAction::Deposit,
    }
}

/// Best-effort friendly name for a zone CDN key.
/// Falls back to the CDN key itself if we don't have a mapping.
fn friendly_zone_name(zone: &str) -> &str {
    match zone {
        "AreaNewbieIsland" => "Anagoge Island",
        "AreaSerbule" => "Serbule",
        "AreaSerbule2" => "Serbule Hills",
        "AreaEltibule" => "Eltibule",
        "AreaSunVale" => "Sun Vale",
        "AreaKurMountains" => "Kur Mountains",
        "AreaCasino" => "Red Wing Casino",
        "AreaDesert1" => "Ilmari",
        "AreaDesert2" => "Phantom Ilmari Desert",
        "AreaRahu" => "Rahu",
        "AreaGazluk" => "Gazluk",
        "AreaFaeRealm1" => "Fae Realm",
        "AreaPovus" => "Povus",
        "AreaVidaria" => "Vidaria",
        "AreaStatehelm" => "Statehelm",
        "AreaPlanes" => "Winter Nexus",
        other => other,
    }
}

// ── Tauri command ────────────────────────────────────────────────────────────

/// Plan a multi-zone trip given a set of stops.
///
/// Returns an ordered list of steps (travel + actions) starting from
/// `start_zone`. The solver uses greedy nearest-neighbor for zone ordering
/// and sorts within-zone stops by purpose priority.
///
/// Bind locations in `travel_config` can be either CDN area keys or
/// friendly names (e.g. "Red Wing Casino") — they're resolved automatically.
#[tauri::command]
pub fn plan_trip(
    start_zone: String,
    stops: Vec<RouteStop>,
    travel_config: Option<TravelConfig>,
) -> Result<PlannedRoute, String> {
    let config = travel_config.unwrap_or_default();

    // Build graph with the correct casino edge based on current moon phase.
    // The portal value is matched case-insensitively (the frontend may send
    // "Rahu", "RAHU", or "rahu") — anything else keeps both edges (optimistic).
    let portal = config.casino_portal.as_deref().unwrap_or("");
    let excluded: Vec<(&str, &str)> = if portal.eq_ignore_ascii_case("rahu") {
        vec![("AreaCasino", "AreaStatehelm")]
    } else if portal.eq_ignore_ascii_case("statehelm") {
        vec![("AreaCasino", "AreaRahu")]
    } else {
        vec![]
    };
    let graph = if excluded.is_empty() {
        ZoneGraph::new()
    } else {
        ZoneGraph::new_excluding(&excluded)
    };

    let config = resolve_travel_config(&graph, config);
    plan_route(&graph, &start_zone, &stops, &config)
}

/// Resolve friendly names in travel config to CDN area keys.
///
/// Unrecognized names are kept as-is (the solver later reports an "Unknown
/// zone" error naming the zone). Keeping the original preserves the user's
/// input for that error; an `eprintln!` is emitted here so the unresolved
/// name is visible in logs at the point of resolution rather than silently
/// passing through.
fn resolve_travel_config(graph: &ZoneGraph, mut config: TravelConfig) -> TravelConfig {
    let resolve = |name: &Option<String>| -> Option<String> {
        name.as_ref().map(|n| {
            graph
                .resolve_friendly_name(n)
                .map(|k| k.to_string())
                .unwrap_or_else(|| {
                    // Only log when it is not already a known CDN key — unknown
                    // keys fail loudly in plan_route; typos deserve a trace.
                    if graph.resolve_overworld(n).is_none() {
                        eprintln!("[trip_router] unresolved friendly zone name kept as-is: {n}");
                    }
                    n.clone()
                })
        })
    };
    config.primary_bind = resolve(&config.primary_bind);
    config.secondary_bind = resolve(&config.secondary_bind);
    config.mushroom_circle_1 = resolve(&config.mushroom_circle_1);
    config.mushroom_circle_2 = resolve(&config.mushroom_circle_2);
    config
}

// ── Work order trip planning ─────────────────────────────────────────────────

/// One work order to route: quest key + quantity.
///
/// Keys come from `get_work_orders_from_snapshot` (active keys) or the
/// uncompleted-work-orders backlog (`WorkOrderTodo` rows carry both fields).
/// Quantities are the quest's total Collect counts — no remaining-count data
/// exists, so partial progress is not modeled. A zero quantity falls back to
/// the quest's Collect objective number during stop construction.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkOrderTripItem {
    pub quest_key: String,
    pub quantity: u32,
}

/// Turn-in board NPC display name → overworld CDN area key.
///
/// Every `WorkOrder`-keyword quest delivers to one of nine fixed boards
/// (verified against `docs/CDN-full-examples/quests.json`). The primary
/// turn-in lookup is the CDN (`NpcInfo.area_name` via `GameData::resolve_npc`);
/// this table is the fallback for boards whose NPC is absent from the CDN
/// snapshot (Ogamboe had no entry in the November 2026 sample).
static BOARD_TURNIN_ZONES: &[(&str, &str)] = &[
    ("Irkima", "AreaCasino"),
    ("Sheyna", "AreaRahu"),
    ("Fitz the Boatman", "AreaSerbule"),
    ("Ogamboe", "AreaFaeRealm1"),
    ("Thimble Pete", "AreaEltibule"),
    ("Viedesi", "AreaSunVale"),
    ("Gremix", "AreaDesert1"),
    ("Laura Neth", "AreaKurMountains"),
    ("Korbok", "AreaVidaria"),
];

/// Station requirement patterns from `docs/features/trip-routing.md`:
/// `"Requires a <station>"` (mostly Cooking) and `"Must be near a <station>"`
/// (Armorsmithing, Leatherworking, …). Single `[^.)]+` captures, no nested
/// quantifiers — no catastrophic-backtracking shape.
static STATION_REQUIRES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[Rr]equires a ([^.)]+)").unwrap());
static STATION_NEAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[Mm]ust be near a ([^.)]+)").unwrap());

/// Extract the crafting-station requirement from a recipe description, if the
/// recipe states one. Trailing prose is trimmed at the first comma (`tanning
/// rack, for some reason` in the live CDN) — no real station name contains one.
fn station_from_description(desc: &str) -> Option<String> {
    STATION_REQUIRES_RE
        .captures(desc)
        .or_else(|| STATION_NEAR_RE.captures(desc))
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().split(',').next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The Scripted `Deliver to <NPC>` objective names the turn-in board NPC.
/// Mirrors `parseWorkOrderBoard` in `QuestsScreen.vue` — the two must agree
/// on which objective carries the NPC.
fn deliver_to_npc(raw: &serde_json::Value) -> Option<String> {
    const PREFIX: &str = "deliver to ";
    let objectives = raw.get("Objectives")?.as_array()?;
    for objective in objectives {
        if objective.get("Type").and_then(|t| t.as_str()) != Some("Scripted") {
            continue;
        }
        let desc = objective.get("Description").and_then(|d| d.as_str())?;
        let prefix = desc.get(..PREFIX.len())?;
        if !prefix.eq_ignore_ascii_case(PREFIX) {
            continue;
        }
        let npc = desc[PREFIX.len()..].trim();
        if !npc.is_empty() {
            return Some(npc.to_string());
        }
    }
    None
}

/// The Collect objective names the crafted item and how many the quest wants.
fn collect_objective(raw: &serde_json::Value) -> (Option<String>, u32) {
    let objectives = match raw.get("Objectives").and_then(|o| o.as_array()) {
        Some(objectives) => objectives,
        None => return (None, 0),
    };
    for objective in objectives {
        if objective.get("Type").and_then(|t| t.as_str()) != Some("Collect") {
            continue;
        }
        let item = objective
            .get("ItemName")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
        let number = objective
            .get("Number")
            .and_then(|n| n.as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0);
        return (item, number);
    }
    (None, 0)
}

/// Whether any known recipe produces the named item: the same
/// `recipes_producing_item` index `station_for_item` reads (no second index).
/// `false` covers unknown items too (no row, no producers) — the order still
/// plans a gather stop instead of dropping.
fn has_producing_recipe(game_data: &GameData, item_internal_name: &str) -> bool {
    game_data
        .item_by_internal_name(item_internal_name)
        .and_then(|item| game_data.recipes_producing_item.get(&item.id))
        .is_some_and(|ids| !ids.is_empty())
}

/// Station hint for the crafted item: the lowest-id recipe producing it that
/// states a station requirement (lowest id keeps the pick deterministic —
/// the producing index is built from hash iteration order). `None` means no
/// producing recipe states one, not "no station needed".
fn station_for_item(game_data: &GameData, item_internal_name: &str) -> Option<String> {
    let item = game_data.item_by_internal_name(item_internal_name)?;
    let producers = game_data.recipes_producing_item.get(&item.id)?;
    let mut recipe_ids: Vec<u32> = producers.clone();
    recipe_ids.sort_unstable();
    for id in recipe_ids {
        let recipe = match game_data.recipes.get(&id) {
            Some(recipe) => recipe,
            None => continue,
        };
        let desc = recipe
            .raw_json
            .get("Description")
            .and_then(|d| d.as_str())?;
        if let Some(station) = station_from_description(desc) {
            return Some(station);
        }
    }
    None
}

/// Resolve the turn-in board NPC to its overworld zone: CDN `area_name`
/// first (exact key/display-name, then a case-insensitive display-name scan
/// for CDN case drift), then the static nine-board fallback. `None` fails
/// the plan loudly at the call site — a work order never silently drops.
fn turn_in_zone(game_data: &GameData, npc_name: &str) -> Option<String> {
    if let Some(area) = game_data
        .resolve_npc(npc_name)
        .and_then(|npc| npc.area_name.clone())
    {
        return Some(area);
    }
    for npc in game_data.npcs.values() {
        if npc.name.eq_ignore_ascii_case(npc_name) {
            if let Some(area) = npc.area_name.clone() {
                return Some(area);
            }
            break;
        }
    }
    BOARD_TURNIN_ZONES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(npc_name))
        .map(|(_, zone)| zone.to_string())
}

/// Build `RouteStop`s for work orders: one production stop at the turn-in board
/// town plus one `TurnIn` stop per order. The production stop is `Craft` when a
/// known recipe produces the item, `Gather` when it does not (same
/// `recipes_producing_item` index `station_for_item` reads — no second index).
/// Boards sit in towns with stations, so crafting rides along in the turn-in
/// zone; the station the recipe needs, when the CDN states one, is named in
/// the Craft details. (Zone→station curation is `docs/features/trip-routing.md`
/// future work — this stays honest by naming the station instead of placing
/// it.) The solver groups same-zone stops and orders Craft/Gather before
/// TurnIn within a zone.
/// Failures name the quest: unknown quest key, missing Deliver-to objective,
/// or unresolvable turn-in zone all error — a work order never silently
/// drops off the trip.
pub fn build_work_order_stops(
    game_data: &GameData,
    orders: &[WorkOrderTripItem],
) -> Result<Vec<RouteStop>, String> {
    let mut stops = Vec::with_capacity(orders.len() * 2);
    for order in orders {
        let quest = game_data.resolve_quest(&order.quest_key).ok_or_else(|| {
            format!("Unknown work order quest: {}", order.quest_key)
        })?;
        let raw = &quest.raw;
        let quest_name = raw
            .get("Name")
            .and_then(|n| n.as_str())
            .unwrap_or(&order.quest_key);
        let skill = raw
            .get("WorkOrderSkill")
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown");
        let (item_internal, collect_qty) = collect_objective(raw);
        let qty = if order.quantity > 0 {
            order.quantity
        } else {
            collect_qty
        };
        let item_label = item_internal
            .as_deref()
            .and_then(|internal| game_data.item_by_internal_name(internal))
            .map(|item| item.name.clone())
            .or(item_internal.clone())
            .unwrap_or_else(|| quest_name.to_string());
        let npc = deliver_to_npc(raw).ok_or_else(|| {
            format!(
                "Work order '{}' has no Deliver-to objective",
                order.quest_key
            )
        })?;
        let zone = turn_in_zone(game_data, &npc).ok_or_else(|| {
            format!(
                "Work order '{}': turn-in NPC '{}' has no known zone",
                order.quest_key, npc
            )
        })?;
        let has_producers = item_internal
            .as_deref()
            .is_some_and(|internal| has_producing_recipe(game_data, internal));
        if has_producers {
            let station = item_internal
                .as_deref()
                .and_then(|internal| station_for_item(game_data, internal));
            let mut craft_details =
                format!("Craft {qty}x {item_label} (total) ({skill}) for '{quest_name}'");
            if let Some(station) = station {
                craft_details.push_str(&format!(" [needs {station}]"));
            }
            stops.push(RouteStop {
                zone: zone.clone(),
                purpose: StopPurpose::Craft,
                details: craft_details,
            });
        } else {
            stops.push(RouteStop {
                zone: zone.clone(),
                purpose: StopPurpose::Gather,
                details: format!("Gather {qty}x {item_label} (total) ({skill}) for '{quest_name}'"),
            });
        }
        stops.push(RouteStop {
            zone,
            purpose: StopPurpose::TurnIn,
            details: format!("Turn in {qty}x {item_label} (total) to {npc} - '{quest_name}'"),
        });
    }
    Ok(stops)
}

/// Plan a multi-zone trip to complete work orders (craft or gather, then turn in).
/// Resolves each order's turn-in zone from CDN quest/NPC data (static
/// nine-board fallback), emits production + TurnIn stops, and runs the existing
/// `plan_route` solver unchanged. Graph construction mirrors `plan_trip`
/// (duplicated intentionally — this slice leaves `plan_trip` untouched).
#[tauri::command]
pub async fn plan_work_order_trip(
    game_data: State<'_, GameDataState>,
    start_zone: String,
    orders: Vec<WorkOrderTripItem>,
    travel_config: Option<TravelConfig>,
) -> Result<PlannedRoute, String> {
    // Scoped read: pure in-memory stop construction under the CDN lock, no I/O.
    let stops = {
        let data = game_data.read().await;
        build_work_order_stops(&data, &orders)?
    };
    let config = travel_config.unwrap_or_default();

    let portal = config.casino_portal.as_deref().unwrap_or("");
    let excluded: Vec<(&str, &str)> = if portal.eq_ignore_ascii_case("rahu") {
        vec![("AreaCasino", "AreaStatehelm")]
    } else if portal.eq_ignore_ascii_case("statehelm") {
        vec![("AreaCasino", "AreaRahu")]
    } else {
        vec![]
    };
    let graph = if excluded.is_empty() {
        ZoneGraph::new()
    } else {
        ZoneGraph::new_excluding(&excluded)
    };

    let config = resolve_travel_config(&graph, config);
    plan_route(&graph, &start_zone, &stops, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph() -> ZoneGraph {
        ZoneGraph::new()
    }

    fn stop(zone: &str, purpose: StopPurpose, details: &str) -> RouteStop {
        RouteStop {
            zone: zone.to_string(),
            purpose,
            details: details.to_string(),
        }
    }

    #[test]
    fn test_empty_stops() {
        let g = make_graph();
        let route = plan_route(&g, "AreaSerbule", &[], &TravelConfig::default()).unwrap();
        assert!(route.steps.is_empty());
        assert_eq!(route.total_hops, 0);
    }

    #[test]
    fn test_single_stop_same_zone() {
        let g = make_graph();
        let stops = vec![stop(
            "AreaSerbule",
            StopPurpose::Pickup,
            "Get iron from vault",
        )];
        let route = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();
        assert_eq!(route.total_hops, 0);
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].action, RouteAction::Pickup);
    }

    #[test]
    fn test_single_stop_different_zone() {
        let g = make_graph();
        let stops = vec![stop("AreaEltibule", StopPurpose::Craft, "Craft sword")];
        let route = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();
        // Should have: Travel to Eltibule, then Craft
        assert_eq!(route.total_hops, 1);
        assert_eq!(route.steps.len(), 2);
        assert_eq!(route.steps[0].action, RouteAction::Travel);
        assert_eq!(route.steps[1].action, RouteAction::Craft);
    }

    #[test]
    fn test_within_zone_ordering() {
        let g = make_graph();
        // Add stops in wrong order — deposit before pickup
        let stops = vec![
            stop("AreaSerbule", StopPurpose::Deposit, "Store leftovers"),
            stop("AreaSerbule", StopPurpose::Craft, "Craft item"),
            stop("AreaSerbule", StopPurpose::Pickup, "Get materials"),
        ];
        let route = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();
        assert_eq!(route.total_hops, 0);
        assert_eq!(route.steps.len(), 3);
        // Should be ordered: Pickup, Craft, Deposit
        assert_eq!(route.steps[0].action, RouteAction::Pickup);
        assert_eq!(route.steps[1].action, RouteAction::Craft);
        assert_eq!(route.steps[2].action, RouteAction::Deposit);
    }

    #[test]
    fn test_multi_zone_route() {
        let g = make_graph();
        let stops = vec![
            stop("AreaRahu", StopPurpose::TurnIn, "Turn in work order"),
            stop("AreaSerbule", StopPurpose::Pickup, "Get materials"),
            stop("AreaEltibule", StopPurpose::Craft, "Craft item"),
        ];
        let route = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();

        // Greedy from Serbule: closest is Serbule (0 hops), then Eltibule (1 hop),
        // then Rahu (Eltibule → Casino → Rahu = 2 hops).
        // Total: 0 + 1 + 2 = 3 hops
        assert_eq!(route.total_hops, 3);

        // Verify action order: Pickup in Serbule, Travel Eltibule, Craft, Travel Casino, Travel Rahu, TurnIn
        let actions: Vec<_> = route.steps.iter().map(|s| &s.action).collect();
        assert_eq!(actions[0], &RouteAction::Pickup); // Serbule
        assert_eq!(actions[1], &RouteAction::Travel); // → Eltibule
        assert_eq!(actions[2], &RouteAction::Craft); // Eltibule
        assert_eq!(actions[3], &RouteAction::Travel); // → Casino
        assert_eq!(actions[4], &RouteAction::Travel); // → Rahu
        assert_eq!(actions[5], &RouteAction::TurnIn); // Rahu
    }

    #[test]
    fn test_subzone_stops_resolve() {
        let g = make_graph();
        // Stop in Gazluk Caves should resolve to Gazluk
        let stops = vec![stop(
            "AreaGazlukCaves",
            StopPurpose::Pickup,
            "Get stuff from Gazluk caves",
        )];
        let route = plan_route(&g, "AreaGazluk", &stops, &TravelConfig::default()).unwrap();
        // Same overworld zone, no travel needed
        assert_eq!(route.total_hops, 0);
        assert_eq!(route.steps.len(), 1);
    }

    #[test]
    fn test_unknown_zone_errors() {
        let g = make_graph();
        let stops = vec![stop("AreaNonexistent", StopPurpose::Pickup, "bad zone")];
        let result = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_start_zone_errors() {
        let g = make_graph();
        let stops = vec![stop("AreaSerbule", StopPurpose::Pickup, "stuff")];
        let result = plan_route(&g, "AreaBogus", &stops, &TravelConfig::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_greedy_picks_nearest() {
        let g = make_graph();
        // From Serbule, with stops in Eltibule (1 hop) and Rahu (3 hops),
        // greedy should pick Eltibule first.
        let stops = vec![
            stop("AreaRahu", StopPurpose::TurnIn, "Rahu stop"),
            stop("AreaEltibule", StopPurpose::Pickup, "Eltibule stop"),
        ];
        let route = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();

        // Find the first non-travel action — should be in Eltibule
        let first_action = route
            .steps
            .iter()
            .find(|s| s.action != RouteAction::Travel)
            .unwrap();
        assert_eq!(first_action.zone, "AreaEltibule");
    }

    // ── Phase 2: teleport-aware tests ───────────────────────────────────────

    #[test]
    fn test_bind_recall_shortcut() {
        let g = make_graph();
        // Player is in Serbule, needs to go to Rahu (normally 3 hops).
        // But they have a bind at Rahu — should use recall (1 hop).
        let stops = vec![stop("AreaRahu", StopPurpose::TurnIn, "Turn in WO")];
        let config = TravelConfig {
            primary_bind: Some("AreaRahu".to_string()),
            ..Default::default()
        };
        let route = plan_route(&g, "AreaSerbule", &stops, &config).unwrap();

        assert_eq!(route.total_hops, 1);
        // Should be: recall to Rahu, then TurnIn
        assert_eq!(route.steps.len(), 2);
        assert!(route.steps[0].details.contains("Bind recall"));
    }

    #[test]
    fn test_mushroom_circle_recall() {
        let g = make_graph();
        // Player is in Serbule, needs to go to Fae Realm (4+ hops walking).
        // But they have a mushroom circle attuned there — should use recall.
        let stops = vec![stop("AreaFaeRealm1", StopPurpose::Pickup, "Get stuff")];
        let config = TravelConfig {
            mushroom_circle_1: Some("AreaFaeRealm1".to_string()),
            ..Default::default()
        };
        let route = plan_route(&g, "AreaSerbule", &stops, &config).unwrap();

        assert_eq!(route.total_hops, 1);
        assert!(route.steps[0].details.contains("Mushroom circle"));
    }

    #[test]
    fn test_tp_machine_shortcut() {
        let g = make_graph();
        // Player in Serbule, needs to go to Statehelm (far away).
        // Has bind at Gazluk Caves (TP machine) and use_tp_machine enabled.
        let stops = vec![stop("AreaStatehelm", StopPurpose::TurnIn, "Turn in")];
        let config = TravelConfig {
            secondary_bind: Some("AreaGazlukCaves".to_string()),
            use_tp_machine: true,
            ..Default::default()
        };
        let route = plan_route(&g, "AreaSerbule", &stops, &config).unwrap();

        assert_eq!(route.total_hops, 2);
        // First step should be recall to Gazluk Caves, second should be TP machine
        assert!(route.steps[0].details.contains("TP Machine"));
        assert!(route.steps[1].details.contains("Teleport machine"));
    }

    #[test]
    fn test_tp_machine_not_used_when_walking_is_shorter() {
        let g = make_graph();
        // Player in Serbule, needs to go to Eltibule (1 hop).
        // TP machine costs 2 hops. Walking should win.
        let stops = vec![stop("AreaEltibule", StopPurpose::Craft, "Craft sword")];
        let config = TravelConfig {
            primary_bind: Some("AreaGazlukCaves".to_string()),
            use_tp_machine: true,
            ..Default::default()
        };
        let route = plan_route(&g, "AreaSerbule", &stops, &config).unwrap();

        assert_eq!(route.total_hops, 1);
        assert!(route.steps[0].details.contains("Walk"));
    }

    #[test]
    fn test_recall_reduces_total_hops() {
        let g = make_graph();
        // From Serbule to Statehelm: walking is 3 hops (Serbule→Eltibule→Casino→Statehelm).
        // With a bind recall, it's 1 hop.
        let stops = vec![stop("AreaStatehelm", StopPurpose::TurnIn, "Statehelm stop")];

        // Without bind: 3 hops walking
        let route_no_bind =
            plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();
        assert_eq!(route_no_bind.total_hops, 3);

        // With bind: 1 hop recall
        let config = TravelConfig {
            primary_bind: Some("AreaStatehelm".to_string()),
            ..Default::default()
        };
        let route_with_bind = plan_route(&g, "AreaSerbule", &stops, &config).unwrap();
        assert_eq!(route_with_bind.total_hops, 1);
        assert!(route_with_bind.steps[0].details.contains("Bind recall"));
    }

    #[test]
    fn test_friendly_name_resolution_in_config() {
        let g = make_graph();
        // Test that resolve_travel_config handles friendly names
        let config = TravelConfig {
            primary_bind: Some("Red Wing Casino".to_string()),
            secondary_bind: Some("Caves Beneath Gazluk".to_string()),
            mushroom_circle_1: Some("Serbule".to_string()),
            ..Default::default()
        };
        let resolved = resolve_travel_config(&g, config);
        assert_eq!(resolved.primary_bind.as_deref(), Some("AreaCasino"));
        assert_eq!(resolved.secondary_bind.as_deref(), Some("AreaGazlukCaves"));
        assert_eq!(resolved.mushroom_circle_1.as_deref(), Some("AreaSerbule"));
    }

    #[test]
    fn test_casino_portal_case_insensitive() {
        // "RAHU" must behave like "rahu": Casino↔Statehelm excluded.
        for portal in ["rahu", "RAHU", "Rahu"] {
            let config = TravelConfig {
                casino_portal: Some(portal.to_string()),
                ..Default::default()
            };
            let stops = vec![stop("AreaRahu", StopPurpose::TurnIn, "Rahu stop")];
            let route = plan_trip("AreaSerbule".to_string(), stops, Some(config));
            assert!(route.is_ok(), "portal {portal} should plan");
        }
        // Unknown values keep both edges (optimistic) — still plans.
        let config = TravelConfig {
            casino_portal: Some("bogus".to_string()),
            ..Default::default()
        };
        assert!(plan_trip(
            "AreaSerbule".to_string(),
            vec![stop("AreaRahu", StopPurpose::TurnIn, "x")],
            Some(config)
        )
        .is_ok());
    }

    // ── Work order stop construction ──────────────────────────────────────
    //
    // Fixture quests mirror real CDN shapes (`docs/CDN-full-examples/quests.json`:
    // quest_50301 Carpentry/Fitz, quest_50054 gathering/Sheyna, quest_50020
    // gathering/mistletoe). NPC rows carry
    // the real CDN areas; Ogamboe is deliberately absent from the NPC map to
    // pin the static nine-board fallback.
    fn fixture_game_data() -> GameData {
        let mut data = GameData::empty();
        data.quests.insert(
            "quest_wo_fitz".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Carpentry_TestStools".to_string(),
                raw: serde_json::json!({
                    "Name": "Test Meditation Stools",
                    "WorkOrderSkill": "Carpentry",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "TestStool", "Number": 10, "Description": "Obtain Test Stools"},
                        {"Type": "Scripted", "Number": 1, "Description": "Deliver to Fitz the Boatman"}
                    ],
                }),
            },
        );
        data.quests.insert(
            "quest_wo_sheyna".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Unknown_ManticoreTailTips".to_string(),
                raw: serde_json::json!({
                    "Name": "Manticore Tail Tips",
                    "WorkOrderSkill": "Unknown",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "ManticoreTailTip", "Number": 10, "Description": "Obtain Manticore Tail Tips"},
                        {"Type": "Scripted", "Number": 1, "Description": "Deliver to Sheyna"}
                    ],
                }),
            },
        );
        data.quests.insert(
            "quest_wo_ogamboe".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Fae_TestOrder".to_string(),
                raw: serde_json::json!({
                    "Name": "Fae Test Order",
                    "WorkOrderSkill": "Textiles",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "FaeCloth", "Number": 5, "Description": "Obtain Fae Cloth"},
                        {"Type": "Scripted", "Number": 1, "Description": "Deliver to Ogamboe"}
                    ],
                }),
            },
        );
        data.npcs.insert(
            "NPC_Fitz".to_string(),
            crate::game_data::NpcInfo {
                key: "NPC_Fitz".to_string(),
                name: "Fitz the Boatman".to_string(),
                desc: None,
                area_name: Some("AreaSerbule".to_string()),
                area_friendly_name: Some("Serbule".to_string()),
                trains_skills: Vec::new(),
                preferences: Vec::new(),
                gift_favor_tiers: Vec::new(),
                pos: None,
                services: None,
                raw_json: serde_json::json!({}),
            },
        );
        data.npcs.insert(
            "NPC_Sheyna".to_string(),
            crate::game_data::NpcInfo {
                key: "NPC_Sheyna".to_string(),
                name: "Sheyna".to_string(),
                desc: None,
                area_name: Some("AreaRahu".to_string()),
                area_friendly_name: Some("Rahu".to_string()),
                trains_skills: Vec::new(),
                preferences: Vec::new(),
                gift_favor_tiers: Vec::new(),
                pos: None,
                services: None,
                raw_json: serde_json::json!({}),
            },
        );
        data.npc_name_index
            .insert("Fitz the Boatman".to_string(), "NPC_Fitz".to_string());
        data.npc_name_index
            .insert("Sheyna".to_string(), "NPC_Sheyna".to_string());
        data.quest_internal_name_index.insert(
            "Unknown_ManticoreTailTips".to_string(),
            "quest_wo_sheyna".to_string(),
        );
        data.quests.insert(
            "quest_wo_mistletoe".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Unknown_EnchantedMistletoe".to_string(),
                raw: serde_json::json!({
                    "Name": "Enchanted Mistletoe",
                    "WorkOrderSkill": "Unknown",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "EnchantedMistletoe", "Number": 10, "Description": "Obtain Enchanted Mistletoe"},
                        {"Type": "Scripted", "Number": 1, "Description": "Deliver to Fitz the Boatman"}
                    ],
                }),
            },
        );
        data.items.insert(
            9001,
            crate::game_data::ItemInfo {
                id: 9001,
                name: "Shoddy Test Stool".to_string(),
                description: None,
                icon_id: None,
                value: None,
                max_stack_size: None,
                keywords: Vec::new(),
                effect_descs: Vec::new(),
                internal_name: Some("TestStool".to_string()),
                food_desc: None,
                equip_slot: None,
                num_uses: None,
                skill_reqs: None,
                behaviors: None,
                bestow_recipes: None,
                bestow_ability: None,
                bestow_quest: None,
                bestow_title: None,
                craft_points: None,
                crafting_target_level: None,
                tsys_profile: None,
                raw_json: serde_json::json!({}),
            },
        );
        data.item_internal_name_index
            .insert("TestStool".to_string(), 9001);
        data.items.insert(
            9002,
            crate::game_data::ItemInfo {
                id: 9002,
                name: "Enchanted Mistletoe".to_string(),
                description: None,
                icon_id: None,
                value: None,
                max_stack_size: None,
                keywords: Vec::new(),
                effect_descs: Vec::new(),
                internal_name: Some("EnchantedMistletoe".to_string()),
                food_desc: None,
                equip_slot: None,
                num_uses: None,
                skill_reqs: None,
                behaviors: None,
                bestow_recipes: None,
                bestow_ability: None,
                bestow_quest: None,
                bestow_title: None,
                craft_points: None,
                crafting_target_level: None,
                tsys_profile: None,
                raw_json: serde_json::json!({}),
            },
        );
        data.item_internal_name_index
            .insert("EnchantedMistletoe".to_string(), 9002);
        // No `recipes_producing_item` entry for 9002: nothing crafts
        // Enchanted Mistletoe (zero hits in docs/CDN-full-examples/recipes.json).
        data.recipes.insert(
            7001,
            crate::game_data::RecipeInfo {
                id: 7001,
                name: "Test Stool Recipe".to_string(),
                description: None,
                internal_name: None,
                icon_id: None,
                skill: Some("Carpentry".to_string()),
                skill_level_req: None,
                ingredients: Vec::new(),
                costs: Vec::new(),
                result_items: Vec::new(),
                reward_skill: None,
                reward_skill_xp: None,
                reward_skill_xp_first_time: None,
                prereq_recipe: None,
                keywords: Vec::new(),
                ingredient_item_ids: Vec::new(),
                result_item_ids: vec![9001],
                result_effects: Vec::new(),
                usage_delay: None,
                reward_skill_xp_drop_off_level: None,
                sort_skill: None,
                action_label: None,
                shares_name_with_item_id: None,
                raw_json: serde_json::json!({
                    "Description": "Must be near a tanning rack, for some reason."
                }),
            },
        );
        data.recipes_producing_item.insert(9001, vec![7001]);
        data.quests.insert(
            "quest_wo_plain".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Carpentry_PlainWidgets".to_string(),
                raw: serde_json::json!({
                    "Name": "Plain Widgets",
                    "WorkOrderSkill": "Carpentry",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "PlainWidget", "Number": 4, "Description": "Obtain Plain Widgets"},
                        {"Type": "Scripted", "Number": 1, "Description": "Deliver to Fitz the Boatman"}
                    ],
                }),
            },
        );
        data.items.insert(
            9003,
            crate::game_data::ItemInfo {
                id: 9003,
                name: "Plain Widget".to_string(),
                description: None,
                icon_id: None,
                value: None,
                max_stack_size: None,
                keywords: Vec::new(),
                effect_descs: Vec::new(),
                internal_name: Some("PlainWidget".to_string()),
                food_desc: None,
                equip_slot: None,
                num_uses: None,
                skill_reqs: None,
                behaviors: None,
                bestow_recipes: None,
                bestow_ability: None,
                bestow_quest: None,
                bestow_title: None,
                craft_points: None,
                crafting_target_level: None,
                tsys_profile: None,
                raw_json: serde_json::json!({}),
            },
        );
        data.item_internal_name_index
            .insert("PlainWidget".to_string(), 9003);
        data.recipes.insert(
            7002,
            crate::game_data::RecipeInfo {
                id: 7002,
                name: "Plain Widget Recipe".to_string(),
                description: None,
                internal_name: None,
                icon_id: None,
                skill: Some("Carpentry".to_string()),
                skill_level_req: None,
                ingredients: Vec::new(),
                costs: Vec::new(),
                result_items: Vec::new(),
                reward_skill: None,
                reward_skill_xp: None,
                reward_skill_xp_first_time: None,
                prereq_recipe: None,
                keywords: Vec::new(),
                ingredient_item_ids: Vec::new(),
                result_item_ids: vec![9003],
                result_effects: Vec::new(),
                usage_delay: None,
                reward_skill_xp_drop_off_level: None,
                sort_skill: None,
                action_label: None,
                shares_name_with_item_id: None,
                raw_json: serde_json::json!({
                    "Description": "A simple widget anyone can assemble anywhere."
                }),
            },
        );
        data.recipes_producing_item.insert(9003, vec![7002]);
        data
    }

    fn wo_item(key: &str, quantity: u32) -> WorkOrderTripItem {
        WorkOrderTripItem {
            quest_key: key.to_string(),
            quantity,
        }
    }

    #[test]
    fn test_wo_stops_empty_input() {
        let data = fixture_game_data();
        let stops = build_work_order_stops(&data, &[]).unwrap();
        assert!(stops.is_empty());
    }

    #[test]
    fn test_wo_stops_resolve_zones_and_purposes() {
        let data = fixture_game_data();
        // Sheyna order passes quantity 0 → falls back to the Collect number (10);
        // the internal name exercises the quest_internal_name_index tier.
        let orders = vec![wo_item("quest_wo_fitz", 10), wo_item("Unknown_ManticoreTailTips", 0)];
        let stops = build_work_order_stops(&data, &orders).unwrap();
        assert_eq!(stops.len(), 4);
        assert_eq!(stops[0].zone, "AreaSerbule");
        assert_eq!(stops[0].purpose, StopPurpose::Craft);
        assert_eq!(stops[1].zone, "AreaSerbule");
        assert_eq!(stops[1].purpose, StopPurpose::TurnIn);
        assert_eq!(stops[2].zone, "AreaRahu");
        // ManticoreTailTip has no producing recipe (real CDN item, zero recipe
        // hits) — a gather stop, not craft.
        assert_eq!(stops[2].purpose, StopPurpose::Gather);
        assert_eq!(stops[3].zone, "AreaRahu");
        assert_eq!(stops[3].purpose, StopPurpose::TurnIn);
        // Display name (not the internal name) + skill land in the details.
        assert!(stops[0].details.contains("10x Shoddy Test Stool"));
        assert!(stops[0].details.contains("10x Shoddy Test Stool (total)"));
        assert!(stops[0].details.contains("Carpentry"));
        assert!(stops[1].details.contains("Fitz the Boatman"));
        assert!(stops[1].details.contains("(total)"));
        // Zero input quantity fell back to the quest Collect number.
        assert!(stops[3].details.contains("10x"));
        assert!(stops[3].details.contains("(total)"));
    }

    #[test]
    fn test_wo_stops_static_board_fallback() {
        // Ogamboe has no NPC entry in the fixture (as in the CDN sample) —
        // the nine-board table still places the stops in the Fae Realm.
        let data = fixture_game_data();
        let stops = build_work_order_stops(&data, &[wo_item("quest_wo_ogamboe", 5)]).unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].zone, "AreaFaeRealm1");
        // FaeCloth has no producing recipe — a gather stop, not craft.
        assert_eq!(stops[0].purpose, StopPurpose::Gather);
        assert_eq!(stops[1].zone, "AreaFaeRealm1");
        assert_eq!(stops[1].purpose, StopPurpose::TurnIn);
        assert!(stops[1].details.contains("Ogamboe"));
    }

    #[test]
    fn test_wo_stops_unknown_quest_errors() {
        let data = fixture_game_data();
        let result = build_work_order_stops(&data, &[wo_item("quest_nope", 1)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("quest_nope"));
    }

    #[test]
    fn test_wo_stops_missing_deliver_to_errors() {
        let mut data = fixture_game_data();
        data.quests.insert(
            "quest_wo_broken".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Broken_Order".to_string(),
                raw: serde_json::json!({
                    "Name": "Broken Order",
                    "WorkOrderSkill": "Carpentry",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "TestStool", "Number": 3}
                    ],
                }),
            },
        );
        let result = build_work_order_stops(&data, &[wo_item("quest_wo_broken", 3)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("quest_wo_broken"));
    }

    #[test]
    fn test_wo_stops_unknown_turnin_zone_errors() {
        let mut data = fixture_game_data();
        data.quests.insert(
            "quest_wo_nowhere".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Nowhere_Order".to_string(),
                raw: serde_json::json!({
                    "Name": "Nowhere Order",
                    "WorkOrderSkill": "Carpentry",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "TestStool", "Number": 3},
                        {"Type": "Scripted", "Number": 1, "Description": "Deliver to Nobody Real"}
                    ],
                }),
            },
        );
        let result = build_work_order_stops(&data, &[wo_item("quest_wo_nowhere", 3)]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Nobody Real"));
    }

    #[test]
    fn test_wo_station_hint_in_craft_details() {
        let data = fixture_game_data();
        let stops = build_work_order_stops(&data, &[wo_item("quest_wo_fitz", 10)]).unwrap();
        assert!(stops[0].details.contains("[needs tanning rack]"));
        // Gathering order with no resolvable producing recipe: Gather purpose,
        // no station hint, still plans.
        let stops = build_work_order_stops(&data, &[wo_item("quest_wo_sheyna", 10)]).unwrap();
        assert_eq!(stops[0].purpose, StopPurpose::Gather);
        assert!(!stops[0].details.contains("[needs"));
    }

    #[test]
    fn test_wo_craft_without_station_hint_for_produced_item() {
        // Producers-gate vs station-gate discrimination: PlainWidget HAS a
        // producing recipe whose Description states no station, so the stop is
        // still Craft (a station.is_some() gate would misplan it as Gather).
        let data = fixture_game_data();
        let stops = build_work_order_stops(&data, &[wo_item("quest_wo_plain", 4)]).unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].zone, "AreaSerbule");
        assert_eq!(stops[0].purpose, StopPurpose::Craft);
        assert!(stops[0].details.contains("Craft 4x Plain Widget (total)"));
        assert!(!stops[0].details.contains("[needs"));
        assert_eq!(stops[1].purpose, StopPurpose::TurnIn);
    }

    #[test]
    fn test_wo_route_end_to_end() {
        let data = fixture_game_data();
        let orders = vec![wo_item("quest_wo_fitz", 10), wo_item("quest_wo_sheyna", 10)];
        let stops = build_work_order_stops(&data, &orders).unwrap();
        let g = make_graph();
        let route = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();
        // Serbule group first (no travel), then Rahu via Eltibule + Casino: 3 hops.
        assert_eq!(route.total_hops, 3);
        let actions: Vec<_> = route.steps.iter().map(|s| &s.action).collect();
        assert_eq!(
            actions,
            [
                &RouteAction::Craft,
                &RouteAction::TurnIn,
                &RouteAction::Travel,
                &RouteAction::Travel,
                &RouteAction::Travel,
                &RouteAction::Gather,
                &RouteAction::TurnIn
            ]
        );
    }

    #[test]
    fn test_wo_gather_stop_for_unproduced_item() {
        // Live bug: Enchanted Mistletoe is a gatherable (no producing recipe in
        // docs/CDN-full-examples/recipes.json) but planned a Craft stop.
        let data = fixture_game_data();
        let stops = build_work_order_stops(&data, &[wo_item("quest_wo_mistletoe", 10)]).unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].zone, "AreaSerbule");
        assert_eq!(stops[0].purpose, StopPurpose::Gather);
        assert_eq!(stops[1].purpose, StopPurpose::TurnIn);
        assert!(stops.iter().all(|s| s.purpose != StopPurpose::Craft));
        assert_eq!(
            stops[0].details,
            "Gather 10x Enchanted Mistletoe (total) (Unknown) for 'Enchanted Mistletoe'"
        );
    }

    #[test]
    fn test_wo_unknown_item_still_plans_gather() {
        // Unknown item (no item row, so no producers): still plans Gather +
        // TurnIn with the internal name as label — never silently drops.
        let mut data = fixture_game_data();
        data.quests.insert(
            "quest_wo_unknown_item".to_string(),
            crate::game_data::QuestInfo {
                internal_name: "Unknown_NoSuchItem".to_string(),
                raw: serde_json::json!({
                    "Name": "No Such Item",
                    "WorkOrderSkill": "Unknown",
                    "Objectives": [
                        {"Type": "Collect", "ItemName": "NoSuchItem", "Number": 7},
                        {"Type": "Scripted", "Number": 1, "Description": "Deliver to Fitz the Boatman"}
                    ],
                }),
            },
        );
        let stops = build_work_order_stops(&data, &[wo_item("quest_wo_unknown_item", 7)]).unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].purpose, StopPurpose::Gather);
        assert_eq!(stops[1].purpose, StopPurpose::TurnIn);
        assert!(stops[0].details.contains("7x NoSuchItem (total)"));
    }

    #[test]
    fn test_gather_orders_before_turn_in_within_zone() {
        let g = make_graph();
        // Input order is TurnIn-first; the solver still emits Gather first.
        let stops = vec![
            stop("AreaSerbule", StopPurpose::TurnIn, "Turn in order"),
            stop("AreaSerbule", StopPurpose::Gather, "Gather mistletoe"),
        ];
        let route = plan_route(&g, "AreaSerbule", &stops, &TravelConfig::default()).unwrap();
        assert_eq!(route.steps.len(), 2);
        assert_eq!(route.steps[0].action, RouteAction::Gather);
        assert_eq!(route.steps[1].action, RouteAction::TurnIn);
    }

    #[test]
    fn test_station_from_description_patterns() {
        assert_eq!(
            station_from_description("Creates a tasty meal. Requires a stove or fire pit."),
            Some("stove or fire pit".to_string())
        );
        assert_eq!(
            // Live CDN wart: trailing prose after a comma is trimmed.
            station_from_description("Must be near a tanning rack, for some reason."),
            Some("tanning rack".to_string())
        );
        assert_eq!(station_from_description("No station mentioned here."), None);
        assert_eq!(station_from_description("Requires a ."), None);
    }
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `trip_router` IPC payload shapes derive `specta::Type` so
// `src/types/tripRouter.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test trip_router_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<RouteStop>()
        .register::<StopPurpose>()
        .register::<TravelConfig>()
        .register::<WorkOrderTripItem>()
        .register::<PlannedRoute>()
        .register::<RouteStep>()
        .register::<RouteAction>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/tripRouter.specta.ts` must equal
    /// what `export_bindings` derives from the `trip_router` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test trip_router_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_trip_router_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/tripRouter.specta.ts",
        );
        if std::env::var("OVERWRITE_BINDINGS").is_ok() {
            std::fs::write(path, &fresh).expect("bindings write failed");
            return;
        }
        let checked_in =
            std::fs::read_to_string(path).expect("checked-in specta bindings missing");
        assert_eq!(
            checked_in, fresh,
            "specta bindings drift: regenerate with OVERWRITE_BINDINGS=1",
        );
    }
}