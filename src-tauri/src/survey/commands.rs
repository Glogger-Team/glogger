//! Tauri commands for the survey tracker.
//!
//! Frontend-visible read/write surface. Reads pull live state from the DB
//! (where the aggregator persists it); writes route through
//! [`SurveySessionAggregator`] so its in-memory caches stay coherent with
//! the persisted state.

use crate::coordinator::DataIngestCoordinator;
use crate::db::DbRead;
use crate::settings::{resolve_item_value, SettingsManager};
use crate::survey::persistence;
use crate::survey::types::{SurveySession, SurveyUse};
use rusqlite::{params, Result as SqlResult};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::State;

// ============================================================
// Public response types (frontend-visible)
// ============================================================

/// Top-level status for the tracker UI. Cheap to compute — used for the
/// status bar / sidebar widget that's always visible.
#[derive(Debug, Clone, Serialize)]
pub struct SurveyTrackerStatus {
    pub active_session: Option<SurveySession>,
    /// Currently-open multihit nodes, oldest `last_hit_at` first. May be
    /// empty even when an active session exists (player isn't mid-multihit).
    pub open_multihit_nodes: Vec<MultihitSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultihitSummary {
    pub node_entity_id: i64,
    pub survey_use_id: i64,
    pub map_display_name: String,
    pub opened_at: String,
    pub last_hit_at: String,
    /// Total loot quantity attributed to the originating use so far.
    /// Lets the UI show "you've gathered N items from this node so far"
    /// without needing a second query per node.
    pub loot_qty: u32,
}

/// Detail payload for a single session. Returns the session header, the
/// list of survey uses that belong to it, a per-item loot summary derived
/// from `item_transactions` filtered by the session's uses'
/// `survey_use_id`, and a precomputed economics rollup.
#[derive(Debug, Clone, Serialize)]
pub struct SurveySessionDetail {
    pub session: SurveySession,
    pub uses: Vec<SurveyUse>,
    pub loot_summary: Vec<LootSummaryRow>,
    pub economics: SessionEconomics,
    /// Crafting material breakdown: which ingredients (and how many) were
    /// consumed to craft all the survey maps in this session. Derived from
    /// `recipe_ingredients` joined through `survey_types.recipe_id`.
    pub craft_materials: Vec<CraftMaterialRow>,
}

/// One ingredient row in the craft-material breakdown of a session.
/// Aggregated across all uses of the same recipe.
#[derive(Debug, Clone, Serialize)]
pub struct CraftMaterialRow {
    pub item_name: String,
    pub item_type_id: Option<i64>,
    pub total_quantity: i64,
    pub unit_cost: Option<i64>,
    pub total_cost: Option<i64>,
}

/// Costs / revenue / profit rollup for one session. Values are in the same
/// units as `market_values.market_value` (copper / gold fractions depending
/// on how the user enters prices — the UI passes them through unchanged).
///
/// `revenue_total` only counts items whose `market_values` row is set.
/// `items_priced` vs `items_unpriced` lets the UI explain "profit is low-
/// balled because N items have no market price set".
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionEconomics {
    pub cost_total: i64,
    pub revenue_total: i64,
    pub bonus_revenue_total: i64,
    pub profit_total: i64,
    pub items_priced: u32,
    pub items_unpriced: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LootSummaryRow {
    pub item_name: String,
    pub item_type_id: Option<i64>,
    pub total_qty: i64,
    /// Share of `total_qty` from non-bonus drops. Only differs from
    /// `total_qty` for items that appeared as speed-bonus drops.
    pub primary_qty: i64,
    /// Share of `total_qty` from speed-bonus drops.
    pub bonus_qty: i64,
    /// Unit value computed by the backend using the user's
    /// `item_valuation_mode` at query time. The frontend may override
    /// this reactively from `marketStore` for live updates; this value
    /// serves as the initial snapshot.
    pub unit_value: Option<i64>,
    /// `total_qty * unit_value`. `None` when `unit_value` is unknown.
    pub total_value: Option<i64>,
}

// ============================================================
// Commands
// ============================================================

/// Live tracker status. Returns the active session (if any) and currently-
/// open multihit nodes for the active character/server.
#[tauri::command]
pub fn survey_tracker_status(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<SurveyTrackerStatus, String> {
    let (character, server, conn) = {
        let coord = coordinator.lock().map_err(|e| e.to_string())?;
        let (character, server) = match coord.active_character_server() {
            Some((c, s)) => (c, s),
            None => {
                // No active character — there can be no active session either.
                return Ok(SurveyTrackerStatus {
                    active_session: None,
                    open_multihit_nodes: Vec::new(),
                });
            }
        };
        let conn = coord.db_pool().get().map_err(|e| e.to_string())?;
        (character, server, conn)
    };
    let active_session =
        persistence::active_session(&conn, &character, &server).map_err(|e| e.to_string())?;
    let open_multihit_nodes =
        list_multihit_summaries(&conn, &character, &server).map_err(|e| e.to_string())?;
    Ok(SurveyTrackerStatus {
        active_session,
        open_multihit_nodes,
    })
}

/// Manually start a session. Errors if one is already active.
#[tauri::command]
pub fn survey_tracker_start_session(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<i64, String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    let (character, server) = coord
        .active_character_server()
        .ok_or_else(|| "no active character".to_string())?;
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let conn = coord.db_pool().get_write().map_err(|e| e.to_string())?;
    coord
        .survey_aggregator_mut()
        .start_manual_session(&conn, &character, &server, &now)
}

/// Manually end the currently-active session. No-op if none is active.
/// Returns the id of the ended session, if any.
#[tauri::command]
pub fn survey_tracker_end_session(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<Option<i64>, String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    let (character, server) = match coord.active_character_server() {
        Some(cs) => cs,
        None => return Ok(None),
    };
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut conn = coord.db_pool().get_write().map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let r = coord
        .survey_aggregator_mut()
        .end_active_session(&tx, &character, &server, &now)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(r)
}

/// Result of recording a hand-framed survey-map use.
#[derive(Debug, Clone, Serialize)]
pub struct RecordUseResult {
    pub use_id: i64,
    pub session_id: Option<i64>,
}

/// Record a hand-framed survey-map use (manual "Record use" UI control).
///
/// Validates the map name against `survey_types`, resolves the kind from the
/// explicit `kind` override (or CDN lookup when omitted), and routes through
/// the aggregator so in-memory caches stay coherent. Returns the new use id
/// plus the (possibly newly-created) session id.
#[tauri::command]
pub fn survey_tracker_record_use(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
    map_internal_name: String,
    kind: Option<String>,
) -> Result<RecordUseResult, String> {
    let map_internal_name = map_internal_name.trim().to_string();
    if map_internal_name.is_empty() {
        return Err("no survey map selected".to_string());
    }
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    let (character, server) = coord
        .active_character_server()
        .ok_or_else(|| "no active character".to_string())?;
    // Area from the persisted live-tracked area (coordinator's private
    // current_area mirrors this table on every AreaTransition).
    let conn = coord.db_pool().get_write().map_err(|e| e.to_string())?;
    let live_area: Option<String> = conn
        .query_row(
            "SELECT area_name FROM game_state_area WHERE character_name = ?1 AND server_name = ?2",
            params![character, server],
            |r| r.get(0),
        )
        .ok();
    // Map must exist in the survey catalog.
    let known: bool = conn
        .query_row(
            "SELECT 1 FROM survey_types WHERE internal_name = ?1",
            params![map_internal_name],
            |r| r.get::<_, i64>(0),
        )
        .is_ok();
    if !known {
        return Err(format!("unknown survey map '{map_internal_name}'"));
    }
    // Kind: explicit override wins; otherwise derive from CDN; otherwise the
    // caller must say which kind it is.
    let use_kind = match kind.as_deref().map(str::trim) {
        Some(k) if !k.is_empty() => crate::survey::types::SurveyUseKind::from_str(
            &k.to_ascii_lowercase(),
        )
        .ok_or_else(|| {
            format!("unknown survey kind '{k}' (expected basic, motherlode, or multihit)")
        })?,
        _ => coord
            .survey_aggregator_mut()
            .lookup_survey_kind(&map_internal_name)
            .ok_or_else(|| {
                format!("unknown survey map '{map_internal_name}': pass kind explicitly")
            })?,
    };
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let (use_id, session_id) = coord.survey_aggregator_mut().record_manual_use(
        &conn,
        crate::survey::aggregator::ManualUse {
            character: &character,
            server: &server,
            area: live_area.as_deref(),
            map_internal_name: &map_internal_name,
            kind: use_kind,
            now_iso: &now,
        },
    )?;
    Ok(RecordUseResult {
        use_id,
        session_id,
    })
}

/// Recent completed (or current) sessions for the active character/server,
/// most-recent first.
#[tauri::command]
pub fn survey_tracker_recent_sessions(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
    limit: Option<u32>,
) -> Result<Vec<SurveySession>, String> {
    let (character, server, conn) = {
        let coord = coordinator.lock().map_err(|e| e.to_string())?;
        let (character, server) = match coord.active_character_server() {
            Some(cs) => cs,
            None => return Ok(Vec::new()),
        };
        let conn = coord.db_pool().get().map_err(|e| e.to_string())?;
        (character, server, conn)
    };
    let limit = limit.unwrap_or(20).min(200);
    list_recent_sessions(&conn, &character, &server, limit).map_err(|e| e.to_string())
}

/// Full detail for one session: header + uses + per-item loot totals +
/// precomputed economics rollup.
#[tauri::command]
pub fn survey_tracker_session_detail(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    session_id: i64,
) -> Result<Option<SurveySessionDetail>, String> {
    // Acquire a pooled connection, then release the coordinator lock before
    // running the read queries. Log ingestion contends on the same
    // coordinator mutex, so holding it across a large query would stall
    // ingestion (and freeze the UI). WAL + the connection pool make
    // concurrent reads safe without the lock.
    let conn = {
        let coord = coordinator.lock().map_err(|e| e.to_string())?;
        coord.db_pool().get().map_err(|e| e.to_string())?
    };
    let valuation_mode = settings_manager.get().item_valuation_mode.clone();

    let Some(session) = persistence::get_session(&conn, session_id).map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let server = session.server_name.clone();
    let uses = persistence::uses_for_session(&conn, session_id).map_err(|e| e.to_string())?;
    let loot_summary = loot_summary_for_session(&conn, session_id, &server, &valuation_mode)
        .map_err(|e| e.to_string())?;
    let economics =
        economics_from_loot_and_uses(&conn, session_id, &server, &valuation_mode, &loot_summary)
            .map_err(|e| e.to_string())?;

    let craft_materials = craft_materials_for_session(&conn, session_id, &server, &valuation_mode)
        .map_err(|e| e.to_string())?;

    Ok(Some(SurveySessionDetail {
        session,
        uses,
        loot_summary,
        economics,
        craft_materials,
    }))
}

// ============================================================
// SQL helpers
// ============================================================

fn list_multihit_summaries<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> SqlResult<Vec<MultihitSummary>> {
    // Join to survey_uses for the map display name + per-use loot total.
    let mut stmt = conn.prepare(
        "SELECT n.node_entity_id, n.survey_use_id, u.map_display_name,
                n.opened_at, n.last_hit_at, u.loot_qty
         FROM open_multihit_nodes n
         JOIN survey_uses u ON u.id = n.survey_use_id
         WHERE n.character_name = ?1 AND n.server_name = ?2
         ORDER BY n.last_hit_at ASC",
    )?;
    let rows = stmt.query_map(params![character, server], |row| {
        Ok(MultihitSummary {
            node_entity_id: row.get(0)?,
            survey_use_id: row.get(1)?,
            map_display_name: row.get(2)?,
            opened_at: row.get(3)?,
            last_hit_at: row.get(4)?,
            loot_qty: row.get::<_, i64>(5)? as u32,
        })
    })?;
    rows.collect()
}

fn list_recent_sessions<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
    limit: u32,
) -> SqlResult<Vec<SurveySession>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM survey_sessions \
         WHERE character_name = ?1 AND server_name = ?2 \
         ORDER BY id DESC LIMIT ?3",
        persistence::SESSION_COLS,
    ))?;
    let rows = stmt.query_map(params![character, server, limit], |row| {
        persistence::row_to_session_offset(row, 0)
    })?;
    rows.collect()
}

