/// Unified search engine with Scryfall-inspired structured query syntax.
///
/// Single `unified_search` Tauri command replaces parallel search_* calls
/// from the frontend quick search. Supports plain text, quoted phrases,
/// typed filters (type:item, skill:Sword, area:Serbule, level:30-50,
/// keyword:Food, has:recipe), and exclusion (-keyword:NotObtainable).
use serde::Serialize;
use tauri::State;

use crate::cdn_commands::GameDataState;
use crate::game_data::GameData;

// ── Query parser ─────────────────────────────────────────────────────────────

/// Known filter keys — a colon after any other word is treated as literal text.
const KNOWN_FILTER_KEYS: &[&str] = &[
    "type", "skill", "area", "level", "keyword", "has", "slot", "name",
];

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    /// Free-text terms (lowercased), ANDed together
    pub text_terms: Vec<String>,
    /// Exact-phrase matches (lowercased, without quotes)
    pub exact_phrases: Vec<String>,
    /// Structured filters
    pub filters: Vec<SearchFilter>,
    /// Negated filters (prefixed with -)
    pub negations: Vec<SearchFilter>,
}

#[derive(Debug, Clone)]
pub enum SearchFilter {
    /// type:item, type:npc, etc.
    EntityType(String),
    /// skill:Sword
    Skill(String),
    /// area:Serbule
    Area(String),
    /// level:30 or level:30-50
    Level { min: Option<u32>, max: Option<u32> },
    /// keyword:Food
    Keyword(String),
    /// has:recipe, has:description
    Has(String),
    /// slot:MainHand
    Slot(String),
    /// name:sword (restrict text match to name field only)
    Name(String),
}

impl ParsedQuery {
    pub fn has_type_filter(&self, entity_type: &str) -> bool {
        self.filters
            .iter()
            .any(|f| matches!(f, SearchFilter::EntityType(t) if t == entity_type))
    }

    /// Whether a specific entity type is excluded via negation
    pub fn excludes_type(&self, entity_type: &str) -> bool {
        self.negations
            .iter()
            .any(|f| matches!(f, SearchFilter::EntityType(t) if t == entity_type))
    }

    /// Whether we should search a given entity type based on type: filters.
    /// If no type filter is set, search everything. If type filters exist,
    /// only search those types. Negated types are always excluded.
    pub fn should_search(&self, entity_type: &str) -> bool {
        if self.excludes_type(entity_type) {
            return false;
        }
        let has_type_filters = self
            .filters
            .iter()
            .any(|f| matches!(f, SearchFilter::EntityType(_)));
        if !has_type_filters {
            return true;
        }
        self.has_type_filter(entity_type)
    }

