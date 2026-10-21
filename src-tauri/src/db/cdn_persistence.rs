use crate::game_data::{
    AbilityInfo, AchievementInfo, AreaInfo, GameData, ItemInfo, ItemUseInfo, QuestInfo,
    RecipeInfo, SkillInfo, SurveyKind, TsysClientInfo, XpTableInfo,
};
use rusqlite::{params, Connection, OptionalExtension, Result, Transaction};

/// Persist all CDN data to the database
pub fn persist_cdn_data(conn: &mut Connection, data: &GameData) -> Result<()> {
    let tx = conn.transaction()?;

    // Clear old data
    clear_cdn_data(&tx)?;

    // Insert new data
    insert_items(&tx, &data.items)?;
    insert_skills(&tx, &data.skills)?;
    insert_abilities(&tx, &data.abilities)?;
    insert_recipes(&tx, data)?;
    insert_npcs(&tx, data)?;
    insert_quests(&tx, &data.quests)?;
    insert_xp_tables(&tx, &data.xp_tables)?;
    insert_tsys_client_info(&tx, &data.tsys.client_info)?;
    insert_item_uses(&tx, &data.item_uses)?;
    insert_areas(&tx, &data.areas)?;
    insert_achievements(&tx, &data.achievements)?;
    insert_foods(&tx, data)?;
    insert_survey_types(&tx, &data.items, &data.recipes, &data.areas)?;

    // Backfill survey_uses.area for any rows that are NULL or still hold
    // old-style zone strings (from before the CDN ingestion started
    // writing proper area keys). This runs on every CDN reload so newly
    // corrected survey_types.zone values propagate to historical uses.
    backfill_survey_use_areas(&tx)?;

    // Update CDN version
    tx.execute(
        "INSERT OR REPLACE INTO cdn_version (id, version) VALUES (1, ?1)",
        params![data.version],
    )?;

    tx.commit()?;
    Ok(())
}

/// Unconditionally overwrite `survey_uses.area` from the freshly-loaded
/// `survey_types.zone` for every row where the survey_types lookup
/// returns a non-NULL zone. This runs on every CDN reload so even rows
/// that were written with old-style zone strings ("Povus9Y",
/// "KurMountains") or stale area keys get corrected to the canonical
/// area key ("AreaPovus", "AreaKurMountains") derived from the CDN
/// description + areas.json lookup.
fn backfill_survey_use_areas(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE survey_uses
            SET area = (
                SELECT st.zone
                  FROM survey_types st
                 WHERE st.internal_name = survey_uses.map_internal_name
                   AND st.zone IS NOT NULL
            )
          WHERE EXISTS (
                SELECT 1 FROM survey_types st
                 WHERE st.internal_name = survey_uses.map_internal_name
                   AND st.zone IS NOT NULL
          )",
        [],
    )?;
    Ok(())
}

/// Clear all CDN data tables
fn clear_cdn_data(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "DELETE FROM recipe_ingredients;
         DELETE FROM recipes;
         DELETE FROM abilities;
         DELETE FROM skills;
         DELETE FROM items;
         DELETE FROM npc_skills;
         DELETE FROM npcs;
         DELETE FROM quests;
         DELETE FROM xp_tables;
         DELETE FROM item_uses;
         DELETE FROM areas;
         DELETE FROM achievements;
         DELETE FROM foods;
         DROP TABLE IF EXISTS survey_types;
         CREATE TABLE survey_types (
             item_id          INTEGER PRIMARY KEY,
             internal_name    TEXT NOT NULL,
             name             TEXT NOT NULL,
             zone             TEXT,
             icon_id          INTEGER,
             survey_category  TEXT NOT NULL,
             is_motherlode    BOOLEAN NOT NULL DEFAULT 0,
             -- Canonical kind, set from ItemInfo::survey_kind() during CDN persist.
             -- One of 'basic' | 'motherlode' | 'multihit'. Prefer this over
             -- is_motherlode for new code; is_motherlode kept for back-compat.
             kind             TEXT NOT NULL DEFAULT 'basic',
             skill_req_name   TEXT,
             skill_req_level  INTEGER,
             survey_skill_req INTEGER,
             recipe_id        INTEGER,
             survey_xp        REAL,
             survey_xp_first_time REAL,
             crafting_cost    REAL
         );
         CREATE INDEX idx_survey_types_zone ON survey_types(zone);
         CREATE INDEX idx_survey_types_category ON survey_types(survey_category);
         CREATE INDEX idx_survey_types_kind ON survey_types(kind);
         CREATE INDEX idx_survey_types_name ON survey_types(name COLLATE NOCASE);",
    )?;
    Ok(())
}

