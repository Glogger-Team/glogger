//! Phase B — stall window OCR scanner.
//!
//! Captures the game window via GDI (BitBlt, with a full-screen fallback for
//! D3D-exclusive fullscreen black frames), locates the red price-bar rows the
//! stall UI draws, OCRs each row's name cell and price digits with a
//! Tesseract child process (no OCR crate), fuzzy-matches item names against
//! the in-memory CDN list, and parses the `<Owner>'s Shop [<slot>]` header.
//!
//! Pure capture: `scan_stall_window` writes NOTHING to the database — the
//! frontend prefills the capture panel from the returned rows.

use crate::cdn_commands::GameDataState;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

// ── Types ───────────────────────────────────────────────────────────────────

/// One OCR'd stall row.
#[derive(Serialize, Clone)]
pub struct StallScanRow {
    pub raw_text: String,
    pub item_name: Option<String>,
    pub price: Option<i64>,
    pub quantity: Option<i64>,
    pub confidence: f32,
}

/// Result of one stall-window scan.
#[derive(Serialize, Clone)]
pub struct StallScanResult {
    pub stall_name: Option<String>,
    pub stall_slot: Option<String>,
    pub owner_name: Option<String>,
    pub rows: Vec<StallScanRow>,
}

#[derive(Serialize, Clone)]
pub struct OcrStatus {
    pub installed: bool,
    pub exe_path: Option<String>,
    pub version: Option<String>,
}

// ── Tesseract sidecar (gst_manager pattern) ─────────────────────────────────

const OCR_SUBFOLDER: &str = "tesseract";
const OCR_EXE_NAME: &str = "tesseract.exe";
/// UB-Mannheim portable build used by PG Emissary's install instructions.
// NB: the original URL (5.5.0 "zip") 404s — UB-Mannheim never published a
// 5.5.0 zip release. The server (digi.bib.uni-mannheim.de/tesseract) hosts
// ONLY .exe installers — **NSIS** builds (Nullsoft signature verified in
// the 5.3.0 binary). The downloader runs it with NSIS silent flags pointed
// at the sidecar dir. Users with a system install never reach this URL
// (see `ocr_download`'s system-install short-circuit).
const OCR_DOWNLOAD_URL: &str =
    "https://digi.bib.uni-mannheim.de/tesseract/tesseract-ocr-w64-setup-5.3.0.20221222.exe";
const OCR_SETUP_PAGE: &str = "https://github.com/UB-Mannheim/tesseract/wiki";

fn ocr_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    Ok(data_dir.join(OCR_SUBFOLDER))
}

fn ocr_exe_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let sidecar = ocr_dir(app)?.join(OCR_EXE_NAME);
    if sidecar.exists() {
        return Ok(sidecar);
    }
    // Fall back to a system-wide Tesseract install (the UB-Mannheim setup
    // writes here by default). Lets users who already installed the engine
    // skip the sidecar download entirely.
    const SYSTEM_PATHS: &[&str] = &[
        r"C:\Program Files\Tesseract-OCR\tesseract.exe",
        r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
    ];
    for p in SYSTEM_PATHS {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
    }
    Ok(sidecar) // not installed; caller reports the missing engine
}

/// Check whether the Tesseract sidecar is installed.
#[tauri::command]
pub fn ocr_check_status(app: tauri::AppHandle) -> Result<OcrStatus, String> {
    if !cfg!(target_os = "windows") {
        return Ok(OcrStatus {
            installed: false,
            exe_path: None,
            version: None,
        });
    }
    let exe = ocr_exe_path(&app)?;
    let installed = exe.exists();
    let version = if installed {
        std::process::Command::new(&exe)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.lines().next().map(str::to_string))
    } else {
        None
    };
    Ok(OcrStatus {
        installed,
        exe_path: exe.to_str().map(str::to_string),
        version,
    })
}

/// Download the portable Tesseract build into appdata (Windows only).
/// Prefers a system-wide install (`C:\Program Files\Tesseract-OCR`) when
/// present — no download needed.
#[tauri::command]
pub async fn ocr_download(app: tauri::AppHandle) -> Result<OcrStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("The OCR engine download is only available on Windows.".to_string());
    }

    // A system-wide Tesseract install satisfies the requirement — adopt it
    // instead of downloading the sidecar (also sidesteps the hardcoded URL).
    let resolved = ocr_exe_path(&app)?;
    if resolved != ocr_dir(&app)?.join(OCR_EXE_NAME) {
        return ocr_check_status(app);
    }

    let client = reqwest::Client::builder()
        .user_agent("glogger-ocr-installer")
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let bytes = client
        .get(OCR_DOWNLOAD_URL)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {e}"))?;

    let dir = ocr_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create OCR dir: {e}"))?;

    // The Mannheim server hosts **NSIS** installers (verified: 'Nullsoft'
    // signature in the binary; the comment above saying "Inno Setup" was
    // wrong). NSIS silent flags: /S (capital, no slash-form /VERYSILENT —
    // that is Inno Setup syntax and NSIS ignores it, popping the wizard),
    // /NCRC to skip the CRC prompt, and /D=<dir> (no quotes, must be the
    // LAST argument) to target the sidecar dir.
    let installer_path = dir.join("tesseract-setup.exe");
    std::fs::write(&installer_path, &bytes)
        .map_err(|e| format!("Failed to write download: {e}"))?;

    let status = std::process::Command::new(&installer_path)
        .args(["/S", "/NCRC", &format!("/D={}", dir.display())])
        .status()
        .map_err(|e| format!("Failed to run installer: {e}"))?;
    let _ = std::fs::remove_file(&installer_path);

    if !status.success() {
        return Err(format!(
            "Installer exited with status {status} — open the setup page and install manually."
        ));
    }

    let exe = dir.join(OCR_EXE_NAME);
    if !exe.exists() {
        return Err(
            "Installer did not produce tesseract.exe — open the setup page and install manually."
                .to_string(),
        );
    }

    ocr_check_status(app)
}

