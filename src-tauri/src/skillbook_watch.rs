/// Auto-watchword generation for skillbooks.
///
/// Composes the landed pieces for TODO #17 (no new infrastructure):
/// - [`crate::game_data::GameData::skillbooks_by_skill`] CDN index (read via
///   [`crate::cdn_commands::get_skillbooks_for_skill`]),
/// - [`crate::db::character_commands::get_snapshot_skill_abilities`] for the
///   player's known abilities (migration v74 `snapshot_skill_abilities`),
/// - [`crate::settings::WatchRule`] with `source: Some("auto_skillbook")`
///   (PR #152), evaluated by [`crate::watch_rules::evaluate_rules`].
///
/// The command takes caller-supplied `known_abilities` / `owned_names` (fetched
/// via the existing snapshot / game-state commands, which carry the
/// character+server scoping) so this module stays a pure mapping + filter +
/// rule-builder with no DB access and no new store. Rules are written through
/// the existing settings `watch_rules` path by the caller.
use std::collections::HashSet;

use tauri::State;

use crate::cdn_commands::GameDataState;
use crate::game_data::{GameData, SkillbookInfo};
use crate::settings::{ConditionMatch, WatchCondition, WatchNotifyConfig, WatchRule};

/// `WatchRule.source` marker for auto-generated skillbook rules.
pub const AUTO_SKILLBOOK_SOURCE: &str = "auto_skillbook";

/// Look up all skillbook items for one skill, resolving display names,
/// internal names, and numeric IDs exactly like `get_skillbooks_for_skill`.
pub fn books_for_skill(data: &GameData, skill_name: &str) -> Vec<SkillbookInfo> {
    let key = data
        .resolve_skill(skill_name)
        .map(|s| s.internal_name.clone())
        .unwrap_or_else(|| skill_name.to_string());
    data.skillbooks_by_skill.get(&key).cloned().unwrap_or_default()
}

/// Merge per-skill book lists, deduping by `item_id` (one book teaches one
/// ability but may be reachable under two skill keys, e.g. the "Unknown"
/// fallback). Sorted by `item_id` for deterministic rule output.
pub fn merge_books(lists: Vec<Vec<SkillbookInfo>>) -> Vec<SkillbookInfo> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for list in lists {
        for book in list {
            if seen.insert(book.item_id) {
                out.push(book);
            }
        }
    }
    out.sort_by_key(|b| b.item_id);
    out
}

/// Keep only books the player could still learn: drop books whose bestowed
/// ability is already known and books the player already owns.
///
/// `known_abilities` holds ability keys (`BestowAbility` space, same as
/// `snapshot_skill_abilities.ability_key`); matched exactly plus a lowercase
/// fallback so a case drift between CDN and report import can't re-add a
/// known book. `owned_names` holds item display names in any case.
pub fn filter_learnable_books(
    books: Vec<SkillbookInfo>,
    known_abilities: &[String],
    owned_names: &[String],
) -> Vec<SkillbookInfo> {
    let mut known = HashSet::with_capacity(known_abilities.len() * 2);
    for ability in known_abilities {
        known.insert(ability.clone());
        known.insert(ability.to_lowercase());
    }
    let owned: HashSet<String> = owned_names.iter().map(|n| n.to_lowercase()).collect();
    books
        .into_iter()
        .filter(|book| {
            !known.contains(&book.ability)
                && !known.contains(&book.ability.to_lowercase())
                && !owned.contains(&book.name.to_lowercase())
        })
        .collect()
}

/// Build one auto watch rule per book: `ContainsItemLink` on the exact book
/// name (chat item links render as `[Item: <name>]`), toast + highlight, no
/// sound. Books already covered by `existing_names` (lowercased condition
/// values of current auto rules) are skipped so regeneration never duplicates.
pub fn build_auto_skillbook_rules(
    books: &[SkillbookInfo],
    existing_names: &[String],
    next_id: u64,
) -> Vec<WatchRule> {
    let existing: HashSet<String> = existing_names.iter().map(|n| n.to_lowercase()).collect();
    books
        .iter()
        .filter(|book| !existing.contains(&book.name.to_lowercase()))
        .enumerate()
        .map(|(i, book)| WatchRule {
            id: next_id + i as u64,
            name: format!("Skillbook: {}", book.name),
            enabled: true,
            channels: None,
            match_mode: ConditionMatch::Any,
            conditions: vec![WatchCondition::ContainsItemLink(book.name.clone())],
            notify: WatchNotifyConfig {
                sound: false,
                toast: true,
                highlight: true,
            },
            source: Some(AUTO_SKILLBOOK_SOURCE.to_string()),
        })
        .collect()
}