/// Insert items into database
fn insert_items(tx: &Transaction, items: &std::collections::HashMap<u32, ItemInfo>) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO items (id, name, description, icon_id, value, max_stack_size,
                            keywords, effect_descs, internal_name, food_desc, equip_slot,
                            num_uses, skill_reqs, behaviors, bestow_recipes, bestow_ability,
                            bestow_quest, bestow_title, craft_points, crafting_target_level,
                            tsys_profile, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)"
    )?;

    for (id, item) in items {
        let keywords_json =
            serde_json::to_string(&item.keywords).unwrap_or_else(|_| "[]".to_string());
        let effects_json =
            serde_json::to_string(&item.effect_descs).unwrap_or_else(|_| "[]".to_string());
        let skill_reqs_json = item.skill_reqs.as_ref().map(|v| v.to_string());
        let behaviors_json = item
            .behaviors
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));
        let bestow_recipes_json = item
            .bestow_recipes
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));
        let raw_json_str = item.raw_json.to_string();

        stmt.execute(params![
            id,
            &item.name,
            &item.description,
            item.icon_id,
            item.value,
            item.max_stack_size,
            keywords_json,
            effects_json,
            &item.internal_name,
            &item.food_desc,
            &item.equip_slot,
            item.num_uses,
            skill_reqs_json,
            behaviors_json,
            bestow_recipes_json,
            &item.bestow_ability,
            &item.bestow_quest,
            item.bestow_title,
            item.craft_points,
            item.crafting_target_level,
            &item.tsys_profile,
            raw_json_str,
        ])?;
    }

    Ok(())
}

/// Insert skills into database
fn insert_skills(
    tx: &Transaction,
    skills: &std::collections::HashMap<u32, SkillInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO skills (id, name, description, icon_id, xp_table, keywords,
                             combat, max_bonus_levels, parents, advancement_table,
                             guest_level_cap, hide_when_zero, advancement_hints,
                             rewards, reports, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;

    for (id, skill) in skills {
        let keywords_json =
            serde_json::to_string(&skill.keywords).unwrap_or_else(|_| "[]".to_string());
        let parents_json =
            serde_json::to_string(&skill.parents).unwrap_or_else(|_| "[]".to_string());
        let advancement_hints_json = skill.advancement_hints.as_ref().map(|v| v.to_string());
        let rewards_json = skill.rewards.as_ref().map(|v| v.to_string());
        let reports_json = skill
            .reports
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));
        let raw_json_str = skill.raw_json.to_string();

        stmt.execute(params![
            id,
            &skill.name,
            &skill.description,
            skill.icon_id,
            &skill.xp_table,
            keywords_json,
            skill.combat,
            skill.max_bonus_levels,
            parents_json,
            &skill.advancement_table,
            skill.guest_level_cap,
            skill.hide_when_zero,
            advancement_hints_json,
            rewards_json,
            reports_json,
            raw_json_str,
        ])?;
    }

    Ok(())
}

/// Insert abilities into database
fn insert_abilities(
    tx: &Transaction,
    abilities: &std::collections::HashMap<u32, AbilityInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO abilities (id, name, internal_name, description, icon_id, skill, level_req, keywords,
                                damage_type, reset_time, target, prerequisite, upgrade_of, is_harmless,
                                animation, special_info, works_underwater, works_while_falling,
                                pve, pvp, mana_cost, power_cost, armor_cost, health_cost,
                                range, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)"
    )?;

    for (id, ability) in abilities {
        let keywords_json =
            serde_json::to_string(&ability.keywords).unwrap_or_else(|_| "[]".to_string());
        let pve_json = ability
            .pve
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        let pvp_json = ability
            .pvp
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        let raw_json_str = ability.raw_json.to_string();

        stmt.execute(params![
            id,
            &ability.name,
            &ability.internal_name,
            &ability.description,
            ability.icon_id,
            &ability.skill,
            ability.level,
            keywords_json,
            &ability.damage_type,
            ability.reset_time,
            &ability.target,
            &ability.prerequisite,
            &ability.upgrade_of,
            ability.is_harmless,
            &ability.animation,
            &ability.special_info,
            ability.works_underwater,
            ability.works_while_falling,
            pve_json,
            pvp_json,
            ability.mana_cost,
            ability.power_cost,
            ability.armor_cost,
            ability.health_cost,
            ability.range,
            raw_json_str,
        ])?;
    }

    Ok(())
}

