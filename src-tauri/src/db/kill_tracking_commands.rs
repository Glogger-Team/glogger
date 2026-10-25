/// Kill tracking queries — enemy kill stats and loot drop rates.
///
/// Three data scopes:
/// - "mine": the player's own observed kills (`enemy_kills` / `enemy_kill_loot`)
/// - "imported": aggregated data imported from other players' exports
///   (`imported_enemy_kills_agg` / `imported_enemy_kill_loot_agg`), tagged by
///   `source_label` so a re-import of the same file replaces rather than
///   double-counts
/// - "combined": mine + imported, summed
use super::{DbPool, DbRead};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tauri::State;

#[derive(Serialize)]
pub struct EnemyLootDrop {
    pub item_name: String,
    pub total_quantity: i64,
    pub times_dropped: i64,
    /// How many kills had this item drop (times_dropped / total_kills)
    pub drop_rate: f64,
}

#[derive(Serialize)]
pub struct EnemyKillStats {
    pub enemy_name: String,
    pub total_kills: i64,
    pub loot: Vec<EnemyLootDrop>,
}

/// Which zone bucket a drop-rate query is scoped to. Drop rates vary by zone, so
/// stats are keyed by (enemy_name, zone). `All` ignores zone (sums across zones);
/// `Unknown` matches rows whose zone is NULL (kills recorded before zone capture);
/// `Zone(z)` matches a specific area key. From a command's `zone: Option<String>`:
/// absent = `All`, "" = `Unknown`, otherwise `Zone`.
enum ZoneFilter {
    All,
    Unknown,
    Zone(String),
}

impl ZoneFilter {
    fn from_opt(zone: Option<&str>) -> Self {
        match zone {
            None => ZoneFilter::All,
            Some("") => ZoneFilter::Unknown,
            Some(z) => ZoneFilter::Zone(z.to_string()),
        }
    }

    /// From a stored zone value: NULL → the Unknown bucket, a value → that zone.
    fn from_stored(zone: Option<String>) -> Self {
        match zone {
            None => ZoneFilter::Unknown,
            Some(z) => ZoneFilter::Zone(z),
        }
    }

    /// SQL appended after a `WHERE` that already constrains enemy_name; `col` is
    /// the qualified zone column. Uses an anonymous `?` placeholder (see `bind`).
    fn sql(&self, col: &str) -> String {
        match self {
            ZoneFilter::All => String::new(),
            ZoneFilter::Unknown => format!(" AND {col} IS NULL"),
            ZoneFilter::Zone(_) => format!(" AND {col} IS ?"),
        }
    }

    /// The bind value for the `Zone` case, appended after the enemy-name bind.
    fn bind(&self) -> Option<rusqlite::types::Value> {
        match self {
            ZoneFilter::Zone(z) => Some(rusqlite::types::Value::Text(z.clone())),
            _ => None,
        }
    }
}

/// Filter on the equipped combat-skill loadout recorded per kill. `All` ignores
/// it; `Match(loadout)` keeps that loadout PLUS the unattributed (NULL) baseline —
/// legacy + imported rows that have no loadout — so tables stay populated. From a
/// command's `combat_skills: Option<String>`: absent/"" = `All`, else `Match`.
enum LoadoutFilter {
    All,
    Match(String),
}

impl LoadoutFilter {
    fn from_opt(loadout: Option<&str>) -> Self {
        match loadout {
            Some(l) if !l.trim().is_empty() => LoadoutFilter::Match(l.trim().to_string()),
            _ => LoadoutFilter::All,
        }
    }

    /// Clause appended after an existing `WHERE …`; `col` is the qualified column.
    /// Anonymous `?` placeholder, bound after the zone bind (see `drop_binds`).
    fn sql(&self, col: &str) -> String {
        match self {
            LoadoutFilter::All => String::new(),
            LoadoutFilter::Match(_) => format!(" AND ({col} = ? OR {col} IS NULL)"),
        }
    }

    /// Standalone `WHERE …` for queries that don't otherwise filter.
    fn where_clause(&self, col: &str) -> String {
        match self {
            LoadoutFilter::All => String::new(),
            LoadoutFilter::Match(_) => format!(" WHERE ({col} = ? OR {col} IS NULL)"),
        }
    }

    fn bind(&self) -> Option<rusqlite::types::Value> {
        match self {
            LoadoutFilter::Match(l) => Some(rusqlite::types::Value::Text(l.clone())),
            _ => None,
        }
    }
}

fn drop_binds(
    enemy_name: &str,
    zf: &ZoneFilter,
    lf: &LoadoutFilter,
) -> Vec<rusqlite::types::Value> {
    let mut binds = vec![rusqlite::types::Value::Text(enemy_name.to_string())];
    if let Some(b) = zf.bind() {
        binds.push(b);
    }
    if let Some(b) = lf.bind() {
        binds.push(b);
    }
    binds
}

fn mine_total_kills<C: DbRead + ?Sized>(
    conn: &C,
    enemy_name: &str,
    zf: &ZoneFilter,
    lf: &LoadoutFilter,
) -> Result<i64, String> {
    let sql = format!(
        "SELECT COUNT(*) FROM enemy_kills WHERE enemy_name = ?{}{}",
        zf.sql("zone"),
        lf.sql("combat_skills")
    );
    let binds = drop_binds(enemy_name, zf, lf);
    conn.query_row(&sql, rusqlite::params_from_iter(binds.iter()), |row| {
        row.get(0)
    })
    .map_err(|e| format!("Kill count query failed: {e}"))
}

fn imported_total_kills<C: DbRead + ?Sized>(
    conn: &C,
    enemy_name: &str,
    zf: &ZoneFilter,
    lf: &LoadoutFilter,
) -> Result<i64, String> {
    let sql = format!(
        "SELECT COALESCE(SUM(total_kills), 0) FROM imported_enemy_kills_agg WHERE enemy_name = ?{}{}",
        zf.sql("zone"),
        lf.sql("combat_skills")
    );
    let binds = drop_binds(enemy_name, zf, lf);
    conn.query_row(&sql, rusqlite::params_from_iter(binds.iter()), |row| {
        row.get(0)
    })
    .map_err(|e| format!("Imported kill count query failed: {e}"))
}

fn mine_loot_rows<C: DbRead + ?Sized>(
    conn: &C,
    enemy_name: &str,
    zf: &ZoneFilter,
    lf: &LoadoutFilter,
) -> Result<Vec<(String, i64, i64)>, String> {
    let sql = format!(
        "SELECT l.item_name, SUM(l.quantity), COUNT(DISTINCT l.kill_id)
         FROM enemy_kill_loot l
         JOIN enemy_kills k ON l.kill_id = k.id
         WHERE k.enemy_name = ?{}{}
         GROUP BY l.item_name",
        zf.sql("k.zone"),
        lf.sql("k.combat_skills")
    );
    let binds = drop_binds(enemy_name, zf, lf);
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Loot query failed: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(binds.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("Loot query failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Loot row error: {e}"))?);
    }
    Ok(out)
}

fn imported_loot_rows<C: DbRead + ?Sized>(
    conn: &C,
    enemy_name: &str,
    zf: &ZoneFilter,
    lf: &LoadoutFilter,
) -> Result<Vec<(String, i64, i64)>, String> {
    let sql = format!(
        "SELECT item_name, SUM(total_quantity), SUM(times_dropped)
         FROM imported_enemy_kill_loot_agg
         WHERE enemy_name = ?{}{}
         GROUP BY item_name",
        zf.sql("zone"),
        lf.sql("combat_skills")
    );
    let binds = drop_binds(enemy_name, zf, lf);
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Imported loot query failed: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(binds.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("Imported loot query failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Imported loot row error: {e}"))?);
    }
    Ok(out)
}

fn combine_loot_rows(
    mine: Vec<(String, i64, i64)>,
    imported: Vec<(String, i64, i64)>,
) -> Vec<(String, i64, i64)> {
    let mut combined: HashMap<String, (i64, i64)> = HashMap::new();
    for (item, qty, dropped) in mine.into_iter().chain(imported.into_iter()) {
        let entry = combined.entry(item).or_insert((0, 0));
        entry.0 += qty;
        entry.1 += dropped;
    }
    combined
        .into_iter()
        .map(|(item, (qty, dropped))| (item, qty, dropped))
        .collect()
}

