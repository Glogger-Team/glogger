/// Chat Combat channel parser — detects player deaths, incoming damage,
/// and enemy kills from [Combat] messages.
///
/// Stateless parser: each message maps to 0 or 1 events.
use crate::chat_parser::ChatMessage;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub enum ChatCombatEvent {
    /// An enemy killed the player character.
    /// Detected via "(FATALITY!)" suffix where the target matches the active character.
    PlayerDeath {
        timestamp: String,
        killer_name: String,
        killer_entity_id: String,
        killing_ability: String,
        health_damage: u32,
        armor_damage: u32,
    },
    /// The player killed an enemy.
    /// Detected via "(FATALITY!)" suffix where the attacker matches the active character.
    /// Format: "PlayerName: AbilityName on EnemyName #EntityID! Dmg: N health, N armor. (FATALITY!)"
    EnemyKilled {
        timestamp: String,
        enemy_name: String,
        enemy_entity_id: String,
        killing_ability: String,
        health_damage: u32,
        armor_damage: u32,
    },
    /// An enemy dealt damage to the player (non-fatal).
    DamageOnPlayer {
        timestamp: String,
        attacker_name: String,
        attacker_entity_id: String,
        ability_name: String,
        health_damage: u32,
        armor_damage: u32,
        is_crit: bool,
    },
}

/// Parse a self-issued Combat ability line — the cow-curse abilities share one
/// shape (the chat parser splits the sender off player lines, so `text`
/// normally arrives as `"<Ability> on <Target>!"`):
///
/// ```text
/// [Combat] TwinkleofToes: Poop on TwinkleofToes!
/// [Combat] TwinkleofToes: Collect Milk on TwinkleofToes!
/// ```
///
/// Returns the ability's target, or `None` when the line is not that ability or
/// the issuer is not `player_name` (we track *your* actions, not other players').
/// Lines the chat parser keeps whole (it does so when the "sender" half contains
/// `#` or starts with `You `) are still accepted by stripping the
/// `"<player_name>: "` prefix.
///
/// Combat lines never reach `chat_messages` (the channel is excluded by
/// default), so callers feed these straight from the raw stream.
pub fn parse_self_ability_line(
    sender: Option<&str>,
    text: &str,
    player_name: &str,
    ability: &str,
) -> Option<String> {
    let text = text.trim();

    let text = match sender {
        Some(s) if s == player_name => text,
        Some(_) => return None,
        None => text.strip_prefix(player_name)?.strip_prefix(':')?.trim(),
    };

    let target = text
        .strip_prefix(ability)?
        .strip_prefix(" on ")?
        .strip_suffix('!')?
        .trim();
    if target.is_empty() {
        return None;
    }
    Some(target.to_string())
}

/// Parse the cow-curse "Poop" ability line; returns the poop's target.
pub fn parse_self_poop_line(
    sender: Option<&str>,
    text: &str,
    player_name: &str,
) -> Option<String> {
    parse_self_ability_line(sender, text, player_name, "Poop")
}

/// Try to parse a Combat channel ChatMessage into a combat event.
/// Returns None if the message is not a Combat message or not relevant to the player.
pub fn parse_combat_message(
    msg: &ChatMessage,
    player_name: &str,
) -> Option<ChatCombatEvent> {
    if msg.channel.as_deref() != Some("Combat") {
        return None;
    }

    let text = msg.message.trim();
    let ts = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

    // Try FATALITY first (most specific)
    if text.ends_with("(FATALITY!)") {
        if let Some(event) = try_player_death(text, &ts, player_name) {
            return Some(event);
        }
        // For player-kills-enemy lines, the chat parser splits the player name into msg.sender
        // ("Zenith: Ability on Enemy #ID! ...") → sender="Zenith", message="Ability on Enemy #ID! ..."
        if let Some(event) = try_enemy_kill(text, &ts, player_name, msg.sender.as_deref()) {
            return Some(event);
        }
    }

    // Try non-fatal damage on player
    try_damage_on_player(text, &ts, player_name)
}

