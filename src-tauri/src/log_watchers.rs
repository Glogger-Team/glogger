use crate::chat_parser::{parse_chat_lines_tagged, parse_chat_login_line, ChatMessage};
use crate::parsers::{parse_skill_update, SkillUpdate};
use crate::player_event_parser::{PlayerEvent, PlayerEventParser};
use chrono::NaiveDateTime;
/// Log file watching infrastructure
///
/// This module provides the core abstractions for watching and parsing log files.
/// It includes:
/// - LogFileWatcher trait for unified file watching
/// - PlayerLogWatcher for monitoring Player.log
/// - ChatLogWatcher for monitoring chat log files
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::PathBuf;

/// Type alias for pattern matcher functions
/// Takes a line and returns an optional LogEvent if the pattern matches
pub type PatternMatcher = fn(&str, &mut PlayerLogWatcher) -> Option<LogEvent>;

/// Events that can be emitted by log file watchers
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum LogEvent {
    /// Chat message parsed, tagged with the login session that produced it
    /// (`None` while no login line has been seen yet — mid-session resume
    /// from a stored byte offset: the caller falls back to the drain identity
    /// instead of dropping the gain).
    ChatMessage(ChatMessage, Option<crate::chat_parser::ChatSessionTag>),
    /// Character logged in
    CharacterLogin {
        character_name: String,
        timestamp: NaiveDateTime,
        area: Option<String>,
    },

    /// Character logged out
    CharacterLogout {
        character_name: String,
        timestamp: NaiveDateTime,
    },

    /// Chat log file path announced
    ChatLogPath {
        path: String,
        timestamp: NaiveDateTime,
    },

    /// Area transition detected
    AreaTransition {
        area: String,
        timestamp: NaiveDateTime,
    },

    /// Survey completed
    SurveyCompleted {
        survey_type: String,
        quality: Option<u32>,
        timestamp: NaiveDateTime,
    },

    /// XP gained
    XpGained {
        skill: String,
        amount: u32,
        timestamp: NaiveDateTime,
    },

    /// Item looted
    ItemLooted {
        item_name: String,
        quantity: u32,
        timestamp: NaiveDateTime,
    },

    /// Skill update (ProcessUpdateSkill line)
    SkillUpdated(SkillUpdate),

    /// Player log event (items, skills, NPC interactions, vendor, storage, etc.)
    PlayerEventParsed(PlayerEvent),

    /// Server detected from chat log login line
    ServerDetected {
        server_name: String,
        character_name: String,
        /// Timezone offset in seconds from UTC (e.g., -25200 for -07:00:00)
        timezone_offset_seconds: Option<i32>,
    },

    /// Generic log line that wasn't parsed
    Unparsed {
        line: String,
        timestamp: NaiveDateTime,
    },
}

/// Trait for watching and parsing log files
pub trait LogFileWatcher {
    /// Start watching the log file
    fn start(&mut self) -> Result<(), String>;

    /// Stop watching the log file
    fn stop(&mut self);

    /// Poll the file for new content and return parsed events
    fn poll(&mut self) -> Result<Vec<LogEvent>, String>;

    /// Get the current file position
    fn get_position(&self) -> u64;

    /// True if the last `poll` detected a truncation/rotation and reset the
    /// in-memory position — the caller must persist the reset (force) so the
    /// stored cursor doesn't keep the stale larger offset.
    fn position_was_reset(&self) -> bool;

    /// Caller acknowledged the reset (persisted force_position) — clear flag.
    fn clear_position_reset(&mut self);

    /// Check if the watcher is currently active
    fn is_active(&self) -> bool;
}

/// Watcher for Player.log - the master log file
pub struct PlayerLogWatcher {
    file_path: PathBuf,
    current_position: u64,
    /// Set when poll detects truncation/rotation; cleared by the caller
    /// after persisting the forced reset.
    position_reset: bool,
    active: bool,
    active_character: Option<String>,
    current_chat_log: Option<PathBuf>,
    pattern_matchers: Vec<PatternMatcher>,
    player_event_parser: PlayerEventParser,
    /// When true, raw lines are buffered for debug capture.
    capture_raw_lines: bool,
    raw_lines_buffer: Vec<String>,
}

