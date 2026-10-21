/// Chat log parser for Project Gorgon
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Session a chat line was written under: the login line that opened it.
///
/// Chat logs are daily files (`Chat-YY-MM-DD.log`) that span multiple play
/// sessions; each `Logged In As <Name>. Server <Server>.` line starts one.
/// Tagging keeps catch-up gains attributable to the session that produced
/// them instead of the drain-final identity (see `parse_chat_lines_tagged`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSessionTag {
    pub character_name: String,
    pub server_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub timestamp: NaiveDateTime,
    pub channel: Option<String>,
    pub sender: Option<String>,
    pub message: String,
    pub is_system: bool,
    pub from_player: Option<bool>, // For Tell messages: true if sent by player, false if received
    pub item_links: Vec<ItemLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemLink {
    pub raw_text: String,  // e.g., "[Item: Mentalism: System Shock 7]"
    pub item_name: String, // e.g., "Mentalism: System Shock 7" (full name as it appears in game data)
}

#[derive(Debug)]
pub struct ChatLogFile {
    pub file_path: PathBuf,
    pub file_name: String,
    pub file_date: NaiveDate,
}

/// Check if a line starts with a timestamp pattern (YY-MM-DD HH:MM:SS\t)
pub fn is_timestamped_line(line: &str) -> bool {
    // Must be at least "YY-MM-DD HH:MM:SS\t" = 18 chars + tab
    if line.len() < 19 {
        return false;
    }
    // Quick check: digits-digits-digits space digits:digits:digits tab
    let bytes = line.as_bytes();
    bytes[2] == b'-'
        && bytes[5] == b'-'
        && bytes[8] == b' '
        && bytes[11] == b':'
        && bytes[14] == b':'
        && bytes.len() > 17
        && bytes[17] == b'\t'
}

