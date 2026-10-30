/// Chat Status channel parser — converts [Status] messages into structured events.
///
/// Stateless parser: each message maps to 0 or 1 events.
/// Accumulation and cross-stream correlation are left to subscribing features.
use crate::chat_parser::ChatMessage;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
pub enum ChatStatusEvent {
    /// "X added to inventory" / "X xN added to inventory"
    ItemGained {
        timestamp: String,
        item_name: String,
        quantity: u32,
    },

    /// "You earned N XP in Skill."
    XpGained {
        timestamp: String,
        skill: String,
        amount: u32,
    },

    /// "You earned N Prodigy XP in Skill." — combat XP overflow earned by a
    /// maxed combat skill, which feeds Prodigy Potential rather than skill XP.
    ProdigyXpGained {
        timestamp: String,
        skill: String,
        amount: u32,
    },

    /// "You earned N Combat Wisdom: Killed the Aktaari Queen"
    /// Combat Wisdom is a currency awarded for killing notable monsters. The
    /// reason text names the monster (after a verb like "Killed"/"Defeated"),
    /// with an optional trailing " (Zone)". The special "Earned a Prodigy Level"
    /// reason is not a monster (source_name = None).
    CombatWisdomEarned {
        timestamp: String,
        amount: u32,
        source_name: Option<String>,
        verb: String,
        zone: Option<String>,
    },

    /// "You earned N XP and reached level L in Skill!"
    LevelUp {
        timestamp: String,
        skill: String,
        level: u32,
        xp: u32,
    },

    /// "You searched the corpse and found N coins."
    CoinsLooted { timestamp: String, amount: u32 },

    /// "You received N Councils." / "You used N councils."
    CouncilsChanged { timestamp: String, amount: i64 },

    /// "The treasure is N meters from here."
    TreasureDistance { timestamp: String, meters: u32 },

    /// "You bury the corpse." / "You botch the autopsy!"
    AnatomyResult { timestamp: String, success: bool },

    /// "Summoned X xN"
    Summoned {
        timestamp: String,
        item_name: String,
        quantity: u32,
    },

    /// "CrudBurst's Hammer of Thumping carefully studied!"
    ItemStudied {
        timestamp: String,
        item_name: String,
    },

    /// "Saved report to C:/.../Reports/Character_Foo.json"
    /// Fired when the player runs Export Character as JSON or Export Storage as JSON (Settings → VIP).
    ReportSaved {
        timestamp: String,
        file_path: String,
    },

    /// "Roulette ball ended on N!" — the winning number of a casino roulette
    /// spin (European single-zero wheel, so `number` is 0..=36). This is the
    /// only roulette event written to the logs; the player's own bet is an
    /// on-screen toast that is never logged, so only outcomes are trackable.
    RouletteResult { timestamp: String, number: u32 },

    /// "You use the word of power WORD!" — the player spoke a word of power.
    /// A spoken word is consumed game-wide, so every saved copy of it is dead.
    WordOfPowerUsed { timestamp: String, word: String },

    /// "X has milked you."
    MilkedBy { timestamp: String, milker: String },
}

/// Try to parse a Status channel ChatMessage into a structured event.
/// Returns None if the message doesn't match any known pattern.
pub fn parse_status_message(msg: &ChatMessage) -> Option<ChatStatusEvent> {
    // Only process Status channel messages
    if msg.channel.as_deref() != Some("Status") {
        return None;
    }

    let text = msg.message.trim();
    let ts = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

    // Try each pattern in order of frequency/importance
    try_item_gained(text, &ts)
        .or_else(|| try_combat_wisdom_earned(text, &ts))
        .or_else(|| try_prodigy_xp_gained(text, &ts))
        .or_else(|| try_xp_gained(text, &ts))
        .or_else(|| try_level_up(text, &ts))
        .or_else(|| try_treasure_distance(text, &ts))
        .or_else(|| try_anatomy_result(text, &ts))
        .or_else(|| try_coins_looted(text, &ts))
        .or_else(|| try_councils_changed(text, &ts))
        .or_else(|| try_councils_misc(text, &ts))
        .or_else(|| try_summoned(text, &ts))
        .or_else(|| try_item_studied(text, &ts))
        .or_else(|| try_report_saved(text, &ts))
        .or_else(|| try_roulette_result(text, &ts))
        .or_else(|| try_word_of_power_used(text, &ts))
        .or_else(|| try_milked_by(text, &ts))
}

/// "You use the word of power WORD!" — the player spoke a word of power.
fn try_word_of_power_used(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    let word = text
        .strip_prefix("You use the word of power ")?
        .strip_suffix('!')?
        .trim();
    if word.is_empty() {
        return None;
    }
    Some(ChatStatusEvent::WordOfPowerUsed {
        timestamp: ts.to_string(),
        word: word.to_string(),
    })
}