impl PlayerLogWatcher {
    /// Create a new PlayerLogWatcher with default pattern matchers
    pub fn new(file_path: PathBuf) -> Self {
        let mut watcher = Self {
            file_path,
            current_position: 0,
            position_reset: false,
            active: false,
            active_character: None,
            current_chat_log: None,
            pattern_matchers: Vec::new(),
            player_event_parser: PlayerEventParser::new(),
            capture_raw_lines: false,
            raw_lines_buffer: Vec::new(),
        };

        watcher.register_core_patterns();
        watcher
    }

    /// Create from existing position (resume from database)
    pub fn from_position(file_path: PathBuf, position: u64) -> Self {
        let mut watcher = Self {
            file_path,
            current_position: position,
            position_reset: false,
            active: false,
            active_character: None,
            current_chat_log: None,
            pattern_matchers: Vec::new(),
            player_event_parser: PlayerEventParser::new(),
            capture_raw_lines: false,
            raw_lines_buffer: Vec::new(),
        };

        watcher.register_core_patterns();
        watcher
    }

    /// Register a custom pattern matcher
    pub fn register_pattern(&mut self, matcher: PatternMatcher) {
        self.pattern_matchers.push(matcher);
    }

    /// Register the core patterns that are always active
    fn register_core_patterns(&mut self) {
        self.register_pattern(match_character_login);
        self.register_pattern(match_chat_log_path);
        self.register_pattern(match_area_transition);
        self.register_pattern(match_skill_update);
    }

    /// Get the currently active character name
    pub fn get_active_character(&self) -> Option<&str> {
        self.active_character.as_deref()
    }

    /// Seed the active character from a previously saved session.
    /// Used when resuming from a saved log position so we know who was
    /// playing even if no new login line appears.
    pub fn set_active_character(&mut self, name: String) {
        self.active_character = Some(name);
    }

    /// Get the current chat log path
    pub fn get_chat_log_path(&self) -> Option<&PathBuf> {
        self.current_chat_log.as_ref()
    }

    /// Feed a resolved chat `[Status] ItemGained` event into the player event
    /// parser's correlation buffer. The coordinator calls this when it sees
    /// a chat gain so upcoming `ProcessAddItem`s can seed their stack size
    /// from the authoritative chat quantity.
    ///
    /// `item_internal_name` must already be resolved (chat's display name
    /// mapped to the CDN internal name); the parser does the final match and
    /// has no CDN access of its own. `timestamp_hms` should be UTC in
    /// "HH:MM:SS" form to align with player.log timestamps.
    pub fn feed_chat_gain(
        &mut self,
        item_internal_name: String,
        quantity: u32,
        timestamp_hms: &str,
    ) {
        self.player_event_parser
            .feed_chat_gain(item_internal_name, quantity, timestamp_hms);
    }
    /// One-shot login-batch signal from the parser: the current
    /// `is_new=false` AddItem burst crossed `LOGIN_BATCH_MIN_ITEMS`, so the
    /// inventory slice needs one clear before the batch persists. Consumed
    /// here; the coordinator calls `GameStateManager::clear_inventory_slice`.
    pub fn take_inventory_login_batch(&mut self) -> bool {
        self.player_event_parser.take_inventory_login_batch()
    }

    /// Enable or disable raw line capture for debug purposes.
    pub fn set_capture_raw_lines(&mut self, enabled: bool) {
        self.capture_raw_lines = enabled;
        if !enabled {
            self.raw_lines_buffer.clear();
        }
    }

    /// Drain all buffered raw lines captured since the last drain.
    pub fn drain_raw_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.raw_lines_buffer)
    }

    /// Parse a single line from Player.log
    fn parse_line(&mut self, line: &str) -> Option<LogEvent> {
        let matchers = self.pattern_matchers.clone();

        for matcher in matchers {
            if let Some(event) = matcher(line, self) {
                return Some(event);
            }
        }

        None
    }
}

// ============================================================
// Core Pattern Matchers
// ============================================================