/// All-time kill/loot stats for a monster. `zone` scopes the stats by area:
/// absent/null = all zones combined, "" = the unknown-zone bucket (kills recorded
/// before zone capture), otherwise a specific area key.
#[tauri::command]
pub fn get_enemy_kill_stats(
    db: State<'_, DbPool>,
    enemy_name: String,
    scope: String,
    zone: Option<String>,
    combat_skills: Option<String>,
) -> Result<EnemyKillStats, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let zf = ZoneFilter::from_opt(zone.as_deref());
    let lf = LoadoutFilter::from_opt(combat_skills.as_deref());

    let total_kills = match scope.as_str() {
        "mine" => mine_total_kills(&conn, &enemy_name, &zf, &lf)?,
        "imported" => imported_total_kills(&conn, &enemy_name, &zf, &lf)?,
        _ => {
            mine_total_kills(&conn, &enemy_name, &zf, &lf)?
                + imported_total_kills(&conn, &enemy_name, &zf, &lf)?
        }
    };

    if total_kills == 0 {
        return Ok(EnemyKillStats {
            enemy_name,
            total_kills: 0,
            loot: Vec::new(),
        });
    }

    let loot_rows = match scope.as_str() {
        "mine" => mine_loot_rows(&conn, &enemy_name, &zf, &lf)?,
        "imported" => imported_loot_rows(&conn, &enemy_name, &zf, &lf)?,
        _ => combine_loot_rows(
            mine_loot_rows(&conn, &enemy_name, &zf, &lf)?,
            imported_loot_rows(&conn, &enemy_name, &zf, &lf)?,
        ),
    };

    let mut loot: Vec<EnemyLootDrop> = loot_rows
        .into_iter()
        .map(|(item_name, total_quantity, times_dropped)| EnemyLootDrop {
            item_name,
            total_quantity,
            times_dropped,
            drop_rate: times_dropped as f64 / total_kills as f64,
        })
        .collect();
    loot.sort_by(|a, b| {
        b.times_dropped
            .cmp(&a.times_dropped)
            .then(b.total_quantity.cmp(&a.total_quantity))
    });

    Ok(EnemyKillStats {
        enemy_name,
        total_kills,
        loot,
    })
}

#[derive(Serialize)]
pub struct ItemDropSource {
    pub enemy_name: String,
    /// Area key the kills happened in (null = unknown zone). Drop rates are
    /// per (enemy, zone), so the same monster in two zones is two sources.
    pub zone: Option<String>,
    pub total_kills: i64,
    pub times_dropped: i64,
    pub total_quantity: i64,
    pub drop_rate: f64,
}

