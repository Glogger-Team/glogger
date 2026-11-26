use serde::{Deserialize, Serialize};

const GITHUB_RELEASES_URL: &str = "https://api.github.com/repos/Glogger-Team/glogger/releases";

const PG_NEWS_URL: &str = "https://client.projectgorgon.com/news.txt";

// --- GitHub Releases ---

#[derive(Debug, Deserialize)]
struct GitHubReleaseRaw {
    tag_name: String,
    name: Option<String>,
    published_at: Option<String>,
    html_url: String,
    body: Option<String>,
}

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub name: String,
    pub published_at: String,
    pub html_url: String,
    pub body: String,
}

#[tauri::command]
pub async fn fetch_github_releases() -> Result<Vec<ReleaseInfo>, String> {
    let client = reqwest::Client::builder()
        .user_agent("glogger-update-check")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let releases: Vec<GitHubReleaseRaw> = client
        .get(format!("{}?per_page=20", GITHUB_RELEASES_URL))
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub request failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse releases: {e}"))?;

    Ok(releases
        .into_iter()
        .map(|r| ReleaseInfo {
            name: r.name.unwrap_or_else(|| r.tag_name.clone()),
            published_at: r.published_at.unwrap_or_default(),
            html_url: r.html_url,
            body: r.body.unwrap_or_default(),
            tag_name: r.tag_name,
        })
        .collect())
}

// --- PG News ---

#[tauri::command]
pub async fn fetch_pg_news() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("glogger")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let text = client
        .get(PG_NEWS_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to reach PG news: {e}"))?
        .error_for_status()
        .map_err(|e| format!("PG news request failed: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read PG news: {e}"))?;

    Ok(text)
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `external_fetch` IPC payload shapes derive `specta::Type` so
// `src/types/externalFetch.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test external_fetch_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<ReleaseInfo>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/externalFetch.specta.ts` must equal
    /// what `export_bindings` derives from the `external_fetch` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test external_fetch_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_external_fetch_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/externalFetch.specta.ts",
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