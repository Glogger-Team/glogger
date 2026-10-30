//! Debug capture system for recording raw log lines and game state snapshots.
//!
//! When a capture is active, raw Player.log and Chat.log lines are written to
//! temporary files on disk to avoid unbounded memory growth during long sessions.
//! Game state is snapshotted at start and stop. The whole bundle is assembled
//! into a single JSON file for debugging and support purposes.
//!
//! ## Two-phase stop/save flow
//!
//! 1. `stop()` — ends recording and takes the final game state snapshot. The
//!    temp file and snapshots are kept so the user can review stats and edit
//!    notes before saving.
//! 2. `save()` — writes the bundle JSON to the chosen path with the user's
//!    notes and selected filter mode (normal or full).
//! 3. `discard()` — can be called at any time (active or stopped) to throw
//!    away captured data without saving.

use crate::db::{DbPool, DbRead};
use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

/// A single captured log line with its source and timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedLine {
    /// When this line was captured (local clock, HH:MM:SS.mmm).
    pub captured_at: String,
    /// "player" or "chat"
    pub source: String,
    /// The raw line content.
    pub line: String,
}

/// Snapshot of game state tables taken at a point in time.
/// `truncated_sections` names the tables capped at `SNAPSHOT_ROW_LIMIT`
/// rows (see `query_table_as_json`) so a capped snapshot is never mistaken
/// for a complete one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateSnapshot {
    pub taken_at: String,
    pub character: Option<String>,
    pub server: Option<String>,
    pub skills: Vec<serde_json::Value>,
    pub attributes: Vec<serde_json::Value>,
    pub inventory: Vec<serde_json::Value>,
    pub equipment: Vec<serde_json::Value>,
    pub effects: Vec<serde_json::Value>,
    pub favor: Vec<serde_json::Value>,
    pub currencies: Vec<serde_json::Value>,
    pub world: serde_json::Value,
    pub active_skills: serde_json::Value,
    #[serde(default)]
    pub truncated_sections: Vec<String>,
}
/// Status of the debug capture, sent to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct DebugCaptureStatus {
    pub active: bool,
    /// True when recording has stopped but data hasn't been saved or discarded yet.
    pub pending_save: bool,
    pub started_at: Option<String>,
    pub line_count: usize,
    pub player_line_count: usize,
    pub chat_line_count: usize,
}

/// Result returned after saving a capture.
#[derive(Debug, Clone, Serialize)]
pub struct DebugCaptureResult {
    pub line_count: usize,
    pub player_line_count: usize,
    pub chat_line_count: usize,
}

/// Manages the state of an active debug capture.
///
/// Raw log lines are appended to a temp file as tab-separated records
/// (`source\tcaptured_at\tline`) to keep memory usage constant.
pub struct DebugCaptureState {
    active: bool,
    /// True when recording has stopped but data hasn't been saved/discarded yet.
    pending_save: bool,
    started_at: Option<String>,
    stopped_at: Option<String>,
    player_line_count: usize,
    chat_line_count: usize,
    state_at_start: Option<GameStateSnapshot>,
    state_at_stop: Option<GameStateSnapshot>,
    app_version: Option<String>,
    /// Temp file for captured lines. Created on start, deleted after export.
    temp_path: Option<PathBuf>,
    temp_writer: Option<BufWriter<fs::File>>,
}

/// Canonicalize an output path: make absolute (relative to CWD) and
/// lexically normalize `.`/`..` without touching the filesystem.
fn canonical_output_path(output_path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(output_path);
    let abs = if p.is_absolute() {
        p
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(p))
            .map_err(|e| format!("Failed to resolve output path: {e}"))?
    };
    let mut out = PathBuf::new();
    for comp in abs.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c.as_os_str()),
        }
    }
    Ok(out)
}