/// Given an item name (display or internal), find all (enemy, zone) sources that
/// have dropped it and their per-zone drop rates.
#[tauri::command]
pub fn get_item_drop_sources(
    db: State<'_, DbPool>,
    item_name: String,
    internal_name: Option<String>,
    scope: String,
    combat_skills: Option<String>,
) -> Result<Vec<ItemDropSource>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let lf = LoadoutFilter::from_opt(combat_skills.as_deref());

    // (enemy_name, zone) -> (times_dropped, total_qty)
    let mut per_source: HashMap<(String, Option<String>), (i64, i64)> = HashMap::new();

    // item_name + internal_name binds (internal NULL → never matches the 2nd `?`).
    let item_binds = || -> Vec<rusqlite::types::Value> {
        let mut b = vec![
            rusqlite::types::Value::Text(item_name.clone()),
            match &internal_name {
                Some(s) => rusqlite::types::Value::Text(s.clone()),
                None => rusqlite::types::Value::Null,
            },
        ];
        if let Some(lb) = lf.bind() {
            b.push(lb);
        }
        b
    };

    if scope == "mine" || scope == "combined" {
        let sql = format!(
            "SELECT k.enemy_name, k.zone, COUNT(DISTINCT l.kill_id), SUM(l.quantity)
             FROM enemy_kill_loot l
             JOIN enemy_kills k ON l.kill_id = k.id
             WHERE (l.item_name = ? OR l.item_name = ?){}
             GROUP BY k.enemy_name, k.zone",
            lf.sql("k.combat_skills")
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare drop source query: {e}"))?;
        let binds = item_binds();
        let rows = stmt
            .query_map(rusqlite::params_from_iter(binds.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| format!("Drop source query failed: {e}"))?;
        for row in rows {
            let (enemy_name, zone, times_dropped, qty) =
                row.map_err(|e| format!("Drop source row error: {e}"))?;
            let entry = per_source.entry((enemy_name, zone)).or_insert((0, 0));
            entry.0 += times_dropped;
            entry.1 += qty;
        }
    }

    if scope == "imported" || scope == "combined" {
        let sql = format!(
            "SELECT enemy_name, zone, SUM(times_dropped), SUM(total_quantity)
             FROM imported_enemy_kill_loot_agg
             WHERE (item_name = ? OR item_name = ?){}
             GROUP BY enemy_name, zone",
            lf.sql("combat_skills")
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare imported drop source query: {e}"))?;
        let binds = item_binds();
        let rows = stmt
            .query_map(rusqlite::params_from_iter(binds.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| format!("Imported drop source query failed: {e}"))?;
        for row in rows {
            let (enemy_name, zone, times_dropped, qty) =
                row.map_err(|e| format!("Imported drop source row error: {e}"))?;
            let entry = per_source.entry((enemy_name, zone)).or_insert((0, 0));
            entry.0 += times_dropped;
            entry.1 += qty;
        }
    }

    let mut sources = Vec::new();
    for ((enemy_name, zone), (times_dropped, total_quantity)) in per_source {
        let zf = ZoneFilter::from_stored(zone.clone());
        let total_kills = match scope.as_str() {
            "mine" => mine_total_kills(&conn, &enemy_name, &zf, &lf)?,
            "imported" => imported_total_kills(&conn, &enemy_name, &zf, &lf)?,
            _ => {
                mine_total_kills(&conn, &enemy_name, &zf, &lf)?
                    + imported_total_kills(&conn, &enemy_name, &zf, &lf)?
            }
        };
        sources.push(ItemDropSource {
            enemy_name,
            zone,
            total_kills,
            times_dropped,
            total_quantity,
            drop_rate: if total_kills > 0 {
                times_dropped as f64 / total_kills as f64
            } else {
                0.0
            },
        });
    }
    sources.sort_by(|a, b| {
        b.times_dropped
            .cmp(&a.times_dropped)
            .then(b.total_quantity.cmp(&a.total_quantity))
    });

    Ok(sources)
}

// ── Search (Database tab) ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct EnemySearchResult {
    pub enemy_name: String,
    /// Area key these stats are for (null = unknown zone). One result per
    /// (monster, zone) — the same monster in two zones is two rows.
    pub zone: Option<String>,
    pub total_kills: i64,
    pub distinct_loot_items: i64,
}

#[derive(Serialize)]
pub struct ItemSearchResult {
    pub item_name: String,
    pub total_quantity: i64,
    pub distinct_enemies: i64,
}

#[derive(Serialize)]
pub struct HarvestSearchResult {
    pub item_name: String,
    pub total_quantity: i64,
    /// Distinct corpse types this item was harvested from.
    pub distinct_corpses: i64,
    /// How many harvest pulls yielded this item.
    pub total_extracts: i64,
}

/// List/search enemies in the drop-rate database. An empty `query` returns every
/// enemy in the selected scope (the Database tab loads the full list up front and
/// filters client-side); a non-empty `query` does a case-insensitive substring
/// match. `limit` is optional — `None` returns all rows. Aggregates are computed
/// set-based in a handful of grouped queries rather than per-enemy.
#[tauri::command]
pub fn search_database_enemies(
    db: State<'_, DbPool>,
    query: String,
    scope: String,
    limit: Option<usize>,
    combat_skills: Option<String>,
) -> Result<Vec<EnemySearchResult>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let lf = LoadoutFilter::from_opt(combat_skills.as_deref());
    let lf_binds = || -> Vec<rusqlite::types::Value> { lf.bind().into_iter().collect() };

    // (enemy_name, zone) -> (total_kills, set of distinct loot item names)
    type EnemyAgg = HashMap<(String, Option<String>), (i64, std::collections::HashSet<String>)>;
    let mut agg: EnemyAgg = HashMap::new();

    if scope == "mine" || scope == "combined" {
        let sql = format!(
            "SELECT enemy_name, zone, COUNT(*) FROM enemy_kills{} GROUP BY enemy_name, zone",
            lf.where_clause("combat_skills")
        );
        let mut kill_stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {e}"))?;
        let rows = kill_stmt
            .query_map(rusqlite::params_from_iter(lf_binds().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| format!("Query failed: {e}"))?;
        for row in rows {
            let (enemy_name, zone, kills) = row.map_err(|e| format!("Row error: {e}"))?;
            agg.entry((enemy_name, zone)).or_default().0 += kills;
        }

        let sql = format!(
            "SELECT DISTINCT k.enemy_name, k.zone, l.item_name
             FROM enemy_kill_loot l JOIN enemy_kills k ON l.kill_id = k.id{}",
            lf.where_clause("k.combat_skills")
        );
        let mut loot_stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare loot query: {e}"))?;
        let rows = loot_stmt
            .query_map(rusqlite::params_from_iter(lf_binds().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("Loot query failed: {e}"))?;
        for row in rows {
            let (enemy_name, zone, item_name) = row.map_err(|e| format!("Row error: {e}"))?;
            agg.entry((enemy_name, zone))
                .or_default()
                .1
                .insert(item_name);
        }
    }

    if scope == "imported" || scope == "combined" {
        let sql = format!(
            "SELECT enemy_name, zone, COALESCE(SUM(total_kills), 0) FROM imported_enemy_kills_agg{} GROUP BY enemy_name, zone",
            lf.where_clause("combat_skills")
        );
        let mut kill_stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare imported query: {e}"))?;
        let rows = kill_stmt
            .query_map(rusqlite::params_from_iter(lf_binds().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| format!("Imported query failed: {e}"))?;
        for row in rows {
            let (enemy_name, zone, kills) = row.map_err(|e| format!("Row error: {e}"))?;
            agg.entry((enemy_name, zone)).or_default().0 += kills;
        }

        let sql = format!(
            "SELECT DISTINCT enemy_name, zone, item_name FROM imported_enemy_kill_loot_agg{}",
            lf.where_clause("combat_skills")
        );
        let mut loot_stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare imported loot query: {e}"))?;
        let rows = loot_stmt
            .query_map(rusqlite::params_from_iter(lf_binds().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("Imported loot query failed: {e}"))?;
        for row in rows {
            let (enemy_name, zone, item_name) = row.map_err(|e| format!("Row error: {e}"))?;
            agg.entry((enemy_name, zone))
                .or_default()
                .1
                .insert(item_name);
        }
    }

    let needle = query.trim().to_lowercase();
    let mut results: Vec<EnemySearchResult> = agg
        .into_iter()
        .filter(|((enemy_name, _), _)| {
            needle.is_empty() || enemy_name.to_lowercase().contains(&needle)
        })
        .map(
            |((enemy_name, zone), (total_kills, loot_items))| EnemySearchResult {
                enemy_name,
                zone,
                total_kills,
                distinct_loot_items: loot_items.len() as i64,
            },
        )
        .collect();
    results.sort_by(|a, b| {
        b.total_kills
            .cmp(&a.total_kills)
            .then_with(|| {
                a.enemy_name
                    .to_lowercase()
                    .cmp(&b.enemy_name.to_lowercase())
            })
            .then_with(|| a.zone.cmp(&b.zone))
    });
    if let Some(lim) = limit {
        results.truncate(lim);
    }

    Ok(results)
}

/// List/search items in the drop-rate database. An empty `query` returns every
/// item in the selected scope; a non-empty `query` does a case-insensitive
/// substring match. `limit` is optional — `None` returns all rows.
#[tauri::command]
pub fn search_database_items(
    db: State<'_, DbPool>,
    query: String,
    scope: String,
    limit: Option<usize>,
    combat_skills: Option<String>,
) -> Result<Vec<ItemSearchResult>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let lf = LoadoutFilter::from_opt(combat_skills.as_deref());
    let lf_binds = || -> Vec<rusqlite::types::Value> { lf.bind().into_iter().collect() };

    // item_name -> (total_quantity, set of distinct (enemy, zone) sources)
    type ItemAgg = HashMap<String, (i64, std::collections::HashSet<(String, Option<String>)>)>;
    let mut agg: ItemAgg = HashMap::new();

    if scope == "mine" || scope == "combined" {
        let sql = format!(
            "SELECT l.item_name, SUM(l.quantity), k.enemy_name, k.zone
             FROM enemy_kill_loot l
             JOIN enemy_kills k ON l.kill_id = k.id{}
             GROUP BY l.item_name, k.enemy_name, k.zone",
            lf.where_clause("k.combat_skills")
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare query: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(lf_binds().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| format!("Query failed: {e}"))?;
        for row in rows {
            let (item_name, qty, enemy_name, zone) = row.map_err(|e| format!("Row error: {e}"))?;
            let entry = agg.entry(item_name).or_default();
            entry.0 += qty;
            entry.1.insert((enemy_name, zone));
        }
    }

    if scope == "imported" || scope == "combined" {
        let sql = format!(
            "SELECT item_name, SUM(total_quantity), enemy_name, zone
             FROM imported_enemy_kill_loot_agg{}
             GROUP BY item_name, enemy_name, zone",
            lf.where_clause("combat_skills")
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Failed to prepare imported query: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(lf_binds().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| format!("Imported query failed: {e}"))?;
        for row in rows {
            let (item_name, qty, enemy_name, zone) = row.map_err(|e| format!("Row error: {e}"))?;
            let entry = agg.entry(item_name).or_default();
            entry.0 += qty;
            entry.1.insert((enemy_name, zone));
        }
    }

    let needle = query.trim().to_lowercase();
    let mut results: Vec<ItemSearchResult> = agg
        .into_iter()
        .filter(|(item_name, _)| needle.is_empty() || item_name.to_lowercase().contains(&needle))
        .map(|(item_name, (total_quantity, enemies))| ItemSearchResult {
            item_name,
            total_quantity,
            distinct_enemies: enemies.len() as i64,
        })
        .collect();
    results.sort_by(|a, b| {
        b.total_quantity
            .cmp(&a.total_quantity)
            .then_with(|| a.item_name.to_lowercase().cmp(&b.item_name.to_lowercase()))
    });
    if let Some(lim) = limit {
        results.truncate(lim);
    }

    Ok(results)
}

/// List/search harvested items (skinning/butchering yields from `corpse_extracts`).
/// An empty `query` returns everything harvested; a non-empty `query` does a
/// case-insensitive substring match. Local-only — extracts have no imported
/// counterpart, so there is no scope. `limit` is optional (`None` = all).
#[tauri::command]
pub fn search_database_harvested(
    db: State<'_, DbPool>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<HarvestSearchResult>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT item_name, SUM(quantity), COUNT(DISTINCT corpse_name), COUNT(*)
             FROM corpse_extracts
             GROUP BY item_name",
        )
        .map_err(|e| format!("Failed to prepare harvested query: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(HarvestSearchResult {
                item_name: row.get::<_, String>(0)?,
                total_quantity: row.get::<_, i64>(1)?,
                distinct_corpses: row.get::<_, i64>(2)?,
                total_extracts: row.get::<_, i64>(3)?,
            })
        })
        .map_err(|e| format!("Harvested query failed: {e}"))?;

    let needle = query.trim().to_lowercase();
    let mut results: Vec<HarvestSearchResult> = Vec::new();
    for row in rows {
        let row = row.map_err(|e| format!("Row error: {e}"))?;
        if needle.is_empty() || row.item_name.to_lowercase().contains(&needle) {
            results.push(row);
        }
    }
    results.sort_by(|a, b| {
        b.total_quantity
            .cmp(&a.total_quantity)
            .then_with(|| a.item_name.to_lowercase().cmp(&b.item_name.to_lowercase()))
    });
    if let Some(lim) = limit {
        results.truncate(lim);
    }

    Ok(results)
}

// ── Import / Export (community drop-rate sharing) ──────────────────────────

#[derive(Serialize, Deserialize)]
struct ExportedLoot {
    item_name: String,
    total_quantity: i64,
    times_dropped: i64,
}

#[derive(Serialize, Deserialize)]
struct ExportedEnemy {
    enemy_name: String,
    /// Area key (None = unknown zone). The same monster name in two zones is two
    /// `ExportedEnemy` entries. `#[serde(default)]` lets legacy JSON (no zone)
    /// deserialize as None.
    #[serde(default)]
    zone: Option<String>,
    total_kills: i64,
    loot: Vec<ExportedLoot>,
}

/// Legacy JSON export format, still accepted on import for backward
/// compatibility. `format_version`/`exported_at` are only consumed by serde
/// during deserialization (never read directly).
#[derive(Deserialize)]
#[allow(dead_code)]
struct ExportBundle {
    format_version: u32,
    exported_at: String,
    enemies: Vec<ExportedEnemy>,
}

/// True if `path` names a SQLite database by extension (.db / .sqlite / .sqlite3).
/// Drives the export writer choice; the file is one we create, so its extension is
/// the user's intent. (Import detects SQLite by file header instead — see
/// `file_is_sqlite` — since an imported file's extension can't be trusted.)
fn is_sqlite_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            e.eq_ignore_ascii_case("db")
                || e.eq_ignore_ascii_case("sqlite")
                || e.eq_ignore_ascii_case("sqlite3")
        })
        .unwrap_or(false)
}