/// Per-item loot totals across all uses in one session. Pulls from
/// `item_transactions` filtered by `source_details->>'survey_use_id'`
/// matching any use.id in the session.
///
/// Each row carries the primary/bonus split (from `is_speed_bonus`) and
/// the per-item market value from `market_values` when available. Revenue
/// rollups use these values; rows with no market value contribute zero
/// revenue and are flagged via `unit_value = None` so the UI can surface
/// "price missing" hints.
///
/// The `json_extract` index path uses SQLite's JSON1 functions. The query
/// is bounded by session size so even noisy 100x sessions complete in
/// milliseconds.
fn loot_summary_for_session<C: DbRead + ?Sized>(
    conn: &C,
    session_id: i64,
    server: &str,
    valuation_mode: &str,
) -> SqlResult<Vec<LootSummaryRow>> {
    // Join both market_values (user-set prices) AND items (CDN vendor
    // value) so we can apply the user's valuation-mode preference via
    // `resolve_item_value(mode, vendor, market)`.
    let mut stmt = conn.prepare(
        "SELECT t.item_name,
                SUM(t.quantity) AS total_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN 0 ELSE t.quantity END) AS primary_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN t.quantity ELSE 0 END) AS bonus_qty,
                mv.market_value,
                i.value AS vendor_value,
                t.item_type_id
         FROM item_transactions t
         JOIN survey_uses u
           ON u.session_id = ?1
          AND u.id = t.survey_use_id
         LEFT JOIN market_values mv
           ON mv.server_name = ?2 AND mv.item_type_id = t.item_type_id
         LEFT JOIN items i
           ON i.id = t.item_type_id
         WHERE t.quantity > 0
           -- Chat-authoritative: when a use has chat-sourced loot rows
           -- ('survey_chat'), count only those; otherwise fall back to the
           -- Player.log-attributed rows (historical sessions recorded before
           -- the chat path existed). This prevents double-counting a gain that
           -- both pipelines saw while still rendering legacy data.
           AND (
             t.source_kind = 'survey_chat'
             OR NOT EXISTS (
               SELECT 1 FROM item_transactions c
                WHERE c.source_kind = 'survey_chat'
                  AND c.survey_use_id = u.id
             )
           )
         GROUP BY t.item_name
         ORDER BY total_qty DESC",
    )?;
    let rows = stmt.query_map(params![session_id, server], |row| {
        let total_qty: i64 = row.get(1)?;
        let market_value: Option<f64> = row.get::<_, Option<i64>>(4)?.map(|v| v as f64);
        let vendor_value: Option<f64> = row.get(5)?;
        let item_type_id: Option<i64> = row.get(6)?;

        // Apply the user's valuation mode. If neither vendor nor market is
        // known, unit_value stays None and the UI shows "no price".
        let unit_value = match (vendor_value, market_value) {
            (Some(v), Some(m)) => Some(resolve_item_value(valuation_mode, v, m).round() as i64),
            (Some(v), None) => Some(resolve_item_value(valuation_mode, v, 0.0).round() as i64),
            (None, Some(m)) => Some(resolve_item_value(valuation_mode, 0.0, m).round() as i64),
            (None, None) => None,
        };
        let total_value = unit_value.map(|v| v * total_qty);

        Ok(LootSummaryRow {
            item_name: row.get(0)?,
            item_type_id,
            total_qty,
            primary_qty: row.get(2)?,
            bonus_qty: row.get(3)?,
            unit_value,
            total_value,
        })
    })?;
    rows.collect()
}

/// Cost of all survey maps consumed in a session, computed from the
/// recipe ingredients using the user's item-valuation-mode preference.
///
/// For each survey use, looks up the recipe via `survey_types.recipe_id`,
/// then sums `resolve_item_value(mode, vendor, market) × stack_size` for
/// each fully-consumed ingredient (those with `chance_to_consume` NULL or
/// 1.0). Falls back to `survey_types.crafting_cost` (the CDN-precomputed
/// vendor-buy number) if no recipe is found.
fn session_cost_from_recipes<C: DbRead + ?Sized>(
    conn: &C,
    session_id: i64,
    server: &str,
    valuation_mode: &str,
) -> SqlResult<i64> {
    // Per-use: recipe_id from survey_types. One row per use.
    let mut use_stmt = conn.prepare(
        "SELECT u.id, st.recipe_id, st.crafting_cost
           FROM survey_uses u
           LEFT JOIN survey_types st ON st.internal_name = u.map_internal_name
          WHERE u.session_id = ?1",
    )?;

    // Per-recipe: ingredients with their vendor + market values.
    let mut ing_stmt = conn.prepare(
        "SELECT ri.item_id, ri.stack_size, ri.chance_to_consume,
                i.value AS vendor_value,
                mv.market_value
           FROM recipe_ingredients ri
           LEFT JOIN items i ON i.id = ri.item_id
           LEFT JOIN market_values mv ON mv.server_name = ?2 AND mv.item_type_id = ri.item_id
          WHERE ri.recipe_id = ?1
            AND (ri.chance_to_consume IS NULL OR ri.chance_to_consume >= 1.0)",
    )?;

    let mut total_cost: f64 = 0.0;

    let uses: Vec<(i64, Option<i64>, Option<f64>)> = use_stmt
        .query_map(params![session_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<SqlResult<_>>()?;

    for (_use_id, recipe_id, fallback_cost) in &uses {
        let Some(recipe_id) = recipe_id else {
            // No recipe found — use the CDN-precomputed fallback.
            total_cost += fallback_cost.unwrap_or(0.0);
            continue;
        };

        // One recipe-ingredient row: (item_id, stack_size, vendor_value, market_value).
        type IngredientRow = (Option<i64>, i64, Option<f64>, Option<f64>);
        let ingredients: Vec<IngredientRow> = ing_stmt
            .query_map(params![recipe_id, server], |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?, // item_id
                    r.get::<_, i64>(1)?,         // stack_size
                    r.get::<_, Option<f64>>(3)?, // vendor_value
                    r.get::<_, Option<f64>>(4)?, // market_value
                ))
            })?
            .collect::<SqlResult<_>>()?;

        if ingredients.is_empty() {
            total_cost += fallback_cost.unwrap_or(0.0);
            continue;
        }

        let mut recipe_cost: f64 = 0.0;
        for (_item_id, stack_size, vendor, market) in &ingredients {
            let unit_price = match (*vendor, *market) {
                (Some(v), Some(m)) => resolve_item_value(valuation_mode, v, m),
                (Some(v), None) => resolve_item_value(valuation_mode, v, 0.0),
                (None, Some(m)) => resolve_item_value(valuation_mode, 0.0, m),
                (None, None) => 0.0,
            };
            recipe_cost += unit_price * (*stack_size as f64);
        }
        total_cost += recipe_cost;
    }

    Ok(total_cost.round() as i64)
}