impl DebugCaptureState {
    pub fn new() -> Self {
        Self {
            active: false,
            pending_save: false,
            started_at: None,
            stopped_at: None,
            player_line_count: 0,
            chat_line_count: 0,
            state_at_start: None,
            state_at_stop: None,
            app_version: None,
            temp_path: None,
            temp_writer: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_pending_save(&self) -> bool {
        self.pending_save
    }

    pub fn status(&self) -> DebugCaptureStatus {
        DebugCaptureStatus {
            active: self.active,
            pending_save: self.pending_save,
            started_at: self.started_at.clone(),
            line_count: self.player_line_count + self.chat_line_count,
            player_line_count: self.player_line_count,
            chat_line_count: self.chat_line_count,
        }
    }

    /// Start a new capture. Takes an initial game state snapshot.
    /// `temp_dir` is the directory to create the temp file in (typically app data dir).
    ///
    /// Refuses to start while a stopped-but-unsaved capture is pending:
    /// starting would `cleanup_temp()` the stopped capture's temp file and
    /// snapshots. Call `save()` or `discard()` first.
    pub fn start(
        &mut self,
        snapshot: GameStateSnapshot,
        temp_dir: &std::path::Path,
    ) -> Result<(), String> {
        if self.pending_save {
            return Err(
                "A stopped capture is waiting to be saved — save or discard it before starting a new one"
                    .to_string(),
            );
        }
        // Clean up any leftover temp file from a previous capture
        self.cleanup_temp();

        let temp_path = temp_dir.join("debug-capture-lines.tmp");
        let file = fs::File::create(&temp_path)
            .map_err(|e| format!("Failed to create capture temp file: {e}"))?;

        self.active = true;
        self.pending_save = false;
        self.started_at = Some(Utc::now().to_rfc3339());
        self.stopped_at = None;
        self.player_line_count = 0;
        self.chat_line_count = 0;
        self.state_at_start = Some(snapshot);
        self.state_at_stop = None;
        self.app_version = None;
        self.temp_writer = Some(BufWriter::new(file));
        self.temp_path = Some(temp_path);

        Ok(())
    }

    /// Stop recording and take the final snapshot. The capture data is kept
    /// in the temp file so the user can review and edit notes before saving.
    pub fn stop(
        &mut self,
        snapshot: GameStateSnapshot,
        app_version: String,
    ) -> Result<DebugCaptureResult, String> {
        if !self.active {
            return Err("No debug capture is active".to_string());
        }

        self.active = false;
        self.pending_save = true;
        self.stopped_at = Some(Utc::now().to_rfc3339());
        self.state_at_stop = Some(snapshot);
        self.app_version = Some(app_version);

        // Flush and close the temp writer (keep the file for save)
        if let Some(mut writer) = self.temp_writer.take() {
            writer
                .flush()
                .map_err(|e| format!("Failed to flush capture temp file: {e}"))?;
        }

        Ok(DebugCaptureResult {
            line_count: self.player_line_count + self.chat_line_count,
            player_line_count: self.player_line_count,
            chat_line_count: self.chat_line_count,
        })
    }

    /// Save the stopped capture to a file. `filter_mode` is "normal" (filter
    /// noise), "full" (keep everything), "player-only" (player lines only),
    /// or "chat-only" (chat lines only). Anything else is rejected — the old
    /// code treated an unknown mode as full, silently keeping noise the
    /// caller asked to filter. `overwrite` must be true to replace an
    /// existing file. `output_path` is canonicalized (absolute, lexically
    /// normalized) so the bundle lands where the caller expects.
    pub fn save(
        &mut self,
        notes: String,
        filter_mode: String,
        output_path: &str,
        overwrite: bool,
    ) -> Result<DebugCaptureResult, String> {
        if !self.pending_save {
            return Err("No stopped capture is pending save".to_string());
        }
        let mode = match filter_mode.as_str() {
            "normal" => FilterMode::Normal,
            "full" => FilterMode::Full,
            "player-only" => FilterMode::PlayerOnly,
            "chat-only" => FilterMode::ChatOnly,
            other => {
                return Err(format!(
                    "Unknown filter_mode {other:?} (expected normal|full|player-only|chat-only)"
                ));
            }
        };
        let out_path = canonical_output_path(output_path)?;
        if out_path.exists() && !overwrite {
            return Err(format!(
                "Output file already exists (pass overwrite=true to replace): {}",
                out_path.display()
            ));
        }

        let started_at = self.started_at.take().unwrap_or_default();
        let stopped_at = self.stopped_at.take().unwrap_or_default();
        let state_at_start = self
            .state_at_start
            .take()
            .ok_or_else(|| "Missing start snapshot".to_string())?;
        let state_at_stop = self
            .state_at_stop
            .take()
            .ok_or_else(|| "Missing stop snapshot".to_string())?;
        let app_version = self
            .app_version
            .take()
            .unwrap_or_else(|| "unknown".to_string());

        // Read captured lines from temp file
        let lines = self.read_temp_lines(mode);

        let filtered_player = lines.iter().filter(|l| l.source == "player").count();
        let filtered_chat = lines.iter().filter(|l| l.source == "chat").count();

        let result = DebugCaptureResult {
            line_count: filtered_player + filtered_chat,
            player_line_count: filtered_player,
            chat_line_count: filtered_chat,
        };

        // Build the output JSON
        let bundle = serde_json::json!({
            "format_version": 2,
            "app_version": app_version,
            "captured_by": "glogger debug capture",
            "filter_mode": filter_mode,
            "started_at": started_at,
            "stopped_at": stopped_at,
            "notes": notes,
            "state_at_start": state_at_start,
            "state_at_stop": state_at_stop,
            "line_count": result.line_count,
            "player_line_count": result.player_line_count,
            "chat_line_count": result.chat_line_count,
            "unfiltered_line_count": self.player_line_count + self.chat_line_count,
            "lines": lines,
        });

        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create output dir: {e}"))?;
            }
        }
        let out_file = fs::File::create(&out_path)
            .map_err(|e| format!("Failed to create output file: {e}"))?;
        let writer = BufWriter::new(out_file);
        serde_json::to_writer_pretty(writer, &bundle)
            .map_err(|e| format!("Failed to write capture JSON: {e}"))?;

        // Clean up
        self.cleanup_temp();
        self.pending_save = false;
        self.player_line_count = 0;
        self.chat_line_count = 0;

        Ok(result)
    }