/// Match a Player.log character login line, recording the character as active.
///
/// Current clients write `Logged in as character <Name>. Time UTC=MM/DD/YYYY
/// HH:MM:SS. Timezone Offset …` — parsed by
/// [`crate::parsers::parse_player_log_login_line`], which also yields the
/// session's own UTC instant. Older clients bracketed the name
/// (`Logged in as character [<Name>]`) and declared no time; that form still
/// matches, with the event carrying the ingest time.
fn match_character_login(line: &str, watcher: &mut PlayerLogWatcher) -> Option<LogEvent> {
    if let Some((character_name, utc)) = crate::parsers::parse_player_log_login_line(line) {
        watcher.active_character = Some(character_name.clone());
        return Some(LogEvent::CharacterLogin {
            character_name,
            timestamp: utc,
            area: None,
        });
    }

    if let Some(start) = line.find("Logged in as character [") {
        if let Some(end) = line[start..].find(']') {
            let name_start = start + "Logged in as character [".len();
            let name_end = start + end;
            let character_name = line[name_start..name_end].to_string();

            watcher.active_character = Some(character_name.clone());

            return Some(LogEvent::CharacterLogin {
                character_name,
                timestamp: chrono::Utc::now().naive_utc(),
                area: None,
            });
        }
    }
    None
}

/// Match chat log path announcement: "Logging chat to C:/Users/.../ChatLogs/Chat-26-03-06.log"
fn match_chat_log_path(line: &str, watcher: &mut PlayerLogWatcher) -> Option<LogEvent> {
    if let Some(start) = line.find("Logging chat to ") {
        let path_start = start + "Logging chat to ".len();
        let path = line[path_start..].trim().to_string();

        watcher.current_chat_log = Some(PathBuf::from(&path));

        return Some(LogEvent::ChatLogPath {
            path,
            timestamp: chrono::Utc::now().naive_utc(),
        });
    }
    None
}

/// Match area transition: "LOADING LEVEL AreaCasino"
fn match_area_transition(line: &str, _watcher: &mut PlayerLogWatcher) -> Option<LogEvent> {
    if let Some(start) = line.find("LOADING LEVEL ") {
        let area_start = start + "LOADING LEVEL ".len();
        let area = line[area_start..].trim().to_string();

        return Some(LogEvent::AreaTransition {
            area,
            timestamp: chrono::Utc::now().naive_utc(),
        });
    }
    None
}

/// Match skill update: "ProcessUpdateSkill" lines
fn match_skill_update(line: &str, _watcher: &mut PlayerLogWatcher) -> Option<LogEvent> {
    parse_skill_update(line).map(LogEvent::SkillUpdated)
}

impl LogFileWatcher for PlayerLogWatcher {
    fn start(&mut self) -> Result<(), String> {
        if !self.file_path.exists() {
            return Err(format!("Player.log not found at {:?}", self.file_path));
        }

        self.active = true;
        Ok(())
    }

    fn stop(&mut self) {
        self.active = false;
    }

    fn poll(&mut self) -> Result<Vec<LogEvent>, String> {
        use std::io::Read;

        if !self.active {
            return Ok(Vec::new());
        }

        let mut file =
            File::open(&self.file_path).map_err(|e| format!("Failed to open Player.log: {}", e))?;

        let file_size = file
            .metadata()
            .map_err(|e| format!("Failed to get Player.log metadata: {}", e))?
            .len();

        // Detect file truncation / rotation (e.g. game restart creates a new Player.log)
        if file_size < self.current_position {
            eprintln!(
                "[player-poll] Player.log was rotated (size {} < position {}), resetting to beginning",
                file_size, self.current_position
            );
            self.current_position = 0;
            self.position_reset = true;
            self.player_event_parser = PlayerEventParser::new();
        }

        if self.current_position >= file_size {
            return Ok(Vec::new());
        }

        file.seek(SeekFrom::Start(self.current_position))
            .map_err(|e| format!("Failed to seek in Player.log: {}", e))?;

        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| format!("Failed to read Player.log: {}", e))?;

        // Only advance position to the last complete line. If the file doesn't
        // end with a newline the final partial line is left for the next poll.
        let usable_len = match content.iter().rposition(|&b| b == b'\n') {
            Some(pos) => pos + 1,
            None => {
                // No newline found — entire read is a partial line, skip for now
                return Ok(Vec::new());
            }
        };

        self.current_position += usable_len as u64;
        let content_str = String::from_utf8_lossy(&content[..usable_len]);