/// Craft material breakdown for a session. Groups recipe ingredients
/// across all uses by item, summing quantities and applying the user's
/// valuation mode for per-unit pricing.
fn craft_materials_for_session<C: DbRead + ?Sized>(
    conn: &C,
    session_id: i64,
    server: &str,
    valuation_mode: &str,
) -> SqlResult<Vec<CraftMaterialRow>> {
    // One row per (ingredient item, recipe_id). We group by item across
    // all recipe_ids used in the session, multiplying per-recipe stack_size
    // by the number of uses of that recipe's survey type.
    let mut stmt = conn.prepare(
        "SELECT i.name, ri.item_id, ri.stack_size, COUNT(*) AS use_count,
                i.value AS vendor_value, mv.market_value
           FROM survey_uses u
           JOIN survey_types st ON st.internal_name = u.map_internal_name
           JOIN recipe_ingredients ri ON ri.recipe_id = st.recipe_id
           LEFT JOIN items i ON i.id = ri.item_id
           LEFT JOIN market_values mv ON mv.server_name = ?2 AND mv.item_type_id = ri.item_id
          WHERE u.session_id = ?1
            AND (ri.chance_to_consume IS NULL OR ri.chance_to_consume >= 1.0)
          GROUP BY ri.item_id
          ORDER BY i.name",
    )?;
    let rows = stmt.query_map(params![session_id, server], |row| {
        let item_name: String = row
            .get::<_, Option<String>>(0)?
            .unwrap_or_else(|| "Unknown".to_string());
        let item_type_id: Option<i64> = row.get(1)?;
        let stack_size: i64 = row.get(2)?;
        let use_count: i64 = row.get(3)?;
        let vendor_value: Option<f64> = row.get(4)?;
        let market_value: Option<f64> = row.get::<_, Option<i64>>(5)?.map(|v| v as f64);

        let total_quantity = stack_size * use_count;
        let unit_cost = match (vendor_value, market_value) {
            (Some(v), Some(m)) => Some(resolve_item_value(valuation_mode, v, m).round() as i64),
            (Some(v), None) => Some(resolve_item_value(valuation_mode, v, 0.0).round() as i64),
            (None, Some(m)) => Some(resolve_item_value(valuation_mode, 0.0, m).round() as i64),
            (None, None) => None,
        };
        let total_cost = unit_cost.map(|uc| uc * total_quantity);

        Ok(CraftMaterialRow {
            item_name,
            item_type_id,
            total_quantity,
            unit_cost,
            total_cost,
        })
    })?;
    rows.collect()
}

/// Compose the `SessionEconomics` rollup from a precomputed loot summary
/// plus a fresh survey-types cost query. Splitting it out keeps the
/// historical-row query (which wants economics for many sessions in one
/// trip) from having to re-run the full loot summary per session.
fn economics_from_loot_and_uses<C: DbRead + ?Sized>(
    conn: &C,
    session_id: i64,
    server: &str,
    valuation_mode: &str,
    loot: &[LootSummaryRow],
) -> SqlResult<SessionEconomics> {
    let cost_total = session_cost_from_recipes(conn, session_id, server, valuation_mode)?;
    let revenue_total: i64 = loot.iter().filter_map(|r| r.total_value).sum();
    let bonus_revenue_total: i64 = loot
        .iter()
        .filter_map(|r| r.unit_value.map(|v| v * r.bonus_qty))
        .sum();
    let items_priced = loot.iter().filter(|r| r.unit_value.is_some()).count() as u32;
    let items_unpriced = loot.iter().filter(|r| r.unit_value.is_none()).count() as u32;
    Ok(SessionEconomics {
        cost_total,
        revenue_total,
        bonus_revenue_total,
        profit_total: revenue_total - cost_total,
        items_priced,
        items_unpriced,
    })
}

// ============================================================
// Historical: per-session rollups for the History tab
// ============================================================

/// One row in the History tab's session list. Wraps `SurveySession` with
/// derived per-kind use counts and a duration computed from
/// `started_at`/`ended_at`. Active (un-ended) sessions report `None` for
/// `duration_seconds`.
#[derive(Debug, Clone, Serialize)]
pub struct HistoricalSessionRow {
    pub session: SurveySession,
    /// Total `survey_uses` rows belonging to this session (any kind, any status).
    pub total_uses: u32,
    pub basic_uses: u32,
    pub motherlode_uses: u32,
    pub multihit_uses: u32,
    /// Sum of `loot_qty` across all uses in this session.
    pub total_loot_qty: u32,
    /// `ended_at - started_at` in seconds; `None` for active sessions.
    pub duration_seconds: Option<i64>,
    /// Precomputed economics snapshot — the frontend may re-derive
    /// revenue/profit reactively from `loot_summary` + market prices.
    pub economics: SessionEconomics,
    /// Per-item loot totals with `item_type_id` so the frontend can
    /// reactively re-price items when market values change.
    pub loot_summary: Vec<LootSummaryRow>,
    /// Distinct area keys from this session's survey uses.
    pub zones: Vec<String>,
}

/// Recent sessions with derived rollup stats. Ordered most-recent first.
#[tauri::command]
pub fn survey_tracker_historical_sessions(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    limit: Option<u32>,
) -> Result<Vec<HistoricalSessionRow>, String> {
    // See survey_tracker_session_detail: grab the connection + identity, then
    // drop the coordinator lock before the (heavier) rollup queries so a slow
    // read can't stall log ingestion.
    let (character, server, conn) = {
        let coord = coordinator.lock().map_err(|e| e.to_string())?;
        let (character, server) = match coord.active_character_server() {
            Some(cs) => cs,
            None => return Ok(Vec::new()),
        };
        let conn = coord.db_pool().get().map_err(|e| e.to_string())?;
        (character, server, conn)
    };
    let limit = limit.unwrap_or(50).min(500);
    let valuation_mode = settings_manager.get().item_valuation_mode.clone();
    historical_session_rows(&conn, &character, &server, &valuation_mode, limit)
        .map_err(|e| e.to_string())
}

fn historical_session_rows<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
    valuation_mode: &str,
    limit: u32,
) -> SqlResult<Vec<HistoricalSessionRow>> {
    // Group-by query that pulls everything in one trip. Includes sessions
    // with zero uses (LEFT JOIN) so a manually-started empty session still
    // appears in the list.
    // Session columns (0-15) then aggregate columns (16-19).
    let mut stmt = conn.prepare(
        "SELECT s.id, s.character_name, s.server_name, s.started_at, s.ended_at,
                s.start_trigger, s.crafted_count, s.consumed_count, s.notes,
                s.name, s.user_started_at, s.user_ended_at,
                s.first_craft_at, s.last_craft_at, s.first_loot_at, s.last_loot_at,
                COUNT(u.id) AS total_uses,
                SUM(CASE WHEN u.kind = 'basic'      THEN 1 ELSE 0 END) AS basic_uses,
                SUM(CASE WHEN u.kind = 'motherlode' THEN 1 ELSE 0 END) AS motherlode_uses,
                SUM(CASE WHEN u.kind = 'multihit'   THEN 1 ELSE 0 END) AS multihit_uses,
                COALESCE(SUM(u.loot_qty), 0) AS total_loot_qty
         FROM survey_sessions s
         LEFT JOIN survey_uses u ON u.session_id = s.id
         WHERE s.character_name = ?1 AND s.server_name = ?2
         GROUP BY s.id
         ORDER BY s.id DESC
         LIMIT ?3",
    )?;
    let partial: Vec<(HistoricalSessionRow, String)> = stmt
        .query_map(params![character, server, limit], |row| {
            let session = persistence::row_to_session_offset(row, 0)?;
            let eff_start = session
                .user_started_at
                .as_deref()
                .unwrap_or(&session.started_at);
            let eff_end = session
                .user_ended_at
                .as_deref()
                .or(session.ended_at.as_deref());
            let duration = compute_duration_seconds(eff_start, eff_end);
            let server_name = session.server_name.clone();
            Ok((
                HistoricalSessionRow {
                    session,
                    total_uses: row.get::<_, i64>(16)? as u32,
                    basic_uses: row.get::<_, i64>(17)? as u32,
                    motherlode_uses: row.get::<_, i64>(18)? as u32,
                    multihit_uses: row.get::<_, i64>(19)? as u32,
                    total_loot_qty: row.get::<_, i64>(20)? as u32,
                    duration_seconds: duration,
                    economics: SessionEconomics {
                        cost_total: 0,
                        revenue_total: 0,
                        bonus_revenue_total: 0,
                        profit_total: 0,
                        items_priced: 0,
                        items_unpriced: 0,
                    },
                    loot_summary: Vec::new(),
                    zones: Vec::new(),
                },
                server_name,
            ))
        })?
        .collect::<SqlResult<_>>()?;

    // Enrich each row with its economics rollup. One loot-summary query +
    // one cost query per session — both indexed, both bounded by session
    // size. For the default 50-session history list this is still cheap.
    let mut zones_stmt = conn.prepare(
        "SELECT DISTINCT area FROM survey_uses
         WHERE session_id = ?1 AND area IS NOT NULL
         ORDER BY area",
    )?;

    let mut out = Vec::with_capacity(partial.len());
    for (mut row, server_name) in partial {
        let loot = loot_summary_for_session(conn, row.session.id, &server_name, valuation_mode)?;
        row.economics = economics_from_loot_and_uses(
            conn,
            row.session.id,
            &server_name,
            valuation_mode,
            &loot,
        )?;
        row.loot_summary = loot;
        row.zones = zones_stmt
            .query_map(params![row.session.id], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        out.push(row);
    }
    Ok(out)
}

// session_row_to_struct_offset is now provided by
// persistence::row_to_session_offset — all callers use that directly.

fn compute_duration_seconds(started_at: &str, ended_at: Option<&str>) -> Option<i64> {
    use chrono::NaiveDateTime;
    let end = ended_at?;
    let s = NaiveDateTime::parse_from_str(started_at, "%Y-%m-%d %H:%M:%S").ok()?;
    let e = NaiveDateTime::parse_from_str(end, "%Y-%m-%d %H:%M:%S").ok()?;
    let diff = e.signed_duration_since(s).num_seconds();
    if diff < 0 {
        None
    } else {
        Some(diff)
    }
}

/// Update a session's `notes` field. Used by the History tab's editable
/// notes textarea. Returns `Err` if the session doesn't exist.
///
/// Never touches the coordinator or holds its mutex — goes straight to the
/// write pool, so an UPDATE queued behind other writers never stalls log
/// ingestion. `survey_tracker_update_session_name` and
/// `survey_tracker_update_session_times` share this property.
#[tauri::command]
pub fn survey_tracker_update_session_notes(
    db: State<'_, crate::db::DbPool>,
    session_id: i64,
    notes: String,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    let updated = conn
        .execute(
            "UPDATE survey_sessions SET notes = ?2 WHERE id = ?1",
            params![session_id, notes],
        )
        .map_err(|e| e.to_string())?;
    if updated == 0 {
        Err(format!("no session with id {session_id}"))
    } else {
        Ok(())
    }
}

/// Update a session's user-facing name.
#[tauri::command]
pub fn survey_tracker_update_session_name(
    db: State<'_, crate::db::DbPool>,
    session_id: i64,
    name: String,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    persistence::update_session_name(&conn, session_id, &name).map_err(|e| e.to_string())
}

/// Update user-adjusted start/end time overrides. Pass `None` to clear
/// an override (reverts to the computed bounds from event timestamps).
#[tauri::command]
pub fn survey_tracker_update_session_times(
    db: State<'_, crate::db::DbPool>,
    session_id: i64,
    user_started_at: Option<String>,
    user_ended_at: Option<String>,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    persistence::update_session_user_times(
        &conn,
        session_id,
        user_started_at.as_deref(),
        user_ended_at.as_deref(),
    )
    .map_err(|e| e.to_string())
}

/// Delete a session and all its `survey_uses` (FK SET NULL detaches them but
/// the History view filters by session_id so detached rows disappear from
/// per-session breakdowns; the user-visible effect is "session and its data
/// gone"). The associated item_transactions are NOT deleted — they stay in
/// the ledger as historical record, just unattached from any session.
#[tauri::command]
pub fn survey_tracker_delete_session(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
    session_id: i64,
) -> Result<(), String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    let mut conn = coord.db_pool().get_write().map_err(|e| e.to_string())?;
    delete_session_impl(&mut conn, session_id)?;
    drop(conn);
    // The aggregator caches the active-session id. If we just deleted that
    // session, the cache is now stale — the next survey use would insert
    // against a missing session id and fail the FK. Invalidate so it
    // re-resolves (and auto-starts a fresh session) on the next use.
    coord.survey_aggregator_mut().invalidate_session_cache();
    Ok(())
}

