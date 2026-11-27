/// Ephemeral auto-watch set for skillbooks.
///
/// Composes the landed pieces for TODO #17:
/// - [`crate::game_data::GameData::skillbooks_by_skill`] CDN index (read via
///   [`crate::cdn_commands::get_skillbooks_for_skill`]),
/// - [`crate::db::character_commands::get_snapshot_skill_abilities`] for the
///   player's known abilities (migration v74 `snapshot_skill_abilities`),
/// - [`crate::watch_rules::evaluate_rules`] as the single matcher.
///
/// Generated rules live ONLY in [`AutoWatchState`] (in-memory, rebuilt at
/// session load and on character switch, never persisted — manual
/// `watch_rules` in settings are the only persisted rules). The matcher
/// evaluates both slices per message (see `DispatchCtx::auto_watch_rules`).
/// The `source: Some("auto_skillbook")` marker (PR #152) is kept so session
/// rows stay distinguishable from manual rules in transit and in the UI.
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use tauri::State;

use crate::cdn_commands::GameDataState;
use crate::game_data::{GameData, SkillbookInfo};
use crate::settings::{ConditionMatch, WatchCondition, WatchNotifyConfig, WatchRule};

/// In-memory session set of auto-generated skillbook rules. Managed Tauri
/// state (`Arc` snapshot per read so the per-message matcher never blocks
/// writers). Starts empty; rebuilt by the frontend-driven generation flow.
#[derive(Debug, Default)]
pub struct AutoWatchState(pub RwLock<Arc<Vec<WatchRule>>>);

impl AutoWatchState {
    /// Cheap snapshot for the matcher: clones the `Arc`, not the rules.
    pub fn snapshot(&self) -> Arc<Vec<WatchRule>> {
        self.0.read().map(|g| Arc::clone(&g)).unwrap_or_default()
    }
}

/// Replace the session set (frontend-driven generation; never persisted).
#[tauri::command]
pub fn set_ephemeral_auto_rules(rules: Vec<WatchRule>, state: State<'_, AutoWatchState>) {
    *state.0.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(rules);
}

/// Read the session set (UI mirror; the matcher reads the same state).
#[tauri::command]
pub fn get_ephemeral_auto_rules(state: State<'_, AutoWatchState>) -> Vec<WatchRule> {
    state.snapshot().as_ref().clone()
}

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

/// Drop whole skills before lookup (mode-1 per-skill block-list). Matches the
/// raw input and the resolved internal name, both case-insensitively, so a
/// blocked display name also catches its internal form and vice versa.
pub fn apply_skill_block_list(
    skills: &[String],
    blocked_skills: &[String],
    data: &GameData,
) -> Vec<String> {
    let blocked: HashSet<String> = blocked_skills.iter().map(|s| s.to_lowercase()).collect();
    if blocked.is_empty() {
        return skills.to_vec();
    }
    skills
        .iter()
        .filter(|skill| {
            let raw_hit = blocked.contains(&skill.to_lowercase());
            let resolved_hit = data
                .resolve_skill(skill)
                .map(|s| blocked.contains(&s.internal_name.to_lowercase()))
                .unwrap_or(false);
            !raw_hit && !resolved_hit
        })
        .cloned()
        .collect()
}

/// Drop books the player dismissed (persisted dismissal list, display names,
/// case-insensitive — same keying as `owned_names`).
pub fn apply_dismissals(books: Vec<SkillbookInfo>, dismissed_names: &[String]) -> Vec<SkillbookInfo> {
    if dismissed_names.is_empty() {
        return books;
    }
    let dismissed: HashSet<String> = dismissed_names.iter().map(|n| n.to_lowercase()).collect();
    books
        .into_iter()
        .filter(|book| !dismissed.contains(&book.name.to_lowercase()))
        .collect()
}

/// Build one auto watch rule per book: `ContainsItemLink` on the exact book
/// name (chat item links render as `[Item: <name>]`), toast + highlight, no
/// sound. Books already covered by `existing_names` (lowercased condition
/// values of current manual rules) are skipped so regeneration never duplicates.
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