/// Build the (enemy, zone) → aggregate counts the player has personally observed
/// (never previously imported data), shared by every export writer. Loot is sorted
/// most-dropped-first for readability. Sums across loadouts (per enemy + zone); the
/// loadout dimension is a live query-time filter, not part of a shared export. No
/// character, server, or timestamp data is included — only aggregate counts.
fn collect_export_enemies<C: DbRead + ?Sized>(conn: &C) -> Result<Vec<ExportedEnemy>, String> {
    let mut pair_stmt = conn
        .prepare("SELECT DISTINCT enemy_name, zone FROM enemy_kills ORDER BY enemy_name, zone")
        .map_err(|e| format!("Failed to prepare query: {e}"))?;
    let pairs: Vec<(String, Option<String>)> = pair_stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| format!("Query failed: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    let lf = LoadoutFilter::All;
    let mut enemies = Vec::with_capacity(pairs.len());
    for (enemy_name, zone) in pairs {
        let zf = ZoneFilter::from_stored(zone.clone());
        let total_kills = mine_total_kills(conn, &enemy_name, &zf, &lf)?;
        let mut loot = mine_loot_rows(conn, &enemy_name, &zf, &lf)?;
        // Most-dropped first for readability.
        loot.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));
        enemies.push(ExportedEnemy {
            enemy_name,
            zone,
            total_kills,
            loot: loot
                .into_iter()
                .map(|(item_name, total_quantity, times_dropped)| ExportedLoot {
                    item_name,
                    total_quantity,
                    times_dropped,
                })
                .collect(),
        });
    }
    Ok(enemies)
}

/// Write the aggregated drop data as CSV — the spreadsheet-friendly share format.
/// One row per (enemy, zone, looted item); an (enemy, zone) with kills but no
/// recorded loot gets one row with empty item columns so its kill count (the
/// drop-rate denominator) is preserved. The `zone` column is the internal area key
/// (empty = unknown zone). A derived `drop_rate` column (times_dropped /
/// total_kills, 0–1) is included for easy viewing and ignored on re-import. Opens
/// cleanly in a spreadsheet and lets others build/import libraries with the same
/// columns.
fn write_csv_export(path: &str, enemies: &[ExportedEnemy]) -> Result<(), String> {
    let mut wtr = csv::WriterBuilder::new().from_writer(Vec::new());
    wtr.write_record([
        "enemy_name",
        "zone",
        "total_kills",
        "item_name",
        "total_quantity",
        "times_dropped",
        "drop_rate",
    ])
    .map_err(|e| format!("Failed to write CSV header: {e}"))?;

    for enemy in enemies {
        let zone_str = enemy.zone.as_deref().unwrap_or("");
        if enemy.loot.is_empty() {
            wtr.write_record([
                enemy.enemy_name.as_str(),
                zone_str,
                &enemy.total_kills.to_string(),
                "",
                "",
                "",
                "",
            ])
            .map_err(|e| format!("Failed to write CSV row: {e}"))?;
            continue;
        }
        for loot in &enemy.loot {
            let rate = if enemy.total_kills > 0 {
                loot.times_dropped as f64 / enemy.total_kills as f64
            } else {
                0.0
            };
            wtr.write_record([
                enemy.enemy_name.as_str(),
                zone_str,
                &enemy.total_kills.to_string(),
                &loot.item_name,
                &loot.total_quantity.to_string(),
                &loot.times_dropped.to_string(),
                &format!("{rate:.4}"),
            ])
            .map_err(|e| format!("Failed to write CSV row: {e}"))?;
        }
    }

    let data = wtr
        .into_inner()
        .map_err(|e| format!("Failed to finalize CSV: {e}"))?;
    fs::write(path, data).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(())
}