    /// Get skill filter value (lowercased)
    pub fn skill_filter(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Skill(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get area filter value (lowercased)
    pub fn area_filter(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Area(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get level range
    pub fn level_range(&self) -> Option<(Option<u32>, Option<u32>)> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Level { min, max } => Some((*min, *max)),
            _ => None,
        })
    }

    /// Get keyword filter values (lowercased)
    pub fn keyword_filters(&self) -> Vec<&str> {
        self.filters
            .iter()
            .filter_map(|f| match f {
                SearchFilter::Keyword(k) => Some(k.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Get negated skill filter value (lowercased)
    pub fn negated_skill_filter(&self) -> Option<&str> {
        self.negations.iter().find_map(|f| match f {
            SearchFilter::Skill(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get negated area filter value (lowercased)
    pub fn negated_area_filter(&self) -> Option<&str> {
        self.negations.iter().find_map(|f| match f {
            SearchFilter::Area(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get negated level ranges
    pub fn negated_level_ranges(&self) -> Vec<(Option<u32>, Option<u32>)> {
        self.negations
            .iter()
            .filter_map(|f| match f {
                SearchFilter::Level { min, max } => Some((*min, *max)),
                _ => None,
            })
            .collect()
    }

    /// Get negated has: filter values
    pub fn negated_has_filters(&self) -> Vec<&str> {
        self.negations
            .iter()
            .filter_map(|f| match f {
                SearchFilter::Has(h) => Some(h.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Get negated slot filter (lowercased)
    pub fn negated_slot_filter(&self) -> Option<&str> {
        self.negations.iter().find_map(|f| match f {
            SearchFilter::Slot(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get negated name filter (lowercased)
    pub fn negated_name_filter(&self) -> Option<&str> {
        self.negations.iter().find_map(|f| match f {
            SearchFilter::Name(n) => Some(n.as_str()),
            _ => None,
        })
    }

    /// Get negated keyword filter values (lowercased)
    pub fn negated_keyword_filters(&self) -> Vec<&str> {
        self.negations
            .iter()
            .filter_map(|f| match f {
                SearchFilter::Keyword(k) => Some(k.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Get has: filter values
    pub fn has_filters(&self) -> Vec<&str> {
        self.filters.iter().filter_map(|f| match f {
            SearchFilter::Has(h) => Some(h.as_str()),
            _ => None,
        }).collect()
    }

    /// True when a *positive* filter is present that `entity_type` doesn't
    /// understand. Callers return an empty set in that case instead of
    /// silently ignoring the filter. Negated filters for unsupported
    /// properties are vacuously true (the entity can't have that property)
    /// and are ignored. See `filter_applies_to` for the per-type matrix.
    pub fn has_unsupported_filter(&self, entity_type: &str) -> bool {
        self.filters
            .iter()
            .any(|f| !filter_applies_to(f, entity_type))
    }

    /// Get slot filter
    pub fn slot_filter(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Slot(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get name filter (lowercased)
    pub fn name_filter(&self) -> Option<&str> {
        self.filters.iter().find_map(|f| match f {
            SearchFilter::Name(n) => Some(n.as_str()),
            _ => None,
        })
    }
}

pub fn parse_query(input: &str) -> ParsedQuery {
    let mut text_terms = Vec::new();
    let mut exact_phrases = Vec::new();
    let mut filters = Vec::new();
    let mut negations = Vec::new();

    let input = input.trim();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        // Quoted phrase: "exact match"
        if chars[i] == '"' {
            i += 1;
            let start = i;
            while i < len && chars[i] != '"' {
                i += 1;
            }
            let phrase: String = chars[start..i].iter().collect();
            let phrase = phrase.trim().to_lowercase();
            if !phrase.is_empty() {
                exact_phrases.push(phrase);
            }
            if i < len {
                i += 1; // skip closing quote
            }
            continue;
        }

        // Collect a token (up to next whitespace or quote)
        let start = i;
        while i < len && !chars[i].is_whitespace() && chars[i] != '"' {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();

        // Check for negation prefix
        let (negated, token) = if token.starts_with('-') && token.len() > 1 {
            (true, &token[1..])
        } else {
            (false, token.as_str())
        };

        // Check for filter key:value
        if let Some(colon_pos) = token.find(':') {
            let key = &token[..colon_pos].to_lowercase();
            let value = &token[colon_pos + 1..];

            if KNOWN_FILTER_KEYS.contains(&key.as_str()) && !value.is_empty() {
                let filter = match key.as_str() {
                    "type" => SearchFilter::EntityType(value.to_lowercase()),
                    "skill" => SearchFilter::Skill(value.to_lowercase()),
                    "area" => SearchFilter::Area(value.to_lowercase()),
                    "level" => {
                        // Parse "30", "30-50", "-50", "30-"
                        if let Some(dash) = value.find('-') {
                            let min = value[..dash].parse::<u32>().ok();
                            let max = value[dash + 1..].parse::<u32>().ok();
                            SearchFilter::Level { min, max }
                        } else {
                            let exact = value.parse::<u32>().ok();
                            SearchFilter::Level {
                                min: exact,
                                max: exact,
                            }
                        }
                    }
                    "keyword" => SearchFilter::Keyword(value.to_lowercase()),
                    "has" => SearchFilter::Has(value.to_lowercase()),
                    "slot" => SearchFilter::Slot(value.to_lowercase()),
                    "name" => SearchFilter::Name(value.to_lowercase()),
                    _ => unreachable!(),
                };
                if negated {
                    negations.push(filter);
                } else {
                    filters.push(filter);
                }
                continue;
            }
        }

        // Plain text term (re-add the - if it was there but didn't match a filter)
        let full_token = if negated {
            format!("-{}", token)
        } else {
            token.to_string()
        };
        text_terms.push(full_token.to_lowercase());
    }

    ParsedQuery {
        text_terms,
        exact_phrases,
        filters,
        negations,
    }
}

/// Per-type filter scope: which structured filters a search_* fn understands.
/// `skill:` ⇒ items (no — items have no skill field), recipes, npcs
/// (trainers), abilities. `level:` ⇒ items (crafting_target_level) and
/// recipes (skill_level_req — see search_recipes). `name:` ⇒ items only
/// (only search_items restricts matching to the name field). `area:` ⇒
/// npcs and quests. Unknown `has:` values never filter (accepted no-op on
/// items, vacuous on other types). Positive filters outside a type's scope
/// force an empty result for that type (see
/// `ParsedQuery::has_unsupported_filter`) instead of being silently
/// ignored.
fn filter_applies_to(filter: &SearchFilter, entity_type: &str) -> bool {
    match filter {
        SearchFilter::EntityType(_) => true,
        // Text-like filters apply everywhere (matched against the type's
        // searchable fields).
        SearchFilter::Name(_) => entity_type == "item",
        SearchFilter::Skill(_) => matches!(entity_type, "recipe" | "npc" | "ability"),
        SearchFilter::Area(_) => matches!(entity_type, "npc" | "quest"),
        SearchFilter::Level { .. } => matches!(entity_type, "item" | "recipe"),
        SearchFilter::Keyword(_) => entity_type == "item",
        SearchFilter::Has(h) => entity_type == "item" && (h == "recipe" || h == "description"),
        SearchFilter::Slot(_) => entity_type == "item",
    }
}


// ── Search result types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UnifiedSearchResult {
    /// Entity type: "item", "recipe", "npc", "quest", "skill", "ability",
    /// "effect", "enemy", "area", "title", "lorebook"
    pub entity_type: String,
    /// Primary display name
    pub name: String,
    /// Secondary detail line
    pub detail: String,
    /// Entity identifier for navigation (item name, NPC key, quest internal_name, etc.)
    pub entity_id: String,
    /// Relevance score (higher = better match)
    pub score: u32,
    /// Optional icon ID
    pub icon_id: Option<u32>,
}

// ── Relevance scoring ────────────────────────────────────────────────────────

/// Score a text match. Returns 0 if no match.
fn score_match(haystack: &str, query: &str) -> u32 {
    let h = haystack.to_lowercase();
    let q = query;
    if h == q {
        100 // exact match
    } else if h.starts_with(q) {
        80 // starts with
    } else if h.contains(q) {
        60 // contains
    } else {
        0
    }
}

/// Score across multiple fields. Returns the highest score from name, then
/// adds secondary field bonus if those match too.
///
/// Tier note: `.max(1)` at each call site floors a text-term match to score 1,
/// so a row that matched every term but scored 0 on name + secondaries still
/// ranks above a non-match (which is `continue`d out, never pushed). Score 1
/// therefore means "matched all terms, but only in unweighted fields".
fn score_entity(name: &str, secondary_fields: &[&str], query: &str) -> u32 {
    let name_score = score_match(name, query);
    if name_score > 0 {
        return name_score;
    }
    // Check secondary fields with lower base scores
    for field in secondary_fields {
        if field.to_lowercase().contains(query) {
            return 40; // description/secondary match
        }
    }
    0
}

/// Check if text matches all terms (AND logic)
fn matches_all_terms(text: &str, terms: &[String]) -> bool {
    let lower = text.to_lowercase();
    terms.iter().all(|t| lower.contains(t.as_str()))
}

/// Check if text matches all exact phrases
fn matches_all_phrases(text: &str, phrases: &[String]) -> bool {
    let lower = text.to_lowercase();
    phrases.iter().all(|p| lower.contains(p.as_str()))
}

/// Build a combined searchable string from multiple fields
fn combine_fields(fields: &[&str]) -> String {
    fields.join(" ").to_lowercase()
}

// ── Per-type search functions ────────────────────────────────────────────────

fn search_items(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: skill:/area: (and other non-item filters) don't apply
    // to items — return no rows rather than silently ignoring them.
    if query.has_unsupported_filter("item") {
        return Vec::new();
    }
    let mut results = Vec::new();
    let neg_keywords = query.negated_keyword_filters();
    let req_keywords = query.keyword_filters();
    let has = query.has_filters();
    let neg_has = query.negated_has_filters();
    let level_range = query.level_range();
    let neg_levels = query.negated_level_ranges();
    let slot = query.slot_filter();
    let neg_slot = query.negated_slot_filter();
    let name_filter = query.name_filter();
    let neg_name = query.negated_name_filter();

    for item in data.items.values() {
        // Keyword exclusion
        if neg_keywords
            .iter()
            .any(|nk| item.keywords.iter().any(|k| k.to_lowercase() == *nk))
        {
            continue;
        }

        // Required keywords
        if !req_keywords
            .iter()
            .all(|rk| item.keywords.iter().any(|k| k.to_lowercase() == *rk))
        {
            continue;
        }

        // has: filters (positive requires, negated excludes)
        let mut skip = false;
        for h in &has {
            match *h {
                "recipe" if !data.recipes_producing_item.contains_key(&item.id) => {
                    skip = true;
                    break;
                }
                "description" if item.description.is_none() => {
                    skip = true;
                    break;
                }
                _ => {}
            }
        }
        if skip {
            continue;
        }
        for h in &neg_has {
            match *h {
                "recipe" if data.recipes_producing_item.contains_key(&item.id) => {
                    skip = true;
                    break;
                }
                "description" if item.description.is_some() => {
                    skip = true;
                    break;
                }
                _ => {}
            }
        }
        if skip {
            continue;
        }

        // Level range
        // Level range (all ranges — positive and negated — must hold)
        let in_range = |lvl: u32, min: Option<u32>, max: Option<u32>| {
            if let Some(m) = min {
                if lvl < m {
                    return false;
                }
            }
            if let Some(m) = max {
                if lvl > m {
                    return false;
                }
            }
            true
        };
        if let Some((min, max)) = level_range {
            match item.crafting_target_level {
                Some(lvl) => {
                    if !in_range(lvl, min, max) {
                        continue;
                    }
                }
                None => continue,
            }
        }
        match item.crafting_target_level {
            Some(lvl) => {
                if neg_levels.iter().any(|(min, max)| in_range(lvl, *min, *max)) {
                    continue;
                }
            }
            None => {
                if !neg_levels.is_empty() {
                    // Vacuous: an item with no level can't be inside an
                    // excluded range, so it passes.
                }
            }
        }

        // Slot filter
        // Slot filter (positive + negated)
        if let Some(s) = slot {
            match &item.equip_slot {
                Some(es) => {
                    if !es.to_lowercase().contains(s) {
                        continue;
                    }
                }
                None => continue,
            }
        }
        if let Some(ns) = neg_slot {
            if let Some(es) = &item.equip_slot {
                if es.to_lowercase().contains(ns) {
                    continue;
                }
            }
        }

        // Name filter (restricts to name-only matching)
        // Name filter (positive restricts to name-only; negated excludes names)
        if let Some(nf) = name_filter {
            if !item.name.to_lowercase().contains(nf) {
                continue;
            }
        }
        if let Some(nn) = neg_name {
            if item.name.to_lowercase().contains(nn) {
                continue;
            }
        }

        // Text matching
        let searchable = combine_fields(&[
            &item.name,
            item.description.as_deref().unwrap_or(""),
            &item.keywords.join(" "),
            &item.effect_descs.join(" "),
            item.food_desc.as_deref().unwrap_or(""),
        ]);

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            // Score based on the first term against the name
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(
                &item.name,
                &[
                    item.description.as_deref().unwrap_or(""),
                    &item.keywords.join(", "),
                ],
                anchor,
            )
            .max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            // No query at all — skip (require at least some input)
            continue;
        } else {
            // Only filters, no text — everything that passed filters gets base score
            50
        };
        let detail = if !item.keywords.is_empty() {
            item.keywords.join(", ")
        } else {
            item.description.clone().unwrap_or_default()
        };

        results.push(UnifiedSearchResult {
            entity_type: "item".to_string(),
            name: item.name.clone(),
            detail,
            entity_id: item.name.clone(),
            score,
            icon_id: item.icon_id,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_recipes(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: name:/area:/keyword:/has:/slot: don't apply to
    // recipes — return no rows rather than silently ignoring them.
    if query.has_unsupported_filter("recipe") {
        return Vec::new();
    }
    let mut results = Vec::new();
    let skill_filter = query.skill_filter();
    let neg_skill = query.negated_skill_filter();
    let level_range = query.level_range();
    let neg_levels = query.negated_level_ranges();

    for recipe in data.recipes.values() {
        // Skill filter (positive + negated)
        if let Some(sf) = skill_filter {
            match &recipe.skill {
                Some(s) => {
                    if !s.to_lowercase().contains(sf) {
                        continue;
                    }
                }
                None => continue,
            }
        }
        if let Some(ns) = neg_skill {
            if let Some(s) = &recipe.skill {
                if s.to_lowercase().contains(ns) {
                    continue;
                }
            }
        }

        // Level filter on skill_level_req (recipes have no other level).
        // Missing requirement fails a positive range (can't prove a match);
        // a missing requirement passes a negated range (can't be inside it).
        if let Some((min, max)) = level_range {
            match recipe.skill_level_req {
                Some(req) => {
                    let lvl = req as u32;
                    if let Some(m) = min {
                        if lvl < m {
                            continue;
                        }
                    }
                    if let Some(m) = max {
                        if lvl > m {
                            continue;
                        }
                    }
                }
                None => continue,
            }
        }
        if !neg_levels.is_empty() {
            if let Some(req) = recipe.skill_level_req {
                let lvl = req as u32;
                let inside = neg_levels.iter().any(|(min, max)| {
                    if let Some(m) = min {
                        if lvl < *m {
                            return false;
                        }
                    }
                    if let Some(m) = max {
                        if lvl > *m {
                            return false;
                        }
                    }
                    true
                });
                if inside {
                    continue;
                }
            }
        }

        let ingredient_names: Vec<String> = recipe
            .ingredients
            .iter()
            .filter_map(|i| {
                // Resolve ingredient item name from ID
                i.item_id
                    .and_then(|id| data.items.get(&id))
                    .map(|item| item.name.clone())
                    .or_else(|| i.description.clone())
            })
            .collect();
        let result_names: Vec<String> = recipe
            .result_items
            .iter()
            .filter_map(|r| data.items.get(&r.item_id).map(|item| item.name.clone()))
            .collect();

        let searchable = combine_fields(&[
            &recipe.name,
            recipe.description.as_deref().unwrap_or(""),
            recipe.skill.as_deref().unwrap_or(""),
            &ingredient_names.join(" "),
            &result_names.join(" "),
        ]);

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(
                &recipe.name,
                &[recipe.skill.as_deref().unwrap_or("")],
                anchor,
            )
            .max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };
        let detail = [
            recipe.skill.as_deref(),
            recipe
                .skill_level_req
                .map(|l| format!("Lv {}", l as u32))
                .as_deref(),
        ]
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<&str>>()
        .join(" · ");

        results.push(UnifiedSearchResult {
            entity_type: "recipe".to_string(),
            name: recipe.name.clone(),
            detail,
            entity_id: recipe.name.clone(),
            score,
            icon_id: recipe.icon_id,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_npcs(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: keyword:/level:/has:/slot:/name: don't apply to npcs.
    if query.has_unsupported_filter("npc") {
        return Vec::new();
    }
    let mut results = Vec::new();
    let area_filter = query.area_filter();
    let neg_area = query.negated_area_filter();
    let skill_filter = query.skill_filter();
    let neg_skill = query.negated_skill_filter();

    for npc in data.npcs.values() {
        // Area filter (positive + negated)
        if let Some(af) = area_filter {
            let area = npc
                .area_friendly_name
                .as_deref()
                .or(npc.area_name.as_deref())
                .unwrap_or("");
            if !area.to_lowercase().contains(af) {
                continue;
            }
        }
        if let Some(na) = neg_area {
            let area = npc
                .area_friendly_name
                .as_deref()
                .or(npc.area_name.as_deref())
                .unwrap_or("");
            if area.to_lowercase().contains(na) {
                continue;
            }
        }

        // Skill filter (NPCs that train a skill; positive + negated)
        if let Some(sf) = skill_filter {
            if !npc
                .trains_skills
                .iter()
                .any(|s| s.to_lowercase().contains(sf))
            {
                continue;
            }
        }
        if let Some(ns) = neg_skill {
            if npc
                .trains_skills
                .iter()
                .any(|s| s.to_lowercase().contains(ns))
            {
                continue;
            }
        }

        let searchable = combine_fields(&[
            &npc.name,
            npc.desc.as_deref().unwrap_or(""),
            npc.area_friendly_name.as_deref().unwrap_or(""),
        ]);

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(
                &npc.name,
                &[
                    npc.desc.as_deref().unwrap_or(""),
                    npc.area_friendly_name.as_deref().unwrap_or(""),
                ],
                anchor,
            )
            .max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };
        results.push(UnifiedSearchResult {
            entity_type: "npc".to_string(),
            name: npc.name.clone(),
            detail: npc
                .area_friendly_name
                .clone()
                .or(npc.area_name.clone())
                .unwrap_or_default(),
            entity_id: npc.key.clone(),
            score,
            icon_id: None,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_quests(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: skill:/keyword:/level:/has:/slot:/name: don't apply
    // to quests.
    if query.has_unsupported_filter("quest") {
        return Vec::new();
    }
    let mut results = Vec::new();
    let area_filter = query.area_filter();
    let neg_area = query.negated_area_filter();

    for quest in data.quests.values() {
        let name = quest
            .raw
            .get("Name")
            .or_else(|| quest.raw.get("DisplayName"))
            .and_then(|v| v.as_str())
            .unwrap_or(&quest.internal_name);
        let description = quest
            .raw
            .get("Description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let location = quest
            .raw
            .get("DisplayedLocation")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let favor_npc = quest
            .raw
            .get("FavorNpc")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Area filter (positive + negated) on the displayed location
        if let Some(af) = area_filter {
            if !location.to_lowercase().contains(af) {
                continue;
            }
        }
        if let Some(na) = neg_area {
            if location.to_lowercase().contains(na) {
                continue;
            }
        }

        let searchable = combine_fields(&[name, description, location, favor_npc]);

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(name, &[description, location], anchor).max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };
        results.push(UnifiedSearchResult {
            entity_type: "quest".to_string(),
            name: name.to_string(),
            detail: location.to_string(),
            entity_id: quest.internal_name.clone(),
            score,
            icon_id: None,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_skills(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: only free text / exact phrases apply to skills.
    if query.has_unsupported_filter("skill") {
        return Vec::new();
    }
    let mut results = Vec::new();

    for skill in data.skills.values() {
        let searchable = combine_fields(&[&skill.name, skill.description.as_deref().unwrap_or("")]);

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(
                &skill.name,
                &[skill.description.as_deref().unwrap_or("")],
                anchor,
            )
            .max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };

        results.push(UnifiedSearchResult {
            entity_type: "skill".to_string(),
            name: skill.name.clone(),
            detail: skill.description.clone().unwrap_or_default(),
            entity_id: skill.name.clone(),
            score,
            icon_id: skill.icon_id,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_abilities(
    data: &GameData,
    query: &ParsedQuery,
    limit: usize,
) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: name:/keyword:/level:/has:/slot:/area: don't apply.
    if query.has_unsupported_filter("ability") {
        return Vec::new();
    }
    let mut results = Vec::new();
    let skill_filter = query.skill_filter();
    let neg_skill = query.negated_skill_filter();

    for family in data.ability_families.values() {
        // Monster-only families are hidden by default: `Lint_MonsterAbility` on
        // every tier marks the whole family (see game_data ability grouping).
        // There is currently no `show_monster` / `include_monster` query flag
        // to opt back in — a future `has:monster`-style filter could add one.
        if family.is_monster_ability {
            continue;
        }

        // Skill filter (positive + negated)
        if let Some(sf) = skill_filter {
            match &family.skill {
                Some(s) => {
                    if !s.to_lowercase().contains(sf) {
                        continue;
                    }
                }
                None => continue,
            }
        }
        if let Some(ns) = neg_skill {
            if let Some(s) = &family.skill {
                if s.to_lowercase().contains(ns) {
                    continue;
                }
            }
        }

        // Build searchable text from family + tier abilities
        let mut search_parts = vec![family.base_name.clone()];
        if let Some(ref skill) = family.skill {
            search_parts.push(skill.clone());
        }
        for &tier_id in &family.tier_ids {
            if let Some(ability) = data.abilities.get(&tier_id) {
                search_parts.push(ability.name.clone());
                if let Some(ref desc) = ability.description {
                    search_parts.push(desc.clone());
                }
            }
        }

        let searchable = search_parts.join(" ").to_lowercase();

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(&family.base_name, &[], anchor).max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };

        let detail = family.skill.clone().unwrap_or_default();

        results.push(UnifiedSearchResult {
            entity_type: "ability".to_string(),
            name: family.base_name.clone(),
            detail,
            entity_id: family.base_internal_name.clone(),
            score,
            icon_id: family.icon_id,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_effects(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: no structured filter applies to bare effects text.
    if query.has_unsupported_filter("effect") {
        return Vec::new();
    }
    let mut results = Vec::new();

    for effect in data.effects.values() {
        let name = effect.name.as_deref().unwrap_or("");
        let desc = effect.desc.as_deref().unwrap_or("");
        if name.is_empty() {
            continue;
        }

        let searchable = combine_fields(&[name, desc]);

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(name, &[desc], anchor).max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };

        results.push(UnifiedSearchResult {
            entity_type: "effect".to_string(),
            name: name.to_string(),
            detail: desc.to_string(),
            entity_id: effect.id.to_string(),
            score,
            icon_id: effect.icon_id,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_enemies(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: no structured filter applies to enemy key/comment text.
    if query.has_unsupported_filter("enemy") {
        return Vec::new();
    }
    let mut results = Vec::new();
    for (key, ai) in &data.ai {
        let comment = ai.raw.get("Comment").and_then(|v| v.as_str()).unwrap_or("");
        let strategy = ai
            .raw
            .get("Strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let searchable = combine_fields(&[key, comment, strategy]);

        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(key, &[comment, strategy], anchor).max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };

        results.push(UnifiedSearchResult {
            entity_type: "enemy".to_string(),
            name: key.clone(),
            detail: comment.to_string(),
            entity_id: key.clone(),
            score,
            icon_id: None,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_areas(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: no structured filter applies to area names.
    if query.has_unsupported_filter("area") {
        return Vec::new();
    }
    let mut results = Vec::new();

    for (key, area) in &data.areas {
        let name = area.friendly_name.as_deref().unwrap_or(key);

        let searchable = name.to_lowercase();
        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_match(name, anchor).max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };

        results.push(UnifiedSearchResult {
            entity_type: "area".to_string(),
            name: name.to_string(),
            detail: key.clone(),
            entity_id: key.clone(),
            score,
            icon_id: None,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_titles(data: &GameData, query: &ParsedQuery, limit: usize) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: no structured filter applies to titles.
    if query.has_unsupported_filter("title") {
        return Vec::new();
    }
    let mut results = Vec::new();

    for title in data.player_titles.values() {
        let name = title.title.as_deref().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let tooltip = title.tooltip.as_deref().unwrap_or("");

        let searchable = combine_fields(&[name, tooltip]);
        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(name, &[tooltip], anchor).max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };

        results.push(UnifiedSearchResult {
            entity_type: "title".to_string(),
            name: name.to_string(),
            detail: tooltip.to_string(),
            entity_id: title.id.to_string(),
            score,
            icon_id: None,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

fn search_lorebooks(
    data: &GameData,
    query: &ParsedQuery,
    limit: usize,
) -> Vec<UnifiedSearchResult> {
    // Reject-or-empty: no structured filter applies to lorebooks.
    if query.has_unsupported_filter("lorebook") {
        return Vec::new();
    }
    let mut results = Vec::new();

    for entry in data.lorebooks.books.values() {
        let name = entry.title.as_deref().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let text = entry.text.as_deref().unwrap_or("");
        let category = entry.category.as_deref().unwrap_or("");

        let searchable = combine_fields(&[name, text, category]);
        let score = if !query.text_terms.is_empty() || !query.exact_phrases.is_empty() {
            if !matches_all_terms(&searchable, &query.text_terms) {
                continue;
            }
            if !matches_all_phrases(&searchable, &query.exact_phrases) {
                continue;
            }
            let anchor = query
                .text_terms
                .first()
                .map(String::as_str)
                .or(query.exact_phrases.first().map(String::as_str))
                .unwrap_or("");
            score_entity(name, &[text], anchor).max(1)
        } else if query.filters.is_empty() && query.negations.is_empty() {
            continue;
        } else {
            50
        };

        results.push(UnifiedSearchResult {
            entity_type: "lorebook".to_string(),
            name: name.to_string(),
            detail: category.to_string(),
            entity_id: entry.id.to_string(),
            score,
            icon_id: None,
        });
    }

    results.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    results.truncate(limit);
    results
}

// ── Main search orchestrator ─────────────────────────────────────────────────

/// Tauri command: unified search across all game data entity types.
///
/// Returns results grouped by entity type, sorted by relevance within each group.
/// The frontend merges these with client-side player data results.
#[tauri::command]
pub async fn unified_search(
    query: String,
    limit: Option<usize>,
    state: State<'_, GameDataState>,
) -> Result<Vec<UnifiedSearchResult>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() || (trimmed.len() < 2 && !trimmed.contains(':')) {
        return Ok(Vec::new());
    }

    let parsed = parse_query(trimmed);
    let per_type_limit = limit.unwrap_or(10);
    let data = state.read().await;

    let mut all_results = Vec::new();

    // Search each entity type that isn't filtered out
    if parsed.should_search("item") {
        all_results.extend(search_items(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("recipe") {
        all_results.extend(search_recipes(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("npc") {
        all_results.extend(search_npcs(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("quest") {
        all_results.extend(search_quests(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("skill") {
        all_results.extend(search_skills(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("ability") {
        all_results.extend(search_abilities(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("effect") {
        all_results.extend(search_effects(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("enemy") {
        all_results.extend(search_enemies(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("area") {
        all_results.extend(search_areas(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("title") {
        all_results.extend(search_titles(&data, &parsed, per_type_limit));
    }
    if parsed.should_search("lorebook") {
        all_results.extend(search_lorebooks(&data, &parsed, per_type_limit));
    }

    Ok(all_results)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain_text() {
        let q = parse_query("sword shield");
        assert_eq!(q.text_terms, vec!["sword", "shield"]);
        assert!(q.filters.is_empty());
        assert!(q.exact_phrases.is_empty());
    }

    #[test]
    fn test_parse_quoted_phrase() {
        let q = parse_query("\"fire sword\" shield");
        assert_eq!(q.exact_phrases, vec!["fire sword"]);
        assert_eq!(q.text_terms, vec!["shield"]);
    }

    #[test]
    fn test_parse_type_filter() {
        let q = parse_query("type:item sword");
        assert!(q.has_type_filter("item"));
        assert!(!q.has_type_filter("npc"));
        assert_eq!(q.text_terms, vec!["sword"]);
    }

    #[test]
    fn test_parse_level_range() {
        let q = parse_query("level:30-50 type:item");
        assert_eq!(q.level_range(), Some((Some(30), Some(50))));
    }

    #[test]
    fn test_parse_negation() {
        let q = parse_query("-keyword:notobtainable sword");
        assert_eq!(q.negated_keyword_filters(), vec!["notobtainable"]);
        assert_eq!(q.text_terms, vec!["sword"]);
    }

    #[test]
    fn test_parse_literal_colon() {
        // "Knife: Precision Slash" — colon after non-filter word treated as literal
        let q = parse_query("Knife: Precision Slash");
        assert_eq!(q.text_terms, vec!["knife:", "precision", "slash"]);
        assert!(q.filters.is_empty());
    }

    #[test]
    fn test_should_search_no_filter() {
        let q = parse_query("sword");
        assert!(q.should_search("item"));
        assert!(q.should_search("npc"));
    }

    #[test]
    fn test_should_search_with_filter() {
        let q = parse_query("type:item sword");
        assert!(q.should_search("item"));
        assert!(!q.should_search("npc"));
    }

    #[test]
    fn test_should_search_with_negation() {
        let q = parse_query("-type:effect sword");
        assert!(q.should_search("item"));
        assert!(!q.should_search("effect"));
    }

    #[test]
    fn test_score_match() {
        assert_eq!(score_match("Sword", "sword"), 100);
        assert_eq!(score_match("Sword of Fire", "sword"), 80);
        assert_eq!(score_match("Fire Sword", "sword"), 60);
        assert_eq!(score_match("Shield", "sword"), 0);
    }
}