/// Suggest auto-watch rules for `skills`: block-list, CDN lookup per skill,
/// cross-skill dedupe, known/owned/dismissed filtering, then one rule per
/// remaining book.
///
/// `existing_names` should be the condition values of the caller's persisted
/// (manual) rules so the same book never double-fires; `next_id` seeds rule
/// ids (`Date.now()` from the caller, matching the manual editor). Pure over
/// `&GameData` so generation determinism is pinnable without IPC or a DB.
#[allow(clippy::too_many_arguments)]
pub fn suggest_inner(
    skills: &[String],
    blocked_skills: &[String],
    known_abilities: &[String],
    owned_names: &[String],
    dismissed_names: &[String],
    existing_names: &[String],
    enabled: bool,
    next_id: u64,
    data: &GameData,
) -> Vec<WatchRule> {
    // Master toggle OFF means zero generation; manual rules are unaffected
    // (they never flow through here).
    if !enabled {
        return Vec::new();
    }
    let skills = apply_skill_block_list(skills, blocked_skills, data);
    let lists: Vec<Vec<SkillbookInfo>> = skills.iter().map(|s| books_for_skill(data, s)).collect();
    let learnable = filter_learnable_books(merge_books(lists), known_abilities, owned_names);
    let learnable = apply_dismissals(learnable, dismissed_names);
    build_auto_skillbook_rules(&learnable, existing_names, next_id)
}

/// Book name carried by an auto rule name (`"Skillbook: <book>"` — the
/// invariant `build_auto_skillbook_rules` establishes). `None` for manual
/// (or renamed) rules, which the backstop then keeps unconditionally.
pub fn auto_hit_book_name(rule_name: &str) -> Option<&str> {
    rule_name.strip_prefix("Skillbook: ")
}

