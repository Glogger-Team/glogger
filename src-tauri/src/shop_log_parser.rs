/// Parses the body of a `PlayerShopLog` book (delivered via `ProcessBook`)
/// into structured stall events, ready for persistence by the Stall Tracker.
///
/// Six action types are recognized (`bought`, `added`, `removed`, `configured`,
/// `visible`, `collected`); anything that doesn't match lands in `unknown` so
/// nothing is silently dropped. The module is pure — no DB, no I/O — so the
/// parser can be exhaustively unit tested.
use crate::stall_year_resolver;
use once_cell::sync::Lazy;
use regex::Regex;

/// Splits the book content into entries by matching the `"Day Mon DD HH:MM - "`
/// timestamp prefix. Multi-line mode so it works against the whole content.
static ENTRY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)([A-Z][a-z]{2} [A-Z][a-z]{2} \d{1,2} \d{1,2}:\d{2}) - ").unwrap()
});

/// `MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500`
/// `MARCELA bought Aquamarine x5 at a cost of 750 per 1 = 3750`
///
/// The optional `x?N` after the item handles stacked listings — the seller
/// titled the listing as "Aquamarine x5", the game logs it verbatim, and we
/// strip the suffix into `qty` so the item name is canonical for grouping.
static BOUGHT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<player>.+?) bought (?P<item>.+?)(?: ?x(?P<qty>\d+))? at a cost of (?P<unit>\d+) per (?P<qty_unit>\d+) = (?P<total>\d+)",
    )
    .unwrap()
});

/// `Deradon added Barley Seeds x36 to shop` (optional ` x36`).
static ADDED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<player>.+?) added (?P<item>.+?)(?: ?x(?P<qty>\d+))? to shop").unwrap()
});

/// `Deradon removed Decent Horseshoes from shop` (optional ` x36`).
static REMOVED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<player>.+?) removed (?P<item>.+?)(?: ?x(?P<qty>\d+))? from shop").unwrap()
});

/// `Deradon configured Barley Seedsx36 to cost 3000 per 2.` — with or without
/// trailing restriction and with or without a space before `xN`.
static CONFIGURED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<player>.+?) configured (?P<item>.+?)(?: ?x(?P<qty>\d+))? to cost (?P<unit>\d+) per (?P<qty_unit>\d+)",
    )
    .unwrap()
});

/// `Deradon made Nice Saddle visible in shop at a cost of 4000 per 1`
static VISIBLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<player>.+?) made (?P<item>.+?)(?: ?x(?P<qty>\d+))? visible in shop at a cost of (?P<unit>\d+) per (?P<qty_unit>\d+)",
    )
    .unwrap()
});

/// `Deradon collected 30500 Councils from customer purchases`
static COLLECTED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<player>.+?) collected (?P<total>\d+) Councils from customer purchases")
        .unwrap()
});

#[derive(Debug, Clone, PartialEq)]
pub struct ShopLogEntry {
    pub entry_index: i64,
    pub timestamp: String,
    pub event_at: Option<String>,
    pub action: String,
    pub player: String,
    pub item: Option<String>,
    pub quantity: i64,
    pub price_unit: Option<f64>,
    pub price_total: Option<i64>,
    pub raw_message: String,
}

#[derive(Debug, Clone)]
pub struct ShopLog {
    pub log_timestamp: String,
    pub title: String,
    pub entries: Vec<ShopLogEntry>,
    /// Advisory owner hint derived from the first owner-only action
    /// (`added`, `removed`, `configured`, `visible`, `collected`). Used ONLY
    /// by the Import command to distinguish friend-file / alt-file /
    /// bought-only cases. The live-tailing coordinator path ignores this
    /// and stamps the active character as owner instead (the game only lets
    /// a player open their own shop log, so active == owner by
    /// construction); the file watcher does the same via
    /// `effective_owner = owner.or(active_character)`.
    pub owner: Option<String>,
}