/// Insert recipes and their ingredients into database
fn insert_recipes(tx: &Transaction, data: &GameData) -> Result<()> {
    let mut recipe_stmt = tx.prepare(
        "INSERT INTO recipes (id, name, skill, skill_level_req, icon_id, num_result_items,
                              action_label, keywords, shares_name_with_item_id,
                              result_item_ids, ingredient_item_ids,
                              result_effects, usage_delay, reward_skill_xp_drop_off_level,
                              sort_skill, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;

    let mut ingredient_stmt = tx.prepare(
        "INSERT INTO recipe_ingredients (recipe_id, item_id, item_keys, description, stack_size, chance_to_consume)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    )?;

    for (id, recipe) in &data.recipes {
        let keywords_json =
            serde_json::to_string(&recipe.keywords).unwrap_or_else(|_| "[]".to_string());
        let result_ids_json =
            serde_json::to_string(&recipe.result_item_ids).unwrap_or_else(|_| "[]".to_string());
        let ingredient_ids_json =
            serde_json::to_string(&recipe.ingredient_item_ids).unwrap_or_else(|_| "[]".to_string());
        let result_effects_json =
            serde_json::to_string(&recipe.result_effects).unwrap_or_else(|_| "[]".to_string());
        let raw_json_str = recipe.raw_json.to_string();

        let num_result_items = recipe.result_items.len() as i64;

        recipe_stmt.execute(params![
            id,
            &recipe.name,
            &recipe.skill,
            recipe.skill_level_req,
            recipe.icon_id,
            num_result_items,
            &recipe.action_label,
            keywords_json,
            recipe.shares_name_with_item_id,
            result_ids_json,
            ingredient_ids_json,
            result_effects_json,
            recipe.usage_delay,
            recipe.reward_skill_xp_drop_off_level,
            &recipe.sort_skill,
            raw_json_str,
        ])?;

        // Insert ingredients
        for ingredient in &recipe.ingredients {
            let item_keys_json = if ingredient.item_keys.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_string(&ingredient.item_keys)
                        .unwrap_or_else(|_| "[]".to_string()),
                )
            };
            ingredient_stmt.execute(params![
                id,
                ingredient.item_id,
                item_keys_json,
                ingredient.description,
                ingredient.stack_size,
                ingredient.chance_to_consume,
            ])?;
        }
    }

    Ok(())
}

/// Insert NPCs and their trained skills into database
fn insert_npcs(tx: &Transaction, data: &GameData) -> Result<()> {
    let mut npc_stmt = tx.prepare(
        "INSERT INTO npcs (key, name, area_name, area_description, preferences, pos, services, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )?;

    let mut skill_stmt = tx.prepare(
        "INSERT INTO npc_skills (npc_key, skill)
         VALUES (?1, ?2)",
    )?;

    for (key, npc) in &data.npcs {
        let preferences_json =
            serde_json::to_string(&npc.preferences).unwrap_or_else(|_| "[]".to_string());
        let pos_json = npc.pos.as_ref().map(|v| v.to_string());
        let services_json = npc
            .services
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string()));
        let raw_json_str = npc.raw_json.to_string();

        npc_stmt.execute(params![
            key,
            &npc.name,
            &npc.area_name,
            &npc.desc,
            preferences_json,
            pos_json,
            services_json,
            raw_json_str,
        ])?;

        // Insert trained skills
        for skill in &npc.trains_skills {
            skill_stmt.execute(params![key, skill])?;
        }
    }

    Ok(())
}