/// "X has milked you." — shared by the Chat `[Status]` parser and the
/// Player.log `ScreenText(ImportantInfo)` arm in the coordinator, so both
/// producers normalize names identically (the milking dedup key includes
/// `other_player`, so divergent normalization would double-count).
/// Returns the milker name, or None when the text is not a milk line.
pub fn strip_milked_by_suffix(text: &str) -> Option<String> {
    let milker = text.strip_suffix(" has milked you.")?.trim();
    if milker.is_empty() || milker.contains('.') {
        return None;
    }
    Some(milker.to_string())
}

/// "X has milked you."
fn try_milked_by(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    strip_milked_by_suffix(text).map(|milker| ChatStatusEvent::MilkedBy {
        timestamp: ts.to_string(),
        milker,
    })
}

/// "Roulette ball ended on N!" — casino spin outcome.
/// N is a European-wheel number, 0..=36. Anything else is not a roulette
/// result (future format drift) and returns None. The `roulette_results`
/// CHECK (migration v77) and the `record_roulette_result` guard enforce the
/// same range at the DB boundary.
fn try_roulette_result(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    let prefix = "Roulette ball ended on ";
    let inner = text.strip_prefix(prefix)?.strip_suffix('!')?;
    let number: u32 = inner.parse().ok()?;
    if number > 36 {
        return None;
    }
    Some(ChatStatusEvent::RouletteResult {
        timestamp: ts.to_string(),
        number,
    })
}

/// "X added to inventory." or "X xN added to inventory."
fn try_item_gained(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    let suffix = " added to inventory.";
    if !text.ends_with(suffix) {
        return None;
    }
    let prefix = &text[..text.len() - suffix.len()];
    // Reject empty item names (" added to inventory." with no prefix).
    if prefix.trim().is_empty() {
        return None;
    }

    // Check for "ItemName xN" pattern
    let (item_name, quantity) = parse_name_and_quantity(prefix);
    // parse_name_and_quantity returns qty=1 with an empty name for a bare
    // " xN" prefix — reject that too.
    if item_name.trim().is_empty() {
        return None;
    }

    Some(ChatStatusEvent::ItemGained {
        timestamp: ts.to_string(),
        item_name: item_name.to_string(),
        quantity,
    })
}

/// "You earned N Combat Wisdom: <reason>"
/// e.g. "You earned 64 Combat Wisdom: Killed the Aktaari Queen"
///      "You earned 73 Combat Wisdom: Defeated Elite Tactician"
///      "You earned 5 Combat Wisdom: Killed The Productivity Expert (Gazluk)"
///      "You earned 1000 Combat Wisdom: Earned a Prodigy Level"
fn try_combat_wisdom_earned(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if !text.starts_with("You earned ") {
        return None;
    }
    let after = &text["You earned ".len()..];
    let infix = " Combat Wisdom: ";
    let infix_pos = after.find(infix)?;
    // Comma-grouped thousands ("1,250") like the councils twin — the game
    // groups large award amounts the same way it groups wallet deltas.
    let amount: u32 = after[..infix_pos].replace(',', "").parse().ok()?;
    let reason = after[infix_pos + infix.len()..].trim();
    if reason.is_empty() {
        return None;
    }

    // Non-monster award: prodigy level. ("Earned a Prodigy Level")
    if reason == "Earned a Prodigy Level" {
        return Some(ChatStatusEvent::CombatWisdomEarned {
            timestamp: ts.to_string(),
            amount,
            source_name: None,
            verb: "Earned".to_string(),
            zone: None,
        });
    }

    // "<Verb> <monster name>[ (Zone)]" — split verb off the front, zone off the back.
    let (verb, rest) = match reason.split_once(' ') {
        Some((v, r)) => (v.to_string(), r),
        None => (reason.to_string(), ""),
    };

    let (name, zone) = match (rest.strip_suffix(')'), rest.rfind(" (")) {
        (Some(_), Some(open)) => {
            let zone = rest[open + 2..rest.len() - 1].to_string();
            (rest[..open].to_string(), Some(zone))
        }
        _ => (rest.to_string(), None),
    };

    let source_name = if name.is_empty() { None } else { Some(name) };

    Some(ChatStatusEvent::CombatWisdomEarned {
        timestamp: ts.to_string(),
        amount,
        source_name,
        verb,
        zone,
    })
}