/// Open the Tesseract download page as a manual fallback.
#[tauri::command]
pub fn ocr_launch_setup() -> Result<(), String> {
    tauri_plugin_opener::open_url(OCR_SETUP_PAGE, None::<&str>)
        .map_err(|e| format!("Failed to open setup page: {e}"))
}

// ── Item-name fuzzy matching ────────────────────────────────────────────────

/// Trigram similarity between two strings (0.0–1.0), case-insensitive.
/// Dice coefficient over character 3-grams — a plain-Rust stand-in for
/// rapidfuzz's ratio used by PG Emissary (score_cutoff 75/100 ≈ trigram
/// Dice ≥ 0.75).
///
/// Short-string edge: inputs under 3 chars produce no 3-grams at all, so an
/// empty gram set carries no signal — return identity (`a == b ? 1 : 0`)
/// rather than a blanket 1.0, which would match every short OCR fragment
/// against every short catalog name.
pub fn trigram_similarity(a: &str, b: &str) -> f32 {
    let norm = |s: &str| s.to_lowercase().chars().collect::<Vec<_>>();
    let a = norm(a);
    let b = norm(b);
    if a.len() < 2 || b.len() < 2 {
        return if a == b { 1.0 } else { 0.0 };
    }
    let grams = |v: &[char]| -> std::collections::HashSet<Vec<char>> {
        v.windows(3).map(<[char]>::to_vec).collect()
    };
    let (ga, gb) = (grams(&a), grams(&b));
    if ga.is_empty() || gb.is_empty() {
        // No trigrams on one or both sides (2-char inputs): identity only.
        return if a == b { 1.0 } else { 0.0 };
    }
    let inter = ga.intersection(&gb).count();
    (2 * inter) as f32 / (ga.len() + gb.len()) as f32
}

/// Confidence threshold for accepting a fuzzy match (Emissary: 75/100).
const MATCH_THRESHOLD: f32 = 0.75;

/// Match an OCR'd item-name string against the CDN `item_name_index`.
/// Returns `(canonical_name, item_type_id, confidence)` or None.
pub fn match_item_name(
    ocr_name: &str,
    item_name_index: &std::collections::HashMap<String, u32>,
) -> Option<(String, u32, f32)> {
    let cleaned = ocr_name.trim();
    if cleaned.len() < 3 {
        return None;
    }
    // Exact (case-insensitive) hit first.
    for (name, id) in item_name_index {
        if name.eq_ignore_ascii_case(cleaned) {
            return Some((name.clone(), *id, 1.0));
        }
    }
    let mut best: Option<(&String, u32, f32)> = None;
    for (name, id) in item_name_index {
        let score = trigram_similarity(cleaned, name);
        if score >= MATCH_THRESHOLD && best.map(|(_, _, s)| score > s).unwrap_or(true) {
            best = Some((name, *id, score));
        }
    }
    best.map(|(name, id, score)| (name.clone(), id, score))
}

// ── OCR invocation ──────────────────────────────────────────────────────────