/// Delete a session's uses then the session row in one transaction. Split out
/// from the command (which also invalidates the aggregator cache) so the
/// atomicity is unit-testable.
fn delete_session_impl(conn: &mut rusqlite::Connection, session_id: i64) -> Result<(), String> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    // Delete uses first (no cascade defined). Then delete the session header.
    tx.execute(
        "DELETE FROM survey_uses WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM survey_sessions WHERE id = ?1",
        params![session_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================
// Analytics: per-zone, per-survey-type, per-item rollups
// ============================================================

/// One row of the per-zone analytics table. Derived from `survey_uses`
/// grouped by `area`. Lists totals and per-kind counts.
///
/// Speed-bonus fields are Basic-only:
/// - `basic_uses_with_bonus` counts distinct Basic uses that received at
///   least one bonus-flagged transaction.
/// - `bonus_items_total` sums the quantities of all bonus-flagged
///   transactions attributed to this zone's Basic uses.
#[derive(Debug, Clone, Serialize)]
pub struct ZoneSummary {
    pub area: String,
    pub total_uses: u32,
    pub basic_uses: u32,
    pub motherlode_uses: u32,
    pub multihit_uses: u32,
    pub total_loot_qty: u32,
    pub basic_uses_with_bonus: u32,
    pub bonus_items_total: u32,
    /// Per-item totals for this zone, joined via `survey_use_id`.
    /// Sorted by `total_qty DESC`. Enables the zone-detail drill-in view.
    pub items: Vec<ItemSummary>,
}

/// One row of the per-survey-type analytics table. Grouped by
/// `(map_internal_name, area)` so the same map name across two areas
/// stays distinct (e.g., Astounding Metal Motherlode in Ilmari vs Gazluk).
#[derive(Debug, Clone, Serialize)]
pub struct SurveyTypeSummary {
    pub map_internal_name: String,
    pub map_display_name: String,
    pub area: Option<String>,
    pub kind: String,
    pub total_uses: u32,
    pub total_loot_qty: u32,
    /// Average loot quantity per use. None when total_uses is 0.
    pub avg_loot_per_use: Option<f64>,
    /// Basic-only. Distinct uses that received at least one bonus drop.
    /// Always 0 for Motherlode/Multihit.
    pub uses_with_bonus: u32,
    /// Basic-only. Total bonus-flagged item quantity for this type.
    pub bonus_items_total: u32,
    /// Per-item totals for this survey type, joined via `survey_use_id`.
    /// Sorted by `total_qty DESC`. Enables the type-detail drill-in view.
    pub items: Vec<ItemSummary>,
}

/// One row of the per-item analytics table. Grouped by item across the
/// entire ledger of survey-attributed gains.
///
/// `primary_qty` + `bonus_qty` == `total_qty`. `bonus_qty` is only non-zero
/// for items that have appeared as a speed-bonus drop on a Basic survey.
#[derive(Debug, Clone, Serialize)]
pub struct ItemSummary {
    pub item_name: String,
    pub total_qty: i64,
    pub primary_qty: i64,
    pub bonus_qty: i64,
    pub times_received: i64,
}

/// Top-level analytics payload. The frontend receives this once and
/// renders all three sections from it.
///
/// `basic_uses_with_bonus` / `bonus_items_total` are Basic-only rollups;
/// `speed_bonus_rate` = `basic_uses_with_bonus / total_basic_uses` (0..1).
#[derive(Debug, Clone, Serialize)]
pub struct SurveyAnalytics {
    pub zones: Vec<ZoneSummary>,
    pub survey_types: Vec<SurveyTypeSummary>,
    pub items: Vec<ItemSummary>,
    pub total_sessions: u32,
    pub total_uses: u32,
    pub total_basic_uses: u32,
    pub basic_uses_with_bonus: u32,
    pub bonus_items_total: u32,
}

/// Aggregate analytics for the active character/server. All-time today —
/// future enhancement could add a date-range filter parameter.
#[tauri::command]
pub fn survey_tracker_analytics(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<SurveyAnalytics, String> {
    // See survey_tracker_session_detail: release the coordinator lock before
    // the analytics rollups so they don't stall log ingestion.
    let (character, server, conn) = {
        let coord = coordinator.lock().map_err(|e| e.to_string())?;
        let (character, server) = match coord.active_character_server() {
            Some(cs) => cs,
            None => {
                return Ok(SurveyAnalytics {
                    zones: vec![],
                    survey_types: vec![],
                    items: vec![],
                    total_sessions: 0,
                    total_uses: 0,
                    total_basic_uses: 0,
                    basic_uses_with_bonus: 0,
                    bonus_items_total: 0,
                });
            }
        };
        let conn = coord.db_pool().get().map_err(|e| e.to_string())?;
        (character, server, conn)
    };

    let zones = zone_summaries(&conn, &character, &server).map_err(|e| e.to_string())?;
    let survey_types =
        survey_type_summaries(&conn, &character, &server).map_err(|e| e.to_string())?;
    let items = item_summaries(&conn, &character, &server).map_err(|e| e.to_string())?;

    let total_sessions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM survey_sessions WHERE character_name = ?1 AND server_name = ?2",
            params![character, server],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_uses: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM survey_uses WHERE character_name = ?1 AND server_name = ?2",
            params![character, server],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_basic_uses: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM survey_uses
             WHERE character_name = ?1 AND server_name = ?2 AND kind = 'basic'",
            params![character, server],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Basic uses with at least one bonus drop + total bonus items across all.
    let (basic_uses_with_bonus, bonus_items_total): (i64, i64) = conn
        .query_row(
            "WITH bonus_per_use AS (
                SELECT u.id AS use_id, SUM(t.quantity) AS bonus_qty
                  FROM item_transactions t
                  JOIN survey_uses u
                    ON u.id = t.survey_use_id
                 WHERE u.character_name = ?1 AND u.server_name = ?2
                   AND json_extract(t.source_details, '$.is_speed_bonus') = 1
                   AND t.quantity > 0
                 GROUP BY u.id
             )
             SELECT COUNT(*), COALESCE(SUM(bonus_qty), 0) FROM bonus_per_use",
            params![character, server],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 0));

    Ok(SurveyAnalytics {
        zones,
        survey_types,
        items,
        total_sessions: total_sessions as u32,
        total_uses: total_uses as u32,
        total_basic_uses: total_basic_uses as u32,
        basic_uses_with_bonus: basic_uses_with_bonus as u32,
        bonus_items_total: bonus_items_total as u32,
    })
}

fn zone_summaries<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> SqlResult<Vec<ZoneSummary>> {
    // Bonus stats join item_transactions to survey_uses through the
    // survey_use_id in source_details, filter to is_speed_bonus, and
    // aggregate by (area, use). Left-joined into the per-area rollup so
    // zones with no bonuses still appear with zeros.
    let mut stmt = conn.prepare(
        "WITH bonus_per_use AS (
            SELECT u.id AS use_id,
                   COALESCE(u.area, '(unknown)') AS area_label,
                   SUM(t.quantity) AS bonus_qty
              FROM item_transactions t
              JOIN survey_uses u
                ON u.id = t.survey_use_id
             WHERE u.character_name = ?1 AND u.server_name = ?2
               AND json_extract(t.source_details, '$.is_speed_bonus') = 1
               AND t.quantity > 0
             GROUP BY u.id
         ), bonus_per_area AS (
             SELECT area_label,
                    COUNT(*) AS uses_with_bonus,
                    COALESCE(SUM(bonus_qty), 0) AS bonus_items_total
               FROM bonus_per_use
              GROUP BY area_label
         )
         SELECT COALESCE(u.area, '(unknown)') AS area_label,
                COUNT(*),
                SUM(CASE WHEN u.kind = 'basic'      THEN 1 ELSE 0 END),
                SUM(CASE WHEN u.kind = 'motherlode' THEN 1 ELSE 0 END),
                SUM(CASE WHEN u.kind = 'multihit'   THEN 1 ELSE 0 END),
                COALESCE(SUM(u.loot_qty), 0),
                COALESCE(MAX(b.uses_with_bonus), 0),
                COALESCE(MAX(b.bonus_items_total), 0)
           FROM survey_uses u
           LEFT JOIN bonus_per_area b ON b.area_label = COALESCE(u.area, '(unknown)')
          WHERE u.character_name = ?1 AND u.server_name = ?2
          GROUP BY COALESCE(u.area, '(unknown)')
          ORDER BY area_label",
    )?;
    let rows: Vec<ZoneSummary> = stmt
        .query_map(params![character, server], |row| {
            Ok(ZoneSummary {
                area: row.get(0)?,
                total_uses: row.get::<_, i64>(1)? as u32,
                basic_uses: row.get::<_, i64>(2)? as u32,
                motherlode_uses: row.get::<_, i64>(3)? as u32,
                multihit_uses: row.get::<_, i64>(4)? as u32,
                total_loot_qty: row.get::<_, i64>(5)? as u32,
                basic_uses_with_bonus: row.get::<_, i64>(6)? as u32,
                bonus_items_total: row.get::<_, i64>(7)? as u32,
                items: Vec::new(),
            })
        })?
        .collect::<SqlResult<_>>()?;

    // Enrich each zone with its per-item breakdown. One extra query per
    // zone; indexed and tiny in practice (typical: 2-5 zones per character).
    let mut out = Vec::with_capacity(rows.len());
    for mut z in rows {
        z.items = items_for_zone(conn, character, server, &z.area)?;
        out.push(z);
    }
    Ok(out)
}

