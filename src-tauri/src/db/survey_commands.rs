use serde::Serialize;
use tauri::State;

use super::DbPool;

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct SurveyTypeInfo {
    pub item_id: u32,
    pub internal_name: String,
    pub name: String,
    pub zone: Option<String>,
    pub icon_id: Option<u32>,
    pub survey_category: String,
    pub is_motherlode: bool,
    pub skill_req_name: Option<String>,
    #[specta(type = Option<u32>)]
    pub skill_req_level: Option<i64>,
    #[specta(type = Option<u32>)]
    pub survey_skill_req: Option<i64>,
    pub recipe_id: Option<u32>,
    pub survey_xp: Option<f32>,
    pub survey_xp_first_time: Option<f32>,
    pub crafting_cost: Option<f64>,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Get all survey types from the pre-parsed survey_types table
#[tauri::command]
pub fn get_all_survey_types(db: State<'_, DbPool>) -> Result<Vec<SurveyTypeInfo>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT item_id, internal_name, name, zone, icon_id,
                survey_category, is_motherlode, skill_req_name,
                skill_req_level, survey_skill_req, recipe_id,
                survey_xp, survey_xp_first_time, crafting_cost
         FROM survey_types
         ORDER BY survey_category ASC, zone ASC, skill_req_level ASC",
        )
        .map_err(|e| format!("Query prepare error: {e}"))?;

    let surveys = stmt
        .query_map([], |row| {
            Ok(SurveyTypeInfo {
                item_id: row.get(0)?,
                internal_name: row.get(1)?,
                name: row.get(2)?,
                zone: row.get(3)?,
                icon_id: row.get(4)?,
                survey_category: row.get(5)?,
                is_motherlode: row.get(6)?,
                skill_req_name: row.get(7)?,
                skill_req_level: row.get(8)?,
                survey_skill_req: row.get(9)?,
                recipe_id: row.get(10)?,
                survey_xp: row.get(11)?,
                survey_xp_first_time: row.get(12)?,
                crafting_cost: row.get(13)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let result: Vec<SurveyTypeInfo> = surveys.filter_map(|r| r.ok()).collect();
    Ok(result)
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `survey` IPC payload shapes derive `specta::Type` so
// `src/types/survey.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test survey_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<SurveyTypeInfo>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/survey.specta.ts` must equal
    /// what `export_bindings` derives from the `survey` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test survey_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_survey_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/survey.specta.ts",
        );
        if std::env::var("OVERWRITE_BINDINGS").is_ok() {
            std::fs::write(path, &fresh).expect("bindings write failed");
            return;
        }
        let checked_in =
            std::fs::read_to_string(path).expect("checked-in specta bindings missing");
        assert_eq!(
            checked_in, fresh,
            "specta bindings drift: regenerate with OVERWRITE_BINDINGS=1",
        );
    }
}