/// Parse a single chat log line into a ChatMessage.
///
/// All channels are parsed — filtering for display/persistence is handled downstream.
pub fn parse_chat_line(line: &str) -> Option<ChatMessage> {
    if line.trim().is_empty() {
        return None;
    }

    // Split on tab
    let parts: Vec<&str> = line.splitn(2, '\t').collect();
    if parts.len() != 2 {
        return None;
    }

    let timestamp_str = parts[0];
    let content = parts[1];

    // Parse timestamp: YY-MM-DD HH:MM:SS
    let timestamp = parse_timestamp(timestamp_str)?;

    // Check if this is a channeled message [Channel] ...
    if content.starts_with('[') {
        let channel_end = content.find(']')?;
        let channel = content[1..channel_end].to_string();

        let remaining = content[channel_end + 1..].trim();

        // Special handling for Tell messages: [Tell] Sender->Recipient: Message
        if channel == "Tell" {
            if let Some(arrow_pos) = remaining.find("->") {
                // Find the colon AFTER the arrow to separate recipient from message
                let after_arrow = &remaining[arrow_pos + 2..];
                if let Some(colon_offset) = after_arrow.find(':') {
                    let sender = remaining[..arrow_pos].trim().to_string();
                    let recipient = after_arrow[..colon_offset].trim().to_string();
                    let message_text = after_arrow[colon_offset + 1..].trim().to_string();

                    let (actual_sender, is_outgoing) = if sender == "You" {
                        (recipient.clone(), true)
                    } else {
                        (sender.clone(), false)
                    };

                    let item_links = extract_item_links(&message_text);
                    Some(ChatMessage {
                        timestamp,
                        channel: Some(channel),
                        sender: Some(actual_sender),
                        message: message_text,
                        is_system: false,
                        from_player: Some(is_outgoing),
                        item_links,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else if let Some(colon_pos) = remaining.find(':') {
            let potential_sender = &remaining[..colon_pos];

            // System messages in channels don't have senders
            // Status channel is always system messages (never player chat)
            if channel == "Status"
                || potential_sender.starts_with('-')
                || potential_sender.starts_with("You ")
                || potential_sender.contains('#')
            {
                let msg_text = remaining.to_string();
                let item_links = extract_item_links(&msg_text);
                Some(ChatMessage {
                    timestamp,
                    channel: Some(channel),
                    sender: None,
                    message: msg_text,
                    is_system: true,
                    from_player: None,
                    item_links,
                })
            } else {
                // Player message with sender
                let sender = potential_sender.trim().to_string();
                let message = remaining[colon_pos + 1..].trim().to_string();
                let item_links = extract_item_links(&message);

                Some(ChatMessage {
                    timestamp,
                    channel: Some(channel),
                    sender: Some(sender),
                    message,
                    is_system: false,
                    from_player: None,
                    item_links,
                })
            }
        } else {
            // Channel message without colon (system message)
            let msg_text = remaining.to_string();
            let item_links = extract_item_links(&msg_text);
            Some(ChatMessage {
                timestamp,
                channel: Some(channel),
                sender: None,
                message: msg_text,
                is_system: true,
                from_player: None,
                item_links,
            })
        }
    } else {
        // Non-channeled message (system/area transition)
        let msg_text = content.to_string();
        let item_links = extract_item_links(&msg_text);
        Some(ChatMessage {
            timestamp,
            channel: None,
            sender: None,
            message: msg_text,
            is_system: true,
            from_player: None,
            item_links,
        })
    }
}

/// Parse a block of chat log text with multiline message support.
///
/// Lines that start with a timestamp (`YY-MM-DD HH:MM:SS\t`) begin a new message.
/// Lines without that prefix are continuation lines — their content is appended
/// to the previous message's text, and item links are re-extracted.
pub fn parse_chat_lines(text: &str) -> Vec<ChatMessage> {
    let mut messages: Vec<ChatMessage> = Vec::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if is_timestamped_line(line) {
            // New timestamped line — parse as a new message
            if let Some(msg) = parse_chat_line(line) {
                messages.push(msg);
            }
        } else {
            // Continuation line — append to previous message
            if let Some(last_msg) = messages.last_mut() {
                last_msg.message.push('\n');
                last_msg.message.push_str(line.trim());
                // Re-extract item links from the updated message
                last_msg.item_links = extract_item_links(&last_msg.message);
            }
            // If no previous message, skip the orphaned continuation line
        }
    }

    messages
}

/// Parse a block of chat log text, tagging each message with the login
/// session that produced it.
///
/// Same message stream as [`parse_chat_lines`] (login lines stay ordinary
/// untagged system messages — [`parse_chat_line`] has no channel for them),
/// plus one tag per message: the `Logged In As <Name>. Server <Server>.`
/// session in force when the line was read. `Logged Out` lines are messages
/// too; they close the session, so a later resume without a fresh login
/// yields `None` tags (caller falls back to the drain identity) rather than
/// the previous session's tag.
pub fn parse_chat_lines_tagged(text: &str) -> Vec<(ChatMessage, Option<ChatSessionTag>)> {
    let mut tagged: Vec<(ChatMessage, Option<ChatSessionTag>)> = Vec::new();
    let mut session: Option<ChatSessionTag> = None;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Login lines open the session for the lines that follow. Gated on
        // the strict login predicate (same as the watcher's marker), so a
        // player-chat line that merely mentions logging in falls through to
        // ordinary message parsing. They are still parsed as messages (same
        // stream as `parse_chat_lines`); the login line itself keeps the
        // *previous* session's tag.
        if let Some(info) = parse_chat_login_line(line) {
            if let Some(msg) = parse_chat_line(line) {
                tagged.push((msg, session.clone()));
            }
            session = Some(ChatSessionTag {
                character_name: info.character_name,
                server_name: info.server_name,
            });
            continue;
        }
        // Same strictness for logout (watcher matches `"******** Logged Out"`,
        // the game's own marker): a chat line mentioning logging out must not
        // close the session or be swallowed.
        if line.contains("******** Logged Out") {
            if let Some(msg) = parse_chat_line(line) {
                tagged.push((msg, session.clone()));
            }
            session = None;
            continue;
        }

        if is_timestamped_line(line) {
            if let Some(msg) = parse_chat_line(line) {
                tagged.push((msg, session.clone()));
            }
        } else if let Some((last_msg, _)) = tagged.last_mut() {
            // Continuation line — append to previous message, keep its tag.
            last_msg.message.push('\n');
            last_msg.message.push_str(line.trim());
            last_msg.item_links = extract_item_links(&last_msg.message);
        }
        // Orphaned continuation line: skipped, same as `parse_chat_lines`.
    }

    tagged
}

/// Extract item links from a chat message
/// Format: [Item: SkillName: ItemName] or [Item: ItemName]
/// Examples:
///   - [Item: Leatherworking: Great Evasion Shirt]
///   - [Item: Amazing Cloth Shirt]
pub fn extract_item_links(message: &str) -> Vec<ItemLink> {
    let mut links = Vec::new();

    // Find all [Item: ...] patterns
    let mut start = 0;
    while let Some(item_start) = message[start..].find("[Item:") {
        let abs_start = start + item_start;
        if let Some(end_pos) = message[abs_start..].find(']') {
            let abs_end = abs_start + end_pos;
            let link_text = &message[abs_start..=abs_end];

            // Extract content between [Item: and ] — keep the full name as-is
            let item_name = link_text[6..link_text.len() - 1].trim().to_string();

            if item_name.is_empty() {
                start = abs_end + 1;
                continue;
            }

            links.push(ItemLink {
                raw_text: link_text.to_string(),
                item_name,
            });

            start = abs_end + 1;
        } else {
            break;
        }
    }

    links
}

/// Parse timestamp in format YY-MM-DD HH:MM:SS
fn parse_timestamp(s: &str) -> Option<NaiveDateTime> {
    let parts: Vec<&str> = s.split(' ').collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }

    let year = format!("20{}", date_parts[0]).parse::<i32>().ok()?;
    let month = date_parts[1].parse::<u32>().ok()?;
    let day = date_parts[2].parse::<u32>().ok()?;

    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }

    let hour = time_parts[0].parse::<u32>().ok()?;
    let minute = time_parts[1].parse::<u32>().ok()?;
    let second = time_parts[2].parse::<u32>().ok()?;

    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::from_hms_opt(hour, minute, second)?;

    Some(NaiveDateTime::new(date, time))
}

/// Get all chat log files from the ChatLogs directory
pub fn get_chat_log_files(chat_logs_dir: &Path) -> std::io::Result<Vec<ChatLogFile>> {
    let mut files = Vec::new();

    if !chat_logs_dir.exists() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(chat_logs_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("Chat-") && file_name.ends_with(".log") {
                    if let Some(date) = parse_chat_log_filename(file_name) {
                        files.push(ChatLogFile {
                            file_path: path.clone(),
                            file_name: file_name.to_string(),
                            file_date: date,
                        });
                    }
                }
            }
        }
    }

    // Sort by date, newest first
    files.sort_by(|a, b| b.file_date.cmp(&a.file_date));

    Ok(files)
}