/// Run Tesseract on one prepared image (PNG bytes) and return its text.
fn run_tesseract(
    exe: &PathBuf,
    png_bytes: &[u8],
    psm: u8,
    digit_whitelist: bool,
) -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!(
        "glogger_ocr_{}_{}.png",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    std::fs::write(&tmp, png_bytes).map_err(|e| format!("Failed to write temp image: {e}"))?;

    let mut cmd = std::process::Command::new(exe);
    cmd.arg(&tmp)
        .arg("stdout")
        // Tesseract CLI requires "--psm N" as two separate argv tokens; a
        // combined "--psm 6" token is rejected ("unknown command line
        // argument '--psm 6'").
        .arg("--psm")
        .arg(psm.to_string())
        .arg("--oem")
        .arg("3");
    if digit_whitelist {
        cmd.arg("-c").arg("tessedit_char_whitelist=0123456789,");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run tesseract: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if !output.status.success() {
        return Err(format!(
            "tesseract failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ── Stall text parsing (Emissary parse_vendor_text / parse_vendor_name) ─────

/// Parse the `<Owner>'s Shop [<slot>]` header from OCR text.
/// Returns (stall_name, slot). Mirrors Emissary `parse_vendor_name`.
pub fn parse_stall_header(text: &str) -> (Option<String>, Option<String>) {
    let idx = match text.find("'s Shop") {
        Some(i) => i,
        None => return (None, None),
    };
    // Walk back to the start of the header line.
    let line_start = text[..idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let owner_full = text[line_start..idx].trim();
    if owner_full.is_empty() {
        return (None, None);
    }
    // Slot is a bracketed `[AA-9]` code inside the owner text.
    let (name, slot) = match (owner_full.find('['), owner_full.find(']')) {
        (Some(bs), Some(be)) if be > bs => (owner_full[..bs].trim(), owner_full[bs + 1..be].trim()),
        _ => (owner_full, ""),
    };
    (
        (!name.is_empty()).then(|| name.to_string()),
        (!slot.is_empty()).then(|| slot.to_string()),
    )
}

/// Parse one full-image OCR pass into (name, price) pairs the legacy
/// Emissary way: find `Pay <digits>` lines, take the nearest earlier
/// candidate line as the item name.
pub fn parse_vendor_text(raw_text: &str) -> Vec<(String, i64)> {
    let mut items = Vec::new();
    let lines: Vec<&str> = raw_text.lines().collect();
    let mut used = std::collections::HashSet::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // "Pay 1,234" or "Pay 1234"
        let price_str = match trimmed
            .split_whitespace()
            .position(|w| w.eq_ignore_ascii_case("Pay"))
            .and_then(|pos| trimmed.split_whitespace().nth(pos + 1))
        {
            Some(tok) => tok,
            None => continue,
        };
        let digits: String = price_str.chars().filter(|c| c.is_ascii_digit()).collect();
        let price: i64 = match digits.parse() {
            Ok(p) if p > 0 => p,
            _ => continue,
        };

        // Nearest earlier unused line that looks like an item name.
        for offset in 1..4 {
            let check = i.checked_sub(offset);
            let Some(check) = check else { break };
            if used.contains(&check) {
                continue;
            }
            let raw_candidate = lines[check].trim();
            // Reject UI chrome: "Buy N" quantity rows, header rows, digit soup.
            if raw_candidate.is_empty() || is_ui_chrome_line(raw_candidate) {
                continue;
            }
            let candidate = clean_item_name(raw_candidate);
            if candidate.len() >= 3 && candidate.chars().any(|c| c.is_alphabetic()) {
                items.push((candidate, price));
                used.insert(check);
                break;
            }
        }
    }
    items
}

/// True for OCR lines that are stall-UI chrome rather than item names
/// ("Buy 1" rows, "'s Shop" header fragments, digit/punctuation soup).
/// Mirrors Emissary's `is_item_candidate` rejects. Gates, in order:
/// 1. substring `"'s shop"` (any owner header), `"buy"`, `"pay"` (price-box
///    rows), matched case-insensitively — `"buy"` also catches "Buy 1/5/10";
/// 2. no alphabetic char at all (digit/punctuation soup like "1,234").
/// Anything with at least one letter and none of the substrings passes.
fn is_ui_chrome_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("'s shop") || lower.contains("buy") || lower.contains("pay") {
        return true;
    }
    // Digit/punctuation soup: "1,234", "[CI-6]", "-----".
    !line.chars().any(|c| c.is_alphabetic())
}

/// Clean OCR artifacts from an item-name candidate (subset of Emissary
/// `clean_item_name`: trailing prices/currency glyphs/UI words).
pub fn clean_item_name(name: &str) -> String {
    let mut name = name.trim().to_string();
    // Trailing numbers with separators ("1,234") first, so a merged price
    // digit tail doesn't hide the "Buy N" box that follows it.
    while let Some(sp) = name.rfind(' ') {
        let last = &name[sp + 1..];
        if !last.is_empty()
            && last
                .chars()
                .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
        {
            name = name[..sp].trim().to_string();
        } else {
            break;
        }
    }
    // Inline quantity box: "Basic Potion Buy 1" → "Basic Potion" (only cut
    // when something alphabetic precedes it — a bare "Buy 1" line is UI
    // chrome rejected upstream).
    for prefix in ["Buy 10", "Buy 5", "Buy 1"] {
        let suffix = format!(" {prefix}");
        if let Some(idx) = name.rfind(&suffix) {
            if idx > 0 && name[..idx].chars().any(|c| c.is_alphabetic()) {
                name = name[..idx].trim().to_string();
            }
        }
    }
    // Leading quantity box (OCR merged it into the name line):
    // "Buy 1 Basic Potion" → "Basic Potion".
    for prefix in ["Buy 10 ", "Buy 5 ", "Buy 1 "] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            if !stripped.is_empty() {
                name = stripped.trim().to_string();
            }
        }
    }
    // Leading non-alpha junk.
    name = name
        .trim_start_matches(|c: char| !c.is_alphabetic())
        .to_string();
    // Trailing punctuation.
    name.trim_end_matches([';', ',', '.', ':', '|']).to_string()
}

// ── Row pairing from structured OCR passes ──────────────────────────────────

// ── Panel-scan pipeline (2026-10-03 live-frame proven) ─────────────────────

/// Bright-text binarization threshold. Panel bg is ~37 gray, text 146–248.
const PANEL_TEXT_THRESHOLD: u8 = 100;

/// Scan the stall panel from a captured BGRA frame.
///
/// The stall window is a tall dark column on the left side of the game
/// viewport. Steps, each verified against a live 3440x1440 frame:
/// 1. Locate the panel: find the widest run of dark columns (mean gray < 60)
///    left of center; bail to legacy when nothing matches (theme/dpi drift).
/// 2. Binarize the panel: gray > threshold → ink(0), else 255 — the stall
///    text is bright-on-dark.
/// 3. 2x nearest-neighbor upscale.
/// 4. Two Tesseract passes: name column (panel x-range minus the right ~35%)
///    and price column (right ~35%).
/// 5. Extract `Pay <digits>` prices in order; pair with the name lines
///    positionally (the price sits directly below its row's name).
///
/// Returns None when panel detection fails (caller falls back to legacy).
pub fn scan_panel(
    pixels: &[u8],
    w: u32,
    h: u32,
    exe: &PathBuf,
    item_name_index: &std::collections::HashMap<String, u32>,
) -> Result<Option<(Vec<StallScanRow>, String, Option<String>, Option<String>)>, String> {
    let Some((panel_x0, panel_x1, panel_y0, panel_y1)) = locate_panel(pixels, w, h) else {
        return Ok(None);
    };

    // Binarize the whole panel once.
    let cw = (panel_x1 - panel_x0) as usize;
    let ch = (panel_y1 - panel_y0) as usize;
    let mut gray_bin = vec![255u8; cw * ch];
    for y in 0..ch {
        let row = ((panel_y0 as usize) + y) * (w as usize) * 4;
        for x in 0..cw {
            let i = row + ((panel_x0 as usize) + x) * 4;
            let g = (u32::from(pixels[i]) * 299
                + u32::from(pixels[i + 1]) * 587
                + u32::from(pixels[i + 2]) * 114)
                / 1000;
            gray_bin[y * cw + x] = if g > u32::from(PANEL_TEXT_THRESHOLD) {
                0
            } else {
                255
            };
        }
    }

    // Detect item row bands via horizontal projection of bright pixels —
    // one band per stall row. Per-band two-column OCR (name left, price
    // right) is alignment-proof: no cross-column line counting.
    let bands = detect_row_bands(&gray_bin, cw, ch);
    let mut rows: Vec<StallScanRow> = Vec::new();
    let mut price_raws: Vec<String> = Vec::new();
    for (by0, by1) in &bands {
        let nx0 = panel_x0 + (panel_x1 - panel_x0) / 100; // skip icon edge
        let nx1 = panel_x0 + (panel_x1 - panel_x0) * 55 / 100; // names end before Buy/Pay boxes
                                                               // Price digits sit in the rightmost ~8% of the panel; the crop starts
                                                               // at 89% to skip the council-glyph column and ends 2% past the panel
                                                               // edge (the last digit can touch it).
        let px0 = panel_x0 + (panel_x1 - panel_x0) * 89 / 100;
        let px1 = (panel_x1 + (panel_x1 - panel_x0) * 2 / 100).min(w);
        // ±4px vertical padding gives Tesseract breathing room around the
        // line (proved necessary in offline tuning).
        let py0 = panel_y0 + (*by0 as u32).saturating_sub(4);
        let py1 = panel_y0 + *by1 as u32 + 4;
        let name_text = ocr_bin_region(
            exe,
            &gray_bin,
            cw,
            ch,
            (panel_x0, panel_y0),
            nx0,
            nx1,
            py0,
            py1,
            3,
            "7",
        )?;
        let price_raw = ocr_bin_region(
            exe,
            &gray_bin,
            cw,
            ch,
            (panel_x0, panel_y0),
            px0,
            px1,
            py0,
            py1,
            4,
            "7",
        )?;
        let name = {
            let trimmed = name_text.trim();
            if trimmed.is_empty() || is_ui_chrome_line(trimmed) {
                continue;
            }
            let cleaned = clean_item_name(trimmed);
            if cleaned.len() < 3 || !cleaned.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            cleaned
        };
        let price = extract_price(&price_raw);
        price_raws.push(format!(
            "band {by0}-{by1}: name={name:?} raw={price_raw:?} -> {price}"
        ));
        let (item_name, confidence) = match match_item_name(&name, item_name_index) {
            Some((canonical, _id, score)) => (Some(canonical), score),
            None => (None, 0.0),
        };
        let key = (item_name.clone(), price);
        // Junk-row gate: drop bands with no item match AND no readable price
        // (`extract_price` returns 0 for letter-soup). Either signal alone
        // keeps the row — a known name with an unreadable price still needs
        // manual pricing, while an unmatched name with digits is usually an
        // HP-bar/overlay fragment. Thresholds: price 0 == unreadable,
        // match score >= MATCH_THRESHOLD (0.75, Emissary's cutoff).
        if item_name.is_none() && price == 0 {
            continue;
        }
        // Dedup gate: identical (item_name, price) pairs collapse — the same
        // red-bar row can produce two overlapping bands. Rows that differ in
        // either field survive (e.g. same item at two prices).
        if rows
            .iter()
            .any(|r: &StallScanRow| r.item_name == key.0 && r.price == Some(price))
        {
            continue;
        }
        rows.push(StallScanRow {
            raw_text: format!("{name} @ {price}"),
            item_name,
            price: Some(price),
            // Stall rows show no stack count — OCR has no quantity signal, so
            // this stays None by design; `save_stall_capture` clamps to ≥1 at
            // insert (`entry.quantity.max(1)`). Not a wiring gap.
            quantity: None,
            confidence,
        });
    }
    let debug = format!(
        "-- bands: {} --\n{:?}\n-- price_raws --\n{:?}\n",
        rows.len(),
        rows.iter().map(|r| r.raw_text.clone()).collect::<Vec<_>>(),
        price_raws,
    );
    // Header identity: NOT parsed from the panel pass. Row bands exclude the
    // header strip, and the header (`<Owner>'s Shop [<slot>]`) needs a
    // full-width legacy OCR pass to read reliably, so this returns None/None
    // by design — on panel success the header stays None; the legacy pass
    // (with header parse) runs only when the panel yields no rows.
    let (stall_name, stall_slot) = (None, None);
    if rows.is_empty() {
        // Nothing paired — treat as scan failure so the caller's legacy path
        // gets a chance (keeps behavior comparable to pre-panel scans).
        if std::env::var("GLOGGER_DEBUG").is_ok() {
            println!("--- debug ---\n{debug}");
        }
        eprintln!("[scan_panel] no pairs found");
        return Ok(None);
    }
    Ok(Some((rows, debug, stall_name, stall_slot)))
}

/// Locate the stall panel: a tall run of dark columns on the left half of
/// the frame. Returns (x0, x1, y0, y1) in frame coordinates.
fn locate_panel(pixels: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let w = w as usize;
    let h = h as usize;
    let col_mean = |x: usize| -> u32 {
        let mut total = 0u64;
        let mut n = 0u64;
        let step = (h / 90).max(1); // sample ~90 rows per column
        for y in (0..h).step_by(step) {
            let i = y * w * 4 + x * 4;
            total += u64::from(pixels[i]) * 299
                + u64::from(pixels[i + 1]) * 587
                + u64::from(pixels[i + 2]) * 114;
            n += 1;
        }
        (total / n / 1000) as u32
    };

    // Dark columns: mean brightness < 60. Find the longest run in x < w/2.
    let half = w / 2;
    let mut best: Option<(usize, usize)> = None; // (start, end) exclusive
    let mut run_start: Option<usize> = None;
    for x in 0..half {
        if col_mean(x) < 60 {
            if run_start.is_none() {
                run_start = Some(x);
            }
        } else if let Some(s) = run_start.take() {
            if best.map_or(true, |(bs, be)| x - s > be - bs) {
                best = Some((s, x));
            }
        }
    }
    if let Some(s) = run_start {
        if best.map_or(true, |(bs, be)| half - s > be - bs) {
            best = Some((s, half));
        }
    }
    let (x0, x1) = best?;
    if x1 - x0 < w / 8 {
        return None; // too narrow to be the stall panel
    }

    // The dark-column run often stops at the panel's internal scrollbar
    // (a thin bright gap splits it — observed at ~1030 on a live frame while
    // the stall's price column extends to ~1170). Extend the right edge by
    // a fraction of the detected width; content beyond the true panel is
    // dark game background, so over-extending only adds empty margin.
    let width = x1 - x0;
    let x1 = (x1 + width * 16 / 100).min(half);

    // Vertical extent: dark rows within the column band (top/bottom trims of
    // the chat overlay region are excluded by thresholding on darkness).
    let mut dark_rows = Vec::new();
    for y in 0..h {
        let mut total = 0u64;
        let mut n = 0u64;
        let step = ((x1 - x0) / 40).max(1);
        for x in (x0..x1).step_by(step) {
            let i = y * w * 4 + x * 4;
            total += u64::from(pixels[i]) * 299
                + u64::from(pixels[i + 1]) * 587
                + u64::from(pixels[i + 2]) * 114;
            n += 1;
        }
        if total / n / 1000 < 90 {
            dark_rows.push(y);
        }
    }
    if dark_rows.is_empty() {
        return None;
    }
    let y0 = *dark_rows.first().unwrap() as u32;
    let y1 = *dark_rows.last().unwrap() as u32;
    if y1 - y0 < (h / 4) as u32 {
        return None;
    }
    Some((x0 as u32, x1 as u32, y0, y1))
}

/// Detect item row bands from the binarized panel via horizontal projection:
/// rows with ≥3 bright samples form bands; gaps ≤4 rows merge; bands shorter
/// than 35px are UI fragments (icons, separators), not item rows.
fn detect_row_bands(gray_bin: &[u8], cw: usize, ch: usize) -> Vec<(usize, usize)> {
    let bright: Vec<usize> = (0..ch)
        .map(|y| {
            let row = y * cw;
            (0..cw)
                .step_by(2)
                .filter(|&x| gray_bin[row + x] == 0)
                .count()
        })
        .collect();
    let mut bands = Vec::new();
    let mut start: Option<usize> = None;
    let mut last = 0usize;
    for (y, n) in bright.iter().enumerate() {
        if *n >= 3 {
            if start.is_none() {
                start = Some(y);
            }
            last = y;
        } else if let Some(s) = start {
            if y - last > 4 {
                if last - s >= 35 {
                    bands.push((s, last));
                }
                start = None;
            }
        }
    }
    if let Some(s) = start {
        if last - s >= 35 {
            bands.push((s, last));
        }
    }
    bands
}

/// Extract a price from a per-row price-column OCR text. Gate order:
/// 1. first comma-grouped `\d{1,3}(,\d{3})+` number (`6,000` beats any junk
///    around it — junk prefixes like `7 5,500` and trailing glyph digits
///    like `7,000 4` lose to the comma rule);
/// 2. else the first digit run with value ≥ 100 (length ≥3 in practice;
///    smaller runs are OCR specks, not prices);
/// 3. else 0 = unreadable → the capture panel shows the row for manual
///    correction, and the junk-row gate drops it when the name is unknown too.
fn extract_price(text: &str) -> i64 {
    let s = text.trim();
    if let Some(m) = rust_helpers::find_comma_number(s) {
        return m;
    }
    let runs: Vec<i64> = rust_helpers::digit_runs(s)
        .into_iter()
        .filter(|r| *r >= 100)
        .collect();
    runs.first().copied().unwrap_or(0)
}

/// Small regex-free helpers for `extract_price`.
mod rust_helpers {
    /// First `\d{1,3}(,\d{3})+` occurrence as an integer (commas stripped).
    pub fn find_comma_number(s: &str) -> Option<i64> {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() {
                // Group start: 1-3 digits then ',' + exactly 3 digits.
                let start = i;
                let mut j = i;
                while j < b.len() && b[j].is_ascii_digit() && j - start < 3 {
                    j += 1;
                }
                if j < b.len()
                    && b[j] == b','
                    && j + 4 <= b.len()
                    && b[j + 1..j + 4].iter().all(|c| c.is_ascii_digit())
                {
                    // `end` = one past the last digit of the group:
                    // comma at j, digits j+1..j+4.
                    let mut end = j + 4;
                    while end + 4 <= b.len()
                        && b[end] == b','
                        && b[end + 1..end + 4].iter().all(|c| c.is_ascii_digit())
                    {
                        end += 4;
                    }
                    let cleaned: String = s[start..end]
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect();
                    return cleaned.parse().ok();
                }
            }
            i += 1;
        }
        None
    }

    /// All maximal digit runs as integers.
    pub fn digit_runs(s: &str) -> Vec<i64> {
        let mut out = Vec::new();
        let mut cur = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                cur.push(c);
            } else if !cur.is_empty() {
                if let Ok(v) = cur.parse() {
                    out.push(v);
                }
                cur.clear();
            }
        }
        if !cur.is_empty() {
            if let Ok(v) = cur.parse() {
                out.push(v);
            }
        }
        out
    }
}

