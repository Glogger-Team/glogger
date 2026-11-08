/// Watch rule evaluation engine
///
/// Evaluates incoming chat messages against user-defined watch rules.
/// Rules are stored in settings and evaluated during tailing so alerts
/// fire even when the chat UI isn't open.
use crate::chat_parser::ChatMessage;
use crate::settings::{ConditionMatch, WatchCondition, WatchNotifyConfig, WatchRule};
use serde::Serialize;

/// Payload emitted when a watch rule matches a message
#[derive(Debug, Clone, Serialize)]
pub struct WatchRuleTriggered {
    pub rule_id: u64,
    pub rule_name: String,
    pub notify: WatchNotifyConfig,
    pub channel: Option<String>,
    pub sender: Option<String>,
    pub message: String,
    pub timestamp: String,
}

/// Evaluate all enabled rules against a single message.
/// Returns a list of triggered payloads for rules that matched.
pub fn evaluate_rules(message: &ChatMessage, rules: &[WatchRule]) -> Vec<WatchRuleTriggered> {
    let mut triggered = Vec::new();

    for rule in rules {
        if !rule.enabled || rule.conditions.is_empty() {
            continue;
        }

        // Channel filter: if rule specifies channels, message must be in one of them
        if let Some(ref channels) = rule.channels {
            match &message.channel {
                Some(msg_channel) => {
                    if !channels.iter().any(|c| c.eq_ignore_ascii_case(msg_channel)) {
                        continue;
                    }
                }
                None => continue, // System messages without a channel don't match channel-filtered rules
            }
        }

        // Evaluate conditions based on match_mode
        let matched = match rule.match_mode {
            ConditionMatch::All => rule
                .conditions
                .iter()
                .all(|cond| evaluate_condition(message, cond)),
            ConditionMatch::Any => rule
                .conditions
                .iter()
                .any(|cond| evaluate_condition(message, cond)),
        };

        if matched {
            triggered.push(WatchRuleTriggered {
                rule_id: rule.id,
                rule_name: rule.name.clone(),
                notify: rule.notify.clone(),
                channel: message.channel.clone(),
                sender: message.sender.clone(),
                message: message.message.clone(),
                timestamp: message.timestamp.to_string(),
            });
        }
    }

    triggered
}