/// Parse chat log filename to extract date
/// Format: Chat-YY-MM-DD.log
pub fn parse_chat_log_filename(filename: &str) -> Option<NaiveDate> {
    let date_str = filename.strip_prefix("Chat-")?.strip_suffix(".log")?;

    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }

    let year = format!("20{}", parts[0]).parse::<i32>().ok()?;
    let month = parts[1].parse::<u32>().ok()?;
    let day = parts[2].parse::<u32>().ok()?;

    NaiveDate::from_ymd_opt(year, month, day)
}

/// Result of parsing a chat log login line
#[derive(Debug, Clone)]
pub struct ChatLoginInfo {
    pub character_name: String,
    pub server_name: String,
    /// Timezone offset in seconds from UTC (e.g., -25200 for -07:00:00)
    pub timezone_offset_seconds: Option<i32>,
}

/// Parse a chat log login line to extract character name and server name.
/// Format: "**************************************** Logged In As PlayerName. Server Dreva. Timezone Offset -07:00:00."
pub fn parse_chat_login_line(line: &str) -> Option<ChatLoginInfo> {
    if !line.contains("Logged In As") {
        return None;
    }

    let name_start = line.find("As ")? + 3;
    let after_name = &line[name_start..];
    let name_end = after_name.find(". Server")?;
    let character_name = after_name[..name_end].trim().to_string();

    let server_start = name_start + name_end + ". Server ".len();
    let after_server = &line[server_start..];
    let server_end = after_server.find('.')?;
    let server_name = after_server[..server_end].trim().to_string();

    if character_name.is_empty() || server_name.is_empty() {
        return None;
    }

    // Parse timezone offset: "Timezone Offset -07:00:00" or "Timezone Offset 05:30:00"
    let timezone_offset_seconds = parse_timezone_offset(line);

    Some(ChatLoginInfo {
        character_name,
        server_name,
        timezone_offset_seconds,
    })
}

