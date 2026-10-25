//! Static zone connectivity graph for Project: Gorgon.
//!
//! The CDN `areas.json` provides only friendly names — no connectivity data.
//! This module maintains the zone adjacency graph manually, sourced from:
//! - <https://wiki.projectgorgon.com/wiki/Zones>
//! - <https://wiki.projectgorgon.com/wiki/Dungeons>
//!
//! The graph is small (~16 overworld nodes, ~20 edges) so we precompute
//! a full BFS distance matrix at init time.

use std::collections::{HashMap, VecDeque};

// ── Zone IDs ────────────────────────────────────────────────────────────────
// Mirror the CDN area keys. We use string slices so callers can match against
// CDN data without allocation.

pub const AREA_ANAGOGE: &str = "AreaNewbieIsland";
pub const AREA_SERBULE: &str = "AreaSerbule";
pub const AREA_SERBULE_HILLS: &str = "AreaSerbule2";
pub const AREA_ELTIBULE: &str = "AreaEltibule";
pub const AREA_SUN_VALE: &str = "AreaSunVale";
pub const AREA_KUR_MOUNTAINS: &str = "AreaKurMountains";
pub const AREA_CASINO: &str = "AreaCasino";
pub const AREA_ILMARI: &str = "AreaDesert1";
pub const AREA_RAHU: &str = "AreaRahu";
pub const AREA_GAZLUK: &str = "AreaGazluk";
pub const AREA_FAE_REALM: &str = "AreaFaeRealm1";
pub const AREA_POVUS: &str = "AreaPovus";
pub const AREA_VIDARIA: &str = "AreaVidaria";
pub const AREA_STATEHELM: &str = "AreaStatehelm";

// Dungeons/sub-zones that might appear as player locations.
// We map these to their parent overworld zone for routing purposes.
pub const AREA_GAZLUK_CAVES: &str = "AreaGazlukCaves";
pub const AREA_GAZLUK_KEEP: &str = "AreaGazlukKeep";
pub const AREA_SERBULE_CAVES: &str = "AreaSerbuleCaves";
pub const AREA_SERBULE_HILLS_CAVES: &str = "AreaCave1"; // Dungeons Beneath Eltibule? Actually Cave1 = Eltibule dungeon
pub const AREA_KUR_CAVES: &str = "AreaKurCaves";
pub const AREA_RAHU_CAVES: &str = "AreaRahuCaves";
pub const AREA_SUN_VALE_CAVES: &str = "AreaSunValeCaves";
pub const AREA_ILMARI_CAVES: &str = "AreaDesert1Caves";
pub const AREA_POVUS_CAVES: &str = "AreaPovusCaves";
pub const AREA_POVUS_CAVES2: &str = "AreaPovusCaves2";
pub const AREA_STATEHELM_CAVES: &str = "AreaStatehelmCaves";
pub const AREA_STATEHELM_CAVES2: &str = "AreaStatehelmCaves2";
pub const AREA_VIDARIA_CAVES: &str = "AreaVidariaCaves";
pub const AREA_FAE_REALM_CAVES: &str = "AreaFaeRealm1Caves";
pub const AREA_MYCONIAN_CAVE: &str = "AreaMyconianCave";
pub const AREA_TOMB1: &str = "AreaTomb1"; // Khyrulek's Crypt — under Kur
pub const AREA_WINTER_NEXUS: &str = "AreaPlanes"; // Existential Planes / Winter Nexus
pub const AREA_PHANTOM_ILMARI: &str = "AreaDesert2"; // Phantom Ilmari Desert

// ── Overworld edges ─────────────────────────────────────────────────────────
// Each tuple is (zone_a, zone_b). All edges are bidirectional unless noted.