/// "You earned N Prodigy XP in Skill."
fn try_prodigy_xp_gained(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if !text.starts_with("You earned ") || !text.ends_with('.') {
        return None;
    }

    // "You earned 45 Prodigy XP in Pig."
    let inner = &text["You earned ".len()..text.len() - 1]; // strip trailing "."
    let xp_pos = inner.find(" Prodigy XP in ")?;
    let amount: u32 = inner[..xp_pos].parse().ok()?;
    let skill = &inner[xp_pos + " Prodigy XP in ".len()..];
    if skill.trim().is_empty() {
        return None;
    }

    Some(ChatStatusEvent::ProdigyXpGained {
        timestamp: ts.to_string(),
        skill: skill.to_string(),
        amount,
    })
}

/// "You earned N XP in Skill." (but NOT the level-up variant)
fn try_xp_gained(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    // Must start with "You earned " and end with a period (not !)
    if !text.starts_with("You earned ") || !text.ends_with('.') {
        return None;
    }
    // Exclude level-up messages (they end with "!")
    if text.contains("reached level") {
        return None;
    }

    // "You earned 62 XP in Endurance."
    let inner = &text["You earned ".len()..text.len() - 1]; // strip trailing "."
    let xp_pos = inner.find(" XP in ")?;
    let amount: u32 = inner[..xp_pos].parse().ok()?;
    let skill = &inner[xp_pos + " XP in ".len()..];
    if skill.trim().is_empty() {
        return None;
    }

    Some(ChatStatusEvent::XpGained {
        timestamp: ts.to_string(),
        skill: skill.to_string(),
        amount,
    })
}

/// "You earned N XP and reached level L in Skill!"
fn try_level_up(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if !text.starts_with("You earned ") || !text.ends_with('!') {
        return None;
    }
    if !text.contains("reached level") {
        return None;
    }

    // "You earned 192 XP and reached level 87 in Cartography!"
    let inner = &text["You earned ".len()..text.len() - 1]; // strip "!"
    let xp_pos = inner.find(" XP and reached level ")?;
    let xp: u32 = inner[..xp_pos].parse().ok()?;

    let after_level = &inner[xp_pos + " XP and reached level ".len()..];
    let level_end = after_level.find(" in ")?;
    let level: u32 = after_level[..level_end].parse().ok()?;
    let skill = &after_level[level_end + " in ".len()..];
    // Reject "You earned 5 XP and reached level 3 in !" — matches the sibling
    // empty-name guards (ItemGained/Xp/Prodigy/Summoned).
    if skill.trim().is_empty() {
        return None;
    }

    Some(ChatStatusEvent::LevelUp {
        timestamp: ts.to_string(),
        skill: skill.to_string(),
        level,
        xp,
    })
}

/// "The treasure is N meters from here." (straight-line ping) or the
/// directional shape "<anything> is <dx>m <east|west> and <dy>m <north|south>."
/// (e.g. "The treasure is 384m east and 1006m south.",
/// "The Fluorite is 814m east and 2328m north."). The directional form
/// reports per-axis offsets, not a distance, so it is collapsed to the
/// Euclidean norm `sqrt(dx*dx + dy*dy)` rounded to the nearest meter —
/// display-grade approximation, no new event variant.
fn try_treasure_distance(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if text.starts_with("The treasure is ") && text.ends_with(" meters from here.") {
        let inner = &text["The treasure is ".len()..text.len() - " meters from here.".len()];
        let meters: u32 = inner.parse().ok()?;
        return Some(ChatStatusEvent::TreasureDistance {
            timestamp: ts.to_string(),
            meters,
        });
    }
    // Directional: "<subject> is <dx>m <east|west> and <dy>m <north|south>."
    let is_pos = text.find(" is ")?;
    let after_is = &text[is_pos + " is ".len()..];
    let and_pos = after_is.find("m east and ").or_else(|| after_is.find("m west and "))?;
    let dx: f64 = after_is[..and_pos].parse().ok()?;
    let rest = &after_is[and_pos..];
    // rest = "m <dir> and <dy>m <dir>."
    let m_end = rest.find("m ")?;
    let after_dir = &rest[m_end + "m ".len()..]; // "<dir> and <dy>m <dir>."
    let and_word = after_dir.find(" and ")?;
    let dy_part = &after_dir[and_word + " and ".len()..]; // "<dy>m <dir>."
    let dy_end = dy_part.find('m')?;
    let dy: f64 = dy_part[..dy_end].parse().ok()?;
    let dir_tail = &dy_part[dy_end..]; // "m <dir>."
    if !(dir_tail.ends_with(" north.")
        || dir_tail.ends_with(" south.")
        || dir_tail.ends_with(" east.")
        || dir_tail.ends_with(" west."))
    {
        return None;
    }
    let meters = (dx * dx + dy * dy).sqrt().round() as u32;
    Some(ChatStatusEvent::TreasureDistance {
        timestamp: ts.to_string(),
        meters,
    })
}

