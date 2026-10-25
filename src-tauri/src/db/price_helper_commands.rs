use super::DbPool;
use serde::{Deserialize, Serialize};
use tauri::State;

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateQuoteInput {
    pub name: String,
    pub notes: Option<String>,
    pub fee_config: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateQuoteInput {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub fee_config: String,
    pub customer_provides: String,
}

#[derive(Deserialize)]
pub struct AddQuoteEntryInput {
    pub quote_id: i64,
    pub recipe_id: i64,
    pub recipe_name: String,
    pub quantity: i32,
}

#[derive(Deserialize)]
pub struct UpdateQuoteEntryInput {
    pub id: i64,
    pub quantity: i32,
}

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PriceHelperQuote {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub fee_config: String,
    pub customer_provides: String,
    pub created_at: String,
    pub updated_at: String,
    pub entries: Vec<PriceHelperQuoteEntry>,
}

#[derive(Serialize)]
pub struct PriceHelperQuoteEntry {
    pub id: i64,
    pub quote_id: i64,
    pub recipe_id: i64,
    pub recipe_name: String,
    pub quantity: i32,
    pub sort_order: i32,
}

#[derive(Serialize)]
pub struct PriceHelperQuoteSummary {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
    pub entry_count: i64,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn create_price_helper_quote(
    db: State<'_, DbPool>,
    input: CreateQuoteInput,
) -> Result<i64, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let default_fee =
        r#"{"per_craft_fee":0,"material_pct":0,"material_pct_basis":"total","flat_fee":0}"#;
    let fee_config = input.fee_config.as_deref().unwrap_or(default_fee);

    conn.execute(
        "INSERT INTO price_helper_quotes (name, notes, fee_config) VALUES (?1, ?2, ?3)",
        rusqlite::params![input.name, input.notes.unwrap_or_default(), fee_config],
    )
    .map_err(|e| format!("Failed to create quote: {e}"))?;

    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn get_price_helper_quotes(
    db: State<'_, DbPool>,
) -> Result<Vec<PriceHelperQuoteSummary>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT q.id, q.name, q.notes, datetime(q.created_at), datetime(q.updated_at),
                (SELECT COUNT(*) FROM price_helper_entries WHERE quote_id = q.id)
         FROM price_helper_quotes q
         ORDER BY q.updated_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(PriceHelperQuoteSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                notes: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                entry_count: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut quotes = Vec::new();
    for row in rows {
        quotes.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }

    Ok(quotes)
}

#[tauri::command]
pub fn get_price_helper_quote(
    db: State<'_, DbPool>,
    quote_id: i64,
) -> Result<PriceHelperQuote, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let quote = conn
        .query_row(
            "SELECT id, name, notes, fee_config, customer_provides, datetime(created_at), datetime(updated_at)
         FROM price_helper_quotes WHERE id = ?1",
            [quote_id],
            |row| {
                Ok(PriceHelperQuote {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    notes: row.get(2)?,
                    fee_config: row.get(3)?,
                    customer_provides: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    entries: Vec::new(),
                })
            },
        )
        .map_err(|e| format!("Quote not found: {e}"))?;

    let mut entry_stmt = conn
        .prepare(
            "SELECT id, quote_id, recipe_id, recipe_name, quantity, sort_order
         FROM price_helper_entries
         WHERE quote_id = ?1
         ORDER BY sort_order ASC",
        )
        .map_err(|e| format!("Failed to prepare entry query: {e}"))?;

    let entry_rows = entry_stmt
        .query_map([quote_id], |row| {
            Ok(PriceHelperQuoteEntry {
                id: row.get(0)?,
                quote_id: row.get(1)?,
                recipe_id: row.get(2)?,
                recipe_name: row.get(3)?,
                quantity: row.get(4)?,
                sort_order: row.get(5)?,
            })
        })
        .map_err(|e| format!("Entry query failed: {e}"))?;

    let mut quote = quote;
    for row in entry_rows {
        quote
            .entries
            .push(row.map_err(|e| format!("Entry row error: {e}"))?);
    }

    Ok(quote)
}

#[tauri::command]
pub fn update_price_helper_quote(
    db: State<'_, DbPool>,
    input: UpdateQuoteInput,
) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute(
        "UPDATE price_helper_quotes SET name = ?1, notes = ?2, fee_config = ?3, customer_provides = ?4, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?5",
        rusqlite::params![input.name, input.notes, input.fee_config, input.customer_provides, input.id],
    )
    .map_err(|e| format!("Failed to update quote: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn delete_price_helper_quote(db: State<'_, DbPool>, quote_id: i64) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute("DELETE FROM price_helper_quotes WHERE id = ?1", [quote_id])
        .map_err(|e| format!("Failed to delete quote: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn add_price_helper_entry(
    db: State<'_, DbPool>,
    input: AddQuoteEntryInput,
) -> Result<i64, String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    let next_order: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM price_helper_entries WHERE quote_id = ?1",
            [input.quote_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get sort order: {e}"))?;

    conn.execute(
        "INSERT INTO price_helper_entries (quote_id, recipe_id, recipe_name, quantity, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            input.quote_id,
            input.recipe_id,
            input.recipe_name,
            input.quantity,
            next_order
        ],
    )
    .map_err(|e| format!("Failed to add entry: {e}"))?;

    conn.execute(
        "UPDATE price_helper_quotes SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        [input.quote_id],
    )
    .map_err(|e| format!("Failed to touch quote: {e}"))?;

    let new_id = conn.last_insert_rowid();
    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(new_id)
}

#[tauri::command]
pub fn update_price_helper_entry(
    db: State<'_, DbPool>,
    input: UpdateQuoteEntryInput,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    conn.execute(
        "UPDATE price_helper_entries SET quantity = ?1 WHERE id = ?2",
        rusqlite::params![input.quantity, input.id],
    )
    .map_err(|e| format!("Failed to update entry: {e}"))?;

    conn.execute(
        "UPDATE price_helper_quotes SET updated_at = CURRENT_TIMESTAMP
         WHERE id = (SELECT quote_id FROM price_helper_entries WHERE id = ?1)",
        [input.id],
    )
    .map_err(|e| format!("Failed to touch quote: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn remove_price_helper_entry(db: State<'_, DbPool>, entry_id: i64) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    conn.execute(
        "UPDATE price_helper_quotes SET updated_at = CURRENT_TIMESTAMP
         WHERE id = (SELECT quote_id FROM price_helper_entries WHERE id = ?1)",
        [entry_id],
    )
    .map_err(|e| format!("Failed to touch quote: {e}"))?;

    conn.execute("DELETE FROM price_helper_entries WHERE id = ?1", [entry_id])
        .map_err(|e| format!("Failed to remove entry: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}