/// True when any persisted (manual) rule already watches `book`: a
/// `ContainsItemLink`/`ContainsText` condition value containing the book name
/// — or contained in it, so a broader manual watch (e.g. `SWORD`) also counts
/// as covering (case-insensitive). Sender-scoped conditions never cover a book.
/// (A one-letter manual rule covers everything; correctly so — it fires on
/// every message and would double-fire with any session twin.)
pub fn manual_covers_book(manual_rules: &[WatchRule], book: &str) -> bool {
    let needle = book.to_lowercase();
    manual_rules
        .iter()
        .filter(|r| r.enabled)
        .flat_map(|r| &r.conditions)
        .any(|c| match c {
            WatchCondition::ContainsItemLink(value) | WatchCondition::ContainsText(value) => {
                let v = value.to_lowercase();
                v.contains(&needle) || needle.contains(&v)
            }
            WatchCondition::FromSender(_) => false,
        })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn suggest_auto_skillbook_rules(
    skills: Vec<String>,
    blocked_skills: Vec<String>,
    known_abilities: Vec<String>,
    owned_names: Vec<String>,
    dismissed_names: Vec<String>,
    existing_names: Vec<String>,
    enabled: bool,
    next_id: u64,
    state: State<'_, GameDataState>,
) -> Result<Vec<WatchRule>, String> {
    let data = state.read().await;
    Ok(suggest_inner(
        &skills,
        &blocked_skills,
        &known_abilities,
        &owned_names,
        &dismissed_names,
        &existing_names,
        enabled,
        next_id,
        &data,
    ))
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

    #[derive(Default)]
    struct SuggestArgs {
        skills: Vec<String>,
        blocked: Vec<String>,
        known: Vec<String>,
        owned: Vec<String>,
        dismissed: Vec<String>,
        existing: Vec<String>,
        enabled: bool,
        next_id: u64,
    }

    fn suggest_args() -> SuggestArgs {
        SuggestArgs {
            skills: vec!["Sword".to_string()],
            enabled: true,
            next_id: 7,
            ..Default::default()
        }
    }

    fn suggest_with(data: &GameData, args: &SuggestArgs) -> Vec<WatchRule> {
        suggest_inner(
            &args.skills,
            &args.blocked,
            &args.known,
            &args.owned,
            &args.dismissed,
            &args.existing,
            args.enabled,
            args.next_id,
            data,
        )
    }

    #[test]
    fn toggle_off_means_empty() {
        let data = stub_data();
        let mut args = suggest_args();
        assert_eq!(suggest_with(&data, &args).len(), 1);
        args.enabled = false;
        assert!(suggest_with(&data, &args).is_empty());
    }

    #[test]
    fn block_list_drops_skill_by_display_or_internal_name() {
        let data = stub_data();
        let mut args = suggest_args();
        // Display-name block.
        args.blocked = vec!["sword".to_string()];
        assert!(suggest_with(&data, &args).is_empty());
        // Internal-name block catches the same skill.
        args.blocked = vec!["SWORDINTERNAL".to_string()];
        assert!(suggest_with(&data, &args).is_empty());
        // Unrelated block leaves the skill alone.
        args.blocked = vec!["Unarmed".to_string()];
        assert_eq!(suggest_with(&data, &args).len(), 1);
    }

    #[test]
    fn dismissals_drop_books_case_insensitively() {
        let data = stub_data();
        let mut args = suggest_args();
        args.dismissed = vec!["SWORD BASICS".to_string()];
        assert!(suggest_with(&data, &args).is_empty());
    }

    #[test]
    fn generation_is_deterministic_and_manual_independent() {
        let data = stub_data();
        let args = suggest_args();
        let first = suggest_with(&data, &args);
        let second = suggest_with(&data, &args);
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
        // A persisted manual rule covering the book suppresses its session
        // twin; the manual list itself is only read, never mutated.
        let mut args = suggest_args();
        let manual_condition = "Sword Basics".to_string();
        args.existing = vec![manual_condition.clone()];
        assert!(suggest_with(&data, &args).is_empty());
        assert_eq!(args.existing, vec![manual_condition]);
        // Every generated rule carries the session marker, never a manual shape.
        assert!(first.iter().all(|r| r.source.as_deref() == Some(AUTO_SKILLBOOK_SOURCE)));
    }

    #[test]
    fn backstop_parses_prefix_and_covers_item_conditions() {
        assert_eq!(auto_hit_book_name("Skillbook: Sword Basics"), Some("Sword Basics"));
        assert_eq!(auto_hit_book_name("Manual"), None);
        let item_rule = WatchRule {
            id: 1,
            name: "Manual".to_string(),
            enabled: true,
            channels: None,
            match_mode: ConditionMatch::Any,
            conditions: vec![WatchCondition::ContainsItemLink("sword basics".to_string())],
            notify: WatchNotifyConfig { sound: false, toast: true, highlight: true },
            source: None,
        };
        let text_rule = WatchRule {
            id: 2,
            name: "Manual text".to_string(),
            enabled: true,
            channels: None,
            match_mode: ConditionMatch::Any,
            conditions: vec![WatchCondition::ContainsText("SWORD".to_string())],
            notify: WatchNotifyConfig { sound: false, toast: true, highlight: true },
            source: None,
        };
        let sender_rule = WatchRule {
            id: 3,
            name: "From someone".to_string(),
            enabled: true,
            channels: None,
            match_mode: ConditionMatch::Any,
            conditions: vec![WatchCondition::FromSender("TraderJoe".to_string())],
            notify: WatchNotifyConfig { sound: false, toast: true, highlight: true },
            source: None,
        };
        // Item/text coverage suppresses; sender scope never covers a book.
        assert!(manual_covers_book(std::slice::from_ref(&item_rule), "Sword Basics"));
        assert!(manual_covers_book(std::slice::from_ref(&text_rule), "Sword Basics"));
        assert!(!manual_covers_book(std::slice::from_ref(&sender_rule), "Sword Basics"));
        assert!(!manual_covers_book(&[item_rule, text_rule, sender_rule], "Unarmed Basics"));
    }

    #[test]
    fn disabled_manual_rule_does_not_cover_book() {
        let mk = |enabled: bool| WatchRule {
            id: 9,
            name: "Manual".to_string(),
            enabled,
            channels: None,
            match_mode: ConditionMatch::Any,
            conditions: vec![WatchCondition::ContainsItemLink("Sword Basics".to_string())],
            notify: WatchNotifyConfig { sound: false, toast: true, highlight: true },
            source: None,
        };
        // Enabled manual twin suppresses the auto rule (no double-fire).
        assert!(manual_covers_book(&[mk(true)], "Sword Basics"));
        // Disabled manual rule fires nothing (evaluate_rules skips it), so it
        // must not suppress the auto twin either — else the book goes silent.
        assert!(!manual_covers_book(&[mk(false)], "Sword Basics"));
    }
}