/// Parse a timezone offset string like "-07:00:00" or "05:30:00" into total seconds from UTC.
fn parse_timezone_offset(line: &str) -> Option<i32> {
    let marker = "Timezone Offset ";
    let offset_start = line.find(marker)? + marker.len();
    let after_marker = &line[offset_start..];
    // Offset ends at the next period
    let offset_end = after_marker.find('.')?;
    let offset_str = after_marker[..offset_end].trim();

    // Determine sign
    let (sign, time_part) = if offset_str.starts_with('-') {
        (-1, &offset_str[1..])
    } else if offset_str.starts_with('+') {
        (1, &offset_str[1..])
    } else {
        (1, offset_str)
    };

    let parts: Vec<&str> = time_part.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: i32 = parts[0].parse().ok()?;
    let minutes: i32 = parts[1].parse().ok()?;
    let seconds: i32 = parts[2].parse().ok()?;

    Some(sign * (hours * 3600 + minutes * 60 + seconds))
}

/// Extract player name from the first line of a chat log
/// Format: "**************************************** Logged In As PlayerName. Server Dreva. ..."
pub fn extract_player_name(file_path: &Path) -> std::io::Result<Option<String>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if let Some(info) = parse_chat_login_line(&line) {
            return Ok(Some(info.character_name));
        }
    }

    Ok(None)
}