/// Write the aggregated drop data as a standalone SQLite database — a portable,
/// loss-free share format that round-trips back through import without the CSV's
/// empty-placeholder rows. Schema: `meta(key, value)` plus `enemies(enemy_name,
/// zone, total_kills)` and `loot(enemy_name, zone, item_name, total_quantity,
/// times_dropped)`, with `zone` left NULL for the unknown-zone bucket. Any existing
/// file at `path` is replaced — the save dialog already confirmed the overwrite, and
/// starting clean keeps a prior export's tables from lingering.
fn write_sqlite_export(path: &str, enemies: &[ExportedEnemy]) -> Result<(), String> {
    let _ = fs::remove_file(path);
    let mut conn = rusqlite::Connection::open(path)
        .map_err(|e| format!("Failed to create SQLite file: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE enemies (enemy_name TEXT NOT NULL, zone TEXT, total_kills INTEGER NOT NULL);
         CREATE TABLE loot (
             enemy_name TEXT NOT NULL, zone TEXT, item_name TEXT NOT NULL,
             total_quantity INTEGER NOT NULL, times_dropped INTEGER NOT NULL
         );",
    )
    .map_err(|e| format!("Failed to create SQLite schema: {e}"))?;
    conn.execute(
        "INSERT INTO meta (key, value) VALUES
            ('format', 'glogger-drop-rates'),
            ('format_version', '1'),
            ('exported_at', datetime('now'))",
        [],
    )
    .map_err(|e| format!("Failed to write export metadata: {e}"))?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to start transaction: {e}"))?;
    {
        let mut enemy_stmt = tx
            .prepare("INSERT INTO enemies (enemy_name, zone, total_kills) VALUES (?1, ?2, ?3)")
            .map_err(|e| format!("Failed to prepare enemy insert: {e}"))?;
        let mut loot_stmt = tx
            .prepare(
                "INSERT INTO loot (enemy_name, zone, item_name, total_quantity, times_dropped)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|e| format!("Failed to prepare loot insert: {e}"))?;
        for enemy in enemies {
            enemy_stmt
                .execute(rusqlite::params![
                    enemy.enemy_name,
                    enemy.zone,
                    enemy.total_kills
                ])
                .map_err(|e| format!("Failed to write enemy '{}': {e}", enemy.enemy_name))?;
            for loot in &enemy.loot {
                loot_stmt
                    .execute(rusqlite::params![
                        enemy.enemy_name,
                        enemy.zone,
                        loot.item_name,
                        loot.total_quantity,
                        loot.times_dropped
                    ])
                    .map_err(|e| format!("Failed to write loot row: {e}"))?;
            }
        }
    }
    tx.commit()
        .map_err(|e| format!("Failed to commit SQLite export: {e}"))?;
    Ok(())
}

/// Export the player's own personally-observed kills/loot (never previously
/// imported data) to `path`. The format follows `path`'s extension: a SQLite
/// database for `.db`/`.sqlite`/`.sqlite3` (see `write_sqlite_export`), otherwise
/// CSV (see `write_csv_export`). Returns the number of (enemy, zone) entries
/// written.
#[tauri::command]
pub fn export_kill_loot_database(db: State<'_, DbPool>, path: String) -> Result<usize, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let enemies = collect_export_enemies(&conn)?;
    if is_sqlite_path(&path) {
        write_sqlite_export(&path, &enemies)?;
    } else {
        write_csv_export(&path, &enemies)?;
    }
    Ok(enemies.len())
}

#[derive(Serialize)]
pub struct ImportSummary {
    pub source_label: String,
    pub enemies_imported: usize,
    pub loot_rows_imported: usize,
}

/// Parse an import file into a list of enemies. Accepts the CSV format produced
/// by `export_kill_loot_database` — and tolerates external spreadsheets sharing
/// those columns (header order independent, a few common aliases accepted) — plus
/// the legacy JSON bundle format for backward compatibility with older exports.
fn parse_drop_data(content: &str) -> Result<Vec<ExportedEnemy>, String> {
    // Strip a leading UTF-8 BOM (Excel adds one), which would otherwise break
    // both the JSON sniff and the first CSV header match.
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    // Legacy JSON bundle: older exports are a JSON object (`{ ... }`).
    if content.trim_start().starts_with('{') {
        let bundle: ExportBundle =
            serde_json::from_str(content).map_err(|e| format!("Failed to parse JSON file: {e}"))?;
        return Ok(bundle.enemies);
    }
    parse_csv_drop_data(content)
}

/// Parse a CSV into enemies, auto-detecting the layout:
/// - **Aggregated** (our own export, or any sheet with `total_kills` /
///   `total_quantity` / `times_dropped`): one row per (enemy, item) with
///   precomputed counts.
/// - **Raw loot-event log** (one row per looted item, with a per-corpse id column
///   like `enemy_id`): aggregated here — distinct corpse ids per enemy = kills,
///   distinct corpse ids that yielded an item = times_dropped, `Item_Amount`
///   summed = quantity. Lets people import their own raw collection logs even if
///   they never computed drop rates themselves.
fn parse_csv_drop_data(content: &str) -> Result<Vec<ExportedEnemy>, String> {
    let mut head_rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());
    let headers = head_rdr
        .headers()
        .map_err(|e| format!("Failed to read CSV header: {e}"))?
        .clone();
    let norm = |s: &str| s.trim().to_lowercase().replace(' ', "_");
    let has = |aliases: &[&str]| headers.iter().any(|h| aliases.contains(&norm(h).as_str()));

    let has_aggregate_cols = has(&[
        "total_kills",
        "kills",
        "total_quantity",
        "quantity",
        "qty",
        "times_dropped",
        "drops",
    ]);
    let has_corpse_id = has(&[
        "enemy_id",
        "enemy_entity_id",
        "corpse_id",
        "corpse_entity_id",
    ]);

    if has_corpse_id && !has_aggregate_cols {
        parse_csv_raw_events(content)
    } else {
        parse_csv_aggregated(content)
    }
}

/// Aggregate a raw per-loot-event CSV (one row per looted item). A per-corpse id
/// column (`enemy_id`/`corpse_id`) distinguishes individual kills: distinct corpse
/// ids per enemy = total_kills, distinct corpse ids that yielded an item =
/// times_dropped, summed item amounts = total_quantity. When an event-type column
/// is present (e.g. `log_event_description`), only rows whose value contains
/// "loot" are counted, so any non-loot rows don't inflate kill counts.
///
/// Note: corpses searched that dropped nothing won't appear in such a log, so the
/// kill denominator can be slightly low (drop rates a touch high) — an inherent
/// limit of loot-only logs, not this importer.
fn parse_csv_raw_events(content: &str) -> Result<Vec<ExportedEnemy>, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());

    let headers = rdr
        .headers()
        .map_err(|e| format!("Failed to read CSV header: {e}"))?
        .clone();
    let norm = |s: &str| s.trim().to_lowercase().replace(' ', "_");
    let find = |aliases: &[&str]| -> Option<usize> {
        headers
            .iter()
            .position(|h| aliases.contains(&norm(h).as_str()))
    };

    let enemy_idx = find(&["enemy_name", "enemy", "monster", "monster_name"]).ok_or_else(|| {
        "CSV is missing an enemy column (expected a header like 'enemy_name').".to_string()
    })?;
    let corpse_idx = find(&[
        "enemy_id",
        "enemy_entity_id",
        "corpse_id",
        "corpse_entity_id",
    ])
    .ok_or_else(|| "CSV is missing a per-corpse id column (expected 'enemy_id').".to_string())?;
    // Prefer a display-name item column (first header match wins; e.g. "Item_Name"
    // comes before an internal "item_name" column).
    let item_idx = find(&["item_name", "item", "item_display_name"]);
    let qty_idx = find(&["item_amount", "amount", "total_quantity", "quantity", "qty"]);
    let event_idx = find(&[
        "log_event_description",
        "event",
        "event_description",
        "event_type",
    ]);
    let zone_idx = find(&[
        "zone",
        "area",
        "area_key",
        "zone_id",
        "zone_name",
        "zone_key",
    ]);

    let parse_qty = |s: &str| -> i64 {
        let t = s.trim().replace(',', "");
        if t.is_empty() {
            return 1;
        }
        let v = t
            .parse::<i64>()
            .ok()
            .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
            .unwrap_or(1);
        if v <= 0 {
            1
        } else {
            v
        }
    };

    use std::collections::{HashMap, HashSet};
    struct Acc {
        kills: HashSet<String>,
        items: HashMap<String, (i64, HashSet<String>)>,
        item_order: Vec<String>,
    }
    // Keyed by (enemy_name, zone) — drop rates are per zone.
    type Key = (String, Option<String>);
    let mut order: Vec<Key> = Vec::new();
    let mut map: HashMap<Key, Acc> = HashMap::new();

    for (i, result) in rdr.records().enumerate() {
        let rec = result.map_err(|e| format!("CSV parse error on row {}: {e}", i + 2))?;

        // Restrict to loot events when an event-type column exists.
        if let Some(ei) = event_idx {
            if !rec.get(ei).unwrap_or("").to_lowercase().contains("loot") {
                continue;
            }
        }

        let enemy = rec.get(enemy_idx).unwrap_or("").trim().to_string();
        let corpse = rec.get(corpse_idx).unwrap_or("").trim().to_string();
        if enemy.is_empty() || corpse.is_empty() {
            continue;
        }
        let zone = zone_idx
            .and_then(|z| rec.get(z))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let key = (enemy, zone);

        if !map.contains_key(&key) {
            order.push(key.clone());
            map.insert(
                key.clone(),
                Acc {
                    kills: HashSet::new(),
                    items: HashMap::new(),
                    item_order: Vec::new(),
                },
            );
        }
        let acc = map.get_mut(&key).unwrap();
        acc.kills.insert(corpse.clone());

        if let Some(ix) = item_idx {
            let item = rec.get(ix).unwrap_or("").trim().to_string();
            if !item.is_empty() {
                let qty = qty_idx
                    .map(|q| parse_qty(rec.get(q).unwrap_or("")))
                    .unwrap_or(1);
                if !acc.items.contains_key(&item) {
                    acc.item_order.push(item.clone());
                }
                let e = acc.items.entry(item).or_insert((0, HashSet::new()));
                e.0 += qty;
                e.1.insert(corpse.clone());
            }
        }
    }

    if map.is_empty() {
        return Err("No loot rows found in CSV.".to_string());
    }

    let mut enemies = Vec::with_capacity(order.len());
    for key in order {
        let (enemy_name, zone) = key.clone();
        let Acc {
            kills,
            items,
            item_order,
        } = map.remove(&key).unwrap();
        let total_kills = kills.len() as i64;
        let loot = item_order
            .into_iter()
            .map(|item_name| {
                let (qty, corpses) = &items[&item_name];
                ExportedLoot {
                    item_name: item_name.clone(),
                    total_quantity: *qty,
                    times_dropped: corpses.len() as i64,
                }
            })
            .collect();
        enemies.push(ExportedEnemy {
            enemy_name,
            zone,
            total_kills,
            loot,
        });
    }
    Ok(enemies)
}

/// Parse the aggregated CSV drop-rate format. Required column: an enemy name.
/// Optional: total_kills, item_name, total_quantity, times_dropped (any order;
/// common header aliases accepted). Duplicate (enemy, item) rows are summed; an
/// enemy's total_kills is taken as the max seen across its rows (the export
/// repeats it).
fn parse_csv_aggregated(content: &str) -> Result<Vec<ExportedEnemy>, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());

    let headers = rdr
        .headers()
        .map_err(|e| format!("Failed to read CSV header: {e}"))?
        .clone();
    let norm = |s: &str| s.trim().to_lowercase().replace(' ', "_");
    let find = |aliases: &[&str]| -> Option<usize> {
        headers
            .iter()
            .position(|h| aliases.contains(&norm(h).as_str()))
    };

    let enemy_idx = find(&["enemy_name", "enemy", "monster", "monster_name"]).ok_or_else(|| {
        "CSV is missing an enemy column (expected a header like 'enemy_name').".to_string()
    })?;
    let kills_idx = find(&["total_kills", "kills"]);
    let item_idx = find(&["item_name", "item"]);
    let qty_idx = find(&["total_quantity", "quantity", "qty"]);
    let drops_idx = find(&["times_dropped", "drops", "times"]);
    let zone_idx = find(&[
        "zone",
        "area",
        "area_key",
        "zone_id",
        "zone_name",
        "zone_key",
    ]);

    let parse_int = |s: &str| -> Option<i64> {
        let t = s.trim().replace(',', "");
        if t.is_empty() {
            return None;
        }
        t.parse::<i64>()
            .ok()
            .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
    };

    use std::collections::HashMap;
    // Keyed by (enemy_name, zone) — drop rates are per zone.
    type Key = (String, Option<String>);
    let mut order: Vec<Key> = Vec::new();
    // (enemy, zone) -> (max_kills, item -> (qty, drops), item insertion order)
    let mut map: HashMap<Key, (i64, HashMap<String, (i64, i64)>, Vec<String>)> = HashMap::new();

    for (i, result) in rdr.records().enumerate() {
        let rec = result.map_err(|e| format!("CSV parse error on row {}: {e}", i + 2))?;
        let enemy = rec.get(enemy_idx).unwrap_or("").trim().to_string();
        if enemy.is_empty() {
            continue;
        }
        let zone = zone_idx
            .and_then(|z| rec.get(z))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let key = (enemy, zone);
        if !map.contains_key(&key) {
            order.push(key.clone());
            map.insert(key.clone(), (0, HashMap::new(), Vec::new()));
        }
        let entry = map.get_mut(&key).unwrap();
        if let Some(k) = kills_idx.and_then(|ix| rec.get(ix)).and_then(parse_int) {
            if k > entry.0 {
                entry.0 = k;
            }
        }
        if let Some(ix) = item_idx {
            let item = rec.get(ix).unwrap_or("").trim().to_string();
            if !item.is_empty() {
                let qty = qty_idx
                    .and_then(|q| rec.get(q))
                    .and_then(parse_int)
                    .unwrap_or(0);
                let drops = drops_idx
                    .and_then(|d| rec.get(d))
                    .and_then(parse_int)
                    .unwrap_or(0);
                if !entry.1.contains_key(&item) {
                    entry.2.push(item.clone());
                }
                let li = entry.1.entry(item).or_insert((0, 0));
                li.0 += qty;
                li.1 += drops;
            }
        }
    }

    if map.is_empty() {
        return Err("No data rows found in CSV.".to_string());
    }

    let mut enemies = Vec::with_capacity(order.len());
    for key in order {
        let (enemy_name, zone) = key.clone();
        let (total_kills, items, item_order) = map.remove(&key).unwrap();
        let loot = item_order
            .into_iter()
            .map(|item_name| {
                let (total_quantity, times_dropped) = items[&item_name];
                ExportedLoot {
                    item_name,
                    total_quantity,
                    times_dropped,
                }
            })
            .collect();
        enemies.push(ExportedEnemy {
            enemy_name,
            zone,
            total_kills,
            loot,
        });
    }
    Ok(enemies)
}

