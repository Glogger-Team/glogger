use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, Default)]
pub struct NpcPreference {
    pub name: Option<String>,
    pub desire: String,
    pub keywords: Vec<String>,
    pub pref: f32,
}

#[derive(Debug, Serialize, Clone)]
pub struct NpcInfo {
    pub key: String,
    pub name: String,
    pub desc: Option<String>,
    pub area_name: Option<String>,
    pub area_friendly_name: Option<String>,
    pub trains_skills: Vec<String>,
    pub preferences: Vec<NpcPreference>,
    pub gift_favor_tiers: Vec<String>,

    // Phase 4 typed fields
    pub pos: Option<Value>,
    pub services: Option<Vec<Value>>,

    // Full raw JSON
    pub raw_json: Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<String, NpcInfo>, String> {
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "npcs.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut npcs = HashMap::with_capacity(raw.len());
    for (key, value) in raw {
        // Basic stub: just store the key and name if available
        let name = value
            .get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or(&key)
            .to_string();

        let desc = value
            .get("Description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let area_name = value
            .get("AreaName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let area_friendly_name = value
            .get("AreaFriendlyName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract skills from Services array where Type == "Training"
        let trains_skills = value
            .get("Services")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|service| {
                        service
                            .get("Type")
                            .and_then(|t| t.as_str())
                            .map(|t| t == "Training")
                            .unwrap_or(false)
                    })
                    .flat_map(|service| {
                        service
                            .get("Skills")
                            .and_then(|s| s.as_array())
                            .map(|skills| {
                                skills
                                    .iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse preferences
        let preferences = value
            .get("Preferences")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|pref_val| {
                        Some(NpcPreference {
                            name: pref_val
                                .get("Name")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            desire: pref_val
                                .get("Desire")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown")
                                .to_string(),
                            keywords: pref_val
                                .get("Keywords")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            pref: pref_val.get("Pref").and_then(|v| v.as_f64()).unwrap_or(0.0)
                                as f32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse gift favor tiers (favor levels at which this NPC accepts gifts)
        let gift_favor_tiers = value
            .get("ItemGifts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let pos = value.get("Pos").cloned();
        let services = value.get("Services").and_then(|v| v.as_array().cloned());

        npcs.insert(
            key.clone(),
            NpcInfo {
                key,
                name,
                desc,
                area_name,
                area_friendly_name,
                trains_skills,
                preferences,
                gift_favor_tiers,
                pos,
                services,
                raw_json: value,
            },
        );
    }

    Ok(npcs)
}

// ── Gift-cap region ──────────────────────────────────────────────────────────

/// The areas where the game caps weekly gifts per NPC. The game's own cap note
/// names the rule — "Statehelm townsfolk are more sophisticated than villagers
/// elsewhere. It would be uncouth to receive too many gifts per week … will
/// accept up to <b>5</b> gifts per calendar week and has received <b>N</b> so
/// far this week. (The week resets on Monday EDT.)" — and it prints even at zero
/// usage (verified in `AreaStatehelm` at Gossamyr the Great and in the sewers:
/// Hamilton, Smoke Tail, Fuzzlebun), so it doubles as the NPC's giftability
/// marker.
///
/// Used to keep *scripted* favor payouts out of the gift log: the game sets the
/// gift flag on those too — Gossamyr's card reading logs
/// `ProcessDeltaFavor(33081, "NPC_Gossamyr", 25, True)` — while a capped NPC's
/// authoritative weekly count arrives in the note, so an `is_gift` delta for one
/// would record a gift the player never spent.
///
/// Twin of the area test in `statehelmNpcs`
/// (`src/composables/useStatehelmTracker.ts`); a newly capped region must be
/// added in both places.
pub const GIFT_CAPPED_AREAS: &[&str] = &["areastatehelm", "areastatehelmcaves"];

/// True when an NPC's `AreaName` is one the game caps weekly gifts in
/// (case-insensitive). `None` — an NPC whose CDN entry carries no area, or one
/// that could not be resolved because the game data was not loaded — is not
/// treated as capped, which leaves the delta-derived gift in place for the cap
/// note to correct.
pub fn is_gift_capped_area(area_name: Option<&str>) -> bool {
    area_name.is_some_and(|area| {
        GIFT_CAPPED_AREAS
            .iter()
            .any(|capped| area.eq_ignore_ascii_case(capped))
    })
}

// ── Gift tastes ──────────────────────────────────────────────────────────────

/// True when the CDN lists any gift data for the NPC — item tastes
/// (`Preferences`) or gift tiers (`ItemGifts`).
///
/// This is the rule behind "gifting cannot move this NPC's reputation, so its
/// weekly count is noise": with no tastes there is no "appropriate gift".
/// Verified 2026-10-16 against the logs — every NPC whose favor deltas are
/// preference-scaled (a real gift: Zealo 49.84, Merriana 797.92, Corinth
/// 58.97, Selaxi 5.90) has a non-empty `Preferences`, while the NPCs whose
/// rows are flat-only have none: Gossamyr the Great (his accepted gift emitted
/// **no** `ProcessDeltaFavor` line at all — the game accepted and capped it,
/// but his reputation only moves via his card reading) and Cheb (flat 25s from
/// a repeatable quest, `npc_id = 0`).
///
/// Mirror of the `giftTracking` field in `useStatehelmTracker.ts`; the two must
/// change together.
pub fn has_gift_tastes(info: &NpcInfo) -> bool {
    !info.preferences.is_empty() || !info.gift_favor_tiers.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gift_capped_areas_cover_the_statehelm_region_only() {
        assert!(is_gift_capped_area(Some("AreaStatehelm")));
        assert!(is_gift_capped_area(Some("AreaStatehelmCaves")));
        // The CDN's own casing is the same, but the rule must not depend on it.
        assert!(is_gift_capped_area(Some("areastatehelm")));
        // Uncapped neighbours and missing areas stay gifts-counted-by-delta.
        // (`AreaStatehelmCaves2` — "Statehelm Depths" — is deliberately not in
        // the list: no NPC there has been observed showing the note, and the
        // tracker's filter excludes it too.)
        assert!(!is_gift_capped_area(Some("AreaKurMountains")));
        assert!(!is_gift_capped_area(None));
    }

    #[test]
    fn parse_keeps_the_area_the_cap_rule_reads() {
        let npcs = parse(
            r#"{"NPC_Gossamyr": {"Name": "Gossamyr the Great", "AreaName": "AreaStatehelm"}}"#,
        )
        .expect("parse");
        assert_eq!(
            npcs["NPC_Gossamyr"].area_name.as_deref(),
            Some("AreaStatehelm")
        );
    }

    /// Either CDN gift field marks an NPC as worth gift tracking; neither one
    /// means gifting cannot move their reputation. Real values: Selaxi carries
    /// four preferences *and* five gift tiers, Kleave only tiers, Gossamyr
    /// neither (and his accepted gift produced no favor line at all).
    #[test]
    fn gift_data_is_what_marks_an_npc_as_worth_tracking() {
        let npcs = parse(
            r#"{
                 "NPC_Selaxi": { "Name": "Selaxi", "Preferences": [ { "Name": "Gems", "Desire": "Love" } ] },
                 "NPC_Kleave": { "Name": "Kleave", "ItemGifts": ["Friends"] },
                 "NPC_Gossamyr": { "Name": "Gossamyr the Great" }
               }"#,
        )
        .expect("parse");
        assert!(has_gift_tastes(&npcs["NPC_Selaxi"]), "item tastes");
        assert!(
            has_gift_tastes(&npcs["NPC_Kleave"]),
            "gift tiers alone count"
        );
        assert!(
            !has_gift_tastes(&npcs["NPC_Gossamyr"]),
            "no gift data in either field"
        );
    }
}