/// Parse: "EnemyName #ID: AbilityName on PlayerName! Dmg: N health, N armor. (FATALITY!)"
fn try_player_death(text: &str, ts: &str, player_name: &str) -> Option<ChatCombatEvent> {
    if !text.ends_with("(FATALITY!)") {
        return None;
    }

    let parsed = parse_attack_line(text, player_name)?;
    let dmg_text = parsed.dmg_text?;

    // Strip trailing ". (FATALITY!)" or " (FATALITY!)"
    let dmg_text = dmg_text
        .strip_suffix("(FATALITY!)")
        .unwrap_or(dmg_text)
        .trim()
        .trim_end_matches('.');
    let dmg_text = dmg_text.trim();

    let (health_damage, armor_damage) = parse_damage(dmg_text)?;

    Some(ChatCombatEvent::PlayerDeath {
        timestamp: ts.to_string(),
        killer_name: parsed.attacker_name.to_string(),
        killer_entity_id: parsed.entity_id.to_string(),
        killing_ability: parsed.ability_name.to_string(),
        health_damage,
        armor_damage,
    })
}

/// Parse player-kills-enemy FATALITY.
/// Original chat format: "PlayerName: AbilityName on EnemyName #EntityID! Dmg: N health, N armor. (FATALITY!)"
/// But the chat parser splits the sender out, so we receive:
///   sender = Some("PlayerName")
///   text (msg.message) = "AbilityName on EnemyName #EntityID! Dmg: N health, N armor. (FATALITY!)"
fn try_enemy_kill(text: &str, ts: &str, player_name: &str, sender: Option<&str>) -> Option<ChatCombatEvent> {
    if !text.ends_with("(FATALITY!)") {
        return None;
    }

    // The sender must match the player name
    if sender? != player_name {
        return None;
    }

    // text is "AbilityName on EnemyName #EntityID! Dmg: ..."
    // Find " on " to split ability from target
    let on_pos = text.find(" on ")?;
    let ability_name = &text[..on_pos];

    let after_on = &text[on_pos + 4..];

    // Target is "EnemyName #EntityID! Dmg: ... (FATALITY!)"
    // Find the entity ID separator in the target portion
    let hash_pos = find_entity_id_separator(after_on)?;
    let enemy_name = &after_on[..hash_pos];

    let after_hash = &after_on[hash_pos + 2..];

    // Entity ID ends at "!"
    let bang_pos = after_hash.find('!')?;
    let entity_id = &after_hash[..bang_pos];

    if !entity_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    // Extract damage — find "Dmg: " marker
    let dmg_marker = "Dmg: ";
    let dmg_start = after_hash.find(dmg_marker)?;
    let dmg_text = &after_hash[dmg_start + dmg_marker.len()..];

    // Strip trailing "(FATALITY!)"
    let dmg_text = dmg_text
        .strip_suffix("(FATALITY!)")
        .unwrap_or(dmg_text)
        .trim()
        .trim_end_matches('.');
    let dmg_text = dmg_text.trim();

    let (health_damage, armor_damage) = parse_damage(dmg_text)?;

    Some(ChatCombatEvent::EnemyKilled {
        timestamp: ts.to_string(),
        enemy_name: enemy_name.to_string(),
        enemy_entity_id: entity_id.to_string(),
        killing_ability: ability_name.to_string(),
        health_damage,
        armor_damage,
    })
}

/// Parse non-fatal damage: "EnemyName #ID: AbilityName on PlayerName! Dmg: N health, N armor"
fn try_damage_on_player(text: &str, ts: &str, player_name: &str) -> Option<ChatCombatEvent> {
    let parsed = parse_attack_line(text, player_name)?;

    // Must have damage text — lines ending in "(EVADED!)" won't have "Dmg:"
    let dmg_text = parsed.dmg_text?;
    let dmg_text = dmg_text.trim().trim_end_matches('.');
    let dmg_text = dmg_text.trim();

    let (health_damage, armor_damage) = parse_damage(dmg_text)?;

    // Skip zero-damage hits (Dmg: none) — not useful in the damage log
    if health_damage == 0 && armor_damage == 0 {
        return None;
    }

    Some(ChatCombatEvent::DamageOnPlayer {
        timestamp: ts.to_string(),
        attacker_name: parsed.attacker_name.to_string(),
        attacker_entity_id: parsed.entity_id.to_string(),
        ability_name: parsed.ability_name.to_string(),
        health_damage,
        armor_damage,
        is_crit: parsed.is_crit,
    })
}