/// Parses a full book body.
///
/// * `title` — book title (`"Today's Shop Logs"` etc.).
/// * `content` — raw book body, may contain literal `\n` pairs (escaped by
///   ProcessBook) which we normalize to real newlines.
/// * `log_timestamp` — timestamp when the book was opened (not the entry times).
/// * `base_year` — the year to start the resolver at; use
///   [`stall_year_resolver::base_year_for_live`] for live tailing.
pub fn parse_shop_log(title: &str, content: &str, log_timestamp: &str, base_year: i32) -> ShopLog {
    // Live book bodies carry literal `\n` escapes (ProcessBook shape); files
    // read from disk already have real newlines, for which this is a no-op.
    // Strip a leading UTF-8 BOM first, symmetric with `parse_drop_data`
    // (kill_tracking_commands.rs) and `stats_owner` (books_watcher.rs): an
    // Excel-saved or re-exported book would otherwise glue `\u{feff}` to the
    // first timestamp and shift its entry's match start.
    let content = content
        .strip_prefix('\u{feff}')
        .unwrap_or(content)
        .replace("\\n", "\n");
    let matches: Vec<_> = ENTRY_RE.find_iter(&content).collect();
    let mut raw_entries: Vec<(String, String)> = Vec::new();
    for (i, m) in matches.iter().enumerate() {
        // The match captures `"Day Mon DD HH:MM - "`; strip the trailing ` - `.
        let ts = content[m.start()..m.end()]
            .trim_end_matches([' ', '-'])
            .trim()
            .to_string();
        let msg_start = m.end();
        let msg_end = matches.get(i + 1).map_or(content.len(), |n| n.start());
        let message = content[msg_start..msg_end].trim().to_string();
        if !message.is_empty() {
            raw_entries.push((ts, message));
        }
    }

    // CRITICAL: book content arrives newest-first. Reverse before indexing so
    // the oldest entry always gets index 0 — indices are stable across re-opens
    // as new entries append rather than renumber. See plan §3.3.
    raw_entries.reverse();

    let mut entries: Vec<ShopLogEntry> = raw_entries
        .iter()
        .enumerate()
        .map(|(i, (ts, msg))| parse_entry(i as i64, ts, msg))
        .collect();

    let timestamps: Vec<&str> = entries.iter().map(|e| e.timestamp.as_str()).collect();
    let resolved = stall_year_resolver::resolve_timestamps_oldest_first(&timestamps, base_year);
    for (entry, event_at) in entries.iter_mut().zip(resolved) {
        entry.event_at = event_at;
    }

    let owner = entries
        .iter()
        .find(|e| {
            matches!(
                e.action.as_str(),
                "added" | "removed" | "configured" | "visible" | "collected"
            )
        })
        .map(|e| e.player.clone());

    ShopLog {
        log_timestamp: log_timestamp.to_string(),
        title: title.to_string(),
        entries,
        owner,
    }
}