const OVERWORLD_EDGES: &[(&str, &str)] = &[
    (AREA_ANAGOGE, AREA_SERBULE),
    (AREA_SERBULE, AREA_SERBULE_HILLS),
    (AREA_SERBULE, AREA_ELTIBULE),
    (AREA_SERBULE, AREA_SUN_VALE), // boatman
    (AREA_ELTIBULE, AREA_CASINO),
    (AREA_ELTIBULE, AREA_KUR_MOUNTAINS),
    (AREA_CASINO, AREA_RAHU),      // portal during major moon phases
    (AREA_CASINO, AREA_STATEHELM), // portal during crescent/gibbous phases
    (AREA_KUR_MOUNTAINS, AREA_ILMARI),
    (AREA_ILMARI, AREA_RAHU),
    (AREA_KUR_MOUNTAINS, AREA_GAZLUK),
    (AREA_SUN_VALE, AREA_WINTER_NEXUS),
    (AREA_WINTER_NEXUS, AREA_FAE_REALM),
    (AREA_GAZLUK, AREA_POVUS),
    (AREA_POVUS, AREA_RAHU),
    (AREA_POVUS, AREA_VIDARIA),
    (AREA_VIDARIA, AREA_STATEHELM),
    (AREA_ILMARI, AREA_PHANTOM_ILMARI),
];

// ── Sub-zone → parent mapping ───────────────────────────────────────────────
// Dungeons and caves are children of their overworld zone for routing.

const SUBZONE_PARENTS: &[(&str, &str)] = &[
    (AREA_GAZLUK_CAVES, AREA_GAZLUK),
    (AREA_GAZLUK_KEEP, AREA_GAZLUK),
    (AREA_SERBULE_CAVES, AREA_SERBULE),
    (AREA_SERBULE_HILLS_CAVES, AREA_ELTIBULE), // "Dungeons Beneath Eltibule"
    (AREA_KUR_CAVES, AREA_KUR_MOUNTAINS),
    (AREA_RAHU_CAVES, AREA_RAHU),
    (AREA_SUN_VALE_CAVES, AREA_SUN_VALE),
    (AREA_ILMARI_CAVES, AREA_ILMARI),
    (AREA_POVUS_CAVES, AREA_POVUS),
    (AREA_POVUS_CAVES2, AREA_POVUS),
    (AREA_STATEHELM_CAVES, AREA_STATEHELM),
    (AREA_STATEHELM_CAVES2, AREA_STATEHELM),
    (AREA_VIDARIA_CAVES, AREA_VIDARIA),
    (AREA_FAE_REALM_CAVES, AREA_FAE_REALM),
    (AREA_MYCONIAN_CAVE, AREA_SERBULE),
    (AREA_TOMB1, AREA_KUR_MOUNTAINS),
    // AreaCave2 is another dungeon entry for Kur caves
    ("AreaCave2", AREA_KUR_MOUNTAINS),
    // Instanced zones — map to a reasonable parent for routing
    ("AreaApartment1", AREA_SERBULE),
    ("AreaGuildHall1", AREA_SERBULE),
];

// ── ZoneGraph ───────────────────────────────────────────────────────────────

/// Precomputed zone connectivity with BFS distance matrix.
pub struct ZoneGraph {
    /// All overworld zone IDs, in stable order.
    zones: Vec<&'static str>,
    /// zone_id → index into `zones`
    zone_index: HashMap<&'static str, usize>,
    /// Adjacency list: zone index → list of neighbor indices
    adjacency: Vec<Vec<usize>>,
    /// BFS shortest distance matrix: `dist[i][j]` = hop count from zone i to j.
    /// `u32::MAX` means unreachable.
    dist: Vec<Vec<u32>>,
    /// Sub-zone → parent overworld zone
    subzone_map: HashMap<&'static str, &'static str>,
}

impl ZoneGraph {
    /// Build the graph and precompute the distance matrix.
    pub fn new() -> Self {
        Self::build(&[])
    }

    /// Build the graph excluding specific edges.
    /// Each excluded edge is `(zone_a, zone_b)` — both directions are removed.
    pub fn new_excluding(excluded: &[(&str, &str)]) -> Self {
        Self::build(excluded)
    }