/// Common fields extracted from an attack line.
struct ParsedAttackLine<'a> {
    attacker_name: &'a str,
    entity_id: &'a str,
    ability_name: &'a str,
    is_crit: bool,
    /// Text after "Dmg: " if present. None for EVADED or lines without damage.
    dmg_text: Option<&'a str>,
}

/// Parse the common structure of an attack line:
/// "ActorName #EntityID: AbilityName on TargetName! Dmg: ..."
/// "ActorName #EntityID: AbilityName on TargetName (CRIT!) Dmg: ..."
/// "ActorName #EntityID: AbilityName on TargetName (EVADED!)"
///
/// Returns None if the target doesn't match player_name or the format is wrong.
fn parse_attack_line<'a>(text: &'a str, player_name: &str) -> Option<ParsedAttackLine<'a>> {
    let hash_pos = find_entity_id_separator(text)?;
    let attacker_name = &text[..hash_pos];

    let after_hash = &text[hash_pos + 2..];
    let colon_pos = after_hash.find(": ")?;
    let entity_id = &after_hash[..colon_pos];

    if !entity_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let after_colon = &after_hash[colon_pos + 2..];
    let on_pos = after_colon.find(" on ")?;
    let ability_name = &after_colon[..on_pos];

    let after_on = &after_colon[on_pos + 4..];

    let target_end = after_on.find('!')?;
    let target_name_part = &after_on[..target_end];

    let is_crit = target_name_part.ends_with(" (CRIT");
    let target_name = target_name_part
        .strip_suffix(" (CRIT")
        .unwrap_or(target_name_part)
        .trim();

    if target_name != player_name {
        return None;
    }

    let dmg_marker = "Dmg: ";
    let dmg_text = after_on.find(dmg_marker).map(|pos| &after_on[pos + dmg_marker.len()..]);

    Some(ParsedAttackLine {
        attacker_name,
        entity_id,
        ability_name,
        is_crit,
        dmg_text,
    })
}

/// Find the position of " #" that separates entity name from entity ID.
/// Must be followed by digits. Returns the position of the space before "#".
fn find_entity_id_separator(text: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find(" #") {
        let abs_pos = search_from + pos;
        let after = &text[abs_pos + 2..];
        if after.starts_with(|c: char| c.is_ascii_digit()) {
            return Some(abs_pos);
        }
        search_from = abs_pos + 2;
    }
    None
}

/// Parse damage text like "N health, N armor" or "N health" or " none"
fn parse_damage(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();

    if text == "none" {
        return Some((0, 0));
    }

    let parts: Vec<&str> = text.split(", ").collect();

    let health = parse_damage_component(parts.first()?)?;
    let armor = if parts.len() > 1 {
        parse_damage_component(parts.get(1)?)?
    } else {
        0
    };

    Some((health, armor))
}

