//! Trip routing solver for multi-zone travel planning.
//!
//! Given a set of stops (zone + purpose), produces an ordered trip plan.
//! Phase 1: walking only (zone graph BFS). Phase 2 will add teleportation.
//!
//! Multiple features feed stops into the solver: work order fulfillment,
//! crafting project pickup lists, delivery quests, inventory consolidation.

use crate::zone_graph::ZoneGraph;
use serde::{Deserialize, Serialize};

// ── Input types ─────────────────────────────────────────────────────────────

/// A stop the player needs to make during a trip.
#[derive(Debug, Clone, Deserialize)]
pub struct RouteStop {
    /// Area CDN key (e.g. "AreaSerbule") or sub-zone key.
    pub zone: String,
    /// What the player needs to do here.
    pub purpose: StopPurpose,
    /// Human-readable description (e.g. "Pick up Iron Filament x20 from vault").
    pub details: String,
}

/// The type of action at a stop, used for within-zone ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
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
}

/// Configuration for available travel methods.
#[derive(Debug, Clone, Default, Deserialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct PlannedRoute {
    pub steps: Vec<RouteStep>,
    pub total_hops: u32,
}

/// A single step in a planned route.
#[derive(Debug, Clone, Serialize)]
pub struct RouteStep {
    /// Overworld zone CDN key.
    pub zone: String,
    /// What to do at this step.
    pub action: RouteAction,
    /// Human-readable description.
    pub details: String,
}

/// The action type for a route step.
#[derive(Debug, Clone, Serialize, PartialEq)]
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
        if let Some(bind_zone) = graph.resolve_overworld(bind) {
            if bind_zone == to_resolved && best.cost() > 1 {
                best = TravelMethod::BindRecall {
                    bind_name: bind.clone(),
                };
            }
        }
    }

    // Check mushroom circles.
    for circle in [&config.mushroom_circle_1, &config.mushroom_circle_2]
        .iter()
        .copied()
        .flatten()
    {
        if let Some(circle_zone) = graph.resolve_overworld(circle) {
            if circle_zone == to_resolved && best.cost() > 1 {
                best = TravelMethod::MushroomRecall {
                    circle_name: circle.clone(),
                };
            }
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
/// 4. Within each zone, order stops by purpose (pickup → vendor → craft → turn-in → deposit).
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
        zone_stops.sort_by_key(|s| s.purpose);

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
    // If casino_portal is "rahu", exclude Casino↔Statehelm edge (and vice versa).
    // If not specified, both edges are available (optimistic).
    let excluded = match config.casino_portal.as_deref() {
        Some("rahu") => vec![("AreaCasino", "AreaStatehelm")],
        Some("statehelm") => vec![("AreaCasino", "AreaRahu")],
        _ => vec![],
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
fn resolve_travel_config(graph: &ZoneGraph, mut config: TravelConfig) -> TravelConfig {
    let resolve = |name: &Option<String>| -> Option<String> {
        name.as_ref().and_then(|n| {
            graph
                .resolve_friendly_name(n)
                .map(|k| k.to_string())
                .or_else(|| Some(n.clone())) // keep original if unresolved
        })
    };
    config.primary_bind = resolve(&config.primary_bind);
    config.secondary_bind = resolve(&config.secondary_bind);
    config.mushroom_circle_1 = resolve(&config.mushroom_circle_1);
    config.mushroom_circle_2 = resolve(&config.mushroom_circle_2);
    config
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
}