/// "You bury the corpse." or "You botch the autopsy!"
fn try_anatomy_result(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if text == "You bury the corpse." {
        return Some(ChatStatusEvent::AnatomyResult {
            timestamp: ts.to_string(),
            success: true,
        });
    }
    if text == "You botch the autopsy!" {
        return Some(ChatStatusEvent::AnatomyResult {
            timestamp: ts.to_string(),
            success: false,
        });
    }
    None
}

/// "You searched the corpse and found N coins."
fn try_coins_looted(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if !text.starts_with("You searched the corpse and found ") || !text.ends_with(" coins.") {
        return None;
    }
    let inner = &text["You searched the corpse and found ".len()..text.len() - " coins.".len()];
    let amount: u32 = inner.parse().ok()?;

    Some(ChatStatusEvent::CoinsLooted {
        timestamp: ts.to_string(),
        amount,
    })
}

/// "You received N Councils." / "You used N councils."
fn try_councils_changed(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if text.starts_with("You received ") && text.ends_with(" Councils.") {
        let inner = &text["You received ".len()..text.len() - " Councils.".len()];
        let amount: i64 = inner.replace(',', "").parse().ok()?;
        return Some(ChatStatusEvent::CouncilsChanged {
            timestamp: ts.to_string(),
            amount,
        });
    }
    if text.starts_with("You used ") && text.ends_with(" councils.") {
        let inner = &text["You used ".len()..text.len() - " councils.".len()];
        let amount: i64 = inner.replace(',', "").parse().ok()?;
        return Some(ChatStatusEvent::CouncilsChanged {
            timestamp: ts.to_string(),
            amount: -amount,
        });
    }
    None
}

/// Wallet-affecting [Status] lines beyond the corpse-loot / received-Councils
/// basics. The wallet is ONE currency (internal `GOLD`, shown in-game as
/// "councils"), so these all map to a signed `CouncilsChanged`. Gains are
/// positive; the mugger theft is negative. From a real-log audit, these are the
/// remaining visible wallet deltas (spends are mostly invisible in the logs):
///   "You receive N coins."                       (+)  autoloot / coin-sack open
///   "You recovered N Councils stolen by <mob>."  (+)
///   "You retrieve N stolen coins."               (+)
///   "You were given N coins by <player>!"        (+)
///   "<mob> stole N Councils!"                    (−)
fn try_councils_misc(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    /// Parse a comma-grouped integer ("1,700" → 1700).
    fn num(s: &str) -> Option<i64> {
        s.replace(',', "").parse().ok()
    }
    let signed = |amount: i64| ChatStatusEvent::CouncilsChanged {
        timestamp: ts.to_string(),
        amount,
    };

    // "You receive N coins." — distinct from "You received N Councils." and from
    // the corpse-search "found N coins." line (both handled earlier).
    if let Some(rest) = text.strip_prefix("You receive ") {
        if let Some(n) = rest.strip_suffix(" coins.") {
            return num(n).map(signed);
        }
    }

    // "You recovered N Councils stolen by <mob>."
    if let Some(rest) = text.strip_prefix("You recovered ") {
        if let Some(idx) = rest.find(" Councils stolen by ") {
            return num(&rest[..idx]).map(signed);
        }
    }

    // "You retrieve N stolen coins."
    if let Some(rest) = text.strip_prefix("You retrieve ") {
        if let Some(n) = rest.strip_suffix(" stolen coins.") {
            return num(n).map(signed);
        }
    }

    // "You were given N coins by <player>!"
    if let Some(rest) = text.strip_prefix("You were given ") {
        if let Some(idx) = rest.find(" coins by ") {
            return num(&rest[..idx]).map(signed);
        }
    }

    // "<mob> stole N Councils!" — a wallet loss (often partly recovered later).
    // Anchored form: `<thief> stole <amount> Councils!` with a non-empty
    // thief prefix and a numeric amount (previously an unanchored `find`,
    // which could match a quoted/truncated line). Verified against
    // parser tests: "Ratkin Mugger stole 320 Councils!" parses; "You ..."
    // lines never reach here (matched by earlier arms).
    if let Some(body) = text.strip_suffix(" Councils!") {
        if let Some((thief, amount)) = body.split_once(" stole ") {
            let t = thief.trim();
            // Prefix anchor: reject quoted/channel chatter
            // (e.g. `Bob says "Rat stole 5 Councils!"` must not parse as theft).
            if !t.is_empty() && !t.contains('"') && !t.contains(" says ") && !amount.trim().is_empty() {
                return num(amount).map(|a| signed(-a));
            }
        }
    }

    None
}