        let mut events = Vec::new();
        for line_content in content_str.lines() {
            // Buffer raw line for debug capture if enabled
            if self.capture_raw_lines {
                self.raw_lines_buffer.push(line_content.to_string());
            }

            if let Some(event) = self.parse_line(line_content) {
                events.push(event);
            }

            // Feed every line through the player event parser first
            let player_events = self.player_event_parser.process_line(line_content);
            for pe in player_events {
                events.push(LogEvent::PlayerEventParsed(pe));
            }
        }

        // Poll-boundary drain: emit only deletes that already outlived the
        // 5 s / 5-line window. Young entries survive into the next poll so
        // cross-poll Delete/confirmation pairs still resolve. (Rotation
        // above rebuilds the parser — nothing survives that by design.
        // Login-line ordering uses the explicit full flush in process_line;
        // end-of-stream callers use flush_all_pending directly.)
        let now_secs = self.player_event_parser.last_line_secs();
        let mut drain_events = Vec::new();
        self.player_event_parser
            .flush_expired_deletes(now_secs, &mut drain_events);
        for pe in drain_events {
            events.push(LogEvent::PlayerEventParsed(pe));
        }

        Ok(events)
    }

    fn get_position(&self) -> u64 {
        self.current_position
    }

    fn position_was_reset(&self) -> bool {
        self.position_reset
    }

    /// Caller acknowledged the reset (persisted force_position) — clear flag.
    fn clear_position_reset(&mut self) {
        self.position_reset = false;
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

/// Watcher for chat log files
///
/// Chat logs are daily files (Chat-YY-MM-DD.log) that span multiple play sessions.
/// They contain login/logout markers: `******** Logged In As [Name]` / `******** Logged Out`
///
/// This watcher handles:
/// - Session tracking within daily files
/// - Message parsing (all channels — filtering is handled downstream)
/// - Position tracking for incremental reads
pub struct ChatLogWatcher {
    file_path: PathBuf,
    current_position: u64,
    /// Set when poll detects truncation/rotation; cleared by the caller
    /// after persisting the forced reset.
    position_reset: bool,
    active: bool,
    player_name: Option<String>,
    current_session_start: Option<NaiveDateTime>,
    /// When true, raw lines are buffered for debug capture.
    capture_raw_lines: bool,
    raw_lines_buffer: Vec<String>,
}

impl ChatLogWatcher {
    /// Create a new ChatLogWatcher
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            current_position: 0,
            position_reset: false,
            active: false,
            player_name: None,
            current_session_start: None,
            capture_raw_lines: false,
            raw_lines_buffer: Vec::new(),
        }
    }

    /// Create from existing position (resume from database)
    pub fn from_position(file_path: PathBuf, position: u64) -> Self {
        Self {
            file_path,
            current_position: position,
            position_reset: false,
            active: false,
            player_name: None,
            current_session_start: None,
            capture_raw_lines: false,
            raw_lines_buffer: Vec::new(),
        }
    }

    /// Set the player name for this chat log
    pub fn set_player_name(&mut self, name: String) {
        self.player_name = Some(name);
    }

    /// Get the current session start time
    #[allow(dead_code)]
    pub fn get_session_start(&self) -> Option<NaiveDateTime> {
        self.current_session_start
    }

    /// Get the file name (just the filename component, not the full path)
    pub fn get_file_name(&self) -> &str {
        self.file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    }

    /// Get the full file path
    pub fn get_file_path(&self) -> &PathBuf {
        &self.file_path
    }

    /// Enable or disable raw line capture for debug purposes.
    pub fn set_capture_raw_lines(&mut self, enabled: bool) {
        self.capture_raw_lines = enabled;
        if !enabled {
            self.raw_lines_buffer.clear();
        }
    }

    /// Drain all buffered raw lines captured since the last drain.
    pub fn drain_raw_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.raw_lines_buffer)
    }

    /// Check a line for session markers (login/logout).
    /// Returns a vec of LogEvents if the line is a session marker.
    fn check_session_marker(&mut self, line: &str) -> Vec<LogEvent> {
        let mut events = Vec::new();

        // Check for login marker using the chat login line parser
        // Format: "******** Logged In As Zenith. Server Dreva. Timezone Offset -07:00:00."
        if let Some(info) = parse_chat_login_line(line) {
            self.player_name = Some(info.character_name.clone());
            self.current_session_start = Some(chrono::Utc::now().naive_utc());

            events.push(LogEvent::ServerDetected {
                server_name: info.server_name,
                character_name: info.character_name.clone(),
                timezone_offset_seconds: info.timezone_offset_seconds,
            });

            events.push(LogEvent::CharacterLogin {
                character_name: info.character_name,
                timestamp: chrono::Utc::now().naive_utc(),
                area: None,
            });

            return events;
        }

        // Check for logout marker: ******** Logged Out
        if line.contains("******** Logged Out") {
            if let Some(player) = &self.player_name {
                events.push(LogEvent::CharacterLogout {
                    character_name: player.clone(),
                    timestamp: chrono::Utc::now().naive_utc(),
                });
                self.current_session_start = None;
            }
        }

        events
    }
}