fn survey_type_summaries<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> SqlResult<Vec<SurveyTypeSummary>> {
    // Same pattern as zone_summaries — bonus metrics come from a LEFT JOIN
    // onto a CTE that counts bonus-flagged transactions per survey use.
    let mut stmt = conn.prepare(
        "WITH bonus_per_use AS (
            SELECT u.id AS use_id,
                   u.map_internal_name,
                   u.area,
                   SUM(t.quantity) AS bonus_qty
              FROM item_transactions t
              JOIN survey_uses u
                ON u.id = t.survey_use_id
             WHERE u.character_name = ?1 AND u.server_name = ?2
               AND json_extract(t.source_details, '$.is_speed_bonus') = 1
               AND t.quantity > 0
             GROUP BY u.id
         ), bonus_per_type AS (
             SELECT map_internal_name, area,
                    COUNT(*) AS uses_with_bonus,
                    COALESCE(SUM(bonus_qty), 0) AS bonus_items_total
               FROM bonus_per_use
              GROUP BY map_internal_name, area
         )
         SELECT u.map_internal_name, u.map_display_name, u.area, u.kind,
                COUNT(*) AS uses,
                COALESCE(SUM(u.loot_qty), 0) AS qty,
                COALESCE(MAX(b.uses_with_bonus), 0),
                COALESCE(MAX(b.bonus_items_total), 0)
           FROM survey_uses u
           LEFT JOIN bonus_per_type b
             ON b.map_internal_name = u.map_internal_name
            AND ((b.area IS NULL AND u.area IS NULL) OR b.area = u.area)
          WHERE u.character_name = ?1 AND u.server_name = ?2
          GROUP BY u.map_internal_name, u.area
          ORDER BY uses DESC, u.map_display_name",
    )?;
    let rows: Vec<SurveyTypeSummary> = stmt
        .query_map(params![character, server], |row| {
            let total_uses: i64 = row.get(4)?;
            let total_loot_qty: i64 = row.get(5)?;
            let avg = if total_uses > 0 {
                Some(total_loot_qty as f64 / total_uses as f64)
            } else {
                None
            };
            Ok(SurveyTypeSummary {
                map_internal_name: row.get(0)?,
                map_display_name: row.get(1)?,
                area: row.get(2)?,
                kind: row.get(3)?,
                total_uses: total_uses as u32,
                total_loot_qty: total_loot_qty as u32,
                avg_loot_per_use: avg,
                uses_with_bonus: row.get::<_, i64>(6)? as u32,
                bonus_items_total: row.get::<_, i64>(7)? as u32,
                items: Vec::new(),
            })
        })?
        .collect::<SqlResult<_>>()?;

    // Enrich with per-item breakdowns (one query per type; indexed).
    let mut out = Vec::with_capacity(rows.len());
    for mut t in rows {
        t.items = items_for_survey_type(
            conn,
            character,
            server,
            &t.map_internal_name,
            t.area.as_deref(),
        )?;
        out.push(t);
    }
    Ok(out)
}

/// Per-item breakdown scoped to a single zone (`area` value). `"(unknown)"`
/// is the sentinel for `area IS NULL` and matches whatever `zone_summaries`
/// emits.
fn items_for_zone<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
    area_label: &str,
) -> SqlResult<Vec<ItemSummary>> {
    let mut stmt = conn.prepare(
        "SELECT t.item_name,
                SUM(t.quantity) AS total_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN 0 ELSE t.quantity END) AS primary_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN t.quantity ELSE 0 END) AS bonus_qty,
                COUNT(*) AS times_received
         FROM item_transactions t
         JOIN survey_uses u
           ON u.id = t.survey_use_id
         WHERE t.quantity > 0
           AND u.character_name = ?1
           AND u.server_name = ?2
           AND COALESCE(u.area, '(unknown)') = ?3
         GROUP BY t.item_name
         ORDER BY total_qty DESC",
    )?;
    let rows = stmt.query_map(params![character, server, area_label], |row| {
        Ok(ItemSummary {
            item_name: row.get(0)?,
            total_qty: row.get(1)?,
            primary_qty: row.get(2)?,
            bonus_qty: row.get(3)?,
            times_received: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// Per-item breakdown scoped to a single survey type (map_internal_name +
/// area). `area` of `None` here means the surveys of this map were recorded
/// without an area tag, matching the NULL case in the DB.
fn items_for_survey_type<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
    map_internal_name: &str,
    area: Option<&str>,
) -> SqlResult<Vec<ItemSummary>> {
    // SQLite's `IS` handles NULL-safe equality; parameter binding to
    // `Option<&str>` gives us the right NULL at the right spot.
    let mut stmt = conn.prepare(
        "SELECT t.item_name,
                SUM(t.quantity) AS total_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN 0 ELSE t.quantity END) AS primary_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN t.quantity ELSE 0 END) AS bonus_qty,
                COUNT(*) AS times_received
         FROM item_transactions t
         JOIN survey_uses u
           ON u.id = t.survey_use_id
         WHERE t.quantity > 0
           AND u.character_name = ?1
           AND u.server_name = ?2
           AND u.map_internal_name = ?3
           AND u.area IS ?4
         GROUP BY t.item_name
         ORDER BY total_qty DESC",
    )?;
    let rows = stmt.query_map(params![character, server, map_internal_name, area], |row| {
        Ok(ItemSummary {
            item_name: row.get(0)?,
            total_qty: row.get(1)?,
            primary_qty: row.get(2)?,
            bonus_qty: row.get(3)?,
            times_received: row.get(4)?,
        })
    })?;
    rows.collect()
}

fn item_summaries<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> SqlResult<Vec<ItemSummary>> {
    // Items aggregated across all of this character's survey-attributed
    // gains. Joins item_transactions to survey_uses via the same JSON
    // path the per-session loot summary uses. The primary / bonus split
    // falls out of a CASE on json_extract(source_details, '$.is_speed_bonus').
    let mut stmt = conn.prepare(
        "SELECT t.item_name,
                SUM(t.quantity) AS total_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN 0 ELSE t.quantity END) AS primary_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN t.quantity ELSE 0 END) AS bonus_qty,
                COUNT(*) AS times_received
         FROM item_transactions t
         JOIN survey_uses u
           ON u.id = t.survey_use_id
         WHERE t.quantity > 0
           AND u.character_name = ?1
           AND u.server_name = ?2
         GROUP BY t.item_name
         ORDER BY total_qty DESC",
    )?;
    let rows = stmt.query_map(params![character, server], |row| {
        Ok(ItemSummary {
            item_name: row.get(0)?,
            total_qty: row.get(1)?,
            primary_qty: row.get(2)?,
            bonus_qty: row.get(3)?,
            times_received: row.get(4)?,
        })
    })?;
    rows.collect()
}