/// "Summoned X xN"
fn try_summoned(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    if !text.starts_with("Summoned ") {
        return None;
    }
    let rest = &text["Summoned ".len()..];
    // Reject "Summoned " with no item (matches the style of the sibling
    // empty-name guards).
    if rest.trim().is_empty() {
        return None;
    }
    let (item_name, quantity) = parse_name_and_quantity(rest);
    if item_name.trim().is_empty() {
        return None;
    }

    Some(ChatStatusEvent::Summoned {
        timestamp: ts.to_string(),
        item_name: item_name.to_string(),
        quantity,
    })
}

/// "CrudBurst's Hammer of Thumping carefully studied!"
/// Hoplology equipment study — fires when a player studies a piece of equipment.
/// The 5-minute cooldown between studies is tracked on the frontend.
fn try_item_studied(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    let suffix = " carefully studied!";
    if !text.ends_with(suffix) {
        return None;
    }
    let item_name = &text[..text.len() - suffix.len()];
    if item_name.is_empty() {
        return None;
    }

    Some(ChatStatusEvent::ItemStudied {
        timestamp: ts.to_string(),
        item_name: item_name.to_string(),
    })
}

/// "Saved report to C:/.../Reports/Character_Foo.json"
fn try_report_saved(text: &str, ts: &str) -> Option<ChatStatusEvent> {
    let prefix = "Saved report to ";
    if !text.starts_with(prefix) {
        return None;
    }
    let file_path = &text[prefix.len()..];
    if file_path.is_empty() {
        return None;
    }

    Some(ChatStatusEvent::ReportSaved {
        timestamp: ts.to_string(),
        file_path: file_path.to_string(),
    })
}

