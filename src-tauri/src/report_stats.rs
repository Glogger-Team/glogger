//! Parse structured stats from PlayerAge and Behavior Report (HelpScreen) books.
//!
//! Both reports use simple HTML-like markup. This module extracts key-value
//! stats and persists them to the `character_report_stats` table.

use rusqlite::Connection;

/// A single parsed stat.
pub struct ParsedStat {
    pub category: String,
    pub name: String,
    pub value: String,
}

/// Parse the PlayerAge report content.
///
/// Format:
/// ```text
/// Zenith was created on <b>Sat Jan 31 09:40:49 EST 2026</b>.
/// You have spent <b>40 days 13 hours</b> logged into the game.
/// You have died <b>829</b> times.
/// You have performed <b>196,208</b> attacks.
/// You have killed <b>61,453</b> monsters.
/// You have dealt <b>134,065,964</b> damage.
/// You have taken <b>5,933,874</b> damage.
/// You have used <b>71 days</b> of VIP from a Steam subscription.
/// ```
pub fn parse_player_age(content: &str) -> Vec<ParsedStat> {
    let mut stats = Vec::new();
    let cat = "age".to_string();

    // Log content from ProcessBook uses literal \n and \t escape sequences
    let normalized = content.replace("\\n", "\n").replace("\\t", "\t");
    for line in normalized.lines() {
        let line = line.trim();

        // Extract bold value: text <b>VALUE</b> text
        let Some(value) = extract_bold(line) else {
            continue;
        };

        if line.contains("was created on") {
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "created_on".into(),
                value,
            });
        } else if line.contains("logged into the game") {
            // Duration string like "40 days 13 hours" — keep as-is
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "time_played".into(),
                value,
            });
        } else if line.contains("died") && line.contains("times") {
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "deaths".into(),
                value: value.replace(',', ""),
            });
        } else if line.contains("attacks") {
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "attacks_made".into(),
                value: value.replace(',', ""),
            });
        } else if line.contains("killed") && line.contains("monsters") {
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "kills".into(),
                value: value.replace(',', ""),
            });
        } else if line.contains("dealt") && line.contains("damage") {
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "damage_dealt".into(),
                value: value.replace(',', ""),
            });
        } else if line.contains("taken") && line.contains("damage") {
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "damage_taken".into(),
                value: value.replace(',', ""),
            });
        } else if line.contains("VIP") {
            // Duration string like "71 days" — keep as-is
            stats.push(ParsedStat {
                category: cat.clone(),
                name: "vip_days".into(),
                value,
            });
        }
    }

    stats
}

/// Parse the Behavior Report (HelpScreen) content.
///
/// Format uses sections with `<color>` headers and `<i>` values:
/// ```text
/// <b><color=#77ff00>Challenge Restrictions</color></b>
/// \tBought Stock Items?  <i>YES (1,315)</i>
/// ...
/// <b><color=#77ff00>Killing Stats</color></b>
/// \tKilled Foes (Any)  <i>61,453</i>
/// ```
pub fn parse_behavior_report(content: &str) -> Vec<ParsedStat> {
    let mut stats = Vec::new();
    let mut current_section = String::new();

    // Log content from ProcessBook uses literal \n and \t escape sequences
    let normalized = content.replace("\\n", "\n").replace("\\t", "\t");
    for line in normalized.lines() {
        let line = line.trim();

        // Detect section headers: <b><color=#77ff00>Section Name</color></b>
        if line.contains("<color=") {
            if let Some(section) = extract_color_header(line) {
                current_section = categorize_section(&section);
            }
            continue;
        }

        // Skip non-data lines
        if current_section.is_empty() || line.is_empty() || line.starts_with('*') {
            // Extract creation date from the first line (no section)
            if line.contains("was created on") {
                // "Zenith was created on Sat Jan 31 09:40:49 EST 2026"
                if let Some(pos) = line.find("was created on ") {
                    let date = &line[pos + "was created on ".len()..];
                    stats.push(ParsedStat {
                        category: "age".into(),
                        name: "created_on".into(),
                        value: date.to_string(),
                    });
                }
            }
            continue;
        }

        // Parse stat lines: "Label  <i>VALUE</i>" or "Badge Name  <i></i>"
        if let Some((label, value)) = extract_italic_stat(line) {
            if current_section == "badges" {
                // Badges are just names (value is empty)
                stats.push(ParsedStat {
                    category: "badges".into(),
                    name: label,
                    value: "true".into(),
                });
            } else {
                // Numeric or YES/NO stats — clean the value
                let clean = clean_stat_value(&value);
                if !clean.is_empty() {
                    stats.push(ParsedStat {
                        category: current_section.clone(),
                        name: normalize_stat_name(&label),
                        value: clean,
                    });
                }
            }
        }
    }

    stats
}