// ============================================================
// Item cost analysis — powers the right-pane calculator on the
// Analytics tab ("how many surveys of which map to get N of X").
// ============================================================

/// One (item, survey-type) pair of historical drop data. The frontend
/// computes "surveys needed", "total cost/time", "profit/hr" from these
/// rows plus the user's desired-qty and sell-price inputs.
///
/// `primary_*` and `bonus_*` are split from `item_transactions` by the
/// `is_speed_bonus` flag in `source_details` — same rule as the rest of
/// the analytics queries. `avg_seconds_per_survey` is derived from
/// session duration / consumed count, weighted across all sessions that
/// consumed this map type (so it improves as more data is collected).
#[derive(Debug, Clone, Serialize)]
pub struct ItemSourceAnalysis {
    pub item_name: String,
    pub survey_type: String,
    pub map_internal_name: String,
    pub zone: Option<String>,
    pub category: String,
    pub crafting_cost: f64,
    pub total_completions: i64,
    pub primary_total_qty: i64,
    pub primary_times_seen: i64,
    pub bonus_total_qty: i64,
    pub bonus_times_seen: i64,
    pub bonus_avg_per_proc: f64,
    pub speed_bonus_rate: f64,
    /// Average real-clock time between a survey use starting and ending.
    /// `0.0` when we don't have enough ended-session data to compute it
    /// (the UI renders "N/A" in that case and skips time-sort ordering).
    pub avg_seconds_per_survey: f64,
}