/// Detect a SQLite database by its 16-byte file header magic. Import uses this
/// rather than the extension because an imported file's name can't be trusted
/// (and reading a binary SQLite file as UTF-8 text would fail or produce garbage).
fn file_is_sqlite(path: &str) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 16];
    f.read_exact(&mut magic).is_ok() && &magic == b"SQLite format 3\0"
}

/// Read a glogger SQLite drop-rate export (written by `write_sqlite_export`) into
/// the shared `ExportedEnemy` list the importer merges. Opened read-only. Loot is
/// attached to its (enemy, zone) and ordered most-dropped-first. Errors clearly if
/// the file is some other SQLite database (no `enemies` table).
fn parse_sqlite_drop_data(path: &str) -> Result<Vec<ExportedEnemy>, String> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("Failed to open SQLite file: {e}"))?;

    let table_exists = |name: &str| -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(true),
        )
        .unwrap_or(false)
    };
    if !table_exists("enemies") {
        return Err(
            "This SQLite file isn't a glogger drop-rate export (no 'enemies' table).".to_string(),
        );
    }

    // Loot grouped by (enemy, zone). A malformed export with no `loot` table just
    // yields lootless enemies rather than erroring.
    use std::collections::HashMap;
    type Key = (String, Option<String>);
    let mut loot_map: HashMap<Key, Vec<ExportedLoot>> = HashMap::new();
    if table_exists("loot") {
        let mut stmt = conn
            .prepare(
                "SELECT enemy_name, zone, item_name, total_quantity, times_dropped
                 FROM loot ORDER BY enemy_name, zone, times_dropped DESC, total_quantity DESC",
            )
            .map_err(|e| format!("Failed to read loot table: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    ExportedLoot {
                        item_name: row.get::<_, String>(2)?,
                        total_quantity: row.get::<_, i64>(3)?,
                        times_dropped: row.get::<_, i64>(4)?,
                    },
                ))
            })
            .map_err(|e| format!("Loot query failed: {e}"))?;
        for r in rows {
            let (enemy_name, zone, loot) = r.map_err(|e| format!("Loot row error: {e}"))?;
            loot_map.entry((enemy_name, zone)).or_default().push(loot);
        }
    }

    let mut stmt = conn
        .prepare("SELECT enemy_name, zone, total_kills FROM enemies ORDER BY enemy_name, zone")
        .map_err(|e| format!("Failed to read enemies table: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("Enemies query failed: {e}"))?;

    let mut enemies = Vec::new();
    for r in rows {
        let (enemy_name, zone, total_kills) = r.map_err(|e| format!("Enemy row error: {e}"))?;
        let loot = loot_map
            .remove(&(enemy_name.clone(), zone.clone()))
            .unwrap_or_default();
        enemies.push(ExportedEnemy {
            enemy_name,
            zone,
            total_kills,
            loot,
        });
    }
    Ok(enemies)
}

/// Import a previously-exported drop-rate file. Accepts a glogger SQLite export
/// (detected by file header), the CSV export, a friend's raw loot-event CSV, or a
/// legacy JSON bundle. The data merges permanently into the local database —
/// removing the source from the "Imported Sources" list (see
/// `delete_imported_source`) no longer deletes it. Tagged by the file's name
/// (`source_label`) so re-importing the same file replaces just that source's rows
/// instead of double-counting. Never touches the player's own
/// `enemy_kills`/`enemy_kill_loot` ground truth.
#[tauri::command]
pub fn import_kill_loot_database(
    db: State<'_, DbPool>,
    path: String,
) -> Result<ImportSummary, String> {
    let enemies = if file_is_sqlite(&path) {
        parse_sqlite_drop_data(&path)?
    } else {
        let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {e}"))?;
        parse_drop_data(&content)?
    };

    let source_label = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported".to_string());

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    // Idempotent re-import: clear any prior rows for this exact source label.
    tx.execute(
        "DELETE FROM imported_enemy_kill_loot_agg WHERE source_label = ?1",
        [&source_label],
    )
    .map_err(|e| format!("Failed to clear prior loot rows: {e}"))?;
    tx.execute(
        "DELETE FROM imported_enemy_kills_agg WHERE source_label = ?1",
        [&source_label],
    )
    .map_err(|e| format!("Failed to clear prior kill rows: {e}"))?;
    tx.execute(
        "INSERT INTO imported_kill_sources (source_label, display_name, imported_at)
         VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(source_label) DO UPDATE SET imported_at = CURRENT_TIMESTAMP",
        rusqlite::params![source_label, source_label],
    )
    .map_err(|e| format!("Failed to record import source: {e}"))?;

    let mut loot_rows_imported = 0usize;
    for enemy in &enemies {
        tx.execute(
            "INSERT INTO imported_enemy_kills_agg (source_label, enemy_name, total_kills, zone) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![source_label, enemy.enemy_name, enemy.total_kills, enemy.zone],
        )
        .map_err(|e| format!("Failed to import enemy '{}': {e}", enemy.enemy_name))?;

        for loot in &enemy.loot {
            tx.execute(
                "INSERT INTO imported_enemy_kill_loot_agg
                    (source_label, enemy_name, item_name, total_quantity, times_dropped, zone)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    source_label,
                    enemy.enemy_name,
                    loot.item_name,
                    loot.total_quantity,
                    loot.times_dropped,
                    enemy.zone
                ],
            )
            .map_err(|e| format!("Failed to import loot row: {e}"))?;
            loot_rows_imported += 1;
        }
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit import: {e}"))?;

    Ok(ImportSummary {
        source_label,
        enemies_imported: enemies.len(),
        loot_rows_imported,
    })
}

#[derive(Serialize)]
pub struct ImportedSource {
    pub source_label: String,
    pub display_name: String,
    pub imported_at: String,
    pub enemy_count: i64,
}

#[tauri::command]
pub fn list_imported_sources(db: State<'_, DbPool>) -> Result<Vec<ImportedSource>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT s.source_label, s.display_name, datetime(s.imported_at),
                    (SELECT COUNT(*) FROM imported_enemy_kills_agg a WHERE a.source_label = s.source_label)
             FROM imported_kill_sources s
             ORDER BY s.imported_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ImportedSource {
                source_label: row.get(0)?,
                display_name: row.get(1)?,
                imported_at: row.get(2)?,
                enemy_count: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut sources = Vec::new();
    for row in rows {
        sources.push(row.map_err(|e| format!("Row error: {e}"))?);
    }
    Ok(sources)
}

/// Remove an entry from the "Imported Sources" list. This only drops the
/// bookkeeping row; the imported kill/loot data stays merged in the database
/// permanently (migration v53 removed the `ON DELETE CASCADE` that used to wipe
/// it). The aggregate rows keep their `source_label`, so re-importing the same
/// file later still cleanly replaces them rather than double-counting.
#[tauri::command]
pub fn delete_imported_source(db: State<'_, DbPool>, source_label: String) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    conn.execute(
        "DELETE FROM imported_kill_sources WHERE source_label = ?1",
        [&source_label],
    )
    .map_err(|e| format!("Failed to delete import source: {e}"))?;
    Ok(())
}

#[derive(Serialize)]
pub struct ExtractDetail {
    pub corpse_name: Option<String>,
    pub skill: String,
    pub times: i64,
    pub total_quantity: i64,
    /// Representative (current) values. Skill/anatomy levels only ever rise, so
    /// MAX is the player's latest level; equipment bonus is stable per setup.
    pub skill_level: Option<i64>,
    pub equipment_bonus: Option<i64>,
    pub anatomy_family: Option<String>,
    pub anatomy_level: Option<i64>,
}

/// Per-corpse butchering/skinning detail for an item: the conditions under which
/// it was harvested (Butchering/Skinning level, equipment bonus, and the anatomy
/// family + level for that monster type). Drives the farming item hover tooltip.
#[tauri::command]
pub fn get_corpse_extract_details(
    db: State<'_, DbPool>,
    item_name: String,
) -> Result<Vec<ExtractDetail>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT corpse_name, skill, COUNT(*) AS times, SUM(quantity) AS total_quantity,
                    MAX(skill_level), MAX(equipment_bonus), anatomy_family, MAX(anatomy_level)
             FROM corpse_extracts
             WHERE item_name = ?1
             GROUP BY corpse_name, skill, anatomy_family
             ORDER BY total_quantity DESC",
        )
        .map_err(|e| format!("Failed to prepare extract query: {e}"))?;
    let rows = stmt
        .query_map([&item_name], |row| {
            Ok(ExtractDetail {
                corpse_name: row.get(0)?,
                skill: row.get(1)?,
                times: row.get(2)?,
                total_quantity: row.get(3)?,
                skill_level: row.get(4)?,
                equipment_bonus: row.get(5)?,
                anatomy_family: row.get(6)?,
                anatomy_level: row.get(7)?,
            })
        })
        .map_err(|e| format!("Failed to query extract details: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read extract details: {e}"))?;
    Ok(rows)
}