/// Read and parse chat messages from a file, starting from a byte offset.
///
/// Uses `parse_chat_lines` for multiline support. Returns parsed messages
/// and the new file position (file size after read).
pub fn read_chat_log(
    file_path: &Path,
    start_position: u64,
) -> std::io::Result<(Vec<ChatMessage>, u64)> {
    use std::io::Read;

    let mut file = File::open(file_path)?;
    let file_size = file.metadata()?.len();

    if start_position >= file_size {
        return Ok((Vec::new(), start_position));
    }

    file.seek(SeekFrom::Start(start_position))?;

    let mut content = Vec::new();
    file.read_to_end(&mut content)?;

    let content_str = String::from_utf8_lossy(&content);
    let messages = parse_chat_lines(&content_str);

    Ok((messages, file_size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_global_chat() {
        let line = "26-03-09 05:01:46\t[Global] Gunbsnark: one for party for shezak?";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.channel, Some("Global".to_string()));
        assert_eq!(msg.sender, Some("Gunbsnark".to_string()));
        assert_eq!(msg.message, "one for party for shezak?");
        assert!(!msg.is_system);
    }

    #[test]
    fn test_parse_status_message() {
        let line = "26-03-09 05:00:17\t[Status] You have 4 friends online.";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.channel, Some("Status".to_string()));
        assert_eq!(msg.sender, None);
        assert_eq!(msg.message, "You have 4 friends online.");
        assert!(msg.is_system);
    }

    #[test]
    fn test_parse_system_message() {
        let line = "26-03-09 05:00:14\t**************************************** Logged In As Zenith. Server Dreva. Timezone Offset -07:00:00.";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.channel, None);
        assert_eq!(msg.sender, None);
        assert!(msg.is_system);
    }

    #[test]
    fn test_parse_chat_login_line() {
        let line = "26-03-22 18:12:49\t**************************************** Logged In As Zenith. Server Dreva. Timezone Offset -07:00:00.";
        let info = parse_chat_login_line(line).unwrap();
        assert_eq!(info.character_name, "Zenith");
        assert_eq!(info.server_name, "Dreva");
        assert_eq!(info.timezone_offset_seconds, Some(-25200)); // -7 * 3600
    }

    #[test]
    fn test_parse_chat_login_line_no_match() {
        assert!(parse_chat_login_line("just a regular line").is_none());
        assert!(parse_chat_login_line("[Global] Player: hello").is_none());
    }

    #[test]
    fn test_parse_area_change() {
        let line = "26-03-09 05:00:14\t******************** Entering Area: Casino";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.channel, None);
        assert!(msg.is_system);
        assert!(msg.message.contains("Casino"));
    }

    #[test]
    fn test_parse_all_channels() {
        // All channels are now parsed — no exclusion at parse level
        let error_line = "26-03-09 05:01:46\t[Error] Something went wrong";
        assert!(parse_chat_line(error_line).is_some());

        let combat_line = "26-03-09 05:04:55\t[Combat] CaseyPG #4392505: Recovered: 56 power";
        let msg = parse_chat_line(combat_line).unwrap();
        assert_eq!(msg.channel, Some("Combat".to_string()));

        let status_line = "26-03-09 05:00:17\t[Status] You earned 50 XP in Mining.";
        let msg = parse_chat_line(status_line).unwrap();
        assert_eq!(msg.channel, Some("Status".to_string()));

        let trade_line = "26-03-09 05:01:46\t[Trade] PlayerName: WTS items";
        assert!(parse_chat_line(trade_line).is_some());
    }

    #[test]
    fn test_parse_filename() {
        let date = parse_chat_log_filename("Chat-26-03-09.log").unwrap();
        assert_eq!(date.year(), 2026);
        assert_eq!(date.month(), 3);
        assert_eq!(date.day(), 9);
    }

    #[test]
    fn test_extract_item_links_with_skill() {
        let message = "Check out my new [Item: Leatherworking: Great Evasion Shirt]!";
        let links = extract_item_links(message);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].item_name, "Leatherworking: Great Evasion Shirt");
        assert_eq!(
            links[0].raw_text,
            "[Item: Leatherworking: Great Evasion Shirt]"
        );
    }

    #[test]
    fn test_extract_item_links_without_skill() {
        let message = "I found [Item: Amazing Cloth Shirt] in a chest!";
        let links = extract_item_links(message);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].item_name, "Amazing Cloth Shirt");
        assert_eq!(links[0].raw_text, "[Item: Amazing Cloth Shirt]");
    }

    #[test]
    fn test_extract_multiple_item_links() {
        let message = "Trading [Item: Sword] for [Item: Blacksmithing: Iron Hammer]";
        let links = extract_item_links(message);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].item_name, "Sword");
        assert_eq!(links[1].item_name, "Blacksmithing: Iron Hammer");
    }

    #[test]
    fn test_extract_item_links_none() {
        let message = "Just a regular chat message";
        let links = extract_item_links(message);

        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_parse_chat_with_item_links() {
        let line = "26-03-09 05:01:46\t[Trade] PlayerName: Selling [Item: Leatherworking: Great Evasion Shirt] for 1000 councils";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.channel, Some("Trade".to_string()));
        assert_eq!(msg.sender, Some("PlayerName".to_string()));
        assert_eq!(msg.item_links.len(), 1);
        assert_eq!(
            msg.item_links[0].item_name,
            "Leatherworking: Great Evasion Shirt"
        );
    }

    // ── Multiline / parse_chat_lines tests ──────────────────────────────────

    #[test]
    fn test_parse_chat_lines_basic() {
        let text = "26-03-09 05:01:46\t[Global] Player1: hello\n\
                     26-03-09 05:01:47\t[Global] Player2: hi there";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].message, "hello");
        assert_eq!(msgs[1].message, "hi there");
    }

    #[test]
    fn test_parse_chat_lines_multiline_message() {
        let text = "26-03-09 05:01:46\t[Trade] Seller: WTS these items\n\
                     [Item: Sword]\n\
                     [Item: Blacksmithing: Iron Hammer]";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].message.contains("WTS these items"));
        assert!(msgs[0].message.contains("[Item: Sword]"));
        assert_eq!(msgs[0].item_links.len(), 2);
        assert_eq!(msgs[0].item_links[0].item_name, "Sword");
        assert_eq!(
            msgs[0].item_links[1].item_name,
            "Blacksmithing: Iron Hammer"
        );
    }

    #[test]
    fn test_parse_chat_lines_continuation_with_text() {
        let text = "26-03-09 12:46:01\t[Tell] You->AnotherPlayer: you need?\n\
                     [Item: Cow: Moo of Calm 7]";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].message.contains("you need?"));
        assert!(msgs[0].message.contains("[Item: Cow: Moo of Calm 7]"));
        assert_eq!(msgs[0].item_links.len(), 1);
        assert_eq!(msgs[0].item_links[0].item_name, "Cow: Moo of Calm 7");
    }

    #[test]
    fn test_parse_chat_lines_orphaned_continuation() {
        // Continuation line with no preceding message — should be skipped
        let text = "[Item: Sword]\n\
                     26-03-09 05:01:46\t[Global] Player1: hello";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message, "hello");
    }

    #[test]
    fn test_parse_chat_lines_blank_lines_skipped() {
        let text = "26-03-09 05:01:46\t[Global] Player1: hello\n\
                     \n\
                     26-03-09 05:01:47\t[Global] Player2: world";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_parse_dash_channel_with_item_links() {
        // Exact reproduction of real log data with -apptesting channel
        let text = "26-03-11 19:49:42\t[-apptesting] Zenith: this is a test\n\
26-03-11 19:49:56\t[-apptesting] Zenith: this is an item linking test\n\
[Item: Cheesy Veggie Delight]\n\
26-03-11 19:50:08\t[-apptesting] Zenith: \n\
[Item: Baked Swamp Gourd] [Item: Amberjack Sushi Roll] [Item: Beast in the Tub] [Item: Cheesy Veggie Delight]\n\
26-03-11 19:50:29\t[-apptesting] Zenith: Aurest mission time\n\
Make new friends in Wintertide\n\
Dying together\n\
26-03-11 19:50:38\t[-apptesting] Zenith: That was multiline text";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 5, "Should parse 5 messages, got: {:#?}", msgs);

        // Message 1: plain text, no items
        assert_eq!(msgs[0].sender, Some("Zenith".to_string()));
        assert_eq!(msgs[0].message, "this is a test");
        assert_eq!(msgs[0].item_links.len(), 0);

        // Message 2: text + item on continuation line
        assert_eq!(
            msgs[1].message,
            "this is an item linking test\n[Item: Cheesy Veggie Delight]"
        );
        assert_eq!(msgs[1].item_links.len(), 1);
        assert_eq!(msgs[1].item_links[0].item_name, "Cheesy Veggie Delight");

        // Message 3: empty text + 4 items on continuation line
        assert!(msgs[2].message.contains("[Item: Baked Swamp Gourd]"));
        assert_eq!(msgs[2].item_links.len(), 4);
        assert_eq!(msgs[2].item_links[0].item_name, "Baked Swamp Gourd");
        assert_eq!(msgs[2].item_links[1].item_name, "Amberjack Sushi Roll");
        assert_eq!(msgs[2].item_links[2].item_name, "Beast in the Tub");
        assert_eq!(msgs[2].item_links[3].item_name, "Cheesy Veggie Delight");

        // Message 4: multiline text, no items
        assert!(msgs[3].message.contains("Aurest mission time"));
        assert!(msgs[3].message.contains("Make new friends in Wintertide"));
        assert!(msgs[3].message.contains("Dying together"));
        assert_eq!(msgs[3].item_links.len(), 0);

        // Message 5: plain text
        assert_eq!(msgs[4].message, "That was multiline text");
    }

    #[test]
    fn test_is_timestamped_line() {
        assert!(is_timestamped_line("26-03-09 05:01:46\t[Global] hi"));
        assert!(!is_timestamped_line("[Item: Sword]"));
        assert!(!is_timestamped_line("just some text"));
        assert!(!is_timestamped_line(""));
    }

    // ── Tell parsing tests ───────────────────────────────────────────────

    #[test]
    fn test_parse_tell_outgoing() {
        let line = "26-03-11 12:46:01\t[Tell] You->AnotherPlayer: you need?";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.channel, Some("Tell".to_string()));
        assert_eq!(msg.sender, Some("AnotherPlayer".to_string()));
        assert_eq!(msg.message, "you need?");
        assert_eq!(msg.from_player, Some(true));
        assert!(!msg.is_system);
    }

    #[test]
    fn test_parse_tell_incoming() {
        let line = "26-03-11 12:46:21\t[Tell] AnotherPlayer->You: I JUST traded for it today haha";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.channel, Some("Tell".to_string()));
        assert_eq!(msg.sender, Some("AnotherPlayer".to_string()));
        assert_eq!(msg.message, "I JUST traded for it today haha");
        assert_eq!(msg.from_player, Some(false));
        assert!(!msg.is_system);
    }

    #[test]
    fn test_parse_tell_message_with_colon() {
        // Message body contains colons — should not break parsing
        let line = "26-03-11 12:50:00\t[Tell] You->TraderJoe: price: 500 councils";
        let msg = parse_chat_line(line).unwrap();

        assert_eq!(msg.sender, Some("TraderJoe".to_string()));
        assert_eq!(msg.message, "price: 500 councils");
        assert_eq!(msg.from_player, Some(true));
    }

    #[test]
    fn test_parse_tell_with_item_link() {
        let text = "26-03-11 12:46:01\t[Tell] You->AnotherPlayer: check this out\n\
                     [Item: Cow: Moo of Calm 7]";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, Some("AnotherPlayer".to_string()));
        assert_eq!(msgs[0].from_player, Some(true));
        assert_eq!(msgs[0].item_links.len(), 1);
        assert_eq!(msgs[0].item_links[0].item_name, "Cow: Moo of Calm 7");
    }

    #[test]
    fn test_parse_tell_conversation_both_sides() {
        let text = "26-03-11 12:46:01\t[Tell] You->AnotherPlayer: you need?\n\
                     26-03-11 12:46:21\t[Tell] AnotherPlayer->You: I JUST traded for it today haha";
        let msgs = parse_chat_lines(text);

        assert_eq!(msgs.len(), 2);
        // Both messages should have the same sender (conversation partner)
        assert_eq!(msgs[0].sender, Some("AnotherPlayer".to_string()));
        assert_eq!(msgs[1].sender, Some("AnotherPlayer".to_string()));
        // But different directions
        assert_eq!(msgs[0].from_player, Some(true)); // outgoing
        assert_eq!(msgs[1].from_player, Some(false)); // incoming
    }

    // ── Session-tagged parsing tests ──────────────────────────────────────

    #[test]
    fn test_tagged_keeps_parity_with_untagged_stream() {
        // Same message stream as `parse_chat_lines`: login lines stay
        // ordinary system messages, not consumed markers.
        let text = "26-03-09 05:01:46\t**************************************** Logged In As Zenith. Server Dreva. Timezone Offset -07:00:00.\n\
                     26-03-09 05:01:47\t[Status] Fluorite x5 added to inventory.";
        let tagged = parse_chat_lines_tagged(text);
        let plain = parse_chat_lines(text);

        assert_eq!(tagged.len(), plain.len());
        for ((msg, _), expected) in tagged.iter().zip(plain.iter()) {
            assert_eq!(msg.message, expected.message);
            assert_eq!(msg.channel, expected.channel);
        }
        // The login line itself predates any session: untagged.
        assert_eq!(tagged[0].1, None);
        // The gain after it carries the session that produced it.
        assert_eq!(
            tagged[1].1,
            Some(ChatSessionTag {
                character_name: "Zenith".to_string(),
                server_name: "Dreva".to_string(),
            })
        );
    }

    #[test]
    fn test_tagged_attributes_each_gain_to_its_session() {
        // The frozen-review repro: two sessions in one daily file. Each gain
        // must keep the login session that produced it, not the drain-final
        // identity.
        let text = "26-03-09 05:01:46\t**************************************** Logged In As AltA. Server Dreva. Timezone Offset -07:00:00.\n\
                     26-03-09 05:02:00\t[Status] Fluorite x5 added to inventory.\n\
                     26-03-09 06:01:46\t**************************************** Logged In As AltB. Server Legacy. Timezone Offset -07:00:00.\n\
                     26-03-09 06:02:00\t[Status] Fluorite x3 added to inventory.";
        let tagged = parse_chat_lines_tagged(text);

        let gains: Vec<_> = tagged
            .iter()
            .filter(|(msg, _)| msg.channel.as_deref() == Some("Status"))
            .collect();
        assert_eq!(gains.len(), 2);
        assert_eq!(
            gains[0].1,
            Some(ChatSessionTag {
                character_name: "AltA".to_string(),
                server_name: "Dreva".to_string(),
            })
        );
        assert_eq!(
            gains[1].1,
            Some(ChatSessionTag {
                character_name: "AltB".to_string(),
                server_name: "Legacy".to_string(),
            })
        );
    }

    #[test]
    fn test_tagged_logout_closes_session_without_borrowing() {
        // After logout, lines belong to no session until the next login —
        // never to the logged-out session.
        let text = "26-03-09 05:01:46\t**************************************** Logged In As Zenith. Server Dreva. Timezone Offset -07:00:00.\n\
                     26-03-09 05:05:00\t**************************************** Logged Out\n\
                     26-03-09 05:06:00\t[Status] Fluorite x5 added to inventory.";
        let tagged = parse_chat_lines_tagged(text);

        assert_eq!(tagged.len(), 3);
        assert_eq!(tagged[2].1, None);
    }

    #[test]
    fn test_tagged_ignores_session_phrases_in_player_chat() {
        // Player chat merely mentioning login/logout must neither open nor
        // close the session — and a continuation line carrying such a phrase
        // stays appended to its message instead of being swallowed.
        let text = "26-03-09 05:01:46\t**************************************** Logged In As Zenith. Server Dreva. Timezone Offset -07:00:00.\n\
                     26-03-09 05:02:00\t[Guild] Bob: I Logged Out yesterday\n\
                     26-03-09 05:02:01\t[Guild] Bob: I Logged In As someone else\n\
                     still talking here, I Logged Out yesterday\n\
                     26-03-09 05:03:00\t[Status] Fluorite x5 added to inventory.";
        let tagged = parse_chat_lines_tagged(text);
        let plain = parse_chat_lines(text);

        // Same message stream as the untagged parser.
        assert_eq!(tagged.len(), plain.len());
        for ((msg, _), expected) in tagged.iter().zip(plain.iter()) {
            assert_eq!(msg.message, expected.message);
        }
        // Session never closed: every message keeps Zenith/Dreva — the login
        // line itself is untagged, the rest carry the session, including the
        // gain and the chat line with the appended continuation.
        assert_eq!(tagged.len(), 4);
        assert_eq!(tagged[0].1, None);
        for (_, tag) in &tagged[1..] {
            assert_eq!(
                *tag,
                Some(ChatSessionTag {
                    character_name: "Zenith".to_string(),
                    server_name: "Dreva".to_string(),
                })
            );
        }
        assert!(tagged[2].0.message.contains("still talking here, I Logged Out yesterday"));
    }
}