    fn build(excluded: &[(&str, &str)]) -> Self {
        // Collect all unique overworld zone IDs from edges.
        let mut zone_set = Vec::new();
        for &(a, b) in OVERWORLD_EDGES {
            if !zone_set.contains(&a) {
                zone_set.push(a);
            }
            if !zone_set.contains(&b) {
                zone_set.push(b);
            }
        }
        zone_set.sort();

        let zone_index: HashMap<&'static str, usize> =
            zone_set.iter().enumerate().map(|(i, &z)| (z, i)).collect();

        let n = zone_set.len();
        let mut adjacency = vec![Vec::new(); n];
        for &(a, b) in OVERWORLD_EDGES {
            // Skip excluded edges (check both directions).
            let dominated = excluded
                .iter()
                .any(|&(ea, eb)| (a == ea && b == eb) || (a == eb && b == ea));
            if dominated {
                continue;
            }
            let ia = zone_index[a];
            let ib = zone_index[b];
            adjacency[ia].push(ib);
            adjacency[ib].push(ia);
        }

        // BFS from each zone to compute full distance matrix.
        let mut dist = vec![vec![u32::MAX; n]; n];
        for start in 0..n {
            dist[start][start] = 0;
            let mut queue = VecDeque::new();
            queue.push_back(start);
            while let Some(cur) = queue.pop_front() {
                let d = dist[start][cur];
                for &neighbor in &adjacency[cur] {
                    if dist[start][neighbor] == u32::MAX {
                        dist[start][neighbor] = d + 1;
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        let subzone_map: HashMap<&'static str, &'static str> =
            SUBZONE_PARENTS.iter().copied().collect();

        ZoneGraph {
            zones: zone_set,
            zone_index,
            adjacency,
            dist,
            subzone_map,
        }
    }

    /// Resolve a zone to its overworld parent. If the zone is already an
    /// overworld zone, returns it unchanged. Unknown zones return `None`.
    pub fn resolve_overworld(&self, zone: &str) -> Option<&'static str> {
        // Check if it's a known overworld zone.
        if self.zone_index.contains_key(zone) {
            // Return the static str from our zones list.
            return Some(self.zones[self.zone_index[zone]]);
        }
        // Check if it's a sub-zone.
        self.subzone_map.get(zone).copied()
    }

    /// Shortest walking distance (hop count) between two zones.
    /// Resolves sub-zones to their parents automatically.
    /// Returns `None` if either zone is unknown or they're unreachable.
    pub fn distance(&self, from: &str, to: &str) -> Option<u32> {
        let from_resolved = self.resolve_overworld(from)?;
        let to_resolved = self.resolve_overworld(to)?;
        let i = *self.zone_index.get(from_resolved)?;
        let j = *self.zone_index.get(to_resolved)?;
        let d = self.dist[i][j];
        if d == u32::MAX {
            None
        } else {
            Some(d)
        }
    }

    /// Return the shortest walking path between two zones as a list of zone IDs.
    /// Includes both endpoints. Resolves sub-zones.
    /// Returns `None` if either zone is unknown or unreachable.
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<&'static str>> {
        let from_resolved = self.resolve_overworld(from)?;
        let to_resolved = self.resolve_overworld(to)?;
        let start = *self.zone_index.get(from_resolved)?;
        let end = *self.zone_index.get(to_resolved)?;

        if self.dist[start][end] == u32::MAX {
            return None;
        }
        if start == end {
            return Some(vec![self.zones[start]]);
        }

        // BFS to reconstruct path (we already have distances, walk backwards).
        let mut path = vec![end];
        let mut cur = end;
        while cur != start {
            let cur_dist = self.dist[start][cur];
            for &neighbor in &self.adjacency[cur] {
                if self.dist[start][neighbor] == cur_dist - 1 {
                    path.push(neighbor);
                    cur = neighbor;
                    break;
                }
            }
        }
        path.reverse();
        Some(path.iter().map(|&i| self.zones[i]).collect())
    }

    /// All overworld zone IDs.
    #[allow(dead_code)]
    pub fn overworld_zones(&self) -> &[&'static str] {
        &self.zones
    }

    /// Number of overworld zones.
    #[allow(dead_code)]
    pub fn zone_count(&self) -> usize {
        self.zones.len()
    }

    /// Resolve a friendly zone name (from SkillReport, etc.) to a CDN area key.
    /// Falls back to checking if the input is already a CDN key.
    pub fn resolve_friendly_name(&self, name: &str) -> Option<&'static str> {
        // Check if it's already a CDN key.
        if let Some(z) = self.resolve_overworld(name) {
            return Some(z);
        }
        // Match against known friendly names.
        let key = match name {
            "Anagoge Island" | "Anagoge" => AREA_ANAGOGE,
            "Serbule" => AREA_SERBULE,
            "Serbule Hills" => AREA_SERBULE_HILLS,
            "Eltibule" => AREA_ELTIBULE,
            "Sun Vale" => AREA_SUN_VALE,
            "Kur Mountains" | "Kur" => AREA_KUR_MOUNTAINS,
            "Red Wing Casino" | "Casino" => AREA_CASINO,
            "Ilmari" | "Ilmari Desert" => AREA_ILMARI,
            "Rahu" => AREA_RAHU,
            "Gazluk" | "Gazluk Plateau" => AREA_GAZLUK,
            "Fae Realm" => AREA_FAE_REALM,
            "Povus" => AREA_POVUS,
            "Vidaria" => AREA_VIDARIA,
            "Statehelm" => AREA_STATEHELM,
            "Winter Nexus" | "Existential Planes" => AREA_WINTER_NEXUS,
            "Phantom Ilmari Desert" | "Phantom Desert" => AREA_PHANTOM_ILMARI,
            // Sub-zones / dungeons that appear in bind locations
            "Caves Beneath Gazluk" | "Gazluk Caves" => AREA_GAZLUK_CAVES,
            "Caves Under Serbule" | "Serbule Caves" => AREA_SERBULE_CAVES,
            "Rahu Sewers" | "Rahu Sewer" => AREA_RAHU_CAVES,
            "Caves Beneath Kur Mountains" | "Kur Caves" => AREA_KUR_CAVES,
            "Statehelm Undercity" => AREA_STATEHELM_CAVES,
            "Statehelm Depths" => AREA_STATEHELM_CAVES2,
            _ => return None,
        };
        Some(key)
    }

    /// Neighbors of a zone (overworld only). Resolves sub-zones.
    #[allow(dead_code)]
    pub fn neighbors(&self, zone: &str) -> Option<Vec<&'static str>> {
        let resolved = self.resolve_overworld(zone)?;
        let i = *self.zone_index.get(resolved)?;
        Some(self.adjacency[i].iter().map(|&j| self.zones[j]).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_builds() {
        let g = ZoneGraph::new();
        assert!(g.zone_count() > 0);
        // 15 overworld zones from the edges above
        assert_eq!(g.zone_count(), 16);
    }

    #[test]
    fn test_self_distance_is_zero() {
        let g = ZoneGraph::new();
        assert_eq!(g.distance(AREA_SERBULE, AREA_SERBULE), Some(0));
    }

    #[test]
    fn test_adjacent_distance_is_one() {
        let g = ZoneGraph::new();
        assert_eq!(g.distance(AREA_SERBULE, AREA_ELTIBULE), Some(1));
        assert_eq!(g.distance(AREA_ELTIBULE, AREA_SERBULE), Some(1));
    }

    #[test]
    fn test_multi_hop_distance() {
        let g = ZoneGraph::new();
        // Serbule → Eltibule → Kur Mountains = 2 hops
        assert_eq!(g.distance(AREA_SERBULE, AREA_KUR_MOUNTAINS), Some(2));
        // Serbule → Eltibule → Kur → Gazluk = 3 hops
        assert_eq!(g.distance(AREA_SERBULE, AREA_GAZLUK), Some(3));
    }

    #[test]
    fn test_subzone_resolves_to_parent() {
        let g = ZoneGraph::new();
        assert_eq!(g.resolve_overworld(AREA_GAZLUK_CAVES), Some(AREA_GAZLUK));
        assert_eq!(g.resolve_overworld(AREA_SERBULE_CAVES), Some(AREA_SERBULE));
    }

    #[test]
    fn test_subzone_distance() {
        let g = ZoneGraph::new();
        // Gazluk Caves is a child of Gazluk
        // Serbule → Eltibule → Kur → Gazluk = 3 hops
        assert_eq!(g.distance(AREA_SERBULE, AREA_GAZLUK_CAVES), Some(3));
    }

    #[test]
    fn test_shortest_path() {
        let g = ZoneGraph::new();
        let path = g.shortest_path(AREA_SERBULE, AREA_GAZLUK).unwrap();
        assert_eq!(path.len(), 4); // Serbule, Eltibule, Kur, Gazluk
        assert_eq!(path[0], AREA_SERBULE);
        assert_eq!(path[3], AREA_GAZLUK);
    }

    #[test]
    fn test_unknown_zone_returns_none() {
        let g = ZoneGraph::new();
        assert_eq!(g.distance("AreaNonexistent", AREA_SERBULE), None);
        assert_eq!(g.resolve_overworld("AreaNonexistent"), None);
    }

    #[test]
    fn test_symmetry() {
        let g = ZoneGraph::new();
        for &a in g.overworld_zones() {
            for &b in g.overworld_zones() {
                assert_eq!(
                    g.distance(a, b),
                    g.distance(b, a),
                    "Distance asymmetry between {} and {}",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn test_all_zones_reachable() {
        let g = ZoneGraph::new();
        for &a in g.overworld_zones() {
            for &b in g.overworld_zones() {
                assert!(
                    g.distance(a, b).is_some(),
                    "{} should be reachable from {}",
                    b,
                    a
                );
            }
        }
    }

    #[test]
    fn test_neighbors() {
        let g = ZoneGraph::new();
        let neighbors = g.neighbors(AREA_SERBULE).unwrap();
        assert!(neighbors.contains(&AREA_ELTIBULE));
        assert!(neighbors.contains(&AREA_SERBULE_HILLS));
        assert!(neighbors.contains(&AREA_ANAGOGE));
        assert!(neighbors.contains(&AREA_SUN_VALE));
    }

    #[test]
    fn test_casino_connects_to_statehelm() {
        let g = ZoneGraph::new();
        let neighbors = g.neighbors(AREA_CASINO).unwrap();
        assert!(neighbors.contains(&AREA_RAHU));
        assert!(neighbors.contains(&AREA_STATEHELM));
        assert!(neighbors.contains(&AREA_ELTIBULE));
    }

    #[test]
    fn test_excluded_casino_rahu_edge() {
        let g = ZoneGraph::new_excluding(&[(AREA_CASINO, AREA_RAHU)]);
        let neighbors = g.neighbors(AREA_CASINO).unwrap();
        // Casino should connect to Statehelm and Eltibule, but NOT Rahu
        assert!(neighbors.contains(&AREA_STATEHELM));
        assert!(neighbors.contains(&AREA_ELTIBULE));
        assert!(!neighbors.contains(&AREA_RAHU));
        // Rahu is still reachable via other paths (Ilmari, Povus)
        assert!(g.distance(AREA_CASINO, AREA_RAHU).is_some());
    }

    #[test]
    fn test_excluded_casino_statehelm_edge() {
        let g = ZoneGraph::new_excluding(&[(AREA_CASINO, AREA_STATEHELM)]);
        let neighbors = g.neighbors(AREA_CASINO).unwrap();
        assert!(neighbors.contains(&AREA_RAHU));
        assert!(!neighbors.contains(&AREA_STATEHELM));
        // Statehelm still reachable via Vidaria
        assert!(g.distance(AREA_CASINO, AREA_STATEHELM).is_some());
    }

    #[test]
    fn test_resolve_friendly_names() {
        let g = ZoneGraph::new();
        assert_eq!(g.resolve_friendly_name("Serbule"), Some(AREA_SERBULE));
        assert_eq!(
            g.resolve_friendly_name("Red Wing Casino"),
            Some(AREA_CASINO)
        );
        assert_eq!(
            g.resolve_friendly_name("Caves Beneath Gazluk"),
            Some(AREA_GAZLUK_CAVES)
        );
        assert_eq!(g.resolve_friendly_name("Ilmari Desert"), Some(AREA_ILMARI));
        assert_eq!(g.resolve_friendly_name("AreaSerbule"), Some(AREA_SERBULE)); // CDN key passthrough
        assert_eq!(g.resolve_friendly_name("Nonexistent Zone"), None);
    }
}