/// Insert quests into database
fn insert_quests(
    tx: &Transaction,
    quests: &std::collections::HashMap<String, QuestInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO quests (internal_name, raw_data)
         VALUES (?1, ?2)",
    )?;

    for (key, quest) in quests {
        let raw_json = serde_json::to_string(&quest.raw).unwrap_or_else(|_| "{}".to_string());

        stmt.execute(params![key, raw_json])?;
    }

    Ok(())
}

/// Insert XP tables into database
fn insert_xp_tables(
    tx: &Transaction,
    xp_tables: &std::collections::HashMap<u32, XpTableInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO xp_tables (id, internal_name, xp_amounts, raw_json)
         VALUES (?1, ?2, ?3, ?4)",
    )?;

    for (id, table) in xp_tables {
        let xp_amounts_json =
            serde_json::to_string(&table.xp_amounts).unwrap_or_else(|_| "[]".to_string());
        let raw_json_str = table.raw_json.to_string();

        stmt.execute(params![
            id,
            &table.internal_name,
            xp_amounts_json,
            raw_json_str,
        ])?;
    }

    Ok(())
}

/// Insert TSys client info into database
fn insert_tsys_client_info(
    tx: &Transaction,
    client_info: &std::collections::HashMap<String, TsysClientInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO tsys_client_info (key, internal_name, skill, slots, prefix, suffix, tiers,
                                       is_unavailable, is_hidden_from_transmutation, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;

    for (key, info) in client_info {
        let slots_json = serde_json::to_string(&info.slots).unwrap_or_else(|_| "[]".to_string());
        let tiers_json = serde_json::to_string(&info.tiers).ok();
        let raw_json_str = info.raw_json.to_string();

        stmt.execute(params![
            key,
            &info.internal_name,
            &info.skill,
            slots_json,
            &info.prefix,
            &info.suffix,
            tiers_json,
            info.is_unavailable,
            info.is_hidden_from_transmutation,
            raw_json_str,
        ])?;
    }

    Ok(())
}

/// Insert item uses into database
fn insert_item_uses(
    tx: &Transaction,
    item_uses: &std::collections::HashMap<String, ItemUseInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO item_uses (key, recipes_that_use_item, raw_json)
         VALUES (?1, ?2, ?3)",
    )?;

    for (key, info) in item_uses {
        let recipes_json =
            serde_json::to_string(&info.recipes_that_use_item).unwrap_or_else(|_| "[]".to_string());
        let raw_json_str = info.raw_json.to_string();

        stmt.execute(params![key, recipes_json, raw_json_str,])?;
    }

    Ok(())
}

/// Insert areas into database
fn insert_areas(
    tx: &Transaction,
    areas: &std::collections::HashMap<String, AreaInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO areas (key, friendly_name, short_friendly_name, raw_json)
         VALUES (?1, ?2, ?3, ?4)",
    )?;

    for (key, area) in areas {
        let raw_json_str = area.raw_json.to_string();

        stmt.execute(params![
            key,
            &area.friendly_name,
            &area.short_friendly_name,
            raw_json_str,
        ])?;
    }

    Ok(())
}

/// Insert achievements into database
fn insert_achievements(
    tx: &Transaction,
    achievements: &std::collections::HashMap<String, AchievementInfo>,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO achievements (id, name, description, icon_type)
         VALUES (?1, ?2, ?3, ?4)",
    )?;

    for (id, achievement) in achievements {
        stmt.execute(params![
            id,
            &achievement.name,
            &achievement.description,
            &achievement.icon_type,
        ])?;
    }

    Ok(())
}