/// Binarized-region OCR: crop (bx0..bx1, by0..by1) from the binarized panel
/// buffer, nearest-neighbor upscale by `scale`, write a temp 8-bit BMP, run
/// Tesseract with page-separation mode `psm`.
///
/// The panel buffer is in absolute frame coordinates (its origin is
/// `panel_origin`), so the crop indices are relative to that origin.
fn ocr_bin_region(
    exe: &PathBuf,
    gray_bin: &[u8],
    cw: usize,
    ch: usize,
    panel_origin: (u32, u32),
    bx0: u32,
    bx1: u32,
    by0: u32,
    by1: u32,
    scale: usize,
    psm: &str,
) -> Result<String, String> {
    let (ox, oy) = (panel_origin.0 as usize, panel_origin.1 as usize);
    let rx0 = (bx0 as usize).saturating_sub(ox);
    let rx1 = (bx1 as usize).saturating_sub(ox).min(cw);
    let ry0 = (by0 as usize).saturating_sub(oy);
    let ry1 = (by1 as usize).saturating_sub(oy).min(ch);
    if rx1 <= rx0 || ry1 <= ry0 {
        return Ok(String::new());
    }
    let rw = rx1 - rx0;
    let rh = ry1 - ry0;
    let uw = rw * scale;
    let uh = rh * scale;
    let mut up = vec![0u8; uw * uh];
    for y in 0..uh {
        let sy = ry0 + y / scale;
        let src_row = sy * cw;
        let dst_row = y * uw;
        for x in 0..uw {
            up[dst_row + x] = gray_bin[src_row + rx0 + x / scale];
        }
    }

    // 8-bit grayscale BMP, top-down rows (negative height).
    // 8-bit rows must be 4-byte aligned (Leptonica rejects unpadded rows).
    let row_stride = (uw + 3) / 4 * 4;
    let mut padded = up;
    if row_stride != uw {
        let src = std::mem::take(&mut padded);
        let mut out = vec![0u8; row_stride * uh];
        for y in 0..uh {
            out[y * row_stride..y * row_stride + uw].copy_from_slice(&src[y * uw..(y + 1) * uw]);
        }
        padded = out;
    }
    let pixels_offset = 14u32 + 40 + 256 * 4;
    let file_size = pixels_offset + padded.len() as u32;
    let mut bmp = Vec::with_capacity(file_size as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&[0u8; 4]);
    bmp.extend_from_slice(&pixels_offset.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(uw as i32).to_le_bytes());
    bmp.extend_from_slice(&(-(uh as i32)).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&8u16.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    bmp.extend_from_slice(&(padded.len() as u32).to_le_bytes());
    bmp.extend_from_slice(&2835u32.to_le_bytes());
    bmp.extend_from_slice(&2835u32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    // Grayscale palette: index i → (i, i, i).
    for i in 0..=255u8 {
        bmp.extend_from_slice(&[i, i, i, 0]);
    }
    bmp.extend_from_slice(&padded);

    let tmp = std::env::temp_dir().join(format!(
        "glogger_ocr_bin_{}_{}.bmp",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    std::fs::write(&tmp, &bmp).map_err(|e| format!("Failed to write temp image: {e}"))?;
    let output = std::process::Command::new(exe)
        .arg(&tmp)
        .arg("stdout")
        .arg("--psm")
        .arg(psm)
        .arg("--oem")
        .arg("3")
        .output()
        .map_err(|e| format!("Failed to run tesseract: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if !output.status.success() {
        return Err(format!(
            "tesseract failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Pair OCR'd name cells with price cells positionally: names and prices come
/// from the same red-bar rows (top→bottom), so zip them in order.
pub fn pair_rows(
    names: &[String],
    prices: &[i64],
    item_name_index: &std::collections::HashMap<String, u32>,
) -> Vec<StallScanRow> {
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, price) in names.iter().zip(prices.iter()) {
        let cleaned = clean_item_name(name);
        if cleaned.len() < 3 {
            continue;
        }
        let (item_name, confidence) = match match_item_name(&cleaned, item_name_index) {
            Some((canonical, _id, score)) => (Some(canonical), score),
            None => (None, 0.0),
        };
        let key = (item_name.clone(), *price);
        if !seen.insert(key) {
            continue;
        }
        rows.push(StallScanRow {
            raw_text: cleaned.clone(),
            item_name,
            price: Some(*price),
            quantity: None,
            confidence,
        });
    }
    rows
}

/// Legacy full-image pass: one Tesseract run over the whole window, then
/// text-pairing. Used when red-bar detection finds no bars (theme/dpi drift).
pub fn scan_legacy(
    exe: &PathBuf,
    png_bytes: &[u8],
    item_name_index: &std::collections::HashMap<String, u32>,
) -> Result<(Vec<StallScanRow>, String, Option<String>, Option<String>), String> {
    let text = run_tesseract(exe, png_bytes, 6, false)?;
    let (stall_name, stall_slot) = parse_stall_header(&text);
    let pairs = parse_vendor_text(&text);
    let names: Vec<String> = pairs.iter().map(|(n, _)| n.clone()).collect();
    let prices: Vec<i64> = pairs.iter().map(|(_, p)| *p).collect();
    Ok((
        pair_rows(&names, &prices, item_name_index),
        text,
        stall_name,
        stall_slot,
    ))
}

// ── Window capture (Windows GDI) ────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod capture {
    use super::run_tesseract;
    use std::path::PathBuf;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT,
        DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetClientRect, GetWindowRect, IsWindowVisible,
    };

    /// Find the Project Gorgon game window. Returns None when the game isn't
    /// running (or the user hasn't opened it yet).
    pub fn find_game_window() -> Option<HWND> {
        // The game window title is "Project Gorgon".
        let title: Vec<u16> = "Project Gorgon\0".encode_utf16().collect();
        let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR::from_raw(title.as_ptr())) }.ok()?;
        if hwnd.is_invalid() {
            return None;
        }
        let is_visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        is_visible.then_some(hwnd)
    }

    fn capture_client_bgra(hwnd: HWND) -> Result<(u32, u32, Vec<u8>), String> {
        let mut rect = RECT::default();
        unsafe { GetClientRect(hwnd, &mut rect) }
            .map_err(|e| format!("GetClientRect failed: {e}"))?;
        let w = (rect.right - rect.left).max(1) as u32;
        let h = (rect.bottom - rect.top).max(1) as u32;

        match capture_via_window_dc(hwnd, w, h) {
            Ok(pixels) if !is_mostly_black(&pixels) => Ok((w, h, pixels)),
            _ => capture_via_screen_dc(hwnd, w, h),
        }
    }

    fn capture_via_window_dc(hwnd: HWND, w: u32, h: u32) -> Result<Vec<u8>, String> {
        unsafe {
            let hdc_window = GetDC(Some(hwnd));
            if hdc_window.is_invalid() {
                return Err("GetDC(window) failed".to_string());
            }
            let result = bit_blt_region(hdc_window, w, h);
            ReleaseDC(Some(hwnd), hdc_window);
            result
        }
    }

    fn capture_via_screen_dc(hwnd: HWND, w: u32, h: u32) -> Result<(u32, u32, Vec<u8>), String> {
        // Crop to the window's screen rect (fallback for exclusive fullscreen).
        let mut wrect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut wrect) }
            .map_err(|e| format!("GetWindowRect failed: {e}"))?;
        unsafe {
            let hdc_screen = GetDC(None);
            if hdc_screen.is_invalid() {
                return Err("GetDC(screen) failed".to_string());
            }
            let result = bit_blt_region(hdc_screen, w, h);
            ReleaseDC(None, hdc_screen);
            result.map(|pixels| (w, h, pixels))
        }
    }

    fn bit_blt_region(
        source_dc: windows::Win32::Graphics::Gdi::HDC,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, String> {
        unsafe {
            let mem_dc = CreateCompatibleDC(Some(source_dc));
            if mem_dc.is_invalid() {
                return Err("CreateCompatibleDC failed".to_string());
            }
            let bitmap = CreateCompatibleBitmap(source_dc, w as i32, h as i32);
            if bitmap.is_invalid() {
                let _ = DeleteDC(mem_dc);
                return Err("CreateCompatibleBitmap failed".to_string());
            }
            let old = SelectObject(mem_dc, HGDIOBJ::from(bitmap));
            let ok = BitBlt(
                mem_dc,
                0,
                0,
                w as i32,
                h as i32,
                Some(source_dc),
                0,
                0,
                SRCCOPY | CAPTUREBLT,
            );
            let pixels = if ok.is_ok() {
                read_pixels(bitmap, w, h)
            } else {
                Err("BitBlt failed".to_string())
            };
            SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ::from(bitmap));
            let _ = DeleteDC(mem_dc);
            pixels
        }
    }

    unsafe fn read_pixels(
        bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, String> {
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32); // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB
        let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
        let dc = windows::Win32::Graphics::Gdi::CreateCompatibleDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            h,
            Some(pixels.as_mut_ptr() as _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        if lines == 0 {
            return Err("GetDIBits failed".to_string());
        }
        let _ = windows::Win32::Graphics::Gdi::DeleteDC(dc);
        Ok(pixels)
    }

    fn is_mostly_black(pixels: &[u8]) -> bool {
        if pixels.is_empty() {
            return true;
        }
        let nonzero = pixels.iter().step_by(4).filter(|&&b| b != 0).count();
        let total = pixels.len() / 4;
        // "Mostly black" = >98% pure-black sample points (game UI is colorful).
        nonzero * 50 < total
    }

    /// BGRA → 24-bit BMP (bottom-up rows), which Tesseract reads natively —
    /// no image-encoding dependency.
    fn bgra_to_bmp(pixels: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
        let row_bytes = ((w * 3 + 3) / 4) * 4; // 4-byte aligned
        let data_size = row_bytes * h;
        let file_size = 54 + data_size;
        let mut out = Vec::with_capacity(file_size as usize);
        // BITMAPFILEHEADER
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&file_size.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        // BITMAPINFOHEADER (40 bytes)
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(w as i32).to_le_bytes());
        out.extend_from_slice(&(h as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        out.extend_from_slice(&data_size.to_le_bytes());
        out.extend_from_slice(&2835u32.to_le_bytes()); // ~72 DPI
        out.extend_from_slice(&2835u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // Pixel data, bottom-up, BGRA→BGR.
        for y in (0..h).rev() {
            let row_start = (y as usize) * (w as usize) * 4;
            let mut row = Vec::with_capacity(row_bytes as usize);
            for x in 0..w as usize {
                let i = row_start + x * 4;
                row.push(pixels[i]); // B
                row.push(pixels[i + 1]); // G
                row.push(pixels[i + 2]); // R
            }
            while row.len() % 4 != 0 {
                row.push(0);
            }
            out.extend_from_slice(&row);
        }
        Ok(out)
    }

    /// Full OCR pass over a captured window. Primary path: locate the stall
    /// panel (dark column), binarize bright-text-on-dark, 2x upscale, and run
    /// separate name/price column OCR passes (proven on live 3440x1440
    /// frames — the raw full-image pass interleaves chat overlay with the
    /// stall list and yields unusable text). Falls back to the legacy
    /// full-image pass when no panel is found (theme/dpi drift).
    pub fn scan_window(
        hwnd: HWND,
        exe: &PathBuf,
        item_name_index: &std::collections::HashMap<String, u32>,
    ) -> Result<
        (
            Vec<super::StallScanRow>,
            String,
            Option<String>,
            Option<String>,
        ),
        String,
    > {
        let (w, h, pixels) = capture_client_bgra(hwnd)?;
        if let Some(rows) = super::scan_panel(&pixels, w, h, exe, item_name_index)? {
            return Ok(rows);
        }
        let img = bgra_to_bmp(&pixels, w, h)?;
        super::scan_legacy(exe, &img, item_name_index)
    }
    /// Run tesseract over prepared image bytes (used by smoke flows).
    #[allow(dead_code)]
    pub fn tesseract_read(
        exe: &PathBuf,
        bytes: &[u8],
        psm: u8,
        digits: bool,
    ) -> Result<String, String> {
        run_tesseract(exe, bytes, psm, digits)
    }
}

#[cfg(target_os = "windows")]
pub use capture::find_game_window;

// ── Command entry point ─────────────────────────────────────────────────────

/// Scan the game's stall window via OCR. Pure capture: no DB writes.
#[tauri::command]
pub async fn scan_stall_window(
    app: tauri::AppHandle,
    game_data: tauri::State<'_, GameDataState>,
) -> Result<StallScanResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("The stall window scanner is only available on Windows.".to_string());
    }
    let exe = ocr_exe_path(&app)?;
    if !exe.exists() {
        return Err("OCR engine not installed — click \"Install OCR engine…\" first.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let hwnd = find_game_window()
            .ok_or_else(|| "Project Gorgon window not found — is the game running?".to_string())?;
        let item_name_index = game_data
            .try_read()
            .map_err(|_| "Game data is being refreshed — try again in a moment".to_string())?
            .item_name_index
            .clone();

        // HWND is a raw pointer (not Send) — launder through its address.
        // Handles are process-global opaque values, so this is sound.
        let hwnd_addr = hwnd.0 as usize;

        // Offload the GDI + child-process work off the async runtime.
        let result = tokio::task::spawn_blocking(move || {
            let hwnd = windows::Win32::Foundation::HWND(hwnd_addr as *mut std::ffi::c_void);
            capture::scan_window(hwnd, &exe, &item_name_index)
        })
        .await
        .map_err(|e| format!("Scan task failed: {e}"))?;

        let (rows, _debug, stall_name, stall_slot) = result?;
        Ok(StallScanResult {
            owner_name: stall_name.clone(),
            stall_name,
            stall_slot,
            rows,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, game_data, exe);
        Err("The stall window scanner is only available on Windows.".to_string())
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn index() -> HashMap<String, u32> {
        let mut m = HashMap::new();
        m.insert("Cantaloupe Wine".to_string(), 21890);
        m.insert("Astounding Throwing Knife".to_string(), 1000);
        m.insert("Blacksmithing: Mesh Grate".to_string(), 2000);
        m.insert("Carrot".to_string(), 3000);
        m
    }

    #[test]
    fn test_match_item_name_near_miss() {
        let idx = index();
        // The Emissary-verified near-miss OCR spelling.
        let hit = match_item_name("Canteloupe Wine", &idx).expect("should match");
        assert_eq!(hit.0, "Cantaloupe Wine");
        assert_eq!(hit.1, 21890);
        assert!(hit.2 >= MATCH_THRESHOLD);
    }

    #[test]
    fn test_match_item_name_exact() {
        let idx = index();
        let hit = match_item_name("Carrot", &idx).expect("exact match");
        assert_eq!(hit.0, "Carrot");
        assert_eq!(hit.2, 1.0);
    }

    #[test]
    fn test_match_item_name_rejects_garbage() {
        let idx = index();
        assert!(match_item_name("Xyzzyqwop Blorptastic", &idx).is_none());
        assert!(match_item_name("ab", &idx).is_none());
    }

    #[test]
    fn test_parse_stall_header() {
        let text = "Kegoron's Discount Distractions [CI-6]'s Shop\nBuy Pay\nAstounding Throwing Knife  400";
        let (name, slot) = parse_stall_header(text);
        assert_eq!(name.as_deref(), Some("Kegoron's Discount Distractions"));
        assert_eq!(slot.as_deref(), Some("CI-6"));
    }

    #[test]
    fn test_parse_stall_header_no_shop() {
        let (name, slot) = parse_stall_header("Just some text");
        assert!(name.is_none());
        assert!(slot.is_none());
    }

    #[test]
    fn test_parse_vendor_text_pairs() {
        let text = "\
Kegoron's Discount Distractions [CI-6]'s Shop
Astounding Throwing Knife
Buy 1
Pay 400
Superb Throwing Knife
Buy 1
Pay 400
Basic Stoneskin Potion
Buy 1
Pay 20,000";
        let pairs = parse_vendor_text(&text);
        assert_eq!(
            pairs.len(),
            3,
            "three Pay lines with name candidates: {pairs:?}"
        );
        assert_eq!(pairs[0].0, "Astounding Throwing Knife");
        assert_eq!(pairs[0].1, 400);
        assert_eq!(pairs[2].0, "Basic Stoneskin Potion");
        assert_eq!(pairs[2].1, 20000);
    }

    #[test]
    fn test_pair_rows_fuzzy_and_dedup() {
        let idx = index();
        let names = vec![
            "Canteloupe Wine".to_string(),
            "Canteloupe Wine".to_string(), // duplicate of same scan
        ];
        let prices = vec![2500, 2500];
        let rows = pair_rows(&names, &prices, &idx);
        assert_eq!(rows.len(), 1, "duplicate (name, price) deduped");
        assert_eq!(rows[0].item_name.as_deref(), Some("Cantaloupe Wine"));
        assert_eq!(rows[0].price, Some(2500));
    }

    #[test]
    fn test_clean_item_name() {
        assert_eq!(clean_item_name("Buy 1 Basic Potion 1,234"), "Basic Potion");
        assert_eq!(clean_item_name("  Carrot "), "Carrot");
        assert_eq!(clean_item_name("123 carrots;"), "carrots");
    }

    #[test]
    fn test_trigram_similarity_bounds() {
        assert!(trigram_similarity("Carrot", "Carrot") > 0.99);
        assert!(trigram_similarity("Carrot", "Canteloupe") < 0.5);
        assert_eq!(trigram_similarity("", ""), 1.0);
    }

    #[test]
    fn test_trigram_short_strings_need_identity() {
        // 2-char inputs produce no 3-grams: only identity scores 1.0.
        assert_eq!(trigram_similarity("ab", "ab"), 1.0);
        assert_eq!(trigram_similarity("ab", "ac"), 0.0);
        assert_eq!(trigram_similarity("a", "a"), 1.0);
        assert_eq!(trigram_similarity("a", "b"), 0.0);
    }
}