    /// Discard the capture (active or pending save) without saving.
    pub fn discard(&mut self) {
        self.active = false;
        self.pending_save = false;
        self.started_at = None;
        self.stopped_at = None;
        self.state_at_start = None;
        self.state_at_stop = None;
        self.app_version = None;
        self.player_line_count = 0;
        self.chat_line_count = 0;
        if let Some(mut writer) = self.temp_writer.take() {
            let _ = writer.flush();
        }
        self.cleanup_temp();
    }

    /// Append a raw log line to the temp file.
    pub fn push_line(&mut self, source: &'static str, line: String) {
        if !self.active {
            return;
        }
        match source {
            "player" => self.player_line_count += 1,
            "chat" => self.chat_line_count += 1,
            _ => {}
        }
        if let Some(writer) = &mut self.temp_writer {
            let ts = Local::now().format("%H:%M:%S%.3f");
            // Tab-separated: source \t timestamp \t line
            let _ = writeln!(writer, "{}\t{}\t{}", source, ts, line);
        }
    }

    /// Read all captured lines from the temp file into CapturedLine structs.
    /// `Normal` drops known player-log noise; `PlayerOnly`/`ChatOnly` keep
    /// one source; `Full` keeps everything.
    fn read_temp_lines(&self, mode: FilterMode) -> Vec<CapturedLine> {
        let path = match &self.temp_path {
            Some(p) if p.exists() => p,
            _ => return vec![],
        };
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        let reader = BufReader::new(file);
        reader
            .lines()
            .filter_map(|line| {
                let line = line.ok()?;
                let mut parts = line.splitn(3, '\t');
                let source = parts.next()?.to_string();
                let captured_at = parts.next()?.to_string();
                let content = parts.next()?.to_string();

                match mode {
                    FilterMode::PlayerOnly if source != "player" => return None,
                    FilterMode::ChatOnly if source != "chat" => return None,
                    _ => {}
                }
                if matches!(mode, FilterMode::Normal) && source == "player" && is_noise_line(&content) {
                    return None;
                }

                Some(CapturedLine {
                    captured_at,
                    source,
                    line: content,
                })
            })
            .collect()
    }