#[tauri::command]
pub fn survey_tracker_item_cost_analysis(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<Vec<ItemSourceAnalysis>, String> {
    // See survey_tracker_session_detail: release the coordinator lock before
    // the analysis query so it doesn't stall log ingestion.
    let (character, server, conn) = {
        let coord = coordinator.lock().map_err(|e| e.to_string())?;
        let (character, server) = match coord.active_character_server() {
            Some(cs) => cs,
            None => return Ok(Vec::new()),
        };
        let conn = coord.db_pool().get().map_err(|e| e.to_string())?;
        (character, server, conn)
    };
    item_cost_analysis_rows(&conn, &character, &server).map_err(|e| e.to_string())
}

fn item_cost_analysis_rows<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> SqlResult<Vec<ItemSourceAnalysis>> {
    // Per-map-type avg survey duration. Computed as the sum of ended
    // sessions' `duration_seconds` divided by the total number of uses
    // of this map across those sessions. Active (un-ended) sessions are
    // excluded so the ratio reflects completed work only.
    //
    // Note: a session that consumed multiple map types attributes its
    // duration to each type proportionally via the use-count divisor.
    // Good enough for a first-pass estimate; the frontend flags N/A
    // when there's not yet any ended-session data for this type.
    //
    // The per-type statistics aggregate across zones too, grouping by
    // (map_internal_name, area) to match the granularity the UI shows.
    let mut stmt = conn.prepare(
        "WITH type_duration AS (
            -- Compute per-session duration once, then aggregate by map type.
            -- Using DISTINCT session prevents counting a session's duration
            -- multiple times when it contains multiple uses of the same map.
            SELECT u.map_internal_name,
                   u.area,
                   SUM(session_seconds) AS total_seconds,
                   SUM(session_uses) AS total_uses
              FROM (
                SELECT u.map_internal_name,
                       u.area,
                       u.session_id,
                       (julianday(s.ended_at) - julianday(s.started_at)) * 86400.0 AS session_seconds,
                       COUNT(*) AS session_uses
                  FROM survey_uses u
                  JOIN survey_sessions s ON s.id = u.session_id
                 WHERE u.character_name = ?1 AND u.server_name = ?2
                   AND s.ended_at IS NOT NULL
                 GROUP BY u.map_internal_name, u.area, u.session_id
              ) u
             GROUP BY u.map_internal_name, u.area
         ),
         per_type_stats AS (
            SELECT u.map_internal_name,
                   u.map_display_name,
                   u.area,
                   u.kind,
                   COUNT(*) AS total_completions
              FROM survey_uses u
             WHERE u.character_name = ?1 AND u.server_name = ?2
             GROUP BY u.map_internal_name, u.area
         ),
         per_type_bonus AS (
            -- Basic-only: number of *distinct uses* that received any
            -- bonus drop. Used to compute speed_bonus_rate regardless of
            -- which specific item(s) bonused.
            SELECT u.map_internal_name,
                   u.area,
                   COUNT(DISTINCT u.id) AS uses_with_any_bonus
              FROM survey_uses u
              JOIN item_transactions t
                ON u.id = t.survey_use_id
             WHERE u.character_name = ?1 AND u.server_name = ?2
               AND u.kind = 'basic'
               AND json_extract(t.source_details, '$.is_speed_bonus') = 1
               AND t.quantity > 0
             GROUP BY u.map_internal_name, u.area
         )
         SELECT t.item_name,
                pts.map_display_name,
                pts.map_internal_name,
                pts.area,
                pts.kind,
                pts.total_completions,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN 0 ELSE t.quantity END) AS primary_total_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN 0 ELSE 1 END) AS primary_times_seen,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN t.quantity ELSE 0 END) AS bonus_total_qty,
                SUM(CASE WHEN json_extract(t.source_details, '$.is_speed_bonus') = 1
                         THEN 1 ELSE 0 END) AS bonus_times_seen,
                COALESCE(ptb.uses_with_any_bonus, 0) AS uses_with_any_bonus,
                COALESCE(td.total_seconds, 0.0) AS total_seconds,
                COALESCE(td.total_uses, 0) AS td_total_uses
           FROM item_transactions t
           JOIN survey_uses u
             ON u.id = t.survey_use_id
           JOIN per_type_stats pts
             ON pts.map_internal_name = u.map_internal_name
            AND ((pts.area IS NULL AND u.area IS NULL) OR pts.area = u.area)
           LEFT JOIN per_type_bonus ptb
             ON ptb.map_internal_name = u.map_internal_name
            AND ((ptb.area IS NULL AND u.area IS NULL) OR ptb.area = u.area)
           LEFT JOIN type_duration td
             ON td.map_internal_name = u.map_internal_name
            AND ((td.area IS NULL AND u.area IS NULL) OR td.area = u.area)
          WHERE t.quantity > 0
            AND u.character_name = ?1
            AND u.server_name = ?2
          GROUP BY t.item_name, u.map_internal_name, u.area
          ORDER BY t.item_name, primary_total_qty DESC",
    )?;

    let rows = stmt.query_map(params![character, server], |row| {
        let total_completions: i64 = row.get(5)?;
        let bonus_total_qty: i64 = row.get(8)?;
        let bonus_times_seen: i64 = row.get(9)?;
        let uses_with_any_bonus: i64 = row.get(10)?;
        let total_seconds: f64 = row.get(11)?;
        let td_total_uses: i64 = row.get(12)?;

        let bonus_avg_per_proc = if bonus_times_seen > 0 {
            bonus_total_qty as f64 / bonus_times_seen as f64
        } else {
            0.0
        };
        let speed_bonus_rate = if total_completions > 0 {
            (uses_with_any_bonus as f64 / total_completions as f64) * 100.0
        } else {
            0.0
        };
        let avg_seconds_per_survey = if td_total_uses > 0 {
            total_seconds / td_total_uses as f64
        } else {
            0.0
        };

        // Kind is used only to derive the category below. The game-data
        // naming convention makes the split deterministic:
        // MiningSurvey* = mining, everything else (GeologySurvey*) = mineral.
        let map_internal: String = row.get(2)?;
        let category = if map_internal.starts_with("MiningSurvey") {
            "mining".to_string()
        } else {
            "mineral".to_string()
        };

        Ok(ItemSourceAnalysis {
            item_name: row.get(0)?,
            survey_type: row.get(1)?,
            map_internal_name: map_internal,
            zone: row.get(3)?,
            category,
            crafting_cost: 0.0, // filled in after collect
            total_completions,
            primary_total_qty: row.get(6)?,
            primary_times_seen: row.get(7)?,
            bonus_total_qty,
            bonus_times_seen,
            bonus_avg_per_proc,
            speed_bonus_rate,
            avg_seconds_per_survey,
        })
    })?;

    let mut out: Vec<ItemSourceAnalysis> = Vec::new();
    for r in rows {
        out.push(r?);
    }

    // Pull crafting_cost per map_internal_name from survey_types. The
    // same map can have entries in two zones, but the cost is a property
    // of the map itself (same recipe regardless of where you use it),
    // so one lookup per map is enough.
    let mut cost_stmt =
        conn.prepare("SELECT internal_name, COALESCE(crafting_cost, 0.0) FROM survey_types")?;
    let mut costs = std::collections::HashMap::new();
    let cost_rows = cost_stmt.query_map([], |row| {
        Ok::<(String, f64), rusqlite::Error>((row.get(0)?, row.get(1)?))
    })?;
    for r in cost_rows {
        let (name, cost) = r?;
        costs.insert(name, cost);
    }
    for r in out.iter_mut() {
        r.crafting_cost = costs.get(&r.map_internal_name).copied().unwrap_or(0.0);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use crate::survey::persistence;
    use crate::survey::types::{SessionStartTrigger, SurveyUseKind, SurveyUseStatus};
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn, None).unwrap();
        conn
    }

    /// Insert a fully-populated session with two uses + corresponding
    /// item_transactions rows. Returns the session_id. Used by the
    /// helper-function tests below.
    fn seed_session(conn: &Connection) -> i64 {
        let session = persistence::insert_session(
            conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let u1 = persistence::insert_use(
            conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        persistence::set_use_status(conn, u1, SurveyUseStatus::Completed).unwrap();
        persistence::add_loot_qty(conn, u1, 5).unwrap();

        let u2 = persistence::insert_use(
            conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:10:00",
            "MiningSurveyPovus7Y",
            "Povus Astounding Mining Survey",
            SurveyUseKind::Multihit,
            Some("Povus"),
        )
        .unwrap();
        persistence::set_use_status(conn, u2, SurveyUseStatus::Completed).unwrap();
        persistence::add_loot_qty(conn, u2, 7).unwrap();

        // Two item_transactions rows linked to the uses via JSON path.
        let details_for = |use_id: i64| -> String { format!(r#"{{"survey_use_id":{use_id}}}"#) };
        let now = "2026-04-15 12:05:00";
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, quantity,
                 context, source, source_kind, source_details, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, 'loot', 'player_log', 'survey_map_use', ?6, 'confident')",
            params![now, "Zenith", "Dreva", "Fluorite", 3, details_for(u1)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, quantity,
                 context, source, source_kind, source_details, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, 'loot', 'player_log', 'mining', ?6, 'probable')",
            params![
                now,
                "Zenith",
                "Dreva",
                "Marvelous Metal Slab",
                7,
                details_for(u2)
            ],
        )
        .unwrap();

        session
    }

    #[test]
    fn test_historical_session_rows_aggregates_correctly() {
        let conn = fresh_db();
        let session_id = seed_session(&conn);

        let rows =
            historical_session_rows(&conn, "Zenith", "Dreva", "highest_market_vendor", 50).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.session.id, session_id);
        assert_eq!(r.total_uses, 2);
        assert_eq!(r.basic_uses, 1);
        assert_eq!(r.multihit_uses, 1);
        assert_eq!(r.motherlode_uses, 0);
        assert_eq!(r.total_loot_qty, 12); // 5 + 7
                                          // Active session — duration is None until ended_at is set
        assert_eq!(r.duration_seconds, None);
    }

    #[test]
    fn test_historical_session_rows_includes_empty_sessions() {
        // A manually started session that recorded no uses should still appear
        // in the History list (so the user sees their false-start manual ones).
        let conn = fresh_db();
        persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let rows =
            historical_session_rows(&conn, "Zenith", "Dreva", "highest_market_vendor", 50).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_uses, 0);
        assert_eq!(rows[0].total_loot_qty, 0);
    }

    #[test]
    fn test_historical_session_rows_duration_after_end() {
        let conn = fresh_db();
        let session = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        persistence::end_session(&conn, session, "2026-04-15 13:30:00").unwrap();
        let rows =
            historical_session_rows(&conn, "Zenith", "Dreva", "highest_market_vendor", 50).unwrap();
        // 1h 30m = 5400 seconds
        assert_eq!(rows[0].duration_seconds, Some(5400));
    }

    #[test]
    fn test_zone_summaries_groups_by_area() {
        let conn = fresh_db();
        seed_session(&conn);
        let zones = zone_summaries(&conn, "Zenith", "Dreva").unwrap();
        // Two distinct areas: Serbule (1 basic) and Povus (1 multihit)
        assert_eq!(zones.len(), 2);
        let serbule = zones.iter().find(|z| z.area == "Serbule").unwrap();
        assert_eq!(serbule.basic_uses, 1);
        assert_eq!(serbule.multihit_uses, 0);
        assert_eq!(serbule.total_loot_qty, 5);
        let povus = zones.iter().find(|z| z.area == "Povus").unwrap();
        assert_eq!(povus.basic_uses, 0);
        assert_eq!(povus.multihit_uses, 1);
        assert_eq!(povus.total_loot_qty, 7);
    }

    #[test]
    fn test_survey_type_summaries_groups_by_map_and_area() {
        let conn = fresh_db();
        seed_session(&conn);
        let types = survey_type_summaries(&conn, "Zenith", "Dreva").unwrap();
        assert_eq!(types.len(), 2);
        let serbule = types
            .iter()
            .find(|t| t.map_internal_name == "GeologySurveySerbule1")
            .unwrap();
        assert_eq!(serbule.kind, "basic");
        assert_eq!(serbule.total_uses, 1);
        assert_eq!(serbule.avg_loot_per_use, Some(5.0));
    }

    #[test]
    fn test_item_summaries_joins_transactions_to_uses() {
        let conn = fresh_db();
        seed_session(&conn);
        let items = item_summaries(&conn, "Zenith", "Dreva").unwrap();
        // Two distinct items came from this character's surveys
        assert_eq!(items.len(), 2);
        let slab = items
            .iter()
            .find(|i| i.item_name == "Marvelous Metal Slab")
            .expect("slab present");
        assert_eq!(slab.total_qty, 7);
        assert_eq!(slab.times_received, 1);
        assert_eq!(
            slab.primary_qty, 7,
            "no bonus flag set → all qty is primary"
        );
        assert_eq!(slab.bonus_qty, 0);
    }

    #[test]
    fn test_session_economics_cost_revenue_profit() {
        // One Basic use of a survey with crafting_cost=150, two items gained
        // (primary + bonus). Market values on the bonus item, none on the
        // primary — exercises the priced/unpriced split.
        let conn = fresh_db();
        let session = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let use_id = persistence::insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        persistence::set_use_status(&conn, use_id, SurveyUseStatus::Completed).unwrap();

        // Seed a survey_types row so the cost join finds a crafting_cost.
        // Only the columns the migration-created schema has; `kind` gets
        // added by CDN ingestion at runtime but isn't in the migration.
        conn.execute(
            "INSERT INTO survey_types (item_id, internal_name, name, zone,
                                       survey_category, is_motherlode, crafting_cost)
             VALUES (1, 'GeologySurveySerbule1', 'Serbule Blue Mineral Survey', 'Serbule',
                     'mineral', 0, 150.0)",
            [],
        )
        .unwrap();

        // Seed item_transactions with item_type_ids so the market_values join
        // can match. Blue Spinel (id=10) has no market value; Rubywall (id=11)
        // does. Bonus flag on Rubywall.
        let details = format!(r#"{{"survey_use_id":{}}}"#, use_id);
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, item_type_id,
                 quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:05:00','Zenith','Dreva','Blue Spinel',10,1,'loot','player_log','survey_map_use',?1),
                    ('2026-04-15 12:05:00','Zenith','Dreva','Rubywall Crystal',11,2,'loot','player_log','survey_map_use',?1)",
            params![details],
        )
        .unwrap();
        persistence::mark_transactions_as_speed_bonus(&conn, use_id, "Rubywall Crystal").unwrap();

        // Rubywall priced at 50 each; Blue Spinel unpriced.
        conn.execute(
            "INSERT INTO market_values (server_name, item_type_id, item_name, market_value, updated_at)
             VALUES ('Dreva', 11, 'Rubywall Crystal', 50, datetime('now'))",
            [],
        )
        .unwrap();

        let loot =
            loot_summary_for_session(&conn, session, "Dreva", "highest_market_vendor").unwrap();
        assert_eq!(loot.len(), 2);
        let rubywall = loot
            .iter()
            .find(|r| r.item_name == "Rubywall Crystal")
            .unwrap();
        assert_eq!(rubywall.unit_value, Some(50));
        assert_eq!(rubywall.total_value, Some(100));
        assert_eq!(rubywall.bonus_qty, 2);
        let spinel = loot.iter().find(|r| r.item_name == "Blue Spinel").unwrap();
        assert_eq!(spinel.unit_value, None);
        assert_eq!(spinel.total_value, None);

        let econ =
            economics_from_loot_and_uses(&conn, session, "Dreva", "highest_market_vendor", &loot)
                .unwrap();
        assert_eq!(econ.cost_total, 150);
        assert_eq!(econ.revenue_total, 100); // only priced item contributes
        assert_eq!(econ.bonus_revenue_total, 100); // all revenue came from the bonus
        assert_eq!(econ.profit_total, 100 - 150); // -50
        assert_eq!(econ.items_priced, 1);
        assert_eq!(econ.items_unpriced, 1);
    }

    #[test]
    fn test_loot_summary_chat_authoritative_no_double_count() {
        // When a use has BOTH a Player.log-attributed row and a chat-attributed
        // ('survey_chat') row for the same gain, the chat row is authoritative
        // and the Player.log row must NOT be double-counted.
        let conn = fresh_db();
        let session = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let use_id = persistence::insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        let details = format!(r#"{{"survey_use_id":{use_id}}}"#);

        // Player.log row: 3x Fluorite. Chat row: authoritative 9x Fluorite.
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, item_type_id,
                 quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:05:00','Zenith','Dreva','Fluorite',10,3,
                     'loot','player_log','survey_map_use',?1),
                    ('2026-04-15 12:05:01','Zenith','Dreva','Fluorite',10,9,
                     'loot','chat_status','survey_chat',?1)",
            params![details],
        )
        .unwrap();

        let loot =
            loot_summary_for_session(&conn, session, "Dreva", "highest_market_vendor").unwrap();
        assert_eq!(loot.len(), 1);
        // 9 (chat) only — not 12 (chat + player.log).
        assert_eq!(loot[0].total_qty, 9);
    }

    #[test]
    fn test_loot_summary_player_log_only_still_counts() {
        // Historical sessions recorded before the chat path existed have only
        // Player.log rows and no 'survey_chat' rows — those must still render.
        let conn = fresh_db();
        let session = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let use_id = persistence::insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        let details = format!(r#"{{"survey_use_id":{use_id}}}"#);
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, item_type_id,
                 quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:05:00','Zenith','Dreva','Fluorite',10,3,
                     'loot','player_log','survey_map_use',?1)",
            params![details],
        )
        .unwrap();

        let loot =
            loot_summary_for_session(&conn, session, "Dreva", "highest_market_vendor").unwrap();
        assert_eq!(loot.len(), 1);
        assert_eq!(loot[0].total_qty, 3);
    }

    #[test]
    fn test_session_cost_zero_when_no_survey_type_row() {
        // Defensive: if the map_internal_name doesn't match a survey_types row
        // (e.g., retired map or data-sync race), cost_total should be 0 rather
        // than crashing or returning NULL.
        let conn = fresh_db();
        let session = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        persistence::insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "SomeUnknownMap",
            "Some Unknown Map",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        let cost =
            session_cost_from_recipes(&conn, session, "Dreva", "highest_market_vendor").unwrap();
        assert_eq!(cost, 0);
    }

    #[test]
    fn test_analytics_surfaces_speed_bonus_stats() {
        // Seed a basic use, two gains (primary + bonus), flag the bonus row,
        // then verify zone / survey-type / item analytics all reflect it.
        let conn = fresh_db();
        let session = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let use_id = persistence::insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        persistence::set_use_status(&conn, use_id, SurveyUseStatus::Completed).unwrap();
        persistence::add_loot_qty(&conn, use_id, 3).unwrap();

        let details = format!(r#"{{"survey_use_id":{}}}"#, use_id);
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, quantity,
                 context, source, source_kind, source_details, confidence)
             VALUES ('2026-04-15 12:05:00','Zenith','Dreva','Blue Spinel',1,'loot','player_log','survey_map_use',?1,'confident'),
                    ('2026-04-15 12:05:00','Zenith','Dreva','Rubywall Crystal',2,'loot','player_log','survey_map_use',?1,'confident')",
            params![details],
        ).unwrap();
        // Flag the bonus row.
        persistence::mark_transactions_as_speed_bonus(&conn, use_id, "Rubywall Crystal").unwrap();

        // Zone rollup
        let zones = zone_summaries(&conn, "Zenith", "Dreva").unwrap();
        let serbule = zones.iter().find(|z| z.area == "Serbule").unwrap();
        assert_eq!(serbule.basic_uses, 1);
        assert_eq!(serbule.basic_uses_with_bonus, 1);
        assert_eq!(serbule.bonus_items_total, 2);

        // Survey-type rollup
        let types = survey_type_summaries(&conn, "Zenith", "Dreva").unwrap();
        let t = types
            .iter()
            .find(|t| t.map_internal_name == "GeologySurveySerbule1")
            .unwrap();
        assert_eq!(t.uses_with_bonus, 1);
        assert_eq!(t.bonus_items_total, 2);

        // Item rollup — primary / bonus split
        let items = item_summaries(&conn, "Zenith", "Dreva").unwrap();
        let primary = items.iter().find(|i| i.item_name == "Blue Spinel").unwrap();
        assert_eq!(primary.primary_qty, 1);
        assert_eq!(primary.bonus_qty, 0);
        let bonus = items
            .iter()
            .find(|i| i.item_name == "Rubywall Crystal")
            .unwrap();
        assert_eq!(bonus.primary_qty, 0);
        assert_eq!(bonus.bonus_qty, 2);
    }

    #[test]
    fn test_compute_duration_seconds_handles_clock_skew() {
        // Defensive: end before start should return None, not a negative.
        assert_eq!(
            compute_duration_seconds("2026-04-15 12:00:00", Some("2026-04-15 11:00:00")),
            None
        );
    }

    #[test]
    fn test_delete_session_clears_uses_but_keeps_transactions() {
        // We delete the survey_uses row but leave item_transactions in place
        // so the historical ledger isn't corrupted.
        let mut conn = fresh_db();
        let session_id = seed_session(&conn);

        let pre_uses: i64 = conn
            .query_row("SELECT COUNT(*) FROM survey_uses", [], |r| r.get(0))
            .unwrap();
        let pre_tx: i64 = conn
            .query_row("SELECT COUNT(*) FROM item_transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pre_uses, 2);
        assert_eq!(pre_tx, 2);

        // Exercise the same helper the Tauri command delegates to.
        delete_session_impl(&mut conn, session_id).unwrap();

        let post_uses: i64 = conn
            .query_row("SELECT COUNT(*) FROM survey_uses", [], |r| r.get(0))
            .unwrap();
        let post_tx: i64 = conn
            .query_row("SELECT COUNT(*) FROM item_transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(post_uses, 0, "uses should be deleted");
        assert_eq!(
            post_tx, 2,
            "transactions should be preserved as historical record"
        );
    }

    #[test]
    fn test_item_cost_analysis_aggregates_primary_and_bonus() {
        // Scenario: one Basic survey type used twice. Both uses dropped
        // Blue Spinel (primary) and one of them also dropped Rubywall as
        // a bonus. The command should return one row per (item, map),
        // with primary/bonus split and the map's crafting cost.
        let conn = fresh_db();

        // Seed two sessions — both ended, with 1 use apiece. This gives
        // us an avg_seconds_per_survey we can assert on (60s per session,
        // 1 use per session = 60s avg).
        let sess1 = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        conn.execute(
            "UPDATE survey_sessions SET ended_at = '2026-04-15 12:01:00' WHERE id = ?1",
            [sess1],
        )
        .unwrap();
        let sess2 = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:10:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        conn.execute(
            "UPDATE survey_sessions SET ended_at = '2026-04-15 12:11:00' WHERE id = ?1",
            [sess2],
        )
        .unwrap();

        let u1 = persistence::insert_use(
            &conn,
            Some(sess1),
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:30",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        let u2 = persistence::insert_use(
            &conn,
            Some(sess2),
            "Zenith",
            "Dreva",
            "2026-04-15 12:10:30",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();

        // survey_types row so crafting_cost comes back populated.
        conn.execute(
            "INSERT INTO survey_types (item_id, internal_name, name, zone,
                                       survey_category, is_motherlode, crafting_cost)
             VALUES (1, 'GeologySurveySerbule1', 'Serbule Blue Mineral Survey', 'Serbule',
                     'mineral', 0, 120.0)",
            [],
        )
        .unwrap();

        // Transactions: both uses drop primary Blue Spinel; one use also
        // drops a bonus Rubywall.
        let details_u1 = format!(r#"{{"survey_use_id":{}}}"#, u1);
        let details_u2 = format!(r#"{{"survey_use_id":{}}}"#, u2);
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, item_type_id,
                 quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:00:30','Zenith','Dreva','Blue Spinel',10,1,'loot','player_log','survey_map_use',?1),
                    ('2026-04-15 12:10:30','Zenith','Dreva','Blue Spinel',10,1,'loot','player_log','survey_map_use',?2),
                    ('2026-04-15 12:10:30','Zenith','Dreva','Rubywall Crystal',11,2,'loot','player_log','survey_map_use',?2)",
            params![details_u1, details_u2],
        )
        .unwrap();
        persistence::mark_transactions_as_speed_bonus(&conn, u2, "Rubywall Crystal").unwrap();

        let rows = item_cost_analysis_rows(&conn, "Zenith", "Dreva").unwrap();
        assert_eq!(rows.len(), 2, "one row per (item, map) pair");

        let blue = rows.iter().find(|r| r.item_name == "Blue Spinel").unwrap();
        assert_eq!(blue.survey_type, "Serbule Blue Mineral Survey");
        assert_eq!(blue.map_internal_name, "GeologySurveySerbule1");
        assert_eq!(blue.zone.as_deref(), Some("Serbule"));
        assert_eq!(blue.category, "mineral");
        assert_eq!(blue.crafting_cost, 120.0);
        assert_eq!(blue.total_completions, 2);
        assert_eq!(blue.primary_total_qty, 2, "1 + 1 primary drops");
        assert_eq!(blue.primary_times_seen, 2);
        assert_eq!(blue.bonus_total_qty, 0);
        // 50% bonus rate — 1 of 2 uses had any bonus drop
        assert!((blue.speed_bonus_rate - 50.0).abs() < 0.001);
        // 60s per session / 1 use per session = 60.0 avg
        assert!((blue.avg_seconds_per_survey - 60.0).abs() < 0.001);

        let ruby = rows
            .iter()
            .find(|r| r.item_name == "Rubywall Crystal")
            .unwrap();
        assert_eq!(ruby.primary_total_qty, 0);
        assert_eq!(ruby.bonus_total_qty, 2);
        assert_eq!(ruby.bonus_times_seen, 1);
        assert!((ruby.bonus_avg_per_proc - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_item_cost_analysis_skips_active_sessions_for_duration() {
        // A session with no ended_at shouldn't contribute to the
        // avg_seconds_per_survey calculation — we'd otherwise mix in
        // in-progress "duration" that isn't representative.
        let conn = fresh_db();
        let sess = persistence::insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let use_id = persistence::insert_use(
            &conn,
            Some(sess),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();
        let details = format!(r#"{{"survey_use_id":{}}}"#, use_id);
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, item_type_id,
                 quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:05:00','Zenith','Dreva','Blue Spinel',10,1,'loot','player_log','survey_map_use',?1)",
            params![details],
        )
        .unwrap();

        let rows = item_cost_analysis_rows(&conn, "Zenith", "Dreva").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].avg_seconds_per_survey, 0.0,
            "active sessions shouldn't contribute to the duration estimate",
        );
    }
}