/// Insert pre-parsed food items derived from items with food_desc.
///
/// Each row also carries the food's source classification (`source_kinds`,
/// `craft_skills`) so the Gourmand screen can separate what you cook from what
/// only turns up during an event — see `game_data::food_sources`.
fn insert_foods(tx: &Transaction, data: &GameData) -> Result<()> {
    let items = &data.items;
    let skill_names = crate::game_data::food_sources::skill_display_names(&data.skills);

    let mut stmt = tx.prepare(
        "INSERT INTO foods (item_id, name, icon_id, food_category, food_level, gourmand_req, effect_descs, keywords, value, source_kinds, craft_skills)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
    )?;

    for (id, item) in items {
        let food_desc = match &item.food_desc {
            Some(fd) => fd,
            None => continue,
        };

        // Parse "Level {N} {Category}" from food_desc
        let (food_level, food_category) = match parse_food_desc(food_desc) {
            Some(parsed) => parsed,
            None => {
                eprintln!(
                    "foods: skipping item {} — unparseable food_desc: {food_desc}",
                    item.name
                );
                continue;
            }
        };

        if !KNOWN_FOOD_CATEGORIES.contains(&food_category.as_str()) {
            eprintln!(
                "foods: unrecognised food category {food_category:?} on {} — the Gourmand screen \
                 buckets it under Unknown Categories; teach canonical_food_category about it if the \
                 CDN re-spelled a known category",
                item.name
            );
        }

        // Extract Gourmand skill requirement from SkillReqs
        let gourmand_req = item
            .skill_reqs
            .as_ref()
            .and_then(|v| v.get("Gourmand"))
            .and_then(|v| v.as_u64())
            .map(|n| n as i64);

        let effects_json =
            serde_json::to_string(&item.effect_descs).unwrap_or_else(|_| "[]".to_string());
        let keywords_json =
            serde_json::to_string(&item.keywords).unwrap_or_else(|_| "[]".to_string());

        let sources = crate::game_data::food_sources::classify(
            item,
            data.sources.items.get(id),
            &data.recipes,
            &skill_names,
        );
        let source_kinds_json =
            serde_json::to_string(&sources.kinds).unwrap_or_else(|_| "[]".to_string());
        let craft_skills_json =
            serde_json::to_string(&sources.craft_skills).unwrap_or_else(|_| "[]".to_string());

        stmt.execute(params![
            id,
            &item.name,
            item.icon_id,
            food_category,
            food_level,
            gourmand_req,
            effects_json,
            keywords_json,
            item.value,
            source_kinds_json,
            craft_skills_json,
        ])?;
    }

    Ok(())
}

/// Parse a food_desc string like "Level 20 Meal" into (level, category).
///
/// The category token is canonicalised by `canonical_food_category` — CDN 486
/// spells instant snacks `"Level 10 Instant Snack"` (built from the
/// `Food_Effect_Level_InstantSnack` string) where earlier data used
/// `"Level 10 Instant-Snack"`. `gourmandStore` splits the menu on the
/// hyphenated spelling, so storing the raw token silently hides every instant
/// snack from the Gourmand screen.
fn parse_food_desc(food_desc: &str) -> Option<(i64, String)> {
    let rest = food_desc.strip_prefix("Level ")?;
    let space_idx = rest.find(' ')?;
    let level: i64 = rest[..space_idx].parse().ok()?;
    let category = canonical_food_category(&rest[space_idx + 1..]);
    Some((level, category))
}

/// The categories the Gourmand screen has a section for — keep in sync with
/// `KNOWN_FOOD_CATEGORIES` in `src/types/gourmand.ts`. Anything outside this set
/// is surfaced in the screen's "Unknown Categories" bucket and logged by
/// `insert_foods`, so a CDN re-spelling gets noticed instead of silently
/// unsectioned.
const KNOWN_FOOD_CATEGORIES: [&str; 3] = ["Meal", "Snack", "Instant-Snack"];

/// Map a CDN food category to the token glogger stores and the frontend matches
/// on: `Meal`, `Snack`, or `Instant-Snack`.
///
/// Comparison ignores case and `-`/`_`/space separators, so separator drift
/// between CDN versions is absorbed here rather than at every consumer.
/// Unknown categories pass through unchanged — the Gourmand screen shows them
/// as-is instead of guessing.
fn canonical_food_category(raw: &str) -> String {
    let key: String = raw
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect::<String>()
        .to_lowercase();
    match key.as_str() {
        "meal" => "Meal".to_string(),
        "snack" => "Snack".to_string(),
        "instantsnack" => "Instant-Snack".to_string(),
        _ => raw.to_string(),
    }
}

