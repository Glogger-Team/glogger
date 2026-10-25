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

#[derive(Debug, Serialize, Clone)]
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
