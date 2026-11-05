//! Mushroom farming reference data — powers the Mushroom Farming dashboard widget.
//!
//! Source: wiki chart (F:/ProjectGorgon/pg-rag/data/wiki/Mushroom_Farming_ee8545db.txt:92-143),
//! which summarizes in-game box behavior. The CDN dumps (items/skills/recipes) carry no
//! per-mushroom substrate, grow-time, or moon mapping, so this table is curated by hand
//! against that chart and needs a line changed whenever the wiki chart updates.
//!
//! Terminology is the wiki's verbatim: `adequate` / `very_well` substrates, `mycology_req`
//! (N/A = Parasol needs no Mycology to seed), `grow_hours`, and `robust` / `poor` moon
//! phases (harvest-time yield modifiers). Moon phase names match the game's 8-phase
//! cycle in `crate::cdn_commands` (`get_current_moon_phase`) and `useMoonPhase.ts`.
//!
//! Deliberately special rows, all per the chart — do not "fix" these:
//! - Parasol: `mycology_req: None` (chart says N/A).
//! - Ghostshroom: reversed tiers (`adequate: Exotic`, `very_well: Meat`) — the only
//!   mushroom whose high-tier substrate is adequate and low-tier is best.
//! - Granamurch/Ghostshroom: 4 poor phases instead of 2.
//! - Mortaferus: `adequate: None` (chart says `??`), `very_well: Exotic`.

use serde::Serialize;

/// One row of the mushroom farming reference table.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct MushroomFarmingInfo {
    pub name: &'static str,
    pub mycology_req: Option<u32>,
    pub grow_hours: u32,
    pub adequate_substrate: Option<&'static str>,
    pub very_well_substrate: &'static str,
    pub robust_phases: &'static [&'static str],
    pub poor_phases: &'static [&'static str],
}

