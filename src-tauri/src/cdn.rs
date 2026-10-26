/// CDN access for Project: Gorgon data files and icons.
///
/// Version check:  GET https://client.projectgorgon.com/fileversion.txt  → integer
/// Data files:     https://cdn.projectgorgon.com/v{ver}/data/{file}.json
/// Translation:    https://cdn.projectgorgon.com/v{ver}/data/Translation.zip
/// Icons:          https://cdn.projectgorgon.com/v{ver}/icons/icon_{id}.png
use std::io::Read as _;
use std::path::{Path, PathBuf};
use tokio::fs;

const VERSION_URL: &str = "https://client.projectgorgon.com/fileversion.txt";
const CDN_BASE: &str = "https://cdn.projectgorgon.com";

/// The JSON files we want to download and cache locally.
pub const DATA_FILES: &[&str] = &[
    "items",
    "skills",
    "abilities",
    "recipes",
    "effects",
    "npcs",
    "areas",
    "attributes",
    "xptables",
    "advancementtables",
    "abilitykeywords",
    "abilitydynamicdots",
    "abilitydynamicspecialvalues",
    "ai",
    "directedgoals",
    "itemuses",
    "landmarks",
    "lorebooks",
    "lorebookinfo",
    "playertitles",
    "quests",
    "sources_abilities",
    "sources_items",
    "sources_recipes",
    "storagevaults",
    "tsysclientinfo",
    "tsysprofiles",
    "achievements",
];

// ── Version helpers ──────────────────────────────────────────────────────────

/// Fetch the current live version number from the game's version endpoint.
pub async fn fetch_remote_version() -> Result<u32, String> {
    let text = reqwest::get(VERSION_URL)
        .await
        .map_err(|e| format!("Version fetch failed: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Version read failed: {e}"))?;

    text.trim()
        .parse::<u32>()
        .map_err(|e| format!("Version parse failed '{text}': {e}"))
}

/// Read the cached version number we last downloaded, if any.
pub async fn read_cached_version(cache_dir: &Path) -> Option<u32> {
    let path = cache_dir.join("version.txt");
    let text = fs::read_to_string(&path).await.ok()?;
    text.trim().parse().ok()
}

/// Write the version number we just downloaded to disk.
pub async fn write_cached_version(cache_dir: &Path, version: u32) -> Result<(), String> {
    let path = cache_dir.join("version.txt");
    fs::write(&path, version.to_string())
        .await
        .map_err(|e| format!("Failed to write cached version: {e}"))
}

// ── Data file helpers ─────────────────────────────────────────────────────────

/// Download one JSON data file and write it to `cache_dir/{name}.json`.
pub async fn download_data_file(version: u32, name: &str, cache_dir: &Path) -> Result<(), String> {
    let url = format!("{CDN_BASE}/v{version}/data/{name}.json");
    let bytes = reqwest::get(&url)
        .await
        .map_err(|e| format!("Download failed for {name}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Read failed for {name}: {e}"))?;

    let dest = cache_dir.join(format!("{name}.json"));
    fs::write(&dest, &bytes)
        .await
        .map_err(|e| format!("Write failed for {name}: {e}"))
}

/// Download all DATA_FILES for the given version into `cache_dir`.
/// Runs downloads concurrently.
pub async fn download_all_data_files(version: u32, cache_dir: &Path) -> Result<(), String> {
    let tasks: Vec<_> = DATA_FILES
        .iter()
        .map(|name| {
            let cache_dir = cache_dir.to_path_buf();
            let name = *name;
            tokio::spawn(async move { download_data_file(version, name, &cache_dir).await })
        })
        .collect();

    for task in tasks {
        task.await
            .map_err(|e| format!("Task panicked: {e}"))?
            .map_err(|e| e)?;
    }

    Ok(())
}

/// Path for a cached data file.
#[allow(dead_code)]
pub fn data_file_path(cache_dir: &Path, name: &str) -> PathBuf {
    cache_dir.join(format!("{name}.json"))
}

// ── Translation (string) file helpers ────────────────────────────────────────

/// The individual JSON files extracted from Translation.zip.
/// These are flat key→string maps used for localization and for diffing
/// text-only changes between CDN versions.
pub const TRANSLATION_FILES: &[&str] = &[
    "strings_abilities",
    "strings_achievements",
    "strings_ai",
    "strings_areas",
    "strings_attributes",
    "strings_directedgoals",
    "strings_effects",
    "strings_items",
    "strings_lorebookinfo",
    "strings_lorebooks",
    "strings_npcs",
    "strings_playertitles",
    "strings_quests",
    "strings_recipes",
    "strings_requested",
    "strings_skills",
    "strings_storagevaults",
    "strings_tsysclientinfo",
    "strings_ui",
];

/// Download Translation.zip and extract the string JSON files into
/// `cache_dir/translation/`.
pub async fn download_translation_zip(version: u32, cache_dir: &Path) -> Result<(), String> {
    let url = format!("{CDN_BASE}/v{version}/data/Translation.zip");
    let bytes = reqwest::get(&url)
        .await
        .map_err(|e| format!("Translation.zip download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Translation.zip read failed: {e}"))?;

    let translation_dir = cache_dir.join("translation");
    std::fs::create_dir_all(&translation_dir)
        .map_err(|e| format!("Failed to create translation dir: {e}"))?;

    // Extract in a blocking task since zip crate is synchronous
    let dir = translation_dir.clone();
    tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| format!("Failed to open Translation.zip: {e}"))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read zip entry {i}: {e}"))?;

            // enclosed_name() rejects absolute paths, drive letters, and `..`
            // escapes (Zip-Slip) — hardened in zip 9.0.4. Skip unsafe entries
            // instead of joining them onto the cache dir.
            let safe_path = match file.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            if safe_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let name = safe_path.display().to_string();
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .map_err(|e| format!("Failed to extract {name}: {e}"))?;

            let dest = dir.join(&safe_path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir for {name}: {e}"))?;
            }
            std::fs::write(&dest, &contents).map_err(|e| format!("Failed to write {name}: {e}"))?;
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Translation extract task panicked: {e}"))?
}

// ── Icon helpers ──────────────────────────────────────────────────────────────

/// Path for a cached icon file.
pub fn icon_path(icon_dir: &Path, icon_id: u32) -> PathBuf {
    icon_dir.join(format!("icon_{icon_id}.png"))
}

/// Returns the local path for an icon, fetching and caching it first if needed.
pub async fn get_or_fetch_icon(
    version: u32,
    icon_id: u32,
    icon_dir: &Path,
) -> Result<PathBuf, String> {
    let path = icon_path(icon_dir, icon_id);

    if path.exists() {
        return Ok(path);
    }

    let url = format!("{CDN_BASE}/v{version}/icons/icon_{icon_id}.png");
    let bytes = reqwest::get(&url)
        .await
        .map_err(|e| format!("Icon fetch failed for {icon_id}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Icon read failed for {icon_id}: {e}"))?;

    fs::write(&path, &bytes)
        .await
        .map_err(|e| format!("Icon write failed for {icon_id}: {e}"))?;

    Ok(path)
}