/// Persist parsed stats to the character_report_stats table. Forward-only:
/// an older capture replayed after a newer import is a no-op, and an
/// identical re-import writes nothing (no stamp churn, so the caller can stay
/// silent like every other arm). An equal capture with different content
/// still replaces — same retry-eligibility rule as `import_garden_almanac` —
/// so a retry of the current observation heals a failed import. Stamps are
/// RFC3339 (the shape the wall-clock caller wrote); text comparison orders
/// same-shape stamps chronologically.
pub fn persist_stats(
    conn: &Connection,
    character: &str,
    server: &str,
    stats: &[ParsedStat],
    timestamp: &str,
) -> Result<usize, String> {
    if stats.is_empty() {
        return Ok(0);
    }
    // Identical re-import (rescan) is a pure no-op: values already match, so
    // there is nothing to write and the stamp must not advance.
    let current: Vec<(String, String, String)> = conn
        .prepare(
            "SELECT category, stat_name, stat_value FROM character_report_stats
             WHERE character_name = ?1 AND server_name = ?2",
        )
        .map_err(|e| format!("Prepare error: {e}"))?
        .query_map(rusqlite::params![character, server], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| format!("Query error: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("Row error: {e}"))?;
    let mut fresh: Vec<(String, String, String)> = stats
        .iter()
        .map(|s| (s.category.clone(), s.name.clone(), s.value.clone()))
        .collect();
    let mut current_sorted = current;
    current_sorted.sort();
    fresh.sort();
    if current_sorted == fresh {
        return Ok(0);
    }
    // Recency: a rescan can redeliver an OLDER book after a newer import.
    // Captures compare lexicographically (RFC3339 both sides). Equal stays
    // eligible, so a retry of the current observation still replaces.
    let stored_newest: Option<String> = conn
        .query_row(
            "SELECT MAX(updated_at) FROM character_report_stats
             WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character, server],
            |r| r.get(0),
        )
        .map_err(|e| format!("Query error: {e}"))?;
    if stored_newest.as_deref().is_some_and(|s| timestamp < s) {
        return Ok(0);
    }
    let mut stmt = conn
        .prepare(
            "INSERT INTO character_report_stats (character_name, server_name, category, stat_name, stat_value, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(character_name, server_name, category, stat_name) DO UPDATE SET
                stat_value = excluded.stat_value,
                updated_at = excluded.updated_at
             WHERE updated_at <= excluded.updated_at",
        )
        .map_err(|e| format!("Prepare error: {e}"))?;

    let mut changed = 0;
    for stat in stats {
        changed += stmt
            .execute(rusqlite::params![
                character,
                server,
                stat.category,
                stat.name,
                stat.value,
                timestamp,
            ])
            .map_err(|e| format!("Insert error: {e}"))?;
    }

    Ok(changed)
}

// ── Helpers ─────────────────────────────────────────────────────

/// Extract text between `<b>` and `</b>` tags.
fn extract_bold(line: &str) -> Option<String> {
    let open = line.find("<b>")?;
    let start = open + 3;
    // Search for the close tag *after* the open tag: a `</b>` that precedes
    // `<b>` (stray/mismatched markup) must not produce a backwards slice.
    let rel = line[start..].find("</b>")?;
    Some(line[start..start + rel].to_string())
}

/// Extract section name from `<b><color=#hex>Name</color></b>` or similar.
fn extract_color_header(line: &str) -> Option<String> {
    // Find the closing `>` of the `<color=...>` tag
    let color_start = line.find("<color")?;
    let tag_end = line[color_start..].find('>')? + color_start + 1;
    let rest = &line[tag_end..];
    // Content runs until `</color>`
    let end = rest
        .find("</color>")
        .unwrap_or_else(|| rest.find('<').unwrap_or(rest.len()));
    let name = rest[..end].trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Map section header text to a category key.
fn categorize_section(header: &str) -> String {
    let lower = header.to_lowercase();
    if lower.contains("challenge") {
        "challenges".into()
    } else if lower.contains("badge") {
        "badges".into()
    } else if lower.contains("food") {
        "food_stats".into()
    } else if lower.contains("killing") {
        "killing_stats".into()
    } else if lower.contains("misc") {
        "misc_stats".into()
    } else {
        lower.replace(' ', "_")
    }
}

/// Extract label and italic value from a line like `\tLabel  <i>VALUE</i>`.
fn extract_italic_stat(line: &str) -> Option<(String, String)> {
    let i_start = line.find("<i>")?;
    let i_end = line[i_start..].find("</i>").map(|p| i_start + p)?;
    let value = line[i_start + 3..i_end].to_string();
    let label = line[..i_start]
        .trim()
        .trim_end_matches('?')
        .trim()
        .to_string();
    if label.is_empty() {
        return None;
    }
    Some((label, value))
}

/// Clean a stat value: strip "YES " prefix, remove commas from numbers.
fn clean_stat_value(value: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        return String::new();
    }
    // "YES (1,315)" → "1315"
    if let Some(rest) = v.strip_prefix("YES (") {
        if let Some(num) = rest.strip_suffix(')') {
            return num.replace(',', "");
        }
        // Just "YES" with no count
        return "yes".into();
    }
    if v == "YES" {
        return "yes".into();
    }
    if v == "NO" {
        return "no".into();
    }
    // Plain numbers: "61,453" → "61453"
    v.replace(',', "")
}

/// Normalize a stat label to a snake_case key.
fn normalize_stat_name(label: &str) -> String {
    label
        .to_lowercase()
        .replace([' ', '-'], "_")
        .replace(['\'', '(', ')'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_player_age() {
        let content = r#"Zenith was created on <b>Sat Jan 31 09:40:49 EST 2026</b>.
You have spent <b>40 days 13 hours</b> logged into the game.
You have died <b>829</b> times.
You have performed <b>196,208</b> attacks.
You have killed <b>61,453</b> monsters.
You have dealt <b>134,065,964</b> damage.
You have taken <b>5,933,874</b> damage.
You have used <b>71 days</b> of VIP from a Steam subscription.
"#;
        let stats = parse_player_age(content);
        assert_eq!(stats.len(), 8);

        let by_name: std::collections::HashMap<&str, &str> = stats
            .iter()
            .map(|s| (s.name.as_str(), s.value.as_str()))
            .collect();
        assert_eq!(by_name["created_on"], "Sat Jan 31 09:40:49 EST 2026");
        assert_eq!(by_name["time_played"], "40 days 13 hours");
        assert_eq!(by_name["deaths"], "829");
        assert_eq!(by_name["attacks_made"], "196208");
        assert_eq!(by_name["kills"], "61453");
        assert_eq!(by_name["damage_dealt"], "134065964");
        assert_eq!(by_name["damage_taken"], "5933874");
        assert_eq!(by_name["vip_days"], "71 days");
    }

    #[test]
    fn test_parse_behavior_report_challenges() {
        let content = r#"Zenith was created on Sat Jan 31 09:40:49 EST 2026

<b><color=#77ff00>Challenge Restrictions (for self-imposed hardcore challenges)</color></b>
	Bought Stock Items?  <i>YES (1,315)</i>
	Died (In An Avoidable Way)?  <i>YES (822)</i>

<b><color=#77ff00>Current Behavior Badges</color></b>
	Slaughterer of Countless Foes  <i></i>
	Slaughterer of Goblins  <i></i>

<b><color=#77ff00>Killing Stats</color></b>
	Killed Foes (Any)  <i>61,453</i>
	Killed Animals  <i>9,854</i>
	Attacks Made  <i>196,208</i>

<b><color=#77ff00>Misc Stats</color></b>
	Time Spent Online  <i>40 days 13 hours</i>
	Logged Into Game  <i>4,769</i>
"#;
        let stats = parse_behavior_report(content);

        let challenges: Vec<&ParsedStat> = stats
            .iter()
            .filter(|s| s.category == "challenges")
            .collect();
        assert!(challenges.len() >= 2);
        assert!(challenges
            .iter()
            .any(|s| s.name == "bought_stock_items" && s.value == "1315"));
        assert!(challenges
            .iter()
            .any(|s| s.name == "died_in_an_avoidable_way" && s.value == "822"));

        let badges: Vec<&ParsedStat> = stats.iter().filter(|s| s.category == "badges").collect();
        assert_eq!(badges.len(), 2);

        let kills: Vec<&ParsedStat> = stats
            .iter()
            .filter(|s| s.category == "killing_stats")
            .collect();
        assert!(kills
            .iter()
            .any(|s| s.name == "killed_foes_any" && s.value == "61453"));
        assert!(kills
            .iter()
            .any(|s| s.name == "attacks_made" && s.value == "196208"));

        let misc: Vec<&ParsedStat> = stats
            .iter()
            .filter(|s| s.category == "misc_stats")
            .collect();
        assert!(misc
            .iter()
            .any(|s| s.name == "time_spent_online" && s.value == "40 days 13 hours"));
        assert!(misc
            .iter()
            .any(|s| s.name == "logged_into_game" && s.value == "4769"));

        // Creation date from first line
        let age: Vec<&ParsedStat> = stats.iter().filter(|s| s.category == "age").collect();
        assert!(age
            .iter()
            .any(|s| s.name == "created_on" && s.value == "Sat Jan 31 09:40:49 EST 2026"));
    }

    #[test]
    fn test_parse_player_age_literal_newlines() {
        // This is how content actually arrives from ProcessBook — literal \n escapes, not real newlines
        let content = r#"Zenith was created on <b>Sat Jan 31 09:40:49 EST 2026</b>.\nYou have spent <b>40 days 13 hours</b> logged into the game.\nYou have died <b>829</b> times.\nYou have performed <b>196,208</b> attacks.\nYou have killed <b>61,453</b> monsters.\nYou have dealt <b>134,065,964</b> damage.\nYou have taken <b>5,933,874</b> damage.\nYou have used <b>71 days</b> of VIP from a Steam subscription."#;
        let stats = parse_player_age(content);
        assert_eq!(
            stats.len(),
            8,
            "Expected 8 stats, got: {:?}",
            stats.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        let by_name: std::collections::HashMap<&str, &str> = stats
            .iter()
            .map(|s| (s.name.as_str(), s.value.as_str()))
            .collect();
        assert_eq!(by_name["created_on"], "Sat Jan 31 09:40:49 EST 2026");
        assert_eq!(by_name["time_played"], "40 days 13 hours");
        assert_eq!(by_name["deaths"], "829");
        assert_eq!(by_name["attacks_made"], "196208");
        assert_eq!(by_name["kills"], "61453");
        assert_eq!(by_name["damage_dealt"], "134065964");
        assert_eq!(by_name["damage_taken"], "5933874");
        assert_eq!(by_name["vip_days"], "71 days");
    }

    #[test]
    fn test_clean_stat_value() {
        assert_eq!(clean_stat_value("YES (1,315)"), "1315");
        assert_eq!(clean_stat_value("YES"), "yes");
        assert_eq!(clean_stat_value("61,453"), "61453");
        assert_eq!(clean_stat_value("40 days 13 hours"), "40 days 13 hours");
        assert_eq!(clean_stat_value(""), "");
    }
    #[test]
    fn test_extract_italic_stat_rejects_stray_close_tag() {
        // A `</i>` before any `<i>` (mismatched book markup) must not panic
        // on a reversed slice range: the close tag is searched from the open
        // tag onward.
        assert_eq!(
            extract_italic_stat("Stray </i> text <i>YES</i>").map(|(_, v)| v),
            Some("YES".to_string())
        );
        assert_eq!(
            extract_italic_stat("Died?  <i>YES (822)</i>").map(|(_, v)| v),
            Some("YES (822)".to_string())
        );
    }

    // ── recency (derived-state hardening) ─────────────────────────

    fn stat(category: &str, name: &str, value: &str) -> ParsedStat {
        ParsedStat {
            category: category.to_string(),
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    fn setup_stats() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        conn
    }

    fn read_stat(conn: &Connection) -> (String, String) {
        conn.query_row(
            "SELECT stat_value, updated_at FROM character_report_stats
             WHERE character_name = 'T' AND server_name = 'Dreva'
               AND category = 'age' AND stat_name = 'deaths'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    }

    #[test]
    fn persist_stats_older_capture_after_newer_is_noop() {
        let conn = setup_stats();
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "900")],
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 1);
        // An older book replayed after a newer import must not move the
        // value backward: no-op, count and stamp kept.
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "800")],
            "2026-11-18T12:00:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 0);
        assert_eq!(
            read_stat(&conn),
            ("900".to_string(), "2026-11-18T12:10:00+00:00".to_string())
        );
    }

    #[test]
    fn persist_stats_identical_reimport_does_not_advance_stamp() {
        let conn = setup_stats();
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "900")],
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 1);
        // Identical values re-observed later: no write, stamp stays at the
        // first observation instead of advancing.
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "900")],
            "2026-11-18T12:20:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 0);
        assert_eq!(
            read_stat(&conn),
            ("900".to_string(), "2026-11-18T12:10:00+00:00".to_string())
        );
    }

    #[test]
    fn persist_stats_equal_capture_retry_stays_eligible() {
        // Same garden-parity rule: an equal capture with different content
        // still replaces, so a retry of the current observation heals a
        // failed import instead of wedging behind its own stamp.
        let conn = setup_stats();
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "900")],
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 1);
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "901")],
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(read_stat(&conn).0, "901".to_string());
    }

    #[test]
    fn persist_stats_newer_after_older_applies() {
        // Twin arrival order, forward direction: an older book first, then
        // the newer book — the newer values must land with the newer stamp.
        let conn = setup_stats();
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "800")],
            "2026-11-18T12:00:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 1);
        let n = persist_stats(
            &conn,
            "T",
            "Dreva",
            &[stat("age", "deaths", "900")],
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            read_stat(&conn),
            ("900".to_string(), "2026-11-18T12:10:00+00:00".to_string())
        );
    }
}