/// Insert pre-parsed survey types derived from items with MineralSurvey or MiningSurvey keywords.
///
/// `zone` is resolved from the item's `Description` field by matching zone
/// friendly names against the `areas` map. The stored value is the area key
/// (e.g. `"AreaSerbule2"`), which the frontend resolves to a display name
/// via `AreaInline` — consistent with how every other screen shows areas.
fn insert_survey_types(
    tx: &Transaction,
    items: &std::collections::HashMap<u32, ItemInfo>,
    recipes: &std::collections::HashMap<u32, RecipeInfo>,
    areas: &std::collections::HashMap<String, crate::game_data::AreaInfo>,
) -> Result<()> {
    // Build a lookup from area-friendly-name → area-key. Longest names
    // first so "Serbule Hills" matches before "Serbule" when scanning.
    let mut friendly_to_key: Vec<(String, String)> = areas
        .iter()
        .filter_map(|(key, info)| {
            info.friendly_name
                .as_ref()
                .map(|fn_| (fn_.clone(), key.clone()))
        })
        .collect();
    friendly_to_key.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let mut stmt = tx.prepare(
        "INSERT INTO survey_types (item_id, internal_name, name, zone, icon_id,
                                   survey_category, is_motherlode, kind, skill_req_name,
                                   skill_req_level, survey_skill_req, recipe_id,
                                   survey_xp, survey_xp_first_time, crafting_cost)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )?;

    for (id, item) in items {
        let is_mineral = item.keywords.contains(&"MineralSurvey".to_string());
        let is_mining = item.keywords.contains(&"MiningSurvey".to_string());

        if !is_mineral && !is_mining {
            continue;
        }

        let internal_name = match &item.internal_name {
            Some(name) => name.clone(),
            None => continue,
        };

        let survey_category = if is_mineral { "mineral" } else { "mining" };
        let is_motherlode = item.keywords.contains(&"MotherlodeMap".to_string());
        // Canonical kind via the ItemInfo classifier. Falls back to "basic"
        // if the keyword check above passed but the classifier disagrees
        // (shouldn't happen given the predicate, but defensive).
        let kind = match item.survey_kind() {
            Some(SurveyKind::Motherlode) => "motherlode",
            Some(SurveyKind::Multihit) => "multihit",
            Some(SurveyKind::Basic) | None => "basic",
        };

        // Extract skill requirement — Geology for mineral surveys, Mining for mining surveys
        let (skill_req_name, skill_req_level) = if is_mineral {
            (
                "Geology",
                item.skill_reqs
                    .as_ref()
                    .and_then(|v| v.get("Geology"))
                    .and_then(|v| v.as_u64())
                    .map(|n| n as i64),
            )
        } else {
            (
                "Mining",
                item.skill_reqs
                    .as_ref()
                    .and_then(|v| v.get("Mining"))
                    .and_then(|v| v.as_u64())
                    .map(|n| n as i64),
            )
        };

        // Find the matching recipe by internal_name
        let matching_recipe = recipes
            .values()
            .find(|r| r.internal_name.as_deref() == Some(&internal_name));

        let recipe_id = matching_recipe.map(|r| r.id);
        let survey_skill_req = matching_recipe
            .and_then(|r| r.skill_level_req)
            .map(|v| v as i64);
        let survey_xp = matching_recipe.and_then(|r| r.reward_skill_xp);
        let survey_xp_first_time = matching_recipe.and_then(|r| r.reward_skill_xp_first_time);

        // Resolve zone: find the area key whose friendly name appears in
        // the item's Description field. Descriptions follow patterns like
        // "Records the location of a mineral deposit in or around X." where
        // X is a zone's FriendlyName. We check longest names first so e.g.
        // "Serbule Hills" matches before "Serbule".
        let zone: Option<String> = item
            .description
            .as_ref()
            .and_then(|desc| {
                friendly_to_key
                    .iter()
                    .find(|(fname, _)| desc.contains(fname.as_str()))
                    .map(|(_, key)| key.clone())
            })
            .or_else(|| {
                // Fallback: derive from internal name (camel-case zone).
                // Gives "KurMountains" not "AreaKurMountains" — acceptable
                // for older CDN snapshots that might lack descriptions.
                parse_survey_zone(&internal_name)
            });

        // Compute crafting cost from always-consumed recipe ingredients (paper & ink).
        // Estimated acquisition cost = item.value * 2 (value is what NPCs pay you).
        // Skip ingredients with partial chance_to_consume (crystals, etc.) for now.
        let crafting_cost: Option<f64> = matching_recipe.map(|recipe| {
            recipe
                .ingredients
                .iter()
                .filter(|ing| {
                    // Only include ingredients that are always consumed (no ChanceToConsume or 1.0)
                    ing.chance_to_consume.is_none() || ing.chance_to_consume == Some(1.0)
                })
                .map(|ing| {
                    let item_value = ing
                        .item_id
                        .and_then(|iid| items.get(&iid))
                        .and_then(|i| i.value)
                        .unwrap_or(0.0);
                    let est_price = item_value as f64 * 2.0;
                    ing.stack_size as f64 * est_price
                })
                .sum()
        });

        stmt.execute(params![
            id,
            &internal_name,
            &item.name,
            zone,
            item.icon_id,
            survey_category,
            is_motherlode,
            kind,
            skill_req_name,
            skill_req_level,
            survey_skill_req,
            recipe_id,
            survey_xp,
            survey_xp_first_time,
            crafting_cost,
        ])?;
    }

    Ok(())
}

