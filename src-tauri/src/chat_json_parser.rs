//! VIP JSON chat-log parser — `[JSON:Type] {json}` lines (non-combat logging).
//!
//! Stateless: each `ChatMessage` carrying `json_type`/`json_body` maps to 0 or 1
//! events. Unknown types map to `Unknown` (log-and-skip downstream); malformed
//! bodies for known types map to `None` so the caller can count-and-skip while
//! keeping the raw chat row.
use crate::chat_parser::ChatMessage;

/// Structured event from one `[JSON:Type]` chat line. Only game-observed Types
/// have variants (see `docs/features/vip-json-logging.md`); everything else is
/// `Unknown` until a live line freezes its schema.
#[derive(Debug, Clone)]
pub enum ChatJsonEvent {
    /// `AddItem`: item receipt. `from_name`/`from_entity_id` are `Some` only
    /// for loot (the English twin has no source entity).
    ItemReceipt {
        timestamp: String,
        item_name: String,
        quantity: u32,
        from_name: Option<String>,
        from_entity_id: Option<u32>,
    },
    /// `EarnedXP`: skill XP. `new_level > old_level` accompanies a level-up.
    Xp {
        timestamp: String,
        skill: String,
        xp: u32,
        old_level: u32,
        new_level: u32,
    },
    /// `Book`: full book/report body (sole record — no English twin).
    Book {
        timestamp: String,
        title: String,
        body: String,
    },
    /// `DeltaFavor`: exact favor delta for an NPC (English twin is rounded).
    FavorDelta {
        timestamp: String,
        npc: String,
        delta: f64,
    },
    /// `RecipeUsage`: `completions` is an absolute total (snapshot shape),
    /// never a delta — consumers set newer-wins.
    RecipeUsed {
        timestamp: String,
        recipe_id: u32,
        completions: u32,
    },
    /// Any `Type` with no frozen schema yet — structured ingest skips it,
    /// the raw chat row stays searchable.
    Unknown { json_type: String },
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct AddItemJson {
    name: String,
    stack: u32,
    from_name: Option<String>,
    from_entity_id: Option<u32>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct EarnedXpJson {
    skill: String,
    #[serde(rename = "XP")]
    xp: u32,
    old_level: u32,
    new_level: u32,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct BookJson {
    title: String,
    body: String,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct DeltaFavorJson {
    npc: String,
    delta: f64,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RecipeUsageJson {
    recipe_name: String,
    recipe_id: u32,
    completions: u32,
}

/// Parse one `[JSON:Type]` message. Returns `None` when the message is not a
/// JSON line or a known Type's body fails to parse (caller counts-and-skips).
pub fn parse_json_message(msg: &ChatMessage) -> Option<ChatJsonEvent> {
    let json_type = msg.json_type.as_deref()?;
    let body = msg.json_body.as_deref()?;
    let timestamp = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
    match json_type {
        "AddItem" => {
            let v: AddItemJson = serde_json::from_str(body).ok()?;
            if v.name.is_empty() {
                return None;
            }
            Some(ChatJsonEvent::ItemReceipt {
                timestamp,
                item_name: v.name,
                quantity: v.stack.max(1),
                from_name: v.from_name,
                from_entity_id: v.from_entity_id,
            })
        }
        "EarnedXP" => {
            let v: EarnedXpJson = serde_json::from_str(body).ok()?;
            if v.skill.is_empty() || v.xp == 0 {
                return None;
            }
            Some(ChatJsonEvent::Xp {
                timestamp,
                skill: v.skill,
                xp: v.xp,
                old_level: v.old_level,
                new_level: v.new_level,
            })
        }
        "Book" => {
            let v: BookJson = serde_json::from_str(body).ok()?;
            if v.title.is_empty() {
                return None;
            }
            Some(ChatJsonEvent::Book {
                timestamp,
                title: v.title,
                body: v.body,
            })
        }
        "DeltaFavor" => {
            let v: DeltaFavorJson = serde_json::from_str(body).ok()?;
            if v.npc.is_empty() {
                return None;
            }
            Some(ChatJsonEvent::FavorDelta {
                timestamp,
                npc: v.npc,
                delta: v.delta,
            })
        }
        "RecipeUsage" => {
            let v: RecipeUsageJson = serde_json::from_str(body).ok()?;
            if v.recipe_name.is_empty() {
                return None;
            }
            Some(ChatJsonEvent::RecipeUsed {
                timestamp,
                recipe_id: v.recipe_id,
                completions: v.completions,
            })
        }
        _ => Some(ChatJsonEvent::Unknown {
            json_type: json_type.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_parser::parse_chat_lines;

    /// Parse every line of a frozen fixture into JSON events.
    fn parse_all(fixture: &str) -> Vec<ChatJsonEvent> {
        parse_chat_lines(fixture)
            .iter()
            .filter_map(parse_json_message)
            .collect()
    }

    #[test]
    fn add_item_lines_parse_with_entity() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-additem.log"));
        assert_eq!(events.len(), 16);
        let mut saw_loot = false;
        let mut saw_plain = false;
        for ev in &events {
            let ChatJsonEvent::ItemReceipt {
                item_name,
                quantity,
                from_name,
                from_entity_id,
                ..
            } = ev
            else {
                panic!("expected ItemReceipt, got {ev:?}");
            };
            if item_name == "Toxic Sludge" {
                assert_eq!(*quantity, 1);
                assert_eq!(from_name.as_deref(), Some("Olugax The Ever-Pudding"));
                assert_eq!(*from_entity_id, Some(257400));
                saw_loot = true;
            }
            if item_name == "Amazing Phlogiston" {
                assert_eq!(*quantity, 59);
                assert!(from_name.is_none());
                saw_plain = true;
            }
        }
        assert!(saw_loot && saw_plain);
    }

    #[test]
    fn earned_xp_lines_parse() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-earnedxp.log"));
        assert_eq!(events.len(), 8);
        let ChatJsonEvent::Xp {
            skill,
            xp,
            old_level,
            new_level,
            ..
        } = &events[0]
        else {
            panic!("expected Xp, got {:?}", events[0]);
        };
        assert_eq!(skill, "Teleportation");
        assert_eq!(*xp, 53);
        assert_eq!((*old_level, *new_level), (46, 46));
    }

    #[test]
    fn book_lines_parse() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-book.log"));
        assert_eq!(events.len(), 6);
        let titles: Vec<&str> = events
            .iter()
            .map(|ev| match ev {
                ChatJsonEvent::Book { title, .. } => title.as_str(),
                other => panic!("expected Book, got {other:?}"),
            })
            .collect();
        assert!(titles.iter().any(|t| t.contains("Age")));
        assert!(titles.contains(&"Skill Info"));
        assert!(titles.iter().any(|t| t.contains("Behavior Report")));
    }

    #[test]
    fn delta_favor_lines_parse_exact() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-deltafavor.log"));
        assert_eq!(events.len(), 4);
        let ChatJsonEvent::FavorDelta { npc, delta, .. } = &events[0] else {
            panic!("expected FavorDelta, got {:?}", events[0]);
        };
        assert_eq!(npc, "NPC_Elune");
        assert!((delta - 18.06948).abs() < 1e-9);
    }

    #[test]
    fn recipe_usage_lines_parse_absolute() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-recipeusage.log"));
        assert_eq!(events.len(), 2);
        let ChatJsonEvent::RecipeUsed {
            recipe_id, completions, ..
        } = &events[0]
        else {
            panic!("expected RecipeUsed, got {:?}", events[0]);
        };
        assert_eq!(*recipe_id, 9008);
        assert_eq!(*completions, 211);
    }

    #[test]
    fn non_json_lines_yield_none() {
        let msgs = parse_chat_lines("26-11-17 08:50:27\t[Status] Toxic Sludge added to inventory.\n");
        assert_eq!(msgs.len(), 1);
        assert!(parse_json_message(&msgs[0]).is_none());
    }

    #[test]
    fn malformed_known_type_yields_none() {
        let msgs = parse_chat_lines("26-11-17 08:50:27\t[JSON:AddItem] not-json\n");
        assert_eq!(msgs.len(), 1);
        assert!(parse_json_message(&msgs[0]).is_none());
    }

    #[test]
    fn unknown_type_yields_unknown() {
        let msgs = parse_chat_lines("26-11-17 08:50:27\t[JSON:Pigeon] {\"From\":\"SomePlayer\"}\n");
        assert_eq!(msgs.len(), 1);
        match parse_json_message(&msgs[0]) {
            Some(ChatJsonEvent::Unknown { json_type, .. }) => assert_eq!(json_type, "Pigeon"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }
}