/// Parse a single damage component like "N health" or "N armor"
fn parse_damage_component(text: &str) -> Option<u32> {
    let text = text.trim();
    let space_pos = text.find(' ')?;
    text[..space_pos].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_parser::parse_chat_line;

    fn combat_msg(text: &str) -> ChatMessage {
        let line = format!("26-04-01 05:30:53\t[Combat] {}", text);
        parse_chat_line(&line).unwrap()
    }

    // ── PlayerDeath tests ───────────────────────────────────────────────────

    #[test]
    fn test_player_death_basic() {
        let msg = combat_msg(
            "Demon Scout #7059135: Demon Bolt on Zenith! Dmg: 179 health, 178 armor. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::PlayerDeath {
            killer_name,
            killer_entity_id,
            killing_ability,
            health_damage,
            armor_damage,
            ..
        } = event
        else {
            panic!("Expected PlayerDeath");
        };
        assert_eq!(killer_name, "Demon Scout");
        assert_eq!(killer_entity_id, "7059135");
        assert_eq!(killing_ability, "Demon Bolt");
        assert_eq!(health_damage, 179);
        assert_eq!(armor_damage, 178);
    }

    #[test]
    fn test_player_death_kuvou() {
        let msg = combat_msg(
            "Kuvou #7681191: Bash on Zenith! Dmg: 108 health, 11 armor. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::PlayerDeath {
            killer_name,
            killing_ability,
            health_damage,
            armor_damage,
            ..
        } = event
        else {
            panic!("Expected PlayerDeath");
        };
        assert_eq!(killer_name, "Kuvou");
        assert_eq!(killing_ability, "Bash");
        assert_eq!(health_damage, 108);
        assert_eq!(armor_damage, 11);
    }

    #[test]
    fn test_player_death_behemoth() {
        let msg = combat_msg(
            "Behemoth #6323710: Infernal Slam on Zenith! Dmg: 201 health, 201 armor. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::PlayerDeath {
            killer_name,
            killing_ability,
            ..
        } = event
        else {
            panic!("Expected PlayerDeath");
        };
        assert_eq!(killer_name, "Behemoth");
        assert_eq!(killing_ability, "Infernal Slam");
    }

    #[test]
    fn test_player_death_long_enemy_name() {
        let msg = combat_msg(
            "Demonic Equine Analyst #8004000: Demon Bolt on Zenith! Dmg: 174 health, 173 armor. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::PlayerDeath {
            killer_name,
            killer_entity_id,
            ..
        } = event
        else {
            panic!("Expected PlayerDeath");
        };
        assert_eq!(killer_name, "Demonic Equine Analyst");
        assert_eq!(killer_entity_id, "8004000");
    }

    // ── EnemyKilled tests ───────────────────────────────────────────────────

    #[test]
    fn test_enemy_killed_basic() {
        let msg = combat_msg(
            "Zenith: Rib Shatter 9 on Orcish Ranger #7442047! Dmg: 3790 health, 1741 armor. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::EnemyKilled {
            enemy_name,
            enemy_entity_id,
            killing_ability,
            health_damage,
            armor_damage,
            ..
        } = event
        else {
            panic!("Expected EnemyKilled, got {:?}", event);
        };
        assert_eq!(enemy_name, "Orcish Ranger");
        assert_eq!(enemy_entity_id, "7442047");
        assert_eq!(killing_ability, "Rib Shatter 9");
        assert_eq!(health_damage, 3790);
        assert_eq!(armor_damage, 1741);
    }

    #[test]
    fn test_enemy_killed_from_capture() {
        // Real data from kill-and-loot.json devtools capture
        let msg = combat_msg(
            "Zenith: System Shock 8 on Bear Groupie #6749319! Dmg: 804 health, 166 armor. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::EnemyKilled {
            enemy_name,
            enemy_entity_id,
            killing_ability,
            health_damage,
            armor_damage,
            ..
        } = event
        else {
            panic!("Expected EnemyKilled, got {:?}", event);
        };
        assert_eq!(enemy_name, "Bear Groupie");
        assert_eq!(enemy_entity_id, "6749319");
        assert_eq!(killing_ability, "System Shock 8");
        assert_eq!(health_damage, 804);
        assert_eq!(armor_damage, 166);
    }

    #[test]
    fn test_enemy_killed_long_ability_name() {
        let msg = combat_msg(
            "Zenith: Pound To Slag 9 on Bear Groupie #6701962! Dmg: 4521 health, 166 armor. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::EnemyKilled {
            killing_ability,
            health_damage,
            ..
        } = event
        else {
            panic!("Expected EnemyKilled");
        };
        assert_eq!(killing_ability, "Pound To Slag 9");
        assert_eq!(health_damage, 4521);
    }

    #[test]
    fn test_enemy_killed_different_player_ignored() {
        let msg = combat_msg(
            "OtherPlayer: Rib Shatter 9 on Orcish Ranger #7442047! Dmg: 3790 health, 1741 armor. (FATALITY!)",
        );
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_enemy_killed_health_only() {
        let msg = combat_msg(
            "Zenith: Fire Bolt 5 on Skeleton #1234567! Dmg: 500 health. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::EnemyKilled {
            health_damage,
            armor_damage,
            ..
        } = event
        else {
            panic!("Expected EnemyKilled");
        };
        assert_eq!(health_damage, 500);
        assert_eq!(armor_damage, 0);
    }

    #[test]
    fn test_non_combat_channel_ignored() {
        let line = "26-04-01 05:30:52\t[Status] You earned 5 XP in Dying.";
        let msg = parse_chat_line(&line).unwrap();
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_different_player_fatality_ignored() {
        let msg = combat_msg(
            "Demon Scout #7059135: Demon Bolt on SomeOtherPlayer! Dmg: 179 health, 178 armor. (FATALITY!)",
        );
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_health_only_fatality() {
        let msg = combat_msg(
            "Demon Scout #7059135: Tracking Stare on Zenith! Dmg: 144 health. (FATALITY!)",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::PlayerDeath {
            health_damage,
            armor_damage,
            ..
        } = event
        else {
            panic!("Expected PlayerDeath");
        };
        assert_eq!(health_damage, 144);
        assert_eq!(armor_damage, 0);
    }

    #[test]
    fn test_recovered_message_ignored() {
        let msg = combat_msg("Zenith: Recovered: 21 health");
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_self_buff_ignored() {
        let msg = combat_msg("Zenith: Psi Health Wave 7 on Zenith!");
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    // ── DamageOnPlayer tests ────────────────────────────────────────────────

    #[test]
    fn test_damage_on_player_basic() {
        let msg = combat_msg(
            "Demon Scout #6937326: Demon Bolt on Zenith! Dmg: 171 health, 171 armor",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::DamageOnPlayer {
            attacker_name,
            attacker_entity_id,
            ability_name,
            health_damage,
            armor_damage,
            is_crit,
            ..
        } = event
        else {
            panic!("Expected DamageOnPlayer, got {:?}", event);
        };
        assert_eq!(attacker_name, "Demon Scout");
        assert_eq!(attacker_entity_id, "6937326");
        assert_eq!(ability_name, "Demon Bolt");
        assert_eq!(health_damage, 171);
        assert_eq!(armor_damage, 171);
        assert!(!is_crit);
    }

    #[test]
    fn test_damage_on_player_health_only() {
        let msg = combat_msg(
            "Demon Scout #6937326: Tracking Stare on Zenith! Dmg: 144 health",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::DamageOnPlayer {
            health_damage,
            armor_damage,
            ..
        } = event
        else {
            panic!("Expected DamageOnPlayer");
        };
        assert_eq!(health_damage, 144);
        assert_eq!(armor_damage, 0);
    }

    #[test]
    fn test_damage_on_player_crit() {
        let msg = combat_msg(
            "Brain Demon #7463620: Bite on Zenith (CRIT!) Dmg: 205 health, 205 armor",
        );
        let event = parse_combat_message(&msg, "Zenith").unwrap();
        let ChatCombatEvent::DamageOnPlayer {
            attacker_name,
            ability_name,
            health_damage,
            armor_damage,
            is_crit,
            ..
        } = event
        else {
            panic!("Expected DamageOnPlayer");
        };
        assert_eq!(attacker_name, "Brain Demon");
        assert_eq!(ability_name, "Bite");
        assert_eq!(health_damage, 205);
        assert_eq!(armor_damage, 205);
        assert!(is_crit);
    }

    #[test]
    fn test_evaded_attack_ignored() {
        let msg = combat_msg(
            "Demon Scout #6937326: Demon Bolt on Zenith (EVADED!)",
        );
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_none_damage_ignored() {
        let msg = combat_msg(
            "Ranalon Guardian #7681185: Ranalon Guardian Stab on Zenith! Dmg:  none",
        );
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_damage_on_different_player_ignored() {
        let msg = combat_msg(
            "Demon Scout #6937326: Demon Bolt on OtherPlayer! Dmg: 171 health, 171 armor",
        );
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_player_attack_on_mob_ignored() {
        let msg = combat_msg(
            "Zenith: Rib Shatter 9 on Orcish Ranger #7442047! Dmg: 3790 health, 1741 armor",
        );
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    #[test]
    fn test_indirect_damage_ignored() {
        let msg = combat_msg(
            "Demonic Swarmer #7537641: Suffered indirect dmg: -2 health",
        );
        assert!(parse_combat_message(&msg, "Zenith").is_none());
    }

    // ── Cow-curse ability line tests (Poop / Collect Milk) ──────────────────
    #[test]
    fn test_poop_line_matches_self() {
        let msg = combat_msg("TwinkleofToes: Poop on TwinkleofToes!");
        // The chat parser splits the sender out of player lines.
        assert_eq!(msg.sender.as_deref(), Some("TwinkleofToes"));
        assert_eq!(
            parse_self_poop_line(msg.sender.as_deref(), &msg.message, "TwinkleofToes"),
            Some("TwinkleofToes".to_string())
        );
    }

    #[test]
    fn test_poop_line_records_other_target() {
        let msg = combat_msg("TwinkleofToes: Poop on Somebodies Boots!");
        assert_eq!(
            parse_self_poop_line(msg.sender.as_deref(), &msg.message, "TwinkleofToes"),
            Some("Somebodies Boots".to_string())
        );
    }

    #[test]
    fn test_poop_line_other_players_ignored() {
        let msg = combat_msg("OtherPlayer: Poop on OtherPlayer!");
        assert_eq!(
            parse_self_poop_line(msg.sender.as_deref(), &msg.message, "TwinkleofToes"),
            None
        );
    }

    #[test]
    fn test_poop_line_unsplit_prefix_accepted_and_other_lines_rejected() {
        // Unsplit variant (chat parser keeps some lines whole).
        assert_eq!(
            parse_self_poop_line(None, "TwinkleofToes: Poop on TwinkleofToes!", "TwinkleofToes"),
            Some("TwinkleofToes".to_string())
        );
        // Same shape as the sibling self-milking line — must not count as a poop.
        assert_eq!(
            parse_self_poop_line(None, "TwinkleofToes: Collect Milk on TwinkleofToes!", "TwinkleofToes"),
            None
        );
        // Combat damage lines never match.
        let msg = combat_msg("Zenith: Rib Shatter 9 on Orcish Ranger #7442047! Dmg: 3790 health, 1741 armor");
        assert_eq!(
            parse_self_poop_line(msg.sender.as_deref(), &msg.message, "Zenith"),
            None
        );
        // Bare prefix without a target is not an event.
        assert_eq!(parse_self_poop_line(None, "Poop on !", "TwinkleofToes"), None);
    }

    #[test]
    fn test_self_milk_line_matches_self_but_not_raw_line_comparison() {
        let msg = combat_msg("TwinkleofToes: Collect Milk on TwinkleofToes!");
        assert_eq!(msg.sender.as_deref(), Some("TwinkleofToes"));
        // The chat parser strips the sender, so the message no longer carries it.
        assert_eq!(msg.message, "Collect Milk on TwinkleofToes!");

        assert_eq!(
            parse_self_ability_line(
                msg.sender.as_deref(),
                &msg.message,
                "TwinkleofToes",
                "Collect Milk"
            ),
            Some("TwinkleofToes".to_string())
        );

        // Regression guard for the old check: comparing the whole raw line
        // against the split message could never match, so self-milking never
        // recorded anything.
        assert_ne!(
            msg.message.trim(),
            format!(
                "{}: Collect Milk on {}!",
                "TwinkleofToes", "TwinkleofToes"
            )
        );
    }

    #[test]
    fn test_ability_lines_do_not_cross_match() {
        let milk = combat_msg("TwinkleofToes: Collect Milk on TwinkleofToes!");
        assert_eq!(
            parse_self_ability_line(
                milk.sender.as_deref(),
                &milk.message,
                "TwinkleofToes",
                "Poop"
            ),
            None
        );

        let poop = combat_msg("TwinkleofToes: Poop on TwinkleofToes!");
        assert_eq!(
            parse_self_ability_line(
                poop.sender.as_deref(),
                &poop.message,
                "TwinkleofToes",
                "Collect Milk"
            ),
            None
        );

        // Milking someone else (or an NPC cow) is not a SELF-milk: the matcher
        // reports the other target, which the caller compares against the player.
        let other = combat_msg("TwinkleofToes: Collect Milk on Homer!");
        assert_eq!(
            parse_self_ability_line(
                other.sender.as_deref(),
                &other.message,
                "TwinkleofToes",
                "Collect Milk"
            ),
            Some("Homer".to_string())
        );

        // Somebody else's milk line is not ours.
        let theirs = combat_msg("OtherPlayer: Collect Milk on OtherPlayer!");
        assert_eq!(
            parse_self_ability_line(
                theirs.sender.as_deref(),
                &theirs.message,
                "TwinkleofToes",
                "Collect Milk"
            ),
            None
        );
    }
}