impl LogFileWatcher for ChatLogWatcher {
    fn start(&mut self) -> Result<(), String> {
        if !self.file_path.exists() {
            return Err(format!("Chat log not found at {:?}", self.file_path));
        }

        self.active = true;
        Ok(())
    }

    fn stop(&mut self) {
        self.active = false;
    }

    fn poll(&mut self) -> Result<Vec<LogEvent>, String> {
        use std::io::Read;

        if !self.active {
            return Ok(Vec::new());
        }

        let mut file =
            File::open(&self.file_path).map_err(|e| format!("Failed to open chat log: {}", e))?;

        let file_size = file
            .metadata()
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .len();

        // Handle file shrink (game recreated file)
        if self.current_position > file_size {
            self.current_position = 0;
            self.position_reset = true;
        }

        if self.current_position >= file_size {
            return Ok(Vec::new());
        }

        file.seek(SeekFrom::Start(self.current_position))
            .map_err(|e| format!("Failed to seek in chat log: {}", e))?;

        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| format!("Failed to read chat log: {}", e))?;

        // Only advance position to the last complete line to avoid reading
        // a partial line if the game is mid-write.
        let usable_len = match content.iter().rposition(|&b| b == b'\n') {
            Some(pos) => pos + 1,
            None => {
                // No newline found — entire read is a partial line, skip for now
                return Ok(Vec::new());
            }
        };

        self.current_position += usable_len as u64;

        let content_str = String::from_utf8_lossy(&content[..usable_len]);

        // Buffer raw lines for debug capture if enabled
        if self.capture_raw_lines {
            for line in content_str.lines() {
                self.raw_lines_buffer.push(line.to_string());
            }
        }

        let mut events = Vec::new();

        // Check each line for session markers, but only keep the LAST login
        // marker found in the batch. When reading from position 0 the chat log
        // contains every historical login — we only care about the most recent.
        let mut last_login_events: Option<Vec<LogEvent>> = None;
        let mut non_login_marker_events: Vec<LogEvent> = Vec::new();

        for line in content_str.lines() {
            let marker_events = self.check_session_marker(line);
            if !marker_events.is_empty() {
                // Check if this batch contains a login (ServerDetected + CharacterLogin)
                let has_login = marker_events
                    .iter()
                    .any(|e| matches!(e, LogEvent::CharacterLogin { .. }));
                if has_login {
                    // Replace any previous login — we only want the last one
                    last_login_events = Some(marker_events);
                } else {
                    non_login_marker_events.extend(marker_events);
                }
            }
        }

        // Emit the last login events first, then any non-login markers (logouts)
        if let Some(login_events) = last_login_events {
            events.extend(login_events);
        }
        events.extend(non_login_marker_events);

        // Parse all chat messages with the multiline-aware parser, tagged by
        // the login session that produced each one. Login lines were consumed
        // above as markers (only the last survives the collapse) — the tagged
        // parser re-derives the full per-line session sequence, so gains in
        // earlier sessions keep their own identity. Resume-from-offset polls
        // yield `None` tags until the next login line, by design.
        for (msg, session) in parse_chat_lines_tagged(&content_str) {
            events.push(LogEvent::ChatMessage(msg, session));
        }

