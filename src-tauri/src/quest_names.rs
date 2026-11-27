//! Quest-id → display-name resolution for `[JSON:StartQuest]` accepts.
//!
//! The game log carries only a numeric id (`{"QuestId":25542}`); the name
//! resolves CDN-first (`quests.json` key `quest_<id>` → `Name`), else via the
//! daily fallback maps below, else `None` (id-only, honestly, never dropped).
//!
//! Fallback source: `gorgon_monitor.py:627-644` (copied verbatim, prefixes
//! included — CDN `Name` values carry the same prefix, e.g.
//! `"Aurest Mission: Labyrinth Chests"`, so either tier yields identical
//! strings). The Queen's dailies (25541-44) are absent from the checked-in
//! CDN snapshot, so the fallback is the live path for those, not just
//! offline insurance.

use crate::game_data::GameData;

/// Full CDN-`Name`-shaped fallback for a daily quest id, else `None`.
/// Verbatim from the monitor maps (prefix + entry, no re-deriving).
pub fn fallback_quest_name(quest_id: u32) -> Option<&'static str> {
    Some(match quest_id {
        // Aurest Mission (prefix 'Aurest Mission: ').
        25521 => "Aurest Mission: Labyrinth Chests",
        25522 => "Aurest Mission: Claudia's Hoard",
        25523 => "Aurest Mission: Gazluk Assassination",
        25524 => "Aurest Mission: Gazluk's Dark Basement",
        25525 => "Aurest Mission: Rahu Sewers",
        25526 => "Aurest Mission: Wintertide Assault",
        25527 => "Aurest Mission: Wintertide Sabotage",
        // Agrashab's Bounty (prefix "Agrashab's Bounty: ").
        25104 => "Agrashab's Bounty: Phoenices",
        25105 => "Agrashab's Bounty: Fire Droaches",
        25106 => "Agrashab's Bounty: Shocking Droaches",
        25107 => "Agrashab's Bounty: Basilisk Controllers",
        25108 => "Agrashab's Bounty: Winter War-Trolls",
        // Queen's Request (prefix "Queen's Request: ").
        25541 => "Queen's Request: Mope's Dark Innards",
        25542 => "Queen's Request: Mope's Mantis Infestation",
        25543 => "Queen's Request: Nightmare Ingredients",
        25544 => "Queen's Request: Precious Crypt Bones",
        _ => return None,
    })
}

/// Best-effort display name for a quest id: CDN `Name` first, daily fallback
/// second, `None` (id-only) when neither knows the id.
pub fn quest_display_name(game_data: &GameData, quest_id: u32) -> Option<String> {
    // CDN keys are `quest_<id>` (verified: quest_25521..27 Aurest and
    // quest_25104..08 Agrashab in docs/CDN-full-examples/quests.json).
    if let Some(quest) = game_data.quests.get(&format!("quest_{quest_id}"))
        && let Some(name) = quest.raw.get("Name").and_then(|n| n.as_str())
        && !name.is_empty()
    {
        return Some(name.to_string());
    }
    fallback_quest_name(quest_id).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_data_with_quest(key: &str, name: &str) -> GameData {
        let mut gd = GameData::empty();
        gd.quests.insert(
            key.to_string(),
            crate::game_data::QuestInfo {
                internal_name: name.to_string(),
                raw: serde_json::json!({ "Name": name }),
            },
        );
        gd
    }

    #[test]
    fn cdn_name_wins_over_fallback() {
        // Same full string either tier yields for dailies, so assert with a
        // non-daily id where the tiers visibly differ.
        let gd = game_data_with_quest("quest_9001", "CDN-Side Name");
        assert_eq!(
            quest_display_name(&gd, 9001).as_deref(),
            Some("CDN-Side Name")
        );
    }

    #[test]
    fn cdn_daily_resolves_by_name() {
        let gd = game_data_with_quest(
            "quest_25521",
            "Aurest Mission: Labyrinth Chests",
        );
        assert_eq!(
            quest_display_name(&gd, 25521).as_deref(),
            Some("Aurest Mission: Labyrinth Chests")
        );
    }

    #[test]
    fn fallback_covers_all_three_daily_sets() {
        // The checked-in CDN snapshot lacks the Queen's ids (25541-44), so
        // the fallback is the live path there, not just offline insurance.
        assert_eq!(
            fallback_quest_name(25521),
            Some("Aurest Mission: Labyrinth Chests")
        );
        assert_eq!(
            fallback_quest_name(25104),
            Some("Agrashab's Bounty: Phoenices")
        );
        assert_eq!(
            fallback_quest_name(25542),
            Some("Queen's Request: Mope's Mantis Infestation")
        );
        // CDN-absent daily still resolves through the fallback tier.
        let gd = GameData::empty();
        assert_eq!(
            quest_display_name(&gd, 25542).as_deref(),
            Some("Queen's Request: Mope's Mantis Infestation")
        );
    }

    #[test]
    fn unknown_id_stays_id_only() {
        let gd = GameData::empty();
        assert_eq!(fallback_quest_name(999999), None);
        assert_eq!(quest_display_name(&gd, 999999), None);
        // CDN row without a Name yields the fallback, never the internal key.
        let gd = game_data_with_quest("quest_25521", "Aurest Mission: Labyrinth Chests");
        let mut gd_nameless = gd;
        if let Some(q) = gd_nameless.quests.get_mut("quest_25521") {
            q.raw = serde_json::Value::Null;
        }
        assert_eq!(
            quest_display_name(&gd_nameless, 25521).as_deref(),
            Some("Aurest Mission: Labyrinth Chests")
        );
    }
}