/// Evaluate a single condition against a message
fn evaluate_condition(message: &ChatMessage, condition: &WatchCondition) -> bool {
    match condition {
        WatchCondition::ContainsText(text) => {
            let needle = text.to_lowercase();

            // Check the message body (includes raw [Item: ...] text)
            if message.message.to_lowercase().contains(&needle) {
                return true;
            }

            // Also check each item link's parsed item_name
            // This catches cases where the item name differs from the raw text
            // e.g., watchword "flamestrike" matches [Item: Priest: Flamestrike]
            for link in &message.item_links {
                if link.item_name.to_lowercase().contains(&needle) {
                    return true;
                }
            }

            false
        }

        WatchCondition::ContainsItemLink(item_name) => {
            let needle = item_name.to_lowercase();

            message
                .item_links
                .iter()
                .any(|link| link.item_name.to_lowercase().contains(&needle))
        }

        WatchCondition::FromSender(sender_name) => match &message.sender {
            Some(sender) => sender.eq_ignore_ascii_case(sender_name),
            None => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_parser::ItemLink;
    use chrono::NaiveDateTime;

    fn make_message(channel: &str, sender: &str, text: &str, items: Vec<ItemLink>) -> ChatMessage {
        ChatMessage {
            timestamp: NaiveDateTime::parse_from_str("2026-03-11 12:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            channel: Some(channel.to_string()),
            sender: Some(sender.to_string()),
            message: text.to_string(),
            is_system: false,
            from_player: None,
            item_links: items,
        }
    }

    fn make_rule(
        id: u64,
        conditions: Vec<WatchCondition>,
        channels: Option<Vec<String>>,
    ) -> WatchRule {
        WatchRule {
            id,
            name: format!("Rule {}", id),
            enabled: true,
            channels,
            match_mode: ConditionMatch::All,
            conditions,
            notify: WatchNotifyConfig {
                sound: false,
                toast: true,
                highlight: true,
            },
            source: None,
        }
    }

    fn make_rule_any(
        id: u64,
        conditions: Vec<WatchCondition>,
        channels: Option<Vec<String>>,
    ) -> WatchRule {
        WatchRule {
            id,
            name: format!("Rule {}", id),
            enabled: true,
            channels,
            match_mode: ConditionMatch::Any,
            conditions,
            notify: WatchNotifyConfig {
                sound: false,
                toast: true,
                highlight: true,
            },
            source: None,
        }
    }

    fn item_link(name: &str) -> ItemLink {
        ItemLink {
            raw_text: format!("[Item: {}]", name),
            item_name: name.to_string(),
        }
    }

    // ── ContainsText tests ──────────────────────────────────────────

    #[test]
    fn test_contains_text_in_message_body() {
        let msg = make_message("Trade", "Seller", "WTS flamestrike cheap", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            None,
        );

        let results = evaluate_rules(&msg, &[rule]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rule_id, 1);
    }

    #[test]
    fn test_contains_text_case_insensitive() {
        let msg = make_message("Trade", "Seller", "WTS FLAMESTRIKE", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_contains_text_matches_item_link_name() {
        // Message text doesn't contain "flamestrike" but the item link does
        let msg = make_message(
            "Trade",
            "Seller",
            "WTS this spell",
            vec![item_link("Priest: Flamestrike")],
        );
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_contains_text_no_match() {
        let msg = make_message("Trade", "Seller", "WTS sword", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            None,
        );

        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }

    // ── ContainsItemLink tests ──────────────────────────────────────

    #[test]
    fn test_contains_item_link_match() {
        let msg = make_message("Trade", "Seller", "WTS", vec![item_link("Strange Dirt")]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsItemLink("Strange Dirt".into())],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_contains_item_link_partial_match() {
        let msg = make_message("Trade", "Seller", "WTS", vec![item_link("Strange Dirt")]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsItemLink("Strange".into())],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_contains_item_link_matches_skill() {
        let msg = make_message(
            "Trade",
            "Seller",
            "WTS",
            vec![item_link("Mentalism: System Shock 7")],
        );
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsItemLink("Mentalism".into())],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_contains_item_link_no_match_in_body() {
        // Text says "strange dirt" but no actual item link — should NOT match
        let msg = make_message("Trade", "Seller", "looking for strange dirt", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsItemLink("Strange Dirt".into())],
            None,
        );

        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }

    // ── FromSender tests ────────────────────────────────────────────

    #[test]
    fn test_from_sender_match() {
        let msg = make_message("Global", "TraderJoe", "hello", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::FromSender("TraderJoe".into())],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_from_sender_case_insensitive() {
        let msg = make_message("Global", "TraderJoe", "hello", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::FromSender("traderjoe".into())],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_from_sender_no_match() {
        let msg = make_message("Global", "SomeoneElse", "hello", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::FromSender("TraderJoe".into())],
            None,
        );

        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }

    // ── Channel filtering tests ─────────────────────────────────────

    #[test]
    fn test_channel_filter_match() {
        let msg = make_message("Trade", "Seller", "WTS flamestrike", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            Some(vec!["Trade".into()]),
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_channel_filter_no_match() {
        let msg = make_message("Global", "Seller", "WTS flamestrike", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            Some(vec!["Trade".into()]),
        );

        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }

    #[test]
    fn test_no_channel_filter_matches_all() {
        let msg = make_message("Global", "Seller", "WTS flamestrike", vec![]);
        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            None, // matches all channels
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    // ── AND logic tests ─────────────────────────────────────────────

    #[test]
    fn test_multiple_conditions_all_must_match() {
        let msg = make_message("Trade", "Seller", "WTS flamestrike cheap", vec![]);
        let rule = make_rule(
            1,
            vec![
                WatchCondition::ContainsText("WTS".into()),
                WatchCondition::ContainsText("flamestrike".into()),
            ],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_multiple_conditions_partial_match_fails() {
        let msg = make_message("Trade", "Seller", "WTB flamestrike", vec![]);
        let rule = make_rule(
            1,
            vec![
                WatchCondition::ContainsText("WTS".into()),
                WatchCondition::ContainsText("flamestrike".into()),
            ],
            None,
        );

        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }

    // ── Disabled / empty rules ──────────────────────────────────────

    #[test]
    fn test_disabled_rule_skipped() {
        let msg = make_message("Trade", "Seller", "WTS flamestrike", vec![]);
        let mut rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("flamestrike".into())],
            None,
        );
        rule.enabled = false;

        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }

    #[test]
    fn test_empty_conditions_skipped() {
        let msg = make_message("Trade", "Seller", "WTS flamestrike", vec![]);
        let rule = make_rule(1, vec![], None);

        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }

    // ── Multiple rules ──────────────────────────────────────────────

    #[test]
    fn test_multiple_rules_can_match() {
        let msg = make_message("Trade", "TraderJoe", "WTS flamestrike", vec![]);
        let rules = vec![
            make_rule(
                1,
                vec![WatchCondition::ContainsText("flamestrike".into())],
                None,
            ),
            make_rule(
                2,
                vec![WatchCondition::FromSender("TraderJoe".into())],
                None,
            ),
            make_rule(3, vec![WatchCondition::ContainsText("sword".into())], None), // won't match
        ];

        let results = evaluate_rules(&msg, &rules);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].rule_id, 1);
        assert_eq!(results[1].rule_id, 2);
    }

    // ── System message (no sender/channel) ──────────────────────────

    #[test]
    fn test_system_message_no_sender() {
        let msg = ChatMessage {
            timestamp: NaiveDateTime::parse_from_str("2026-03-11 12:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            channel: None,
            sender: None,
            message: "Entering Area: Casino".to_string(),
            is_system: true,
            from_player: None,
            item_links: vec![],
        };

        // FromSender should not match
        let rule = make_rule(1, vec![WatchCondition::FromSender("anyone".into())], None);
        assert!(evaluate_rules(&msg, &[rule]).is_empty());

        // ContainsText should still work
        let rule2 = make_rule(2, vec![WatchCondition::ContainsText("Casino".into())], None);
        assert_eq!(evaluate_rules(&msg, &[rule2]).len(), 1);
    }

    // ── OR (Any) logic tests ─────────────────────────────────────

    #[test]
    fn test_any_mode_matches_single_condition() {
        let msg = make_message(
            "Guild",
            "Zenith",
            "oh handy. i just need to find a place to farm more goats.",
            vec![ItemLink {
                raw_text: "[Item: Mentalism: System Shock 7]".into(),
                item_name: "Mentalism: System Shock 7".into(),
            }],
        );
        let rule = make_rule_any(
            1,
            vec![
                WatchCondition::ContainsText("Hammer:".into()),
                WatchCondition::ContainsText("Mentalism:".into()),
                WatchCondition::ContainsText("Leatherworking:".into()),
                WatchCondition::ContainsItemLink("Mentalism".into()),
                WatchCondition::ContainsItemLink("Hammer".into()),
                WatchCondition::ContainsText("Pound to Slag".into()),
            ],
            None,
        );

        let results = evaluate_rules(&msg, &[rule]);
        assert_eq!(
            results.len(),
            1,
            "Any-mode rule should match when at least one condition matches"
        );
    }

    #[test]
    fn test_any_mode_no_match() {
        let msg = make_message("Guild", "Zenith", "just chatting about nothing", vec![]);
        let rule = make_rule_any(
            1,
            vec![
                WatchCondition::ContainsText("Hammer:".into()),
                WatchCondition::ContainsText("Mentalism:".into()),
            ],
            None,
        );

        assert!(
            evaluate_rules(&msg, &[rule]).is_empty(),
            "Any-mode rule should not match when no conditions match"
        );
    }

    #[test]
    fn test_any_mode_item_link_match() {
        let msg = make_message(
            "Trade",
            "Seller",
            "WTS",
            vec![ItemLink {
                raw_text: "[Item: Hammer: Pound to Slag 6]".into(),
                item_name: "Hammer: Pound to Slag 6".into(),
            }],
        );
        let rule = make_rule_any(
            1,
            vec![
                WatchCondition::ContainsItemLink("Mentalism".into()),
                WatchCondition::ContainsItemLink("Hammer".into()),
            ],
            None,
        );

        assert_eq!(evaluate_rules(&msg, &[rule]).len(), 1);
    }

    #[test]
    fn test_channel_filtered_rule_skips_system_messages() {
        let msg = ChatMessage {
            timestamp: NaiveDateTime::parse_from_str("2026-03-11 12:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            channel: None,
            sender: None,
            message: "Entering Area: Casino".to_string(),
            is_system: true,
            from_player: None,
            item_links: vec![],
        };

        let rule = make_rule(
            1,
            vec![WatchCondition::ContainsText("Casino".into())],
            Some(vec!["Global".into()]),
        );
        assert!(evaluate_rules(&msg, &[rule]).is_empty());
    }
}