#[derive(Serialize)]
pub struct EnemyHarvestStat {
    pub item_name: String,
    /// "Butchering" or "Skinning".
    pub skill: String,
    pub total_quantity: i64,
    /// How many harvest pulls yielded this item.
    pub times: i64,
}

#[derive(Serialize)]
pub struct EnemyHarvestStats {
    pub corpse_name: String,
    /// Total harvest yields recorded for this corpse type.
    pub total_extracts: i64,
    pub harvests: Vec<EnemyHarvestStat>,
}

/// Harvest (skinning/butchering) breakdown for a given corpse/monster name — what
/// it extracts into and how often. The `corpse_extracts.corpse_name` is the same
/// "Search Corpse of X" name stored as `enemy_kills.enemy_name`, so callers pass
/// the monster's display name. Local-only — extracts have no imported counterpart.
#[tauri::command]
pub fn get_enemy_harvest_stats(
    db: State<'_, DbPool>,
    corpse_name: String,
) -> Result<EnemyHarvestStats, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT item_name, skill, SUM(quantity), COUNT(*)
             FROM corpse_extracts
             WHERE corpse_name = ?1
             GROUP BY item_name, skill
             ORDER BY SUM(quantity) DESC",
        )
        .map_err(|e| format!("Failed to prepare harvest query: {e}"))?;
    let rows = stmt
        .query_map([&corpse_name], |row| {
            Ok(EnemyHarvestStat {
                item_name: row.get::<_, String>(0)?,
                skill: row.get::<_, String>(1)?,
                total_quantity: row.get::<_, i64>(2)?,
                times: row.get::<_, i64>(3)?,
            })
        })
        .map_err(|e| format!("Harvest query failed: {e}"))?;
    let mut harvests = Vec::new();
    let mut total_extracts = 0i64;
    for row in rows {
        let row = row.map_err(|e| format!("Harvest row error: {e}"))?;
        total_extracts += row.times;
        harvests.push(row);
    }
    Ok(EnemyHarvestStats {
        corpse_name,
        total_extracts,
        harvests,
    })
}