/// The full reference table in Mycology-req order (matches the wiki chart order).
pub const MUSHROOMS: &[MushroomFarmingInfo] = &[
    MushroomFarmingInfo {
        name: "Parasol Mushroom",
        mycology_req: None,
        grow_hours: 2,
        adequate_substrate: Some("Dirt"),
        very_well_substrate: "Organs",
        robust_phases: &["FullMoon", "WaningCrescentMoon"],
        poor_phases: &["NewMoon", "WaxingGibbousMoon"],
    },
    MushroomFarmingInfo {
        name: "Mycena Mushroom",
        mycology_req: Some(5),
        grow_hours: 3,
        adequate_substrate: Some("Bone"),
        very_well_substrate: "Limbs",
        robust_phases: &["WaxingCrescentMoon", "QuarterMoon"],
        poor_phases: &["WaningGibbousMoon", "LastQuarterMoon"],
    },
    MushroomFarmingInfo {
        name: "Boletus Mushroom",
        mycology_req: Some(10),
        grow_hours: 4,
        adequate_substrate: Some("Meat"),
        very_well_substrate: "Exotic",
        robust_phases: &["NewMoon", "WaningGibbousMoon"],
        poor_phases: &["WaxingCrescentMoon", "FullMoon"],
    },
    MushroomFarmingInfo {
        name: "Goblin Puffball",
        mycology_req: Some(12),
        grow_hours: 5,
        adequate_substrate: Some("Dirt"),
        very_well_substrate: "Exotic",
        robust_phases: &["NewMoon", "WaxingGibbousMoon"],
        poor_phases: &["FullMoon", "WaningCrescentMoon"],
    },
    MushroomFarmingInfo {
        name: "Field Mushroom",
        mycology_req: Some(15),
        grow_hours: 5,
        adequate_substrate: Some("Bone"),
        very_well_substrate: "Organs",
        robust_phases: &["WaxingGibbousMoon", "LastQuarterMoon"],
        poor_phases: &["QuarterMoon", "WaningCrescentMoon"],
    },
    MushroomFarmingInfo {
        name: "Blusher Mushroom",
        mycology_req: Some(20),
        grow_hours: 6,
        adequate_substrate: Some("Meat"),
        very_well_substrate: "Exotic",
        robust_phases: &["NewMoon", "WaningGibbousMoon"],
        poor_phases: &["WaxingCrescentMoon", "FullMoon"],
    },
    MushroomFarmingInfo {
        name: "Milk Cap Mushroom",
        mycology_req: Some(25),
        grow_hours: 7,
        adequate_substrate: Some("Dirt"),
        very_well_substrate: "Organs",
        robust_phases: &["FullMoon", "WaningCrescentMoon"],
        poor_phases: &["NewMoon", "WaxingGibbousMoon"],
    },
    MushroomFarmingInfo {
        name: "Blood Mushroom",
        mycology_req: Some(30),
        grow_hours: 8,
        adequate_substrate: Some("Dirt"),
        very_well_substrate: "Limbs",
        robust_phases: &["WaxingCrescentMoon", "LastQuarterMoon"],
        poor_phases: &["QuarterMoon", "WaningGibbousMoon"],
    },
    MushroomFarmingInfo {
        name: "Blastcap Mushroom",
        mycology_req: Some(33),
        grow_hours: 8,
        adequate_substrate: Some("Meat"),
        very_well_substrate: "Organs",
        robust_phases: &["FullMoon", "WaningGibbousMoon"],
        poor_phases: &["NewMoon", "WaningCrescentMoon"],
    },
    MushroomFarmingInfo {
        name: "Coral Mushroom",
        mycology_req: Some(40),
        grow_hours: 9,
        adequate_substrate: Some("Meat"),
        very_well_substrate: "Limbs",
        robust_phases: &["QuarterMoon", "WaxingGibbousMoon"],
        poor_phases: &["LastQuarterMoon", "WaningCrescentMoon"],
    },
    MushroomFarmingInfo {
        name: "Iocaine Mushroom",
        mycology_req: Some(40),
        grow_hours: 10,
        adequate_substrate: Some("Bone"),
        very_well_substrate: "Limbs",
        robust_phases: &["WaxingCrescentMoon", "QuarterMoon"],
        poor_phases: &["WaningGibbousMoon", "LastQuarterMoon"],
    },
    MushroomFarmingInfo {
        name: "Groxmax Mushroom",
        mycology_req: Some(47),
        grow_hours: 11,
        adequate_substrate: Some("Bone"),
        very_well_substrate: "Organs",
        robust_phases: &["WaxingGibbousMoon", "LastQuarterMoon"],
        poor_phases: &["QuarterMoon", "WaningCrescentMoon"],
    },
    MushroomFarmingInfo {
        name: "Porcini Mushroom",
        mycology_req: Some(55),
        grow_hours: 12,
        adequate_substrate: Some("Meat"),
        very_well_substrate: "Exotic",
        robust_phases: &["FullMoon", "WaningGibbousMoon"],
        poor_phases: &["NewMoon", "WaningCrescentMoon"],
    },
    MushroomFarmingInfo {
        name: "False Agaric",
        mycology_req: Some(57),
        grow_hours: 12,
        adequate_substrate: Some("Bone"),
        very_well_substrate: "Limbs",
        robust_phases: &["WaningCrescentMoon", "LastQuarterMoon"],
        poor_phases: &["QuarterMoon", "WaxingGibbousMoon"],
    },
    MushroomFarmingInfo {
        name: "Black Foot Morel",
        mycology_req: Some(64),
        grow_hours: 13,
        adequate_substrate: Some("Dirt"),
        very_well_substrate: "Exotic",
        robust_phases: &["NewMoon", "WaningCrescentMoon"],
        poor_phases: &["FullMoon", "WaxingGibbousMoon"],
    },
    MushroomFarmingInfo {
        name: "Pixie's Parasol",
        mycology_req: Some(70),
        grow_hours: 14,
        adequate_substrate: Some("Meat"),
        very_well_substrate: "Organs",
        robust_phases: &["QuarterMoon", "WaxingGibbousMoon"],
        poor_phases: &["LastQuarterMoon", "WaningCrescentMoon"],
    },
    MushroomFarmingInfo {
        name: "Fly Amanita",
        mycology_req: Some(77),
        grow_hours: 15,
        adequate_substrate: Some("Bone"),
        very_well_substrate: "Organs",
        robust_phases: &["WaxingCrescentMoon", "FullMoon"],
        poor_phases: &["NewMoon", "WaningGibbousMoon"],
    },
    MushroomFarmingInfo {
        name: "Wizard's Mushroom",
        mycology_req: Some(75),
        grow_hours: 16,
        adequate_substrate: Some("Bone"),
        very_well_substrate: "Organs",
        robust_phases: &["WaxingCrescentMoon", "QuarterMoon"],
        poor_phases: &["WaxingGibbousMoon", "LastQuarterMoon"],
    },
    MushroomFarmingInfo {
        name: "Charged Mycelium",
        mycology_req: Some(80),
        grow_hours: 16,
        adequate_substrate: Some("Meat"),
        very_well_substrate: "Exotic",
        robust_phases: &["NewMoon", "WaxingGibbousMoon"],
        poor_phases: &["WaningCrescentMoon", "FullMoon"],
    },
    MushroomFarmingInfo {
        name: "Granamurch Mushroom",
        mycology_req: Some(85),
        grow_hours: 17,
        adequate_substrate: Some("Dirt"),
        very_well_substrate: "Limbs",
        robust_phases: &["FullMoon", "NewMoon"],
        poor_phases: &[
            "WaningGibbousMoon",
            "LastQuarterMoon",
            "WaningCrescentMoon",
            "QuarterMoon",
        ],
    },
    MushroomFarmingInfo {
        name: "Ghostshroom",
        mycology_req: Some(90),
        grow_hours: 18,
        adequate_substrate: Some("Exotic"),
        very_well_substrate: "Meat",
        robust_phases: &["FullMoon", "NewMoon"],
        poor_phases: &[
            "LastQuarterMoon",
            "WaxingCrescentMoon",
            "QuarterMoon",
            "WaxingGibbousMoon",
        ],
    },
    MushroomFarmingInfo {
        name: "Mortaferus Mushroom",
        mycology_req: Some(95),
        grow_hours: 19,
        adequate_substrate: None,
        very_well_substrate: "Exotic",
        robust_phases: &["WaningGibbousMoon", "LastQuarterMoon"],
        poor_phases: &["WaxingCrescentMoon"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The wiki chart's row count. If the chart gains a row, add it here too.
    #[test]
    fn covers_all_chart_mushrooms() {
        assert_eq!(MUSHROOMS.len(), 22, "expected 22 chart rows");
    }

    /// Mortaferus is the row the old widget got wrong: chart says `?? || Exotic`
    /// (F:/ProjectGorgon/pg-rag/data/wiki/Mushroom_Farming_ee8545db.txt:143),
    /// the widget claimed `Unknown || Organs` (MushroomFarmingWidget.vue:151).
    #[test]
    fn mortaferus_matches_chart() {
        let m = MUSHROOMS
            .iter()
            .find(|m| m.name == "Mortaferus Mushroom")
            .expect("mortaferus row");
        assert_eq!(m.adequate_substrate, None);
        assert_eq!(m.very_well_substrate, "Exotic");
        assert_eq!(m.mycology_req, Some(95));
        assert_eq!(m.grow_hours, 19);
        assert_eq!(m.robust_phases, &["WaningGibbousMoon", "LastQuarterMoon"]);
        assert_eq!(m.poor_phases, &["WaxingCrescentMoon"]);
    }

    /// The chart's deliberate exceptions must survive refactors — see module docs.
    #[test]
    fn chart_exceptions_survive() {
        let ghost = MUSHROOMS
            .iter()
            .find(|m| m.name == "Ghostshroom")
            .expect("ghostshroom row");
        assert_eq!(ghost.adequate_substrate, Some("Exotic"));
        assert_eq!(ghost.very_well_substrate, "Meat");

        let parasol = MUSHROOMS
            .iter()
            .find(|m| m.name == "Parasol Mushroom")
            .expect("parasol row");
        assert_eq!(parasol.mycology_req, None);

        let names: HashSet<&&str> = MUSHROOMS.iter().map(|m| &m.name).collect();
        assert_eq!(names.len(), MUSHROOMS.len(), "duplicate mushroom name");
    }

    /// Phase names must match the backend's 8-phase cycle; a typo here would
    /// silently never highlight. Validates every phase string in the table.
    #[test]
    fn all_phases_are_valid() {
        let valid: HashSet<&str> = [
            "NewMoon",
            "WaxingCrescentMoon",
            "QuarterMoon",
            "WaxingGibbousMoon",
            "FullMoon",
            "WaningGibbousMoon",
            "LastQuarterMoon",
            "WaningCrescentMoon",
        ]
        .into_iter()
        .collect();
        for m in MUSHROOMS {
            for p in m.robust_phases.iter().chain(m.poor_phases.iter()) {
                assert!(valid.contains(p), "{} has unknown phase {p}", m.name);
            }
        }
    }
}