/// Parse "ItemName xN" or "ItemName" → (name, quantity).
/// Returns quantity=1 if no "xN" suffix is found.
fn parse_name_and_quantity(text: &str) -> (&str, u32) {
    // Look for " xN" at the end where N is one or more digits
    if let Some(x_pos) = text.rfind(" x") {
        let after_x = &text[x_pos + 2..];
        if let Ok(qty) = after_x.parse::<u32>() {
            return (&text[..x_pos], qty);
        }
    }
    (text, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_parser::parse_chat_line;

    fn status_msg(text: &str) -> ChatMessage {
        let line = format!("26-03-26 15:12:48\t[Status] {}", text);
        parse_chat_line(&line).unwrap()
    }

    #[test]
    fn test_item_gained_no_quantity() {
        let msg = status_msg("Tundra Rye Seeds added to inventory.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ItemGained {
            item_name,
            quantity,
            ..
        } = event
        {
            assert_eq!(item_name, "Tundra Rye Seeds");
            assert_eq!(quantity, 1);
        } else {
            panic!("Expected ItemGained, got {:?}", event);
        }
    }

    #[test]
    fn test_item_gained_with_quantity() {
        let msg = status_msg("Astounding Metal Slab x26 added to inventory.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ItemGained {
            item_name,
            quantity,
            ..
        } = event
        {
            assert_eq!(item_name, "Astounding Metal Slab");
            assert_eq!(quantity, 26);
        } else {
            panic!("Expected ItemGained, got {:?}", event);
        }
    }

    #[test]
    fn test_item_gained_gypsum() {
        let msg = status_msg("Gypsum x9 added to inventory.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ItemGained {
            item_name,
            quantity,
            ..
        } = event
        {
            assert_eq!(item_name, "Gypsum");
            assert_eq!(quantity, 9);
        } else {
            panic!("Expected ItemGained, got {:?}", event);
        }
    }

    #[test]
    fn test_word_of_power_used() {
        let msg = status_msg("You use the word of power SLOAVHOUJALGLUWFRYLGWODNYD!");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::WordOfPowerUsed { word, .. } = event {
            assert_eq!(word, "SLOAVHOUJALGLUWFRYLGWODNYD");
        } else {
            panic!("Expected WordOfPowerUsed, got {:?}", event);
        }
    }

    #[test]
    fn test_word_of_power_used_requires_exclamation() {
        // Missing trailing "!" must not match
        let msg = status_msg("You use the word of power ABCDEF");
        assert!(parse_status_message(&msg).is_none());
    }

    #[test]
    fn test_milked_by() {
        // Real line shape from Chat-26-10-24.log
        let msg = status_msg("Azyima has milked you.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::MilkedBy { milker, .. } = event {
            assert_eq!(milker, "Azyima");
        } else {
            panic!("Expected MilkedBy, got {:?}", event);
        }
    }

    #[test]
    fn test_milked_by_rejects_empty_and_dotted() {
        // Bare suffix with no name must not match
        assert!(strip_milked_by_suffix(" has milked you.").is_none());
        // Dotted prefix looks like a sentence fragment, not a player name
        assert!(strip_milked_by_suffix("Someone. has milked you.").is_none());
        // Wrong suffix must not match
        assert!(strip_milked_by_suffix("Azyima has milked Homer.").is_none());
    }

    #[test]
    fn test_xp_gained() {
        let msg = status_msg("You earned 62 XP in Endurance.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::XpGained { skill, amount, .. } = event {
            assert_eq!(skill, "Endurance");
            assert_eq!(amount, 62);
        } else {
            panic!("Expected XpGained, got {:?}", event);
        }
    }

    #[test]
    fn test_prodigy_xp_gained() {
        let msg = status_msg("You earned 45 Prodigy XP in Pig.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ProdigyXpGained { skill, amount, .. } = event {
            assert_eq!(skill, "Pig");
            assert_eq!(amount, 45);
        } else {
            panic!("Expected ProdigyXpGained, got {:?}", event);
        }
    }

    #[test]
    fn test_normal_xp_not_parsed_as_prodigy() {
        // A normal combat XP line must not be swallowed by the prodigy parser.
        let msg = status_msg("You earned 45 XP in Battle Chemistry.");
        let event = parse_status_message(&msg).unwrap();
        assert!(
            matches!(event, ChatStatusEvent::XpGained { .. }),
            "Expected XpGained, got {:?}",
            event
        );
    }

    #[test]
    fn test_xp_gained_multi_word_skill() {
        let msg = status_msg("You earned 67 XP in Canine Anatomy.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::XpGained { skill, amount, .. } = event {
            assert_eq!(skill, "Canine Anatomy");
            assert_eq!(amount, 67);
        } else {
            panic!("Expected XpGained, got {:?}", event);
        }
    }

    #[test]
    fn test_combat_wisdom_killed_the() {
        let msg = status_msg("You earned 64 Combat Wisdom: Killed the Aktaari Queen");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::CombatWisdomEarned {
            amount,
            source_name,
            verb,
            zone,
            ..
        } = event
        {
            assert_eq!(amount, 64);
            assert_eq!(source_name.as_deref(), Some("the Aktaari Queen"));
            assert_eq!(verb, "Killed");
            assert_eq!(zone, None);
        } else {
            panic!("Expected CombatWisdomEarned, got {:?}", event);
        }
    }

    #[test]
    fn test_combat_wisdom_defeated() {
        let msg = status_msg("You earned 73 Combat Wisdom: Defeated Elite Tactician");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::CombatWisdomEarned {
            amount,
            source_name,
            verb,
            ..
        } = event
        {
            assert_eq!(amount, 73);
            assert_eq!(source_name.as_deref(), Some("Elite Tactician"));
            assert_eq!(verb, "Defeated");
        } else {
            panic!("Expected CombatWisdomEarned, got {:?}", event);
        }
    }

    #[test]
    fn test_combat_wisdom_with_zone() {
        let msg = status_msg("You earned 5 Combat Wisdom: Killed The Productivity Expert (Gazluk)");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::CombatWisdomEarned {
            amount,
            source_name,
            zone,
            ..
        } = event
        {
            assert_eq!(amount, 5);
            assert_eq!(source_name.as_deref(), Some("The Productivity Expert"));
            assert_eq!(zone.as_deref(), Some("Gazluk"));
        } else {
            panic!("Expected CombatWisdomEarned, got {:?}", event);
        }
    }

    #[test]
    fn test_combat_wisdom_prodigy_level() {
        let msg = status_msg("You earned 1000 Combat Wisdom: Earned a Prodigy Level");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::CombatWisdomEarned {
            amount,
            source_name,
            verb,
            ..
        } = event
        {
            assert_eq!(amount, 1000);
            assert_eq!(source_name, None);
            assert_eq!(verb, "Earned");
        } else {
            panic!("Expected CombatWisdomEarned, got {:?}", event);
        }
    }

    #[test]
    fn test_normal_xp_not_parsed_as_combat_wisdom() {
        // A normal combat XP line must not be swallowed by the wisdom parser.
        let msg = status_msg("You earned 62 XP in Endurance.");
        let event = parse_status_message(&msg).unwrap();
        assert!(
            matches!(event, ChatStatusEvent::XpGained { .. }),
            "Expected XpGained, got {:?}",
            event
        );
    }

    #[test]
    fn test_level_up() {
        let msg = status_msg("You earned 192 XP and reached level 87 in Cartography!");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::LevelUp {
            skill, level, xp, ..
        } = event
        {
            assert_eq!(skill, "Cartography");
            assert_eq!(level, 87);
            assert_eq!(xp, 192);
        } else {
            panic!("Expected LevelUp, got {:?}", event);
        }
    }

    #[test]
    fn test_treasure_distance() {
        // Straight-line ping.
        let msg = status_msg("The treasure is 3215 meters from here.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::TreasureDistance { meters, .. } = event {
            assert_eq!(meters, 3215);
        } else {
            panic!("Expected TreasureDistance, got {:?}", event);
        }
        // Directional fixture (50x-povusmarvelous Chat.log:136):
        // sqrt(384² + 1006²) = 1076.79… → 1077 (nearest meter).
        let msg = status_msg("The treasure is 384m east and 1006m south.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::TreasureDistance { meters, .. } = event {
            assert_eq!(meters, 1077);
        } else {
            panic!("Expected TreasureDistance, got {:?}", event);
        }
        // Directional mineral ping (100x-eltblue Chat.log:346):
        // sqrt(814² + 2328²) = 2466.2… → 2466.
        let msg = status_msg("The Fluorite is 814m east and 2328m north.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::TreasureDistance { meters, .. } = event {
            assert_eq!(meters, 2466);
        } else {
            panic!("Expected TreasureDistance, got {:?}", event);
        }
        // Non-distance status is not a treasure ping.
        assert!(parse_status_message(&status_msg("The treasure is far away.")).is_none());
    }

    #[test]
    fn test_anatomy_success() {
        let msg = status_msg("You bury the corpse.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::AnatomyResult { success, .. } = event {
            assert!(success);
        } else {
            panic!("Expected AnatomyResult, got {:?}", event);
        }
    }

    #[test]
    fn test_anatomy_failure() {
        let msg = status_msg("You botch the autopsy!");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::AnatomyResult { success, .. } = event {
            assert!(!success);
        } else {
            panic!("Expected AnatomyResult, got {:?}", event);
        }
    }

    #[test]
    fn test_summoned() {
        let msg = status_msg("Summoned Nice Phlogiston x5");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::Summoned {
            item_name,
            quantity,
            ..
        } = event
        {
            assert_eq!(item_name, "Nice Phlogiston");
            assert_eq!(quantity, 5);
        } else {
            panic!("Expected Summoned, got {:?}", event);
        }
    }

    #[test]
    fn test_councils_received() {
        let msg = status_msg("You received 500 Councils.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::CouncilsChanged { amount, .. } = event {
            assert_eq!(amount, 500);
        } else {
            panic!("Expected CouncilsChanged, got {:?}", event);
        }
    }

    #[test]
    fn test_councils_used() {
        let msg = status_msg("You used 200 councils.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::CouncilsChanged { amount, .. } = event {
            assert_eq!(amount, -200);
        } else {
            panic!("Expected CouncilsChanged, got {:?}", event);
        }
    }

    #[test]
    fn test_misc_receive_coins() {
        // "You receive N coins." (autoloot / coin-sack open) — a gain, and must
        // NOT be swallowed by "received N Councils" or the corpse-search parser.
        let msg = status_msg("You receive 288 coins.");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::CouncilsChanged { amount, .. } => assert_eq!(amount, 288),
            other => panic!("Expected CouncilsChanged(+288), got {:?}", other),
        }
    }

    #[test]
    fn test_misc_recovered_stolen_councils() {
        let msg = status_msg("You recovered 1,250 Councils stolen by Ratkin Mugger.");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::CouncilsChanged { amount, .. } => assert_eq!(amount, 1250),
            other => panic!("Expected CouncilsChanged(+1250), got {:?}", other),
        }
    }

    #[test]
    fn test_misc_retrieve_stolen_coins() {
        let msg = status_msg("You retrieve 73 stolen coins.");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::CouncilsChanged { amount, .. } => assert_eq!(amount, 73),
            other => panic!("Expected CouncilsChanged(+73), got {:?}", other),
        }
    }

    #[test]
    fn test_misc_given_coins_by_player() {
        let msg = status_msg("You were given 500 coins by Lenia!");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::CouncilsChanged { amount, .. } => assert_eq!(amount, 500),
            other => panic!("Expected CouncilsChanged(+500), got {:?}", other),
        }
    }

    #[test]
    fn test_misc_mugger_stole_councils() {
        // The only visible wallet *loss* in chat — must be negative.
        let msg = status_msg("Ratkin Mugger stole 320 Councils!");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::CouncilsChanged { amount, .. } => assert_eq!(amount, -320),
            other => panic!("Expected CouncilsChanged(-320), got {:?}", other),
        }
    }

    #[test]
    fn test_misc_quoted_stole_is_not_theft() {
        // Quoted player chatter ending in Councils! must not parse as a loss.
        let msg = status_msg("Bob says \"Rat stole 5 Councils!\"");
        assert!(parse_status_message(&msg).is_none());
    }

    #[test]
    fn test_coins_looted() {
        let msg = status_msg("You searched the corpse and found 42 coins.");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::CoinsLooted { amount, .. } = event {
            assert_eq!(amount, 42);
        } else {
            panic!("Expected CoinsLooted, got {:?}", event);
        }
    }

    #[test]
    fn test_item_studied() {
        let msg = status_msg("Moldy Ancient Shoes carefully studied!");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ItemStudied { item_name, .. } = event {
            assert_eq!(item_name, "Moldy Ancient Shoes");
        } else {
            panic!("Expected ItemStudied, got {:?}", event);
        }
    }

    #[test]
    fn test_item_studied_long_name() {
        let msg = status_msg("CrudBurst's Hammer of Thumping of Hammering carefully studied!");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ItemStudied { item_name, .. } = event {
            assert_eq!(item_name, "CrudBurst's Hammer of Thumping of Hammering");
        } else {
            panic!("Expected ItemStudied, got {:?}", event);
        }
    }

    #[test]
    fn test_report_saved_character() {
        let msg = status_msg("Saved report to C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/Character_TestPlayer_Dreva.json");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ReportSaved { file_path, .. } = event {
            assert_eq!(file_path, "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/Character_TestPlayer_Dreva.json");
        } else {
            panic!("Expected ReportSaved, got {:?}", event);
        }
    }

    #[test]
    fn test_report_saved_inventory() {
        let msg = status_msg("Saved report to C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/TestPlayer_Dreva_items_2026-04-20-01-53-45Z.json");
        let event = parse_status_message(&msg).unwrap();
        if let ChatStatusEvent::ReportSaved { file_path, .. } = event {
            assert!(file_path.contains("items_"));
        } else {
            panic!("Expected ReportSaved, got {:?}", event);
        }
    }

    #[test]
    fn test_book_saved_not_report() {
        // "Saved book to ..." should NOT match ReportSaved
        let msg = status_msg("Saved book to C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Books/HelpScreen_260419_185352.txt");
        assert!(parse_status_message(&msg).is_none());
    }

    #[test]
    fn test_roulette_result() {
        let msg = status_msg("Roulette ball ended on 25!");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::RouletteResult { number, .. } => assert_eq!(number, 25),
            other => panic!("Expected RouletteResult(25), got {:?}", other),
        }
    }

    #[test]
    fn test_roulette_result_zero() {
        let msg = status_msg("Roulette ball ended on 0!");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::RouletteResult { number, .. } => assert_eq!(number, 0),
            other => panic!("Expected RouletteResult(0), got {:?}", other),
        }
    }

    #[test]
    fn test_non_status_ignored() {
        let line = "26-03-09 05:01:46\t[Global] Player: hello";
        let msg = parse_chat_line(line).unwrap();
        assert!(parse_status_message(&msg).is_none());
    }

    #[test]
    fn test_unrecognized_status_returns_none() {
        let msg = status_msg("You have 3 friends online.");
        assert!(parse_status_message(&msg).is_none());
    }

    #[test]
    fn test_joined_chat_room_returns_none() {
        let msg = status_msg("Joined chat room \"Trade\". There are 280 other users here.");
        assert!(parse_status_message(&msg).is_none());
    }

    #[test]
    fn test_wisdom_comma_grouped_amount() {
        // Parity with the councils twin (`try_councils_changed` strips commas):
        // a grouped award must parse to the same value.
        let msg = status_msg("You earned 1,250 Combat Wisdom: Killed the Aktaari Queen");
        match parse_status_message(&msg).unwrap() {
            ChatStatusEvent::CombatWisdomEarned { amount, source_name, .. } => {
                assert_eq!(amount, 1250);
                assert_eq!(source_name.as_deref(), Some("the Aktaari Queen"));
            }
            other => panic!("Expected CombatWisdomEarned(1250), got {:?}", other),
        }
    }

    #[test]
    fn test_empty_skill_arms_reject() {
        // ItemGained / Xp / Prodigy / Summoned guards landed earlier; LevelUp
        // is the last of the family to grow one. All must reject blank names.
        assert!(parse_status_message(&status_msg(" added to inventory.")).is_none());
        assert!(parse_status_message(&status_msg("You earned 62 XP in .")).is_none());
        assert!(parse_status_message(&status_msg("You earned 45 Prodigy XP in .")).is_none());
        assert!(parse_status_message(&status_msg("Summoned ")).is_none());
        assert!(parse_status_message(&status_msg("You earned 5 XP and reached level 3 in !")).is_none());
    }
}