#[cfg(test)]
mod tests {
    use crate::db::migrations::run_migrations;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // Mirror production: cascades are only meaningful with FKs enforced.
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn, None).unwrap();
        conn
    }

    /// After migration v53, removing an entry from the Imported Sources list
    /// (i.e. deleting the `imported_kill_sources` row) must NOT delete the
    /// imported drop data — it stays merged in the database permanently.
    #[test]
    fn removing_an_imported_source_keeps_its_merged_data() {
        let conn = setup();

        // Record a source + its aggregate rows, as import_kill_loot_database does.
        conn.execute(
            "INSERT INTO imported_kill_sources (source_label, display_name) VALUES ('friend.json', 'friend.json')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO imported_enemy_kills_agg (source_label, enemy_name, total_kills) VALUES ('friend.json', 'Goblin', 42)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO imported_enemy_kill_loot_agg (source_label, enemy_name, item_name, total_quantity, times_dropped)
             VALUES ('friend.json', 'Goblin', 'Gold Coin', 100, 30)",
            [],
        )
        .unwrap();

        // Remove the source from the list (what delete_imported_source does).
        conn.execute(
            "DELETE FROM imported_kill_sources WHERE source_label = 'friend.json'",
            [],
        )
        .unwrap();

        let kills: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(total_kills), 0) FROM imported_enemy_kills_agg WHERE enemy_name = 'Goblin'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            kills, 42,
            "imported kill aggregate should survive source removal"
        );

        let loot: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(total_quantity), 0) FROM imported_enemy_kill_loot_agg WHERE item_name = 'Gold Coin'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            loot, 100,
            "imported loot aggregate should survive source removal"
        );
    }

    /// Re-importing the same file after its source entry was removed replaces the
    /// orphaned rows (matched by `source_label`) rather than double-counting.
    #[test]
    fn reimport_after_removal_replaces_rather_than_doublecounts() {
        let conn = setup();

        conn.execute(
            "INSERT INTO imported_enemy_kills_agg (source_label, enemy_name, total_kills) VALUES ('friend.json', 'Goblin', 42)",
            [],
        )
        .unwrap();

        // A re-import clears prior rows for the same label first, then re-inserts.
        conn.execute(
            "DELETE FROM imported_enemy_kills_agg WHERE source_label = 'friend.json'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO imported_enemy_kills_agg (source_label, enemy_name, total_kills) VALUES ('friend.json', 'Goblin', 42)",
            [],
        )
        .unwrap();

        let kills: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(total_kills), 0) FROM imported_enemy_kills_agg WHERE enemy_name = 'Goblin'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kills, 42, "re-import should replace, not double-count");
    }

    /// A SQLite export written by `write_sqlite_export` round-trips back through
    /// `parse_sqlite_drop_data` with identical enemies, zones, kills, and loot —
    /// including a lootless enemy and the unknown-zone (NULL) bucket — and the
    /// written file is recognized as SQLite by header magic.
    #[test]
    fn sqlite_export_round_trips() {
        use super::{ExportedEnemy, ExportedLoot};

        let path = std::env::temp_dir().join(format!(
            "glogger_sqlite_roundtrip_{}.db",
            std::process::id()
        ));
        let path_str = path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);

        let original = vec![
            ExportedEnemy {
                enemy_name: "Sand Dog".to_string(),
                zone: Some("AreaDesert".to_string()),
                total_kills: 26,
                loot: vec![
                    ExportedLoot {
                        item_name: "Gold Nugget".into(),
                        total_quantity: 5,
                        times_dropped: 5,
                    },
                    ExportedLoot {
                        item_name: "Watercress".into(),
                        total_quantity: 2,
                        times_dropped: 2,
                    },
                ],
            },
            // Comma in the name (no CSV quoting here) + lootless + unknown zone.
            ExportedEnemy {
                enemy_name: "Late, Great Beast".to_string(),
                zone: None,
                total_kills: 10,
                loot: vec![],
            },
        ];

        super::write_sqlite_export(&path_str, &original).expect("write sqlite export");
        assert!(
            super::file_is_sqlite(&path_str),
            "written file should be detected as SQLite"
        );

        let parsed = super::parse_sqlite_drop_data(&path_str).expect("parse sqlite export");
        let _ = std::fs::remove_file(&path);

        assert_eq!(parsed.len(), 2);

        let dog = parsed.iter().find(|e| e.enemy_name == "Sand Dog").unwrap();
        assert_eq!(dog.zone.as_deref(), Some("AreaDesert"));
        assert_eq!(dog.total_kills, 26);
        assert_eq!(dog.loot.len(), 2);
        // Loot comes back most-dropped first.
        assert_eq!(dog.loot[0].item_name, "Gold Nugget");
        assert_eq!(dog.loot[0].times_dropped, 5);

        let beast = parsed
            .iter()
            .find(|e| e.enemy_name == "Late, Great Beast")
            .unwrap();
        assert_eq!(beast.zone, None, "unknown zone round-trips as NULL/None");
        assert_eq!(beast.total_kills, 10);
        assert!(
            beast.loot.is_empty(),
            "lootless enemy survives with no loot rows"
        );
    }

    /// CSV parsing: groups rows by enemy, sums duplicate (enemy, item) rows,
    /// preserves an enemy that has kills but no loot row, and handles a
    /// comma-containing name (quoted field).
    #[test]
    fn parse_csv_groups_sums_and_keeps_lootless_enemy() {
        let csv = "enemy_name,total_kills,item_name,total_quantity,times_dropped,drop_rate\n\
                   Sand Dog,26,Gold Nugget,4,4,0.1538\n\
                   Sand Dog,26,Watercress,2,2,0.0769\n\
                   Sand Dog,26,Gold Nugget,1,1,0.0385\n\
                   \"Late, Great Beast\",10,,,,\n";
        let enemies = super::parse_drop_data(csv).expect("parse csv");
        assert_eq!(enemies.len(), 2);

        let sd = &enemies[0];
        assert_eq!(sd.enemy_name, "Sand Dog");
        assert_eq!(sd.total_kills, 26);
        let gold = sd
            .loot
            .iter()
            .find(|l| l.item_name == "Gold Nugget")
            .unwrap();
        assert_eq!(gold.total_quantity, 5, "duplicate item rows summed");
        assert_eq!(gold.times_dropped, 5);

        let beast = &enemies[1];
        assert_eq!(
            beast.enemy_name, "Late, Great Beast",
            "quoted comma name preserved"
        );
        assert_eq!(beast.total_kills, 10);
        assert!(
            beast.loot.is_empty(),
            "lootless enemy retained for its kill count"
        );
    }

    /// CSV parsing tolerates external spreadsheets: header aliases, any column
    /// order, and missing optional columns.
    #[test]
    fn parse_csv_accepts_header_aliases_any_order() {
        let csv = "kills,monster,drops,item,qty\n5,Goblin,3,Gold Coin,9\n";
        let enemies = super::parse_drop_data(csv).expect("parse aliased csv");
        assert_eq!(enemies.len(), 1);
        let g = &enemies[0];
        assert_eq!(g.enemy_name, "Goblin");
        assert_eq!(g.total_kills, 5);
        assert_eq!(g.loot.len(), 1);
        assert_eq!(g.loot[0].item_name, "Gold Coin");
        assert_eq!(g.loot[0].total_quantity, 9);
        assert_eq!(g.loot[0].times_dropped, 3);
    }

    /// Legacy JSON exports still import.
    #[test]
    fn parse_legacy_json_still_works() {
        let json = r#"{"format_version":1,"exported_at":"x","enemies":[{"enemy_name":"Goblin","total_kills":7,"loot":[{"item_name":"Gold Coin","total_quantity":3,"times_dropped":2}]}]}"#;
        let enemies = super::parse_drop_data(json).expect("parse json");
        assert_eq!(enemies.len(), 1);
        assert_eq!(enemies[0].enemy_name, "Goblin");
        assert_eq!(enemies[0].total_kills, 7);
        assert_eq!(enemies[0].loot[0].item_name, "Gold Coin");
        assert_eq!(enemies[0].loot[0].times_dropped, 2);
    }

    /// A CSV with no recognizable enemy column is a clear error, not a silent
    /// empty import.
    #[test]
    fn parse_csv_missing_enemy_column_errors() {
        let csv = "item,qty\nGold Coin,5\n";
        assert!(super::parse_drop_data(csv).is_err());
    }

    /// An Excel-style UTF-8 BOM prefix must not break header detection.
    #[test]
    fn parse_csv_tolerates_utf8_bom() {
        let csv = "\u{feff}enemy_name,total_kills,item_name,total_quantity,times_dropped\nGoblin,5,Gold Coin,9,3\n";
        let enemies = super::parse_drop_data(csv).expect("parse bom csv");
        assert_eq!(enemies.len(), 1);
        assert_eq!(enemies[0].enemy_name, "Goblin");
        assert_eq!(enemies[0].loot[0].item_name, "Gold Coin");
    }

    /// A raw per-loot-event log (one row per looted item, with a per-corpse
    /// `enemy_id`) is auto-detected and aggregated: distinct corpses = kills,
    /// distinct corpses per item = times_dropped, amounts summed = quantity.
    /// Non-loot rows are skipped, the display item column wins over an internal
    /// one, and leading/trailing whitespace in names is trimmed. (Shape mirrors a
    /// real third-party collection log.)
    #[test]
    fn parse_raw_event_log_aggregates_by_corpse() {
        let csv = "Item_Name,Item_Amount,log_event_description,item_name,enemy_id,enemy_name\n\
            Transport Security Card,1,Corpse loot,TransportSecurityCard,100, Elite Troll Trooper\n\
            Transport Security Card,1,Corpse loot,TransportSecurityCard,101, Elite Troll Trooper\n\
            Winter Court Greaves,1,Corpse loot,WinterCourtGreaves,101, Elite Troll Trooper\n\
            Cargo Deck Key,1,Corpse loot,CargoDeckKey,200,Troll Trooper\n\
            Troll Flesh,1,Harvest,TrollFlesh,200,Troll Trooper\n";
        let mut enemies = super::parse_drop_data(csv).expect("parse raw event log");
        enemies.sort_by(|a, b| a.enemy_name.cmp(&b.enemy_name));

        // Elite Troll Trooper: 2 distinct corpses (100, 101).
        let elite = &enemies[0];
        assert_eq!(elite.enemy_name, "Elite Troll Trooper");
        assert_eq!(elite.total_kills, 2);
        let tsc = elite
            .loot
            .iter()
            .find(|l| l.item_name == "Transport Security Card") // display name, not internal
            .expect("Transport Security Card");
        assert_eq!(tsc.times_dropped, 2);
        assert_eq!(tsc.total_quantity, 2);
        let greaves = elite
            .loot
            .iter()
            .find(|l| l.item_name == "Winter Court Greaves")
            .unwrap();
        assert_eq!(greaves.times_dropped, 1);

        // Troll Trooper: 1 corpse; the Harvest (non-loot) row is skipped, so it
        // adds neither a kill nor a Troll Flesh drop.
        let troll = &enemies[1];
        assert_eq!(troll.enemy_name, "Troll Trooper");
        assert_eq!(troll.total_kills, 1);
        assert_eq!(troll.loot.len(), 1);
        assert_eq!(troll.loot[0].item_name, "Cargo Deck Key");
    }

    /// Drop data is keyed by (monster, zone): the same monster name in two zones
    /// produces two independent entries with their own kills/drops. A raw log's
    /// `Zone` column drives this.
    #[test]
    fn parse_raw_event_log_splits_by_zone() {
        let csv = "Item_Name,Item_Amount,log_event_description,enemy_id,enemy_name,Zone\n\
            Cargo Deck Key,1,Corpse loot,200,Troll Trooper,AreaFaeRealm1\n\
            Cargo Deck Key,1,Corpse loot,201,Troll Trooper,AreaFaeRealm1\n\
            Troll Flesh,1,Corpse loot,300,Troll Trooper,AreaFaeRealm1Caves\n";
        let enemies = super::parse_drop_data(csv).expect("parse zoned raw log");
        assert_eq!(enemies.len(), 2, "same monster, two zones = two entries");

        let realm = enemies
            .iter()
            .find(|e| e.zone.as_deref() == Some("AreaFaeRealm1"))
            .expect("AreaFaeRealm1 entry");
        assert_eq!(realm.total_kills, 2); // corpses 200, 201
        assert_eq!(realm.loot[0].item_name, "Cargo Deck Key");
        assert_eq!(realm.loot[0].times_dropped, 2);

        let caves = enemies
            .iter()
            .find(|e| e.zone.as_deref() == Some("AreaFaeRealm1Caves"))
            .expect("AreaFaeRealm1Caves entry");
        assert_eq!(caves.total_kills, 1); // corpse 300
        assert_eq!(caves.loot[0].item_name, "Troll Flesh");
    }

    /// The aggregated CSV format round-trips a `zone` column; a blank zone cell
    /// becomes the unknown-zone bucket (None).
    #[test]
    fn parse_csv_aggregated_round_trips_zone() {
        let csv = "enemy_name,zone,total_kills,item_name,total_quantity,times_dropped\n\
            Goblin,AreaCave,10,Gold Coin,5,3\n\
            Goblin,,4,Rusty Sword,1,1\n";
        let enemies = super::parse_drop_data(csv).expect("parse zoned aggregated csv");
        assert_eq!(
            enemies.len(),
            2,
            "same monster, two zones (one unknown) = two entries"
        );

        let cave = enemies
            .iter()
            .find(|e| e.zone.as_deref() == Some("AreaCave"))
            .expect("AreaCave entry");
        assert_eq!(cave.total_kills, 10);
        assert_eq!(cave.loot[0].item_name, "Gold Coin");

        let unknown = enemies
            .iter()
            .find(|e| e.zone.is_none())
            .expect("unknown-zone entry");
        assert_eq!(unknown.total_kills, 4);
        assert_eq!(unknown.loot[0].item_name, "Rusty Sword");
    }
}
