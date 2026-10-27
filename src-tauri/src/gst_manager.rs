use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

const GST_RELEASES_URL: &str = "https://api.github.com/repos/kaeus/GorgonSurveyTracker/releases";
const GST_EXE_NAME: &str = "GorgonSurveyTracker.exe";
const GST_SUBFOLDER: &str = "gst";

/// Metadata file stored alongside the exe to track installed version.
const GST_VERSION_FILE: &str = "gst_version.txt";

#[derive(Debug, Serialize, Clone)]
pub struct GstStatus {
    /// Whether the exe exists on disk (always false on non-Windows).
    pub installed: bool,
    /// The installed version tag (e.g. "v1.19.0"), or None.
    pub installed_version: Option<String>,
    /// The latest release version tag from GitHub, if we've checked.
    pub latest_version: Option<String>,
    /// True when latest_version is newer than installed_version.
    pub update_available: bool,
    /// "windows", "macos", or "linux".
    pub platform: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

fn current_platform() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else {
        "linux".to_string()
    }
}

fn gst_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    Ok(data_dir.join(GST_SUBFOLDER))
}

fn gst_exe_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(gst_dir(app)?.join(GST_EXE_NAME))
}

fn read_installed_version(app: &tauri::AppHandle) -> Option<String> {
    let path = gst_dir(app).ok()?.join(GST_VERSION_FILE);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn write_installed_version(app: &tauri::AppHandle, version: &str) -> Result<(), String> {
    let dir = gst_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create GST dir: {e}"))?;
    std::fs::write(dir.join(GST_VERSION_FILE), version)
        .map_err(|e| format!("Failed to write version file: {e}"))
}

async fn fetch_latest_release() -> Result<GitHubRelease, String> {
    let client = reqwest::Client::builder()
        .user_agent("glogger-gst-manager")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let releases: Vec<GitHubRelease> = client
        .get(format!("{}?per_page=1", GST_RELEASES_URL))
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub request failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse releases: {e}"))?;

    releases
        .into_iter()
        .next()
        .ok_or_else(|| "No releases found".to_string())
}

/// Check whether GST is installed and whether an update is available.
/// On non-Windows platforms, installed is always false (no exe to manage),
/// but latest_version is still fetched so the UI can show the version info.
#[tauri::command]
pub async fn gst_check_status(app: tauri::AppHandle) -> Result<GstStatus, String> {
    let platform = current_platform();

    let (installed, installed_version) = if platform == "windows" {
        let exe = gst_exe_path(&app)?;
        (exe.exists(), read_installed_version(&app))
    } else {
        (false, None)
    };

    // Try to fetch latest — but don't fail the whole status if offline.
    let (latest_version, update_available) = match fetch_latest_release().await {
        Ok(release) => {
            let latest = release.tag_name;
            let needs_update = installed_version
                .as_ref()
                .map(|iv| iv != &latest)
                .unwrap_or(true);
            (Some(latest), installed && needs_update)
        }
        Err(_) => (None, false),
    };

    Ok(GstStatus {
        installed,
        installed_version,
        latest_version,
        update_available,
        platform,
    })
}

/// Download (or update) GST to the app data directory. Windows only.
///
/// Integrity model (documented limit): the release is fetched over TLS from
/// `api.github.com`/`github.com` and written atomically (tmp file + rename),
/// but the bytes are executed without a checksum — GitHub's release API
/// exposes no digest to verify against, and GST publishes no signed manifest.
/// The 256 MiB cap is inherent (a GST exe is tens of MB; anything larger
/// aborts before touching disk — see the chunked read below), not a
/// substitute for a hash. If GST ever ships checksums, hash-before-exec goes
/// here: buffer the body, compare, and only then write `final_path`.
#[tauri::command]
pub async fn gst_download(app: tauri::AppHandle) -> Result<GstStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("GST download is only available on Windows. See the GST GitHub page for Mac/Linux instructions.".to_string());
    }

    let release = fetch_latest_release().await?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == GST_EXE_NAME)
        .ok_or_else(|| "Release has no GorgonSurveyTracker.exe asset".to_string())?;

    let client = reqwest::Client::builder()
        .user_agent("glogger-gst-manager")
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let mut resp = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;
    // Cap: a GST exe is tens of MB. Anything larger is a wrong asset or a
    // hostile response — abort before buffering it all (RAM) or touching
    // disk. Checked two ways: the declared Content-Length up front when
    // present, then the running total per chunk (a lying header can't smuggle
    // a big body past the chunk cap).
    const MAX_GST_BYTES: u64 = 256 * 1024 * 1024;
    if let Some(len) = resp.content_length() {
        if len > MAX_GST_BYTES {
            return Err(format!(
                "Download too large (declared {len} bytes, cap {} MiB): refusing to download",
                MAX_GST_BYTES / (1024 * 1024)
            ));
        }
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("Failed to read download: {e}"))?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > MAX_GST_BYTES {
            return Err(format!(
                "Download too large (over {} MiB): refusing to write",
                MAX_GST_BYTES / (1024 * 1024)
            ));
        }
    }
    let dir = gst_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create GST dir: {e}"))?;

    let tmp_path = dir.join("GorgonSurveyTracker.exe.tmp");
    let final_path = dir.join(GST_EXE_NAME);

    std::fs::write(&tmp_path, &bytes)
        .map_err(|e| format!("Failed to write downloaded file: {e}"))?;

    // Atomic-ish rename
    if final_path.exists() {
        std::fs::remove_file(&final_path).map_err(|e| format!("Failed to remove old exe: {e}"))?;
    }
    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| format!("Failed to rename downloaded file: {e}"))?;

    write_installed_version(&app, &release.tag_name)?;

    Ok(GstStatus {
        installed: true,
        installed_version: Some(release.tag_name.clone()),
        latest_version: Some(release.tag_name),
        update_available: false,
        platform: current_platform(),
    })
}

/// Launch the installed GST executable. Windows only.
#[tauri::command]
pub async fn gst_launch(app: tauri::AppHandle) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("GST launch is only available on Windows. See the GST GitHub page for Mac/Linux instructions.".to_string());
    }

    let exe = gst_exe_path(&app)?;
    if !exe.exists() {
        return Err("GorgonSurveyTracker is not installed. Download it first.".to_string());
    }

    std::process::Command::new(&exe)
        .current_dir(gst_dir(&app)?)
        .spawn()
        .map_err(|e| format!("Failed to launch GST: {e}"))?;

    Ok(())
}