        Ok(events)
    }

    fn get_position(&self) -> u64 {
        self.current_position
    }

    fn position_was_reset(&self) -> bool {
        self.position_reset
    }

    /// Caller acknowledged the reset (persisted force_position) — clear flag.
    fn clear_position_reset(&mut self) {
        self.position_reset = false;
    }

    fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_parser::parse_chat_lines;

    #[test]
    fn test_parse_character_login() {
        let mut watcher = PlayerLogWatcher::new(PathBuf::from("test.log"));

        // Current client: un-bracketed name plus the session's declared UTC time.
        let event = watcher.parse_line(
            "[22:43:41] Logged in as character TestCharacter. Time UTC=10/14/2026 22:43:41. Timezone Offset -05:00:00",
        );
        assert!(event.is_some());

        if let Some(LogEvent::CharacterLogin {
            character_name,
            timestamp,
            ..
        }) = event
        {
            assert_eq!(character_name, "TestCharacter");
            assert_eq!(timestamp.to_string(), "2026-10-14 22:43:41");
        } else {
            panic!("Expected CharacterLogin event");
        }

        assert_eq!(watcher.get_active_character(), Some("TestCharacter"));

        // Older clients bracketed the name and declared no time.
        let mut watcher = PlayerLogWatcher::new(PathBuf::from("test.log"));
        let event = watcher.parse_line("Logged in as character [TestCharacter]");
        assert!(event.is_some());

        if let Some(LogEvent::CharacterLogin { character_name, .. }) = event {
            assert_eq!(character_name, "TestCharacter");
        } else {
            panic!("Expected CharacterLogin event");
        }

        assert_eq!(watcher.get_active_character(), Some("TestCharacter"));
    }

    #[test]
    fn test_parse_chat_log_path() {
        let mut watcher = PlayerLogWatcher::new(PathBuf::from("test.log"));

        let event = watcher.parse_line("Logging chat to C:/Users/Test/ChatLogs/Chat-26-03-06.log");
        assert!(event.is_some());

        if let Some(LogEvent::ChatLogPath { path, .. }) = event {
            assert_eq!(path, "C:/Users/Test/ChatLogs/Chat-26-03-06.log");
        } else {
            panic!("Expected ChatLogPath event");
        }

        assert!(watcher.get_chat_log_path().is_some());
    }

    #[test]
    fn test_parse_area_transition() {
        let mut watcher = PlayerLogWatcher::new(PathBuf::from("test.log"));

        let event = watcher.parse_line("LOADING LEVEL AreaCasino");
        assert!(event.is_some());

        if let Some(LogEvent::AreaTransition { area, .. }) = event {
            assert_eq!(area, "AreaCasino");
        } else {
            panic!("Expected AreaTransition event");
        }
    }

    #[test]
    fn test_chat_login_marker() {
        let mut watcher = ChatLogWatcher::new(PathBuf::from("test.log"));

        // Use the actual chat log format
        let events = watcher.check_session_marker("26-03-22 18:12:49\t**************************************** Logged In As TestCharacter. Server Dreva. Timezone Offset -07:00:00.");
        assert_eq!(events.len(), 2);

        // First event: ServerDetected
        if let LogEvent::ServerDetected {
            server_name,
            character_name,
            timezone_offset_seconds,
        } = &events[0]
        {
            assert_eq!(server_name, "Dreva");
            assert_eq!(character_name, "TestCharacter");
            assert_eq!(*timezone_offset_seconds, Some(-25200)); // -07:00:00
        } else {
            panic!("Expected ServerDetected event, got {:?}", events[0]);
        }

        // Second event: CharacterLogin
        if let LogEvent::CharacterLogin { character_name, .. } = &events[1] {
            assert_eq!(character_name, "TestCharacter");
        } else {
            panic!("Expected CharacterLogin event, got {:?}", events[1]);
        }

        assert_eq!(watcher.player_name, Some("TestCharacter".to_string()));
        assert!(watcher.current_session_start.is_some());
    }

    #[test]
    fn test_chat_logout_marker() {
        let mut watcher = ChatLogWatcher::new(PathBuf::from("test.log"));
        watcher.player_name = Some("TestCharacter".to_string());

        let events = watcher.check_session_marker("******** Logged Out");
        assert_eq!(events.len(), 1);

        if let LogEvent::CharacterLogout { character_name, .. } = &events[0] {
            assert_eq!(character_name, "TestCharacter");
        } else {
            panic!("Expected CharacterLogout event");
        }

        assert!(watcher.current_session_start.is_none());
    }

    #[test]
    fn test_chat_message_parsing() {
        let line = "26-03-09 14:23:45\t[Global] TestPlayer: Hello world!";
        let messages = parse_chat_lines(line);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].channel, Some("Global".to_string()));
        assert_eq!(messages[0].sender, Some("TestPlayer".to_string()));
        assert_eq!(messages[0].message, "Hello world!");
    }

    #[test]
    fn test_all_channels_parsed() {
        // All channels are now parsed — no exclusion at the watcher/parser level
        let line = "26-03-09 14:23:45\t[Status] You earned 50 XP in Mining.";
        let messages = parse_chat_lines(line);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].channel, Some("Status".to_string()));
    }

    #[test]
    fn test_chat_watcher_get_file_name() {
        let watcher = ChatLogWatcher::new(PathBuf::from("/some/path/Chat-26-03-09.log"));
        assert_eq!(watcher.get_file_name(), "Chat-26-03-09.log");
    }

    #[test]
    fn test_custom_pattern_registration() {
        let mut watcher = PlayerLogWatcher::new(PathBuf::from("test.log"));

        fn match_xp_gain(line: &str, _watcher: &mut PlayerLogWatcher) -> Option<LogEvent> {
            if line.contains("You gain") && line.contains("experience in") {
                Some(LogEvent::XpGained {
                    skill: "TestSkill".to_string(),
                    amount: 100,
                    timestamp: chrono::Utc::now().naive_utc(),
                })
            } else {
                None
            }
        }

        watcher.register_pattern(match_xp_gain);

        let event = watcher.parse_line("You gain 100 experience in TestSkill");
        assert!(event.is_some());

        if let Some(LogEvent::XpGained { skill, amount, .. }) = event {
            assert_eq!(skill, "TestSkill");
            assert_eq!(amount, 100);
        } else {
            panic!("Expected XpGained event from custom pattern");
        }
    }

    #[test]
    fn test_pattern_registration_order() {
        let mut watcher = PlayerLogWatcher::new(PathBuf::from("test.log"));

        let event1 = watcher.parse_line("Logged in as character [TestChar]");
        assert!(matches!(event1, Some(LogEvent::CharacterLogin { .. })));

        let event2 = watcher.parse_line("LOADING LEVEL TestArea");
        assert!(matches!(event2, Some(LogEvent::AreaTransition { .. })));
    }

    #[test]
    fn test_chat_poll_tags_gains_by_producing_session() {
        // The frozen-review repro at the watcher level: a daily file with two
        // sessions. The collapsed login markers carry only the last session,
        // but each gain ChatMessage keeps the session that produced it.
        use crate::chat_parser::ChatSessionTag;
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        // Nanos kept for human-sortable names; pid + counter carry uniqueness
        // (`SystemTime` nanos repeat when parallel tests start in one tick).
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "glogger_chat_tag_test_{}_{nanos}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Chat-26-03-09.log");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            "26-03-09 05:01:46\t**************************************** Logged In As AltA. Server Dreva. Timezone Offset -07:00:00.\n\
             26-03-09 05:02:00\t[Status] Fluorite x5 added to inventory.\n\
             26-03-09 06:01:46\t**************************************** Logged In As AltB. Server Legacy. Timezone Offset -07:00:00.\n\
             26-03-09 06:02:00\t[Status] Fluorite x3 added to inventory."
        )
        .unwrap();
        drop(file);

        let mut watcher = ChatLogWatcher::new(path.clone());
        watcher.start().unwrap();
        let events = watcher.poll().unwrap();

        // Last login survives the marker collapse (pre-existing behaviour).
        let logins: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                LogEvent::CharacterLogin { character_name, .. } => Some(character_name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(logins, vec!["AltB".to_string()]);

        // But each gain keeps its own producing session.
        let tags: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                LogEvent::ChatMessage(msg, tag) if msg.channel.as_deref() == Some("Status") => {
                    Some(tag.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(tags.len(), 2);
        assert_eq!(
            tags[0],
            Some(ChatSessionTag {
                character_name: "AltA".to_string(),
                server_name: "Dreva".to_string(),
            })
        );
        assert_eq!(
            tags[1],
            Some(ChatSessionTag {
                character_name: "AltB".to_string(),
                server_name: "Legacy".to_string(),
            })
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
