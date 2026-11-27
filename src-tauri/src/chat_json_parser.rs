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
    /// `Pigeon`: received pigeon mail (send direction emits no line).
    PigeonReceived {
        timestamp: String,
        title: String,
        body: String,
    },
    /// `ParagonXP`: per-kill prodigy flow, same shape as `EarnedXP` (not
    /// cap-overflow). No DB home — the Status twin owns the live tally.
    /// Fields are read by tests and future consumers; the live arm is a
    /// deliberate no-op today (see dispatch).
    #[allow(dead_code)]
    ParagonXp {
        timestamp: String,
        skill: String,
        xp: u32,
        old_level: u32,
        new_level: u32,
    },
    /// `EntityReport`: single entity message (one arena tip observed).
    EntityReport {
        timestamp: String,
        title: String,
        body: String,
    },
    /// `Tip` word discovery: `<sel>WORD</sel>` + `Word of Power: NAME` +
    /// the effect sentence. Discovery auto-records into `words_of_power`
    /// (source `'chat'`); other `Tip` titles stay `Unknown`.
    WordDiscovered {
        timestamp: String,
        word: String,
        power_name: String,
        description: String,
    },
    /// `StartQuest`: quest accept (sole record — since the 2026-11-16 game
    /// update the giver screen no longer names the quest in Player.log).
    /// `quest_id` resolves via CDN `quest_<id>` → `Name`, else the daily
    /// fallback maps, else id-only at read time.
    QuestAccepted {
        timestamp: String,
        quest_id: u32,
    },
    /// Any `Type` with no frozen schema yet — structured ingest skips it,
    /// the raw chat row stays searchable.
    Unknown { json_type: String },
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AddItemJson {
    name: String,
    stack: Option<u32>,
    from_name: Option<String>,
    #[serde(default)]
    from_entity_id: Option<u32>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EarnedXpJson {
    skill: String,
    #[serde(rename = "XP")]
    xp: u32,
    old_level: Option<u32>,
    new_level: Option<u32>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BookJson {
    title: String,
    body: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DeltaFavorJson {
    npc: String,
    delta: Option<f64>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RecipeUsageJson {
    recipe_name: String,
    recipe_id: u32,
    completions: u32,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StartQuestJson {
    quest_id: u32,
}

/// Slice of `hay` strictly between the first `start` and the next `end`.
/// `None` when either marker is absent or inverted.
fn between<'a>(hay: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let s = hay.find(start)? + start.len();
    let e = hay[s..].find(end)? + s;
    (e >= s).then(|| &hay[s..e])
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
            let stack = v.stack?;
            if stack == 0 {
                return None;
            }
            Some(ChatJsonEvent::ItemReceipt {
                timestamp,
                item_name: v.name,
                quantity: stack,
                from_name: v.from_name,
                from_entity_id: v.from_entity_id,
            })
        }
        "EarnedXP" => {
            let v: EarnedXpJson = serde_json::from_str(body).ok()?;
            // Levels are required but may legitimately be zero (first XP in
            // an unlearned skill): `Option` rejects only absent fields, so a
            // missing level never becomes a destructive zero-level overwrite.
            let (Some(old_level), Some(new_level)) = (v.old_level, v.new_level) else {
                return None;
            };
            if v.skill.is_empty() || v.xp == 0 {
                return None;
            }
            Some(ChatJsonEvent::Xp {
                timestamp,
                skill: v.skill,
                xp: v.xp,
                old_level,
                new_level,
            })
        }
        "Book" => {
            let v: BookJson = serde_json::from_str(body).ok()?;
            if v.title.is_empty() || v.body.is_empty() {
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
            let delta = v.delta?;
            // Out-of-range floats (`1e309`) are rejected by `serde_json::from_str`
            // today: vendored serde_json 1.0.154 `f64_from_parts` returns
            // `Err(NumberOutOfRange)` when the result is infinite. The
            // `!delta.is_finite()` check below is therefore belt-and-suspenders
            // against a serde behavior change, not the live rejection path; keep
            // it as future-proofing (an infinite delta would poison SUM(delta)
            // with no repair path).
            if !delta.is_finite() || delta == 0.0 {
                return None;
            }
            Some(ChatJsonEvent::FavorDelta {
                timestamp,
                npc: v.npc,
                delta,
            })
        }
        "RecipeUsage" => {
            let v: RecipeUsageJson = serde_json::from_str(body).ok()?;
            if v.recipe_name.is_empty() || v.recipe_id == 0 {
                return None;
            }
            Some(ChatJsonEvent::RecipeUsed {
                timestamp,
                recipe_id: v.recipe_id,
                completions: v.completions,
            })
        }
        "StartQuest" => {
            let v: StartQuestJson = serde_json::from_str(body).ok()?;
            if v.quest_id == 0 {
                return None;
            }
            Some(ChatJsonEvent::QuestAccepted {
                timestamp,
                quest_id: v.quest_id,
            })
        }
        "Pigeon" => {
            let v: BookJson = serde_json::from_str(body).ok()?;
            if v.title.is_empty() || v.body.is_empty() {
                return None;
            }
            Some(ChatJsonEvent::PigeonReceived {
                timestamp,
                title: v.title,
                body: v.body,
            })
        }
        "ParagonXP" => {
            let v: EarnedXpJson = serde_json::from_str(body).ok()?;
            let (Some(old_level), Some(new_level)) = (v.old_level, v.new_level) else {
                return None;
            };
            if v.skill.is_empty() || v.xp == 0 {
                return None;
            }
            Some(ChatJsonEvent::ParagonXp {
                timestamp,
                skill: v.skill,
                xp: v.xp,
                old_level,
                new_level,
            })
        }
        "EntityReport" => {
            let v: BookJson = serde_json::from_str(body).ok()?;
            if v.title.is_empty() || v.body.is_empty() {
                return None;
            }
            Some(ChatJsonEvent::EntityReport {
                timestamp,
                title: v.title,
                body: v.body,
            })
        }
        "Tip" => {
            let v: BookJson = serde_json::from_str(body).ok()?;
            if v.title.is_empty() || v.body.is_empty() {
                return None;
            }
            if v.title != "You discovered a word of power!" {
                // A general channel (dementia, role-play effects): observed
                // but with no recorder, so it stays Unknown rather than
                // vanishing into a typed no-op.
                return Some(ChatJsonEvent::Unknown {
                    json_type: json_type.to_string(),
                });
            }
            let word = between(&v.body, "<sel>", "</sel>")?;
            let power_name = between(&v.body, "Word of Power: ", "</size>")?;
            let description = between(&v.body, "</b>\n", "\n\n<i>")?;
            if word.is_empty() || power_name.is_empty() || description.is_empty() {
                return None;
            }
            Some(ChatJsonEvent::WordDiscovered {
                timestamp,
                word: word.to_string(),
                power_name: power_name.to_string(),
                description: description.to_string(),
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
        assert_eq!(events.len(), 8);
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
        assert_eq!(events.len(), 4);
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
        assert_eq!(events.len(), 4);
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
        assert!(titles.contains(&"Gardening Almanac"));
    }

    #[test]
    fn delta_favor_lines_parse_exact() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-deltafavor.log"));
        assert_eq!(events.len(), 3);
        let ChatJsonEvent::FavorDelta { npc, delta, .. } = &events[0] else {
            panic!("expected FavorDelta, got {:?}", events[0]);
        };
        assert_eq!(npc, "NPC_Elune");
        assert!((delta - 18.06948).abs() < 1e-9);
        // Tasteless gift (live 2026-11-17): negative delta accrues, no gift row.
        let ChatJsonEvent::FavorDelta { npc, delta, .. } = &events[2] else {
            panic!("expected FavorDelta, got {:?}", events[2]);
        };
        assert_eq!(npc, "NPC_MushroomJack");
        assert!((delta + 52.65).abs() < 1e-9);
    }

    #[test]
    fn recipe_usage_lines_parse_absolute() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-recipeusage.log"));
        assert_eq!(events.len(), 1);
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
    fn recipe_usage_missing_completions_yields_none() {
        let msgs = parse_chat_lines("26-11-17 08:50:27\t[JSON:RecipeUsage] {\"RecipeName\":\"R\",\"RecipeId\":1}\n");
        assert_eq!(msgs.len(), 1);
        assert!(parse_json_message(&msgs[0]).is_none());
    }

    #[test]
    fn recipe_usage_explicit_zero_completions_valid() {
        let msgs = parse_chat_lines("26-11-17 08:50:27\t[JSON:RecipeUsage] {\"RecipeName\":\"R\",\"RecipeId\":1,\"Completions\":0}\n");
        assert_eq!(msgs.len(), 1);
        match parse_json_message(&msgs[0]) {
            Some(ChatJsonEvent::RecipeUsed { completions, .. }) => assert_eq!(completions, 0),
            other => panic!("expected RecipeUsed, got {other:?}"),
        }
    }

    #[test]
    fn start_quest_parses_to_quest_accepted() {
        // Verbatim monitor line 26-11-18 10:22:45 (gorgon_monitor.py:622):
        // single-field PascalCase body, no other keys observed.
        let msgs = parse_chat_lines("26-11-18 10:22:45\t[JSON:StartQuest] {\"QuestId\":25542}\n");
        assert_eq!(msgs.len(), 1);
        match parse_json_message(&msgs[0]) {
            Some(ChatJsonEvent::QuestAccepted { timestamp, quest_id }) => {
                assert_eq!(timestamp, "2026-11-18 10:22:45");
                assert_eq!(quest_id, 25542);
            }
            other => panic!("expected QuestAccepted, got {other:?}"),
        }
    }

    #[test]
    fn start_quest_malformed_yields_none() {
        // Missing QuestId, zero id, wrong-type id, and empty body are all
        // malformed (caller counts-and-skips), never Unknown.
        for body in [
            "{}",
            "{\"QuestId\":0}",
            "{\"QuestId\":\"25542\"}",
            "{\"QuestID\":25542}",
        ] {
            let line = format!("26-11-18 10:22:45\t[JSON:StartQuest] {body}\n");
            let msgs = parse_chat_lines(&line);
            assert_eq!(msgs.len(), 1, "body {body}");
            assert!(parse_json_message(&msgs[0]).is_none(), "body {body}");
        }
    }

    #[test]
    fn book_missing_body_yields_none() {
        let msgs = parse_chat_lines("26-11-17 08:50:27\t[JSON:Book] {\"Title\":\"T\"}\n");
        assert_eq!(msgs.len(), 1);
        assert!(parse_json_message(&msgs[0]).is_none());
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
    fn missing_required_fields_yield_none_not_zeros() {
        // Absent required fields must not default into a destructive
        // zero/empty event: an empty body would wipe book content, and
        // absent XP levels would zero a stored level via the upsert.
        // Favor/additem deltas and stacks must likewise be present and
        // non-zero. (Explicit zero completions are valid — see
        // `recipe_usage_explicit_zero_completions_valid`.)
        // This matrix enumerates the class, not past incidents: every typed
        // arm appears with each of its required fields missing or zeroed,
        // so the next arm inherits the rule instead of re-learning it.
        for line in [
            "26-11-17 08:52:00\t[JSON:RecipeUsage] {\"RecipeName\":\"Recall Heart's Home\", \"RecipeId\": 9008}\n",
            "26-11-17 08:52:00\t[JSON:RecipeUsage] {\"RecipeId\": 9008, \"Completions\": 211}\n",
            "26-11-17 08:52:00\t[JSON:RecipeUsage] {\"RecipeName\":\"Recall Heart's Home\", \"RecipeId\": 0, \"Completions\": 211}\n",
            "26-11-17 08:52:00\t[JSON:Book] {\"Title\":\"Skill Info\"}\n",
            "26-11-17 08:52:00\t[JSON:Book] {\"Body\":\"text\"}\n",
            "26-11-17 08:52:00\t[JSON:Pigeon] {\"Title\":\"Pigeon From TestFriend\"}\n",
            "26-11-17 08:52:00\t[JSON:Pigeon] {\"Body\":\"text\"}\n",
            "26-11-17 08:52:00\t[JSON:EarnedXP] {\"Skill\":\"Sword\", \"XP\": 53}\n",
            "26-11-17 08:52:00\t[JSON:EarnedXP] {\"Skill\":\"Sword\", \"XP\": 53, \"OldLevel\": 4}\n",
            "26-11-17 08:52:00\t[JSON:EarnedXP] {\"XP\": 53, \"OldLevel\": 4, \"NewLevel\": 5}\n",
            "26-11-17 08:52:00\t[JSON:EarnedXP] {\"Skill\":\"Sword\", \"XP\": 0, \"OldLevel\": 4, \"NewLevel\": 5}\n",
            "26-11-17 08:52:00\t[JSON:DeltaFavor] {\"Npc\":\"NPC_Elune\"}\n",
            "26-11-17 08:52:00\t[JSON:DeltaFavor] {\"Npc\":\"NPC_Elune\", \"Delta\": 0}\n",
            "26-11-17 08:52:00\t[JSON:DeltaFavor] {\"Npc\":\"NPC_Elune\", \"Delta\": 1e309}\n", // from_str-level rejection (NumberOutOfRange today; guard covers serde saturation)
            "26-11-17 08:52:00\t[JSON:DeltaFavor] {\"Npc\":\"NPC_Elune\", \"Delta\": -1e309}\n", // from_str-level rejection (NumberOutOfRange today; guard covers serde saturation)
            "26-11-17 08:52:00\t[JSON:DeltaFavor] {\"Delta\": 5.0}\n",
            "26-11-17 08:52:00\t[JSON:AddItem] {\"Name\":\"Toxic Sludge\"}\n",
            "26-11-17 08:52:00\t[JSON:AddItem] {\"Name\":\"Toxic Sludge\", \"Stack\": 0}\n",
            "26-11-17 08:52:00\t[JSON:AddItem] {\"Stack\": 3}\n",
            "26-11-17 08:52:00\t[JSON:ParagonXP] {\"Skill\":\"Sword\", \"XP\": 53}\n",
            "26-11-17 08:52:00\t[JSON:ParagonXP] {\"Skill\":\"Sword\", \"XP\": 0, \"OldLevel\": 4, \"NewLevel\": 5}\n",
            "26-11-17 08:52:00\t[JSON:ParagonXP] {\"XP\": 53, \"OldLevel\": 4, \"NewLevel\": 5}\n",
            "26-11-17 08:52:00\t[JSON:EntityReport] {\"Title\":\"Eveline Rastin\"}\n",
            "26-11-17 08:52:00\t[JSON:EntityReport] {\"Body\":\"tip\"}\n",
        ] {
            let msgs = parse_chat_lines(line);
            assert_eq!(msgs.len(), 1);
            assert!(parse_json_message(&msgs[0]).is_none(), "accepted {line:?}");
        }
    }

    #[test]
    fn explicit_zero_xp_levels_are_accepted_not_defaulted() {
        // First XP in an unlearned skill legitimately carries 0/0: explicit
        // zeros must survive (a fresh insert seeds level 0 correctly), while
        // *absent* levels are rejected above. Proves the `Option` fields
        // distinguish the two instead of defaulting both to zero.
        let line = "26-11-17 08:52:00\t[JSON:EarnedXP] {\"Skill\":\"Sword\", \"XP\": 53, \"OldLevel\": 0, \"NewLevel\": 0}\n";
        let msgs = parse_chat_lines(line);
        assert_eq!(msgs.len(), 1);
        match parse_json_message(&msgs[0]) {
            Some(ChatJsonEvent::Xp { old_level, new_level, .. }) => {
                assert_eq!((old_level, new_level), (0, 0))
            }
            other => panic!("expected Xp, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_yields_unknown() {
        let msgs = parse_chat_lines("26-11-17 08:50:27\t[JSON:Report] {\"Title\":\"Something\"}\n");
        assert_eq!(msgs.len(), 1);
        match parse_json_message(&msgs[0]) {
            Some(ChatJsonEvent::Unknown { json_type, .. }) => assert_eq!(json_type, "Report"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn pigeon_fixture_parses_typed() {
        // Receive direction, with and without item (live 2026-11-17, friend
        // renamed).
        let events = parse_all(include_str!("../../test_data/vip_json/chat-pigeon.log"));
        assert_eq!(events.len(), 3);
        for ev in &events {
            match ev {
                ChatJsonEvent::PigeonReceived { title, body, .. } => {
                    assert!(title.starts_with("Pigeon From "));
                    assert!(!body.is_empty());
                }
                other => panic!("expected PigeonReceived, got {other:?}"),
            }
        }
        match &events[1] {
            ChatJsonEvent::PigeonReceived { body, .. } => {
                assert!(body.contains("Mulberry Salad x10"))
            }
            other => panic!("expected PigeonReceived, got {other:?}"),
        }
    }

    #[test]
    fn kill_slice_yields_typed_json_events() {
        // Solo City Slime kill (live 2026-11-17): ParagonXP twins are typed
        // (no DB home — the Status twin owns the live tally), and the
        // receipt carries the FATALITY entity ID.
        let events = parse_all(include_str!("../../test_data/vip_json/chat-kill-slice.log"));
        assert_eq!(events.len(), 4);
        for ev in &events[0..2] {
            match ev {
                ChatJsonEvent::ParagonXp { skill, xp, .. } => {
                    assert!(!skill.is_empty());
                    assert!(*xp > 0);
                }
                other => panic!("expected ParagonXp, got {other:?}"),
            }
        }
        let ChatJsonEvent::Xp { skill, xp, .. } = &events[2] else {
            panic!("expected Xp, got {:?}", events[2]);
        };
        assert_eq!(skill, "Anatomy_Fungoids");
        assert_eq!(*xp, 105);
        let ChatJsonEvent::ItemReceipt { from_entity_id, .. } = &events[3] else {
            panic!("expected ItemReceipt, got {:?}", events[3]);
        };
        assert_eq!(*from_entity_id, Some(488966));
    }

    #[test]
    fn entity_report_parses_typed() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-entityreport.log"));
        assert_eq!(events.len(), 1);
        let ChatJsonEvent::EntityReport { title, body, .. } = &events[0] else {
            panic!("expected EntityReport, got {:?}", events[0]);
        };
        assert_eq!(title, "Eveline Rastin");
        assert!(body.contains("Ushug"));
    }

    #[test]
    fn tip_fixture_discovers_words() {
        let events = parse_all(include_str!("../../test_data/vip_json/chat-tip.log"));
        assert_eq!(events.len(), 11);
        let mut words = Vec::new();
        let mut unknown = 0;
        for ev in &events {
            match ev {
                ChatJsonEvent::WordDiscovered {
                    word,
                    power_name,
                    description,
                    ..
                } => words.push((word.clone(), power_name.clone(), description.clone())),
                ChatJsonEvent::Unknown { json_type } => {
                    assert_eq!(json_type, "Tip");
                    unknown += 1;
                }
                other => panic!("expected WordDiscovered or Unknown, got {other:?}"),
            }
        }
        // Ten scroll discoveries plus the (observed, unwritten) dementia tip.
        assert_eq!(unknown, 1);
        assert_eq!(words.len(), 10);
        let breath = words.iter().find(|(w, _, _)| w == "BWEPFYW").expect("BWEPFYW");
        assert_eq!(breath.1, "Hold Your Breath");
        assert!(breath.2.contains("lung capacity"));
        let death = words.iter().find(|(w, _, _)| w == "LIBGLEN").expect("LIBGLEN");
        assert_eq!(death.1, "Instant Death");
    }


    fn json_message(json_type: &str, body: &str) -> ChatMessage {
        let line = format!("26-11-17 08:52:00\t[JSON:{json_type}] {body}\n");
        let mut messages = parse_chat_lines(&line);
        assert_eq!(messages.len(), 1);
        messages.remove(0)
    }

    #[test]
    fn known_types_reject_invalid_json_shapes() {
        for kind in [
            "AddItem", "EarnedXP", "Book", "DeltaFavor", "RecipeUsage",
            "Pigeon", "ParagonXP", "EntityReport", "Tip",
        ] {
            for body in ["not-json", "null", "[]", "true", "42", "{}", "{\"Title\":"] {
                assert!(
                    parse_json_message(&json_message(kind, body)).is_none(),
                    "accepted {kind}: {body}"
                );
            }
        }
    }

    #[test]
    fn text_events_preserve_escaped_content_and_timestamp() {
        let title = "Éveline's report: [JSON:Book]";
        let body = "First line\n\n<i>You received:</i>\t\"Salad\" \\ 雪";
        let payload = serde_json::json!({"Title": title, "Body": body}).to_string();
        for kind in ["Book", "Pigeon", "EntityReport"] {
            let event = parse_json_message(&json_message(kind, &payload)).unwrap();
            let (timestamp, actual_title, actual_body) = match event {
                ChatJsonEvent::Book { timestamp, title, body }
                | ChatJsonEvent::PigeonReceived { timestamp, title, body }
                | ChatJsonEvent::EntityReport { timestamp, title, body } => (timestamp, title, body),
                other => panic!("expected text event for {kind}, got {other:?}"),
            };
            assert_eq!(timestamp, "2026-11-17 08:52:00");
            assert_eq!(actual_title, title);
            assert_eq!(actual_body, body);
        }
    }

    #[test]
    fn text_events_reject_empty_null_and_wrong_type_fields() {
        for kind in ["Book", "Pigeon", "EntityReport"] {
            for field in ["Title", "Body"] {
                for invalid in [serde_json::json!(""), serde_json::Value::Null, serde_json::json!(7)] {
                    let mut payload = serde_json::json!({"Title": "Report", "Body": "content"});
                    payload[field] = invalid;
                    assert!(
                        parse_json_message(&json_message(kind, &payload.to_string())).is_none(),
                        "accepted {kind}: {payload}"
                    );
                }
            }
        }
    }

    #[test]
    fn xp_events_accept_explicit_zero_levels_and_u32_boundaries() {
        for kind in ["EarnedXP", "ParagonXP"] {
            for (xp, old, new) in [(1, 0, 0), (u32::MAX, u32::MAX - 1, u32::MAX)] {
                let payload = serde_json::json!({
                    "Skill": "Sword", "XP": xp, "OldLevel": old, "NewLevel": new,
                    "FutureField": {"ignored": true}
                });
                let event = parse_json_message(&json_message(kind, &payload.to_string())).unwrap();
                let (timestamp, skill, actual_xp, old_level, new_level) = match event {
                    ChatJsonEvent::Xp { timestamp, skill, xp, old_level, new_level }
                    | ChatJsonEvent::ParagonXp { timestamp, skill, xp, old_level, new_level } =>
                        (timestamp, skill, xp, old_level, new_level),
                    other => panic!("expected XP for {kind}, got {other:?}"),
                };
                assert_eq!(timestamp, "2026-11-17 08:52:00");
                assert_eq!(skill, "Sword");
                assert_eq!((actual_xp, old_level, new_level), (xp, old, new));
            }
        }
    }

    #[test]
    fn xp_events_reject_null_missing_and_invalid_numeric_fields() {
        for kind in ["EarnedXP", "ParagonXP"] {
            for field in ["XP", "OldLevel", "NewLevel"] {
                for invalid in [
                    serde_json::Value::Null, serde_json::json!(-1),
                    serde_json::json!(4294967296u64), serde_json::json!(1.5),
                    serde_json::json!("1"),
                ] {
                    let mut payload = serde_json::json!({
                        "Skill": "Sword", "XP": 53, "OldLevel": 4, "NewLevel": 5
                    });
                    payload[field] = invalid;
                    assert!(
                        parse_json_message(&json_message(kind, &payload.to_string())).is_none(),
                        "accepted {kind}: {payload}"
                    );
                }
                let mut payload = serde_json::json!({
                    "Skill": "Sword", "XP": 53, "OldLevel": 4, "NewLevel": 5
                });
                payload.as_object_mut().unwrap().remove(field);
                assert!(parse_json_message(&json_message(kind, &payload.to_string())).is_none());
            }
        }
    }

    #[test]
    fn add_item_optional_source_does_not_change_receipt_quantity() {
        for source in [serde_json::json!({}), serde_json::json!({"FromName": null, "FromEntityId": null})] {
            let mut payload = serde_json::json!({"Name": "Salad", "Stack": u32::MAX, "Flags": []});
            payload.as_object_mut().unwrap().extend(source.as_object().unwrap().clone());
            match parse_json_message(&json_message("AddItem", &payload.to_string())) {
                Some(ChatJsonEvent::ItemReceipt { timestamp, item_name, quantity, from_name, from_entity_id }) => {
                    assert_eq!(timestamp, "2026-11-17 08:52:00");
                    assert_eq!(item_name, "Salad");
                    assert_eq!(quantity, u32::MAX);
                    assert_eq!((from_name, from_entity_id), (None, None));
                }
                other => panic!("expected receipt, got {other:?}"),
            }
        }
    }

    #[test]
    fn add_item_and_recipe_reject_invalid_unsigned_values() {
        for (kind, template, fields) in [
            ("AddItem", serde_json::json!({"Name": "Salad", "Stack": 1}), vec!["Stack"]),
            ("RecipeUsage", serde_json::json!({"RecipeName": "Recall", "RecipeId": 9, "Completions": 1}), vec!["RecipeId", "Completions"]),
        ] {
            for field in fields {
                for invalid in [serde_json::Value::Null, serde_json::json!(-1), serde_json::json!(4294967296u64), serde_json::json!(1.5)] {
                    let mut payload = template.clone();
                    payload[field] = invalid;
                    assert!(
                        parse_json_message(&json_message(kind, &payload.to_string())).is_none(),
                        "accepted {kind}: {payload}"
                    );
                }
            }
        }
    }

    #[test]
    fn favor_rejects_negative_zero_and_preserves_small_signed_deltas() {
        assert!(parse_json_message(&json_message("DeltaFavor", r#"{"Npc":"NPC_Elune","Delta":-0.0}"#)).is_none());
        for delta in [0.000001, -0.000001] {
            let payload = serde_json::json!({"Npc": "NPC_Elune", "Delta": delta, "Extra": true});
            match parse_json_message(&json_message("DeltaFavor", &payload.to_string())) {
                Some(ChatJsonEvent::FavorDelta { timestamp, npc, delta: actual }) => {
                    assert_eq!(timestamp, "2026-11-17 08:52:00");
                    assert_eq!(npc, "NPC_Elune");
                    assert!((actual - delta).abs() < 1e-15);
                }
                other => panic!("expected favor, got {other:?}"),
            }
        }
    }

    #[test]
    fn missing_json_metadata_is_not_an_event() {
        let original = json_message("Book", r#"{"Title":"Report","Body":"content"}"#);
        let mut no_type = original.clone();
        no_type.json_type = None;
        assert!(parse_json_message(&no_type).is_none());
        let mut no_body = original;
        no_body.json_body = None;
        assert!(parse_json_message(&no_body).is_none());
    }

    #[test]
    fn malformed_json_does_not_consume_the_next_valid_message() {
        let events = parse_all(concat!(
            "26-11-17 08:52:00\t[JSON:Book] {broken\n",
            "26-11-17 08:52:01\t[JSON:FutureType] not-json\n",
            "26-11-17 08:52:02\t[JSON:AddItem] {\"Name\":\"Salad\",\"Stack\":2}\n",
        ));
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ChatJsonEvent::Unknown { json_type } if json_type == "FutureType"));
        assert!(matches!(&events[1], ChatJsonEvent::ItemReceipt { item_name, quantity: 2, .. } if item_name == "Salad"));
    }

}