/// Suggest auto-watch rules for `skills`: CDN lookup per skill, cross-skill
/// dedupe, known/owned filtering, then one rule per remaining book.
///
/// `existing_names` should be the condition values of the caller's current
/// `source:"auto_skillbook"` rules so re-running never duplicates; `next_id`
/// seeds rule ids (`Date.now()` from the caller, matching the manual editor).
#[tauri::command]
pub async fn suggest_auto_skillbook_rules(
    skills: Vec<String>,
    known_abilities: Vec<String>,
    owned_names: Vec<String>,
    existing_names: Vec<String>,
    next_id: u64,
    state: State<'_, GameDataState>,
) -> Result<Vec<WatchRule>, String> {
    let data = state.read().await;
    let lists: Vec<Vec<SkillbookInfo>> = skills.iter().map(|s| books_for_skill(&data, s)).collect();
    drop(data);
    let learnable = filter_learnable_books(merge_books(lists), &known_abilities, &owned_names);
    Ok(build_auto_skillbook_rules(&learnable, &existing_names, next_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_books() -> Vec<SkillbookInfo> {
        vec![
            SkillbookInfo {
                item_id: 3,
                name: "Unarmed Basics".to_string(),
                skill: "Unarmed".to_string(),
                ability: "ability_300".to_string(),
            },
            SkillbookInfo {
                item_id: 1,
                name: "Sword Basics".to_string(),
                skill: "Sword".to_string(),
                ability: "ability_100".to_string(),
            },
            SkillbookInfo {
                item_id: 2,
                name: "Sword Advanced".to_string(),
                skill: "Sword".to_string(),
                ability: "ability_200".to_string(),
            },
        ]
    }

    fn stub_data() -> GameData {
        use crate::game_data::SkillInfo;
        let mut data = GameData::empty();
        data.skills.insert(
            11,
            SkillInfo {
                id: 11,
                name: "Sword".to_string(),
                internal_name: "SwordInternal".to_string(),
                description: None,
                icon_id: None,
                xp_table: None,
                keywords: Vec::new(),
                combat: None,
                max_bonus_levels: None,
                parents: Vec::new(),
                advancement_table: None,
                guest_level_cap: None,
                hide_when_zero: None,
                advancement_hints: None,
                rewards: None,
                reports: None,
                raw_json: serde_json::Value::Null,
            },
        );
        data.skill_name_index.insert("Sword".to_string(), 11);
        data.skill_internal_name_index.insert("SwordInternal".to_string(), 11);
        data.skillbooks_by_skill.insert(
            "SwordInternal".to_string(),
            vec![SkillbookInfo {
                item_id: 1,
                name: "Sword Basics".to_string(),
                skill: "SwordInternal".to_string(),
                ability: "ability_100".to_string(),
            }],
        );
        data
    }

    #[test]
    fn books_for_skill_resolves_display_internal_numeric_and_unknown() {
        let data = stub_data();
        // Display name, internal name, and numeric ID all reach the same index entry.
        for reference in ["Sword", "SwordInternal", "11"] {
            let books = books_for_skill(&data, reference);
            assert_eq!(books.len(), 1, "reference {reference} should resolve");
            assert_eq!(books[0].name, "Sword Basics");
        }
        // Unresolvable input falls back to the raw string, which has no entry.
        assert!(books_for_skill(&data, "NoSuchSkill").is_empty());
    }

    #[test]
    fn merge_dedupes_shared_book_and_sorts_by_item_id() {
        // Same item reachable under two skill keys yields one entry.
        let shared = SkillbookInfo {
            item_id: 7,
            name: "Shared Primer".to_string(),
            skill: "Unknown".to_string(),
            ability: "ability_700".to_string(),
        };
        let merged = merge_books(vec![fixture_books(), vec![shared.clone(), shared]]);
        let ids: Vec<u32> = merged.iter().map(|b| b.item_id).collect();
        assert_eq!(ids, vec![1, 2, 3, 7]);
    }

    #[test]
    fn filter_excludes_known_ability_and_owned_book() {
        // Known skill (Sword Basics' ability learned) + owned book
        // (Unarmed Basics in inventory) excluded; Sword Advanced included.
        let learnable = filter_learnable_books(
            fixture_books(),
            &["ability_100".to_string()],
            &["UNARMED BASICS".to_string()],
        );
        let names: Vec<&str> = learnable.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["Sword Advanced"]);
    }

    #[test]
    fn filter_known_match_is_case_insensitive() {
        let learnable =
            filter_learnable_books(fixture_books(), &["ABILITY_200".to_string()], &[]);
        let names: Vec<&str> = learnable.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["Unarmed Basics", "Sword Basics"]);
    }

    #[test]
    fn build_rules_mark_source_and_skip_existing() {
        let books = filter_learnable_books(fixture_books(), &[], &[]);
        // Pretend "Sword Basics" is already covered by a current auto rule.
        let rules = build_auto_skillbook_rules(&books, &["sword basics".to_string()], 9000);
        assert_eq!(rules.len(), 2);
        for (i, rule) in rules.iter().enumerate() {
            assert_eq!(rule.id, 9000 + i as u64);
            assert_eq!(rule.source.as_deref(), Some(AUTO_SKILLBOOK_SOURCE));
            assert!(rule.enabled);
            assert!(rule.channels.is_none());
            assert!(matches!(rule.match_mode, ConditionMatch::Any));
            assert!(!rule.notify.sound && rule.notify.toast && rule.notify.highlight);
            assert_eq!(rule.conditions.len(), 1);
            assert!(matches!(
                &rule.conditions[0],
                WatchCondition::ContainsItemLink(_)
            ));
        }
        let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Skillbook: Unarmed Basics", "Skillbook: Sword Advanced"]);
    }

    #[test]
    fn build_rules_empty_when_everything_filtered() {
        let books = filter_learnable_books(
            fixture_books(),
            &[
                "ability_100".to_string(),
                "ability_200".to_string(),
                "ability_300".to_string(),
            ],
            &[],
        );
        assert!(books.is_empty());
        assert!(build_auto_skillbook_rules(&books, &[], 1).is_empty());
    }

    #[test]
    fn suggestion_pipeline_end_to_end_shape() {
        // Maps the brief's fixture proof: known skill book excluded (ability
        // known), owned book excluded (in inventory), unowned unknown book
        // included as exactly one auto rule.
        let books = fixture_books();
        let learnable = filter_learnable_books(
            books,
            &["ability_100".to_string()],
            &["Unarmed Basics".to_string()],
        );
        let rules = build_auto_skillbook_rules(&learnable, &[], 42);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, 42);
        assert_eq!(rules[0].name, "Skillbook: Sword Advanced");
        assert_eq!(rules[0].source.as_deref(), Some(AUTO_SKILLBOOK_SOURCE));
    }
}