fn parse_entry(entry_index: i64, timestamp: &str, message: &str) -> ShopLogEntry {
    let mut entry = ShopLogEntry {
        entry_index,
        timestamp: timestamp.to_string(),
        event_at: None,
        action: "unknown".to_string(),
        player: String::new(),
        item: None,
        quantity: 1,
        price_unit: None,
        price_total: None,
        raw_message: message.to_string(),
    };

    if let Some(c) = BOUGHT_RE.captures(message) {
        let unit_raw: f64 = c["unit"].parse().unwrap_or(0.0);
        let qty_unit: f64 = c["qty_unit"].parse().unwrap_or(1.0);
        let total: i64 = c["total"].parse().unwrap_or(0);
        // Corrected per-unit price for bulk configs ("3000 per 2" → 1500).
        let unit_price = if qty_unit > 0.0 {
            unit_raw / qty_unit
        } else {
            unit_raw
        };
        entry.action = "bought".into();
        entry.player = c["player"].to_string();
        entry.item = Some(c["item"].to_string());
        entry.price_unit = Some(unit_price);
        entry.price_total = Some(total);
        // Quantity resolution:
        // - If the item was a stacked listing ("Aquamarine x5"), the regex
        //   captures the stack count directly — that's the actual unit count
        //   the buyer received.
        // - Otherwise, derive from total / corrected-unit-price, which
        //   correctly handles bulk-priced listings ("3000 per 2 = 6000" → 4
        //   units at 1500 each).
        // Either path preserves the invariant `price_unit * quantity == price_total`.
        entry.quantity = if let Some(qty_match) = c.name("qty") {
            qty_match.as_str().parse().unwrap_or(1)
        } else if unit_price > 0.0 {
            (total as f64 / unit_price).round() as i64
        } else {
            1
        };
        return entry;
    }

    if let Some(c) = ADDED_RE.captures(message) {
        entry.action = "added".into();
        entry.player = c["player"].to_string();
        entry.item = Some(c["item"].to_string());
        entry.quantity = c
            .name("qty")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(1);
        return entry;
    }

    if let Some(c) = REMOVED_RE.captures(message) {
        entry.action = "removed".into();
        entry.player = c["player"].to_string();
        entry.item = Some(c["item"].to_string());
        entry.quantity = c
            .name("qty")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(1);
        return entry;
    }

    if let Some(c) = CONFIGURED_RE.captures(message) {
        let unit: f64 = c["unit"].parse().unwrap_or(0.0);
        let qty_unit: f64 = c["qty_unit"].parse().unwrap_or(1.0);
        entry.action = "configured".into();
        entry.player = c["player"].to_string();
        entry.item = Some(c["item"].to_string());
        entry.quantity = c
            .name("qty")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(1);
        entry.price_unit = Some(if qty_unit > 0.0 {
            unit / qty_unit
        } else {
            unit
        });
        return entry;
    }

    if let Some(c) = VISIBLE_RE.captures(message) {
        let unit: f64 = c["unit"].parse().unwrap_or(0.0);
        let qty_unit: f64 = c["qty_unit"].parse().unwrap_or(1.0);
        entry.action = "visible".into();
        entry.player = c["player"].to_string();
        entry.item = Some(c["item"].to_string());
        entry.quantity = c
            .name("qty")
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(1);
        entry.price_unit = Some(if qty_unit > 0.0 {
            unit / qty_unit
        } else {
            unit
        });
        return entry;
    }

    if let Some(c) = COLLECTED_RE.captures(message) {
        let total: i64 = c["total"].parse().unwrap_or(0);
        entry.action = "collected".into();
        entry.player = c["player"].to_string();
        entry.price_total = Some(total);
        entry.quantity = 1;
        return entry;
    }

    entry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single(msg: &str) -> ShopLogEntry {
        parse_entry(0, "Mon Apr 13 14:29", msg)
    }

    #[test]
    fn test_parse_bought() {
        let e = single("MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500");
        assert_eq!(e.action, "bought");
        assert_eq!(e.player, "MrBonq");
        assert_eq!(e.item.as_deref(), Some("Quality Reins"));
        assert_eq!(e.quantity, 1);
        assert_eq!(e.price_unit, Some(4500.0));
        assert_eq!(e.price_total, Some(4500));
    }

    #[test]
    fn test_parse_bought_with_quantity() {
        let e = single("Zangariel bought Orcish Spell Pouch at a cost of 450 per 1 = 5400");
        assert_eq!(e.action, "bought");
        assert_eq!(e.quantity, 12);
        assert_eq!(e.price_total, Some(5400));
    }

    #[test]
    fn test_parse_bought_bulk_pricing() {
        // "3000 per 2" must produce price_unit = 1500, NEVER 3000.
        // A buyer spending 6000 total at 1500/unit received 4 units — the
        // invariant `price_unit * quantity == price_total` must hold.
        let e = single("MrBonq bought Barley Seeds at a cost of 3000 per 2 = 6000");
        assert_eq!(e.action, "bought");
        assert_eq!(e.price_unit, Some(1500.0));
        assert_eq!(e.quantity, 4);
        let invariant = e.price_unit.unwrap() * e.quantity as f64;
        assert_eq!(invariant as i64, e.price_total.unwrap());
    }

    #[test]
    fn test_parse_bought_invariant_holds_for_single_price() {
        // Also assert the invariant for the common non-bulk case.
        let e = single("Zangariel bought Orcish Spell Pouch at a cost of 450 per 1 = 5400");
        let invariant = e.price_unit.unwrap() * e.quantity as f64;
        assert_eq!(invariant as i64, e.price_total.unwrap());
    }

    #[test]
    fn test_parse_bought_stacked_listing_with_xn() {
        // Real game log line: stacked listing where the seller titled the
        // item "Aquamarine x5". The xN suffix MUST be stripped from the item
        // name so all 5 stacked sales aggregate under "Aquamarine", not as
        // five distinct phantom items ("Aquamarine x5", "Aquamarine x3", ...).
        let e = single("MARCELA bought Aquamarine x5 at a cost of 750 per 1 = 3750");
        assert_eq!(e.action, "bought");
        assert_eq!(e.player, "MARCELA");
        assert_eq!(e.item.as_deref(), Some("Aquamarine"));
        assert_eq!(e.quantity, 5);
        assert_eq!(e.price_unit, Some(750.0));
        assert_eq!(e.price_total, Some(3750));
        // Invariant: price_unit * quantity == price_total
        let invariant = e.price_unit.unwrap() * e.quantity as f64;
        assert_eq!(invariant as i64, e.price_total.unwrap());
    }

    #[test]
    fn test_parse_bought_stacked_listing_no_space_before_xn() {
        // Some game lines omit the space between item and xN. The optional
        // " ?" inside the regex group handles both forms.
        let e = single("MrBonq bought Quality Reinsx2 at a cost of 4500 per 1 = 9000");
        assert_eq!(e.item.as_deref(), Some("Quality Reins"));
        assert_eq!(e.quantity, 2);
    }

    #[test]
    fn test_parse_bought_no_xn_still_works() {
        // The xN group is optional. A plain item name resolves through the
        // total/unit_price fallback path.
        let e = single("Lexxi bought Aquamarine at a cost of 750 per 1 = 750");
        assert_eq!(e.item.as_deref(), Some("Aquamarine"));
        assert_eq!(e.quantity, 1);
        assert_eq!(e.price_unit, Some(750.0));
    }

    #[test]
    fn test_parse_bought_bulk_no_xn_still_works() {
        // Bulk-priced listing without xN: the fallback path computes
        // quantity from total / corrected-unit-price.
        let e = single("MrBonq bought Barley Seeds at a cost of 3000 per 2 = 6000");
        assert_eq!(e.item.as_deref(), Some("Barley Seeds"));
        assert_eq!(e.quantity, 4);
        assert_eq!(e.price_unit, Some(1500.0));
        let invariant = e.price_unit.unwrap() * e.quantity as f64;
        assert_eq!(invariant as i64, e.price_total.unwrap());
    }

    #[test]
    fn test_parse_added_no_quantity() {
        let e = single("Deradon added Nice Saddle to shop");
        assert_eq!(e.action, "added");
        assert_eq!(e.player, "Deradon");
        assert_eq!(e.item.as_deref(), Some("Nice Saddle"));
        assert_eq!(e.quantity, 1);
    }

    #[test]
    fn test_parse_added_with_quantity_space() {
        let e = single("Deradon added Barley Seeds x36 to shop");
        assert_eq!(e.action, "added");
        assert_eq!(e.item.as_deref(), Some("Barley Seeds"));
        assert_eq!(e.quantity, 36);
    }

    #[test]
    fn test_parse_added_with_quantity_no_space() {
        let e = single("Deradon added Barley Seedsx36 to shop");
        assert_eq!(e.action, "added");
        assert_eq!(e.item.as_deref(), Some("Barley Seeds"));
        assert_eq!(e.quantity, 36);
    }

    #[test]
    fn test_parse_removed() {
        let e = single("Deradon removed Decent Horseshoes from shop");
        assert_eq!(e.action, "removed");
        assert_eq!(e.item.as_deref(), Some("Decent Horseshoes"));
        assert_eq!(e.quantity, 1);
    }

    #[test]
    fn test_parse_removed_with_quantity() {
        let e = single("Deradon removed Barley Seeds x12 from shop");
        assert_eq!(e.action, "removed");
        assert_eq!(e.quantity, 12);
    }

    #[test]
    fn test_parse_configured_basic() {
        let e = single("Deradon configured Nice Saddle to cost 4000 per 1.");
        assert_eq!(e.action, "configured");
        assert_eq!(e.item.as_deref(), Some("Nice Saddle"));
        assert_eq!(e.price_unit, Some(4000.0));
    }

    #[test]
    fn test_parse_configured_with_bulk() {
        let e = single("Deradon configured Barley Seedsx36 to cost 3000 per 2.");
        assert_eq!(e.action, "configured");
        assert_eq!(e.item.as_deref(), Some("Barley Seeds"));
        assert_eq!(e.quantity, 36);
        assert_eq!(e.price_unit, Some(1500.0));
    }

    #[test]
    fn test_parse_configured_with_space_before_quantity() {
        let e = single("Deradon configured Barley Seeds x36 to cost 3000 per 2.");
        assert_eq!(e.action, "configured");
        assert_eq!(e.item.as_deref(), Some("Barley Seeds"));
        assert_eq!(e.quantity, 36);
        assert_eq!(e.price_unit, Some(1500.0));
    }

    #[test]
    fn test_parse_visible_basic() {
        let e = single("Deradon made Nice Saddle visible in shop at a cost of 4000 per 1");
        assert_eq!(e.action, "visible");
        assert_eq!(e.item.as_deref(), Some("Nice Saddle"));
        assert_eq!(e.price_unit, Some(4000.0));
    }

    #[test]
    fn test_parse_visible_with_space_before_quantity() {
        let e = single("Deradon made Barley Seeds x36 visible in shop at a cost of 3000 per 2");
        assert_eq!(e.action, "visible");
        assert_eq!(e.item.as_deref(), Some("Barley Seeds"));
        assert_eq!(e.quantity, 36);
        assert_eq!(e.price_unit, Some(1500.0));
    }

    #[test]
    fn test_parse_collected() {
        let e = single("Deradon collected 30500 Councils from customer purchases");
        assert_eq!(e.action, "collected");
        assert_eq!(e.player, "Deradon");
        assert_eq!(e.price_total, Some(30500));
        assert_eq!(e.item, None);
    }

    #[test]
    fn test_parse_unknown_falls_through() {
        // Hire-duration message that doesn't match any action regex.
        let e = single("Deradon paid 5000 Councils to hire a stall for 24 hours");
        assert_eq!(e.action, "unknown");
        assert_eq!(
            e.raw_message,
            "Deradon paid 5000 Councils to hire a stall for 24 hours"
        );
    }

    #[test]
    fn test_parse_shop_log_full_reverses_and_detects_owner() {
        // Content as the game provides it: NEWEST FIRST.
        let content = "\
Wed Apr 15 11:45 - MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500

Tue Apr 14 10:30 - Deradon made Quality Reins visible in shop at a cost of 4500 per 1

Mon Apr 13 09:00 - Deradon added Quality Reins to shop";
        let log = parse_shop_log("Today's Shop Logs", content, "2026-04-15 12:00:00", 2026);
        assert_eq!(log.entries.len(), 3);
        // After reversal, index 0 is the OLDEST (the `added`).
        assert_eq!(log.entries[0].action, "added");
        assert_eq!(log.entries[0].entry_index, 0);
        assert_eq!(
            log.entries[0].event_at.as_deref(),
            Some("2026-04-13 09:00:00")
        );
        assert_eq!(log.entries[1].action, "visible");
        assert_eq!(log.entries[2].action, "bought");
        assert_eq!(log.entries[2].entry_index, 2);
        assert_eq!(log.owner.as_deref(), Some("Deradon"));
    }

    #[test]
    fn test_parse_shop_log_with_escaped_newlines() {
        // ProcessBook hands us literal `\n` pairs, not real newlines.
        // Content is newest-first as the game delivers it.
        let content =
            "Tue Apr 14 10:30 - MrBonq bought Nice Saddle at a cost of 4000 per 1 = 4000\\n\\nMon Apr 13 09:00 - Deradon added Nice Saddle to shop";
        let log = parse_shop_log("Today's Shop Logs", content, "2026-04-14 11:00:00", 2026);
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.entries[0].action, "added");
        assert_eq!(log.entries[1].action, "bought");
    }

    #[test]
    fn test_entry_indices_stable_across_reopens() {
        // First open: two entries.
        let content_v1 = "\
Tue Apr 14 10:30 - MrBonq bought Nice Saddle at a cost of 4000 per 1 = 4000

Mon Apr 13 09:00 - Deradon added Nice Saddle to shop";
        let v1 = parse_shop_log("x", content_v1, "t1", 2026);

        // Second open: a new entry has been prepended (newest-first = top).
        let content_v2 = "\
Wed Apr 15 11:45 - MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500

Tue Apr 14 10:30 - MrBonq bought Nice Saddle at a cost of 4000 per 1 = 4000

Mon Apr 13 09:00 - Deradon added Nice Saddle to shop";
        let v2 = parse_shop_log("x", content_v2, "t2", 2026);

        // The two entries that existed in v1 must keep the SAME indices in v2.
        let find = |log: &ShopLog, needle: &str| {
            log.entries
                .iter()
                .find(|e| e.raw_message.contains(needle))
                .map(|e| e.entry_index)
        };
        assert_eq!(find(&v1, "added Nice Saddle"), Some(0));
        assert_eq!(find(&v2, "added Nice Saddle"), Some(0));
        assert_eq!(find(&v1, "bought Nice Saddle"), Some(1));
        assert_eq!(find(&v2, "bought Nice Saddle"), Some(1));
        // The new entry gets the next index.
        assert_eq!(find(&v2, "bought Quality Reins"), Some(2));
    }

    #[test]
    fn test_duplicate_same_minute_entries_get_distinct_indices() {
        // Two buyers of the same item at the same price in the same minute
        // must get different entry_index values so the unique key doesn't
        // collapse them.
        let content = "\
Mon Apr 13 14:29 - AlestiarWolf bought Nice Saddle at a cost of 4000 per 1 = 4000

Mon Apr 13 14:29 - MrBonq bought Nice Saddle at a cost of 4000 per 1 = 4000";
        let log = parse_shop_log("x", content, "t", 2026);
        assert_eq!(log.entries.len(), 2);
        assert_ne!(log.entries[0].entry_index, log.entries[1].entry_index);
    }

    #[test]
    fn test_parse_shop_log_empty_content_returns_no_entries() {
        // Empty / whitespace-only content should produce zero entries and
        // a None owner — the import command relies on this to surface the
        // "No shop log entries found" message instead of erroring.
        let log = parse_shop_log("Imported", "", "imported", 2026);
        assert!(log.entries.is_empty());
        assert_eq!(log.owner, None);

        let log = parse_shop_log("Imported", "   \n  \n\n", "imported", 2026);
        assert!(log.entries.is_empty());
        assert_eq!(log.owner, None);
    }

    #[test]
    fn test_parse_shop_log_garbage_content_returns_no_entries() {
        // A file with no parseable timestamp prefix produces zero entries.
        // The user-facing path: `import_shop_log_file` returns total_entries=0
        // and the frontend shows "No shop log entries found".
        let log = parse_shop_log("Imported", "this is not a shop log\n", "imported", 2026);
        assert!(log.entries.is_empty());
    }

    #[test]
    fn test_bought_only_log_has_no_owner_hint() {
        // No owner actions (added/removed/etc.) → owner hint is None.
        let content = "Mon Apr 13 14:29 - MrBonq bought Nice Saddle at a cost of 4000 per 1 = 4000";
        let log = parse_shop_log("x", content, "t", 2026);
        assert_eq!(log.owner, None);
    }
}