/// Parse zone name from a survey internal_name.
/// Examples:
///   "GeologySurveyEltibule2" → "Eltibule"
///   "MiningSurveySouthSerbule1X" → "SouthSerbule"
///   "GeologySurveyKurMountains3" → "KurMountains"
fn parse_survey_zone(internal_name: &str) -> Option<String> {
    // Strip the prefix
    let rest = if let Some(r) = internal_name.strip_prefix("GeologySurvey") {
        r
    } else if let Some(r) = internal_name.strip_prefix("MiningSurvey") {
        r
    } else {
        return None;
    };

    if rest.is_empty() {
        return None;
    }

    // Strip trailing non-alpha characters (digits, and 'X' suffix on mining surveys)
    let zone: String = rest
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == 'X')
        .to_string();

    if zone.is_empty() {
        None
    } else {
        Some(zone)
    }
}

/// Load CDN data from database (for initialization)
#[allow(dead_code)]
pub fn load_cdn_data(conn: &Connection) -> Result<(u32, bool)> {
    // Check if we have CDN data loaded
    let version: Option<u32> = conn
        .query_row("SELECT version FROM cdn_version WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()?;

    if let Some(v) = version {
        // Check if we have items (basic sanity check)
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))?;
        Ok((v, count > 0))
    } else {
        Ok((0, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both CDN spellings of the instant-snack category must land on the single
    /// token `gourmandStore` matches on, or the Instant-Snacks section renders
    /// empty (see migration v70).
    #[test]
    fn food_desc_parses_level_and_canonical_category() {
        assert_eq!(
            parse_food_desc("Level 10 Instant Snack"),
            Some((10, "Instant-Snack".to_string()))
        );
        assert_eq!(
            parse_food_desc("Level 0 Instant-Snack"),
            Some((0, "Instant-Snack".to_string()))
        );
        assert_eq!(parse_food_desc("Level 20 Meal"), Some((20, "Meal".to_string())));
        assert_eq!(parse_food_desc("Level 5 Snack"), Some((5, "Snack".to_string())));
    }

    #[test]
    fn unknown_food_categories_survive_verbatim() {
        assert_eq!(
            parse_food_desc("Level 3 Second Breakfast"),
            Some((3, "Second Breakfast".to_string()))
        );
    }

    /// Every known category round-trips unchanged, so the unknown-category
    /// warning in `insert_foods` can't fire for a food the screen does render.
    #[test]
    fn canonicalisation_covers_every_known_category() {
        for known in KNOWN_FOOD_CATEGORIES {
            assert_eq!(canonical_food_category(known), known);
        }
        assert!(!KNOWN_FOOD_CATEGORIES.contains(&canonical_food_category("Brunch").as_str()));
    }

    #[test]
    fn malformed_food_descs_are_rejected() {
        assert_eq!(parse_food_desc("Instant Snack"), None);
        assert_eq!(parse_food_desc("Level X Meal"), None);
    }
}