    /// Remove the temp file if it exists.
    fn cleanup_temp(&mut self) {
        self.temp_writer = None;
        if let Some(path) = self.temp_path.take() {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Capture line filter selected by `filter_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterMode {
    Normal,
    Full,
    PlayerOnly,
    ChatOnly,
}

impl Drop for DebugCaptureState {
    fn drop(&mut self) {
        self.cleanup_temp();
    }
}

/// Check whether a player log line is engine/rendering noise that should be
/// filtered out in "normal" capture mode. Chat lines are never filtered.
fn is_noise_line(line: &str) -> bool {
    // Empty or very short fragment lines (truncation artifacts)
    if line.len() < 4 {
        return true;
    }

    // Exact prefix matches — high-confidence noise patterns
    if line.starts_with("Download appearance loop")
        || line.starts_with("LoadAssetAsync")
        || line.starts_with("IsDoneLoading")
        || line.starts_with("Successfully downloaded Texture")
        || line.starts_with("Cannot remove: entity doesn't have particle")
        || line.starts_with("Ref-count cleanup")
        || line.starts_with("ClearCursor")
        || line.starts_with("Animator.GotoState")
        || line.starts_with("BoxColliders created at Runtime")
        || line.starts_with("Combined Static Meshes")
        || line.starts_with("Either create the Box Collider")
        || line.starts_with("MecanimEx:")
        || line.starts_with("Told to do animation")
        || line.starts_with("Shader ")
        || line.starts_with("New Network State")
    {
        return true;
    }

    // Contains matches
    if line.contains("ProcessMusicPerformance(MusicPerformanceManager+PerformanceInfo)") {
        return true;
    }

    // Sound events: "1234.567: Playing sound ..." or just "Playing sound ..."
    if line.contains(": Playing sound ") || line.starts_with("Playing sound ") {
        return true;
    }

    // Asset bundle loading noise
    if line.starts_with("Completed ") && line.contains("asset bundle") {
        return true;
    }

    false
}

/// Take a snapshot of all game state tables from the database.
/// Returns `Err` when the pool or any table query fails (the old code
/// returned partial/empty snapshots that looked complete).
pub fn snapshot_game_state(
    db: &DbPool,
    character: Option<&str>,
    server: Option<&str>,
) -> Result<GameStateSnapshot, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Snapshot DB connection failed: {e}"))?;

    let (char_filter, server_filter) = match (character, server) {
        (Some(c), Some(s)) => (c.to_string(), s.to_string()),
        _ => (String::new(), String::new()),
    };

    let mut truncated_sections: Vec<String> = Vec::new();
    macro_rules! load {
        ($table:literal) => {{
            let (rows, truncated) =
                query_table_as_json(&conn, $table, &char_filter, &server_filter)?;
            if truncated {
                truncated_sections.push($table.to_string());
            }
            rows
        }};
    }
    let skills = load!("game_state_skills");
    let attributes = load!("game_state_attributes");
    let inventory = load!("game_state_inventory");
    let equipment = load!("game_state_equipment");
    let effects = load!("game_state_effects");
    let favor = load!("game_state_favor");
    let currencies = load!("game_state_currencies");
    let world = query_world_state(&conn, &char_filter, &server_filter)?;
    let active_skills = query_active_skills(&conn, &char_filter, &server_filter)?;
    // World/active-skills sub-tables cap the same way; fold their names in.
    for (key, table) in &[
        ("weather", "game_state_weather"),
        ("combat", "game_state_combat"),
        ("mount", "game_state_mount"),
        ("area", "game_state_area"),
    ] {
        let (_, truncated) = query_table_as_json(&conn, table, &char_filter, &server_filter)?;
        if truncated {
            truncated_sections.push(format!("{key}({table})"));
        }
    }
    let (_, active_truncated) =
        query_table_as_json(&conn, "game_state_active_skills", &char_filter, &server_filter)?;
    if active_truncated {
        truncated_sections.push("game_state_active_skills".to_string());
    }

    Ok(GameStateSnapshot {
        taken_at: Utc::now().to_rfc3339(),
        character: character.map(String::from),
        server: server.map(String::from),
        skills,
        attributes,
        inventory,
        equipment,
        effects,
        favor,
        currencies,
        world,
        active_skills,
        truncated_sections,
    })
}

/// Query a game_state table as JSON, capped at `SNAPSHOT_ROW_LIMIT` rows per
/// table. Returns `(rows, truncated)`: `truncated` is true when the table
/// held more than the cap (the bundle manifest records it per section, so a
/// capped snapshot is never mistaken for a complete one). DB errors are
/// returned to the caller (the old code swallowed prepare/query failures
/// into an empty vec, indistinguishable from "no data").
const SNAPSHOT_ROW_LIMIT: usize = 5000;
fn query_table_as_json<C: DbRead + ?Sized>(
    conn: &C,
    table: &str,
    character: &str,
    server: &str,
) -> Result<(Vec<serde_json::Value>, bool), String> {
    // These tables are safe — names come from our own code, not user input.
    // LIMIT+1 detects truncation: a full page means more rows exist.
    let sql = if character.is_empty() {
        format!("SELECT * FROM {} LIMIT {}", table, SNAPSHOT_ROW_LIMIT + 1)
    } else {
        format!(
            "SELECT * FROM {} WHERE character_name = ?1 AND server_name = ?2 LIMIT {}",
            table,
            SNAPSHOT_ROW_LIMIT + 1
        )
    };

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Snapshot query prepare failed for {table}: {e}"))?;

    let column_names: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();

    let params: Vec<&dyn rusqlite::types::ToSql> = if character.is_empty() {
        vec![]
    } else {
        vec![&character, &server]
    };

    let rows = stmt
        .query_map(params.as_slice(), |row| {
            let mut map = serde_json::Map::new();
            for (i, name) in column_names.iter().enumerate() {
                let val: rusqlite::Result<rusqlite::types::Value> = row.get(i);
                match val {
                    Ok(rusqlite::types::Value::Null) => {
                        map.insert(name.clone(), serde_json::Value::Null);
                    }
                    Ok(rusqlite::types::Value::Integer(n)) => {
                        map.insert(name.clone(), serde_json::json!(n));
                    }
                    Ok(rusqlite::types::Value::Real(f)) => {
                        map.insert(name.clone(), serde_json::json!(f));
                    }
                    Ok(rusqlite::types::Value::Text(s)) => {
                        map.insert(name.clone(), serde_json::json!(s));
                    }
                    Ok(rusqlite::types::Value::Blob(b)) => {
                        map.insert(
                            name.clone(),
                            serde_json::json!(format!("<blob {} bytes>", b.len())),
                        );
                    }
                    Err(_) => {
                        map.insert(name.clone(), serde_json::Value::Null);
                    }
                }
            }
            Ok(serde_json::Value::Object(map))
        })
        .map_err(|e| format!("Snapshot query failed for {table}: {e}"))?;

    let mut out: Vec<serde_json::Value> = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("Snapshot row decode failed for {table}: {e}"))?);
    }
    let truncated = out.len() > SNAPSHOT_ROW_LIMIT;
    if truncated {
        out.truncate(SNAPSHOT_ROW_LIMIT);
    }
    Ok((out, truncated))
}

/// Query the composite world state (weather, combat, mount, area).
fn query_world_state<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> Result<serde_json::Value, String> {
    let mut world = serde_json::Map::new();

    for (key, table) in &[
        ("weather", "game_state_weather"),
        ("combat", "game_state_combat"),
        ("mount", "game_state_mount"),
        ("area", "game_state_area"),
    ] {
        let (rows, _) = query_table_as_json(conn, table, character, server)?;
        if let Some(row) = rows.into_iter().next() {
            world.insert(key.to_string(), row);
        }
    }

    Ok(serde_json::Value::Object(world))
}

/// Query active skills.
fn query_active_skills<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> Result<serde_json::Value, String> {
    let (rows, _) = query_table_as_json(conn, "game_state_active_skills", character, server)?;
    Ok(rows.into_iter().next().unwrap_or(serde_json::Value::Null))
}

// ── Capture Replay ───────────────────────────────────────────────────────

/// A single loot attribution result from replaying a capture file.
#[derive(Debug, Serialize)]
pub struct ReplayLootEvent {
    pub timestamp: String,
    pub item_name: Option<String>,
    pub item_type_id: Option<u16>,
    pub quantity: u32,
    pub corpse_name: Option<String>,
    pub corpse_entity_id: Option<u32>,
    pub instance_id: u64,
    /// "direct" if instance_id matched a known AddItem, "fallback" if resolved
    /// via last_item_event, "unresolved" if neither worked
    pub resolution: String,
}

/// Result of replaying a capture file through the parser.
#[derive(Debug, Serialize)]
pub struct CaptureReplayResult {
    pub total_lines: usize,
    pub player_lines: usize,
    pub chat_lines: usize,
    pub loot_events: Vec<ReplayLootEvent>,
    /// All enemy kills detected from chat combat FATALITY lines
    pub kills: Vec<ReplayKillEvent>,
}

#[derive(Debug, Serialize)]
pub struct ReplayKillEvent {
    pub timestamp: String,
    pub enemy_name: String,
    pub enemy_entity_id: String,
    pub killing_ability: String,
}

/// Replay a debug capture JSON file through the parser and return all
/// LootPickedUp events with their attributions. No database interaction —
/// pure parser validation.
#[tauri::command]
pub fn replay_capture_file(path: String) -> Result<CaptureReplayResult, String> {
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read capture file: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {e}"))?;

    let lines = json
        .get("lines")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Capture file has no 'lines' array".to_string())?;

    // Also check for character name in state_at_start for chat combat parsing
    let character_name = json
        .get("state_at_start")
        .and_then(|s| s.get("active_skills"))
        .and_then(|s| s.get("character_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let mut parser = crate::player_event_parser::PlayerEventParser::new();
    let mut loot_events = Vec::new();
    let mut kills = Vec::new();
    let mut player_lines = 0usize;
    let mut chat_lines = 0usize;

    for line_val in lines {
        let source = line_val
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let line = line_val.get("line").and_then(|v| v.as_str()).unwrap_or("");

        match source {
            "player" => {
                player_lines += 1;
                let events = parser.process_line(line);
                for event in events {
                    if let crate::player_event_parser::PlayerEvent::LootPickedUp {
                        timestamp,
                        instance_id,
                        corpse_entity_id,
                        corpse_name,
                        item_name,
                        item_type_id,
                        quantity,
                    } = event
                    {
                        let resolution = if item_name.is_some() {
                            // Check if it was a direct registry match or fallback
                            if parser.has_instance(instance_id) {
                                "direct"
                            } else {
                                "fallback"
                            }
                        } else {
                            "unresolved"
                        };
                        loot_events.push(ReplayLootEvent {
                            timestamp,
                            item_name,
                            item_type_id,
                            quantity,
                            corpse_name,
                            corpse_entity_id,
                            instance_id,
                            resolution: resolution.to_string(),
                        });
                    }
                }
            }
            "chat" => {
                chat_lines += 1;
                // Parse chat combat for enemy kills
                let combat_event = crate::chat_parser::parse_chat_line(line).and_then(|msg| {
                    crate::chat_combat_parser::parse_combat_message(&msg, &character_name)
                });
                if let Some(crate::chat_combat_parser::ChatCombatEvent::EnemyKilled {
                    timestamp,
                    enemy_name,
                    enemy_entity_id,
                    killing_ability,
                    ..
                }) = combat_event
                {
                    kills.push(ReplayKillEvent {
                        timestamp,
                        enemy_name,
                        enemy_entity_id,
                        killing_ability,
                    });
                }
            }
            _ => {}
        }
    }

    Ok(CaptureReplayResult {
        total_lines: lines.len(),
        player_lines,
        chat_lines,
        loot_events,
        kills,
    })
}
