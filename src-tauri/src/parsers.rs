// Pure parsing logic — no Tauri dependencies here

#[derive(Debug, serde::Serialize, Clone)]
pub struct SkillUpdate {
    pub skill_type: String,
    pub level: u32,
    pub bonus: u32,
    pub xp: u32,
    pub tnl: u32,
    pub timestamp: String, // e.g. "00:08:37"
}

pub fn parse_skill_update(line: &str) -> Option<SkillUpdate> {
    if !line.contains("ProcessUpdateSkill") {
        return None;
    }

    let timestamp = parse_timestamp(line)?;
    let skill_type = extract_field(line, "type=")?;
    let raw: u32 = extract_field(line, "raw=")?.parse().ok()?;
    let bonus: u32 = extract_field(line, "bonus=")?.parse().ok()?;
    let xp: u32 = extract_field(line, "xp=")?.parse().ok()?;
    let tnl: u32 = extract_field(line, "tnl=")?.parse().ok()?;

    Some(SkillUpdate {
        skill_type,
        level: raw + bonus,
        bonus,
        xp,
        tnl,
        timestamp,
    })
}

// Parses "[HH:MM:SS] " from the start of a line, returns "HH:MM:SS"
pub fn parse_timestamp(line: &str) -> Option<String> {
    let line = line.trim_start();
    if !line.starts_with('[') {
        return None;
    }
    let end = line.find(']')?;
    Some(line[1..end].to_string())
}

/// Convert a Player.log `HH:MM:SS` timestamp to a full UTC datetime string, using an
/// explicit UTC base date when provided. Live tailing passes `None`, and the date then
/// comes from the clock — rolled back a day when the line's time of day is still ahead
/// of it.
///
/// A log line cannot have been written *after* it was read, so a time of day ahead of
/// the clock means the line was logged the previous UTC day. Without that rollback,
/// every line read with any lag (catch-up, backlog, a paused watcher) over a UTC
/// midnight boundary is stamped with the *ingest* date — up to ~24 h in the future of
/// the line it describes (observed: a 23:31 note logged 2026-10-14 read during a
/// 01:27 catch-up on 2026-10-15 came out as `2026-10-15 23:31:26`).
pub fn to_utc_datetime_with_base(time_str: &str, base_date: Option<chrono::NaiveDate>) -> String {
    to_utc_datetime_at(time_str, base_date, chrono::Utc::now())
}

/// [`to_utc_datetime_with_base`] against a fixed clock (test seam).
pub(crate) fn to_utc_datetime_at(
    time_str: &str,
    base_date: Option<chrono::NaiveDate>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    use chrono::{Duration, NaiveTime};

    let Ok(time_of_day) = NaiveTime::parse_from_str(time_str, "%H:%M:%S") else {
        return now.format("%Y-%m-%d %H:%M:%S").to_string();
    };

    let date = match base_date {
        Some(date) => date,
        None => {
            // Slack absorbs rounding/sub-second skew between the game's clock and ours
            // while still catching a genuine rollover (a line logged seconds before UTC
            // midnight and read seconds after it).
            let line_at = now.date_naive().and_time(time_of_day);
            if line_at - now.naive_utc() > Duration::seconds(5) {
                now.date_naive() - Duration::days(1)
            } else {
                now.date_naive()
            }
        }
    };

    date.and_time(time_of_day)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

/// Where the date for a Player.log `HH:MM:SS` line comes from, in precedence
/// order:
///
/// 1. an explicit override (`set_base_date` — replay / old-log reparse, whose
///    file-derived date is authoritative), else
/// 2. the session date declared by the log's own login line (`anchor_session`),
///    rolled forward whenever the time of day goes backwards (the log crossed
///    UTC midnight) — correct however far the read lags the log, and
/// 3. the clock, rolled back a day when the line's time of day is still ahead of
///    it (see [`to_utc_datetime_with_base`]) — the fallback when no login line
///    was read (e.g. tailing a file from mid-session).
///
/// Shared by the game-state manager and the survey aggregator, the two places
/// that stamp Player.log times.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogDateResolver {
    base_date: Option<chrono::NaiveDate>,
    /// Session date + the time of day of the last line resolved.
    session: Option<(chrono::NaiveDate, Option<chrono::NaiveTime>)>,
}

impl LogDateResolver {
    /// Fix the date outright (replay / old-log reparse); wins over the anchor.
    pub fn set_base_date(&mut self, date: chrono::NaiveDate) {
        self.base_date = Some(date);
    }

    /// Anchor from a Player.log login line's own `Time UTC=` instant
    /// (`PlayerEvent::LogSessionAnchored`).
    pub fn anchor_session(&mut self, utc: chrono::NaiveDateTime) {
        self.session = Some((utc.date(), Some(utc.time())));
    }

    /// Resolve one `HH:MM:SS` line to a full UTC datetime string.
    pub fn resolve(&mut self, ts: &str) -> String {
        self.resolve_at(ts, chrono::Utc::now())
    }

    /// [`Self::resolve`] against a fixed clock (test seam).
    pub(crate) fn resolve_at(&mut self, ts: &str, now: chrono::DateTime<chrono::Utc>) -> String {
        if let Some(date) = self.base_date {
            // `to_utc_datetime_at`, not `to_utc_datetime_with_base`: the latter
            // reads the real clock on unparsable input, which would break this
            // seam's fixed-clock contract (see `peek_at`, which already threads
            // `now` here).
            return to_utc_datetime_at(ts, Some(date), now);
        }

        let Some((date, last_time)) = self.session.as_mut() else {
            return to_utc_datetime_at(ts, None, now);
        };
        let Ok(time_of_day) = chrono::NaiveTime::parse_from_str(ts, "%H:%M:%S") else {
            return to_utc_datetime_at(ts, None, now);
        };

        if last_time.is_some_and(|previous| time_of_day < previous) {
            if let Some(next) = date.succ_opt() {
                *date = next;
            }
        }
        *last_time = Some(time_of_day);

        to_utc_datetime_with_base(ts, Some(*date))
    }

    /// Like [`Self::resolve`], but **non-advancing**: returns the date the line
    /// would get if it were resolved next in order, leaving the rollover
    /// baseline untouched.
    ///
    /// Side channels that read a subset of the stream out of order must use
    /// this. [`Self::resolve`] rolls the session date whenever the time of day
    /// goes backwards, so resolving a *later* line first makes the in-order
    /// replay of an *earlier* one look like a UTC-midnight crossing and dates
    /// every following row a day into the future.
    pub fn peek(&self, ts: &str) -> String {
        self.peek_at(ts, chrono::Utc::now())
    }

    /// [`Self::peek`] against a fixed clock (test seam).
    pub(crate) fn peek_at(&self, ts: &str, now: chrono::DateTime<chrono::Utc>) -> String {
        if let Some(date) = self.base_date {
            return to_utc_datetime_at(ts, Some(date), now);
        }
        let Some((date, last_time)) = self.session else {
            return to_utc_datetime_at(ts, None, now);
        };
        let Ok(time_of_day) = chrono::NaiveTime::parse_from_str(ts, "%H:%M:%S") else {
            return to_utc_datetime_at(ts, None, now);
        };

        // Apply the hypothetical rollover `resolve_at` would have committed, so
        // a line that really did cross UTC midnight still lands on its own day.
        let date = match last_time {
            Some(previous) if time_of_day < previous => date.succ_opt().unwrap_or(date),
            _ => date,
        };

        date.and_time(time_of_day)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
}

/// Parse a `Player.log` login line, returning `(character_name, utc_instant)`.
///
/// Format: `[HH:MM:SS] Logged in as character <Name>. Time UTC=MM/DD/YYYY HH:MM:SS. Timezone Offset ±HH:MM:SS`
///
/// The bracket time equals the `Time UTC=` time (both UTC), so this one line
/// names the character *and* declares the session's UTC instant — the
/// authoritative date anchor for every `HH:MM:SS` line after it. The kill/loot
/// ingest and the Statehelm gift backfill derive a session's base date from it,
/// and the live parser forwards it as `PlayerEvent::LogSessionAnchored` so a
/// catch-up that lags the log by more than a day still dates lines by the
/// session they came from.
pub fn parse_player_log_login_line(line: &str) -> Option<(String, chrono::NaiveDateTime)> {
    let marker = "Logged in as character ";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    // Name runs up to the first '.' (character names contain no periods).
    let dot = rest.find('.')?;
    let character_name = rest[..dot].trim().trim_matches(['[', ']']).to_string();
    if character_name.is_empty() {
        return None;
    }

    let utc_marker = "Time UTC=";
    let utc_start = line.find(utc_marker)? + utc_marker.len();
    let mut tokens = line[utc_start..].split_whitespace();
    let date_token = tokens.next()?;
    // The time token ends with the sentence's period: "22:43:41."
    let time_token = tokens.next()?.trim_end_matches('.');
    let date = chrono::NaiveDate::parse_from_str(date_token, "%m/%d/%Y").ok()?;
    let time = chrono::NaiveTime::parse_from_str(time_token, "%H:%M:%S").ok()?;
    Some((character_name, date.and_time(time)))
}

/// Convert a Chat.log local-time NaiveDateTime to UTC by subtracting the timezone offset.
///
/// Chat.log timestamps are in the player's local timezone. The offset (seconds east of UTC)
/// is parsed from the chat login line's "Timezone Offset" field.
pub fn chat_local_to_utc(
    local_dt: chrono::NaiveDateTime,
    tz_offset_seconds: i32,
) -> chrono::NaiveDateTime {
    use chrono::Duration;
    local_dt - Duration::seconds(tz_offset_seconds as i64)
}

// Extracts the value after `key` up to the next comma or `}`
pub fn extract_field(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let end = rest.find(|c| c == ',' || c == '}').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

// ============================================================
// Loot Text Parsing (reusable across features)
// ============================================================
//
// Used by the survey aggregator to extract speed-bonus item markers from
// ProcessScreenText lines like:
//   "Blue Spinel collected! Also found Rubywall Crystal x2 (speed bonus!)"

/// Individual loot item parsed from screen text
#[derive(Debug, serde::Serialize, Clone)]
pub struct LootItem {
    pub item_name: String,
    pub quantity: u32,
    pub is_speed_bonus: bool,
    pub is_primary: bool,
}

/// Parse loot text into individual items.
/// Example: "Tsavorite collected! Also found Moss Agate x2, Onyx (speed bonus!)"
/// Returns (Vec<LootItem>, speed_bonus_earned)
pub fn parse_loot_items(loot_text: &str) -> (Vec<LootItem>, bool) {
    let mut items = Vec::new();
    let speed_bonus_earned = loot_text.contains("(speed bonus!)");

    // Primary item: everything before " collected!"
    // May include quantity for ring bonus, e.g. "Lapis Lazuli x2 collected!"
    if let Some(collected_idx) = loot_text.find(" collected!") {
        let primary_text = loot_text[..collected_idx].trim();
        if let Some((name, qty)) = parse_item_with_quantity(primary_text) {
            items.push(LootItem {
                item_name: name,
                quantity: qty,
                is_speed_bonus: false,
                is_primary: true,
            });
        }
    }

    // Secondary items: after "Also found "
    if let Some(also_idx) = loot_text.find("Also found ") {
        // Strip trailing "(speed bonus!)" or similar parentheticals
        let mut secondary = &loot_text[also_idx + "Also found ".len()..];

        // Check if these are speed bonus items
        let is_bonus = if let Some(bonus_idx) = secondary.find(" (speed bonus!)") {
            secondary = &secondary[..bonus_idx];
            true
        } else {
            // Remove any other parentheticals
            if let Some(paren_idx) = secondary.find(" (") {
                secondary = &secondary[..paren_idx];
            }
            false
        };

        // Split on commas and parse each item
        for part in secondary.split(',') {
            let piece = part.trim();
            if piece.is_empty() {
                continue;
            }

            // Match "Item Name x3" or just "Item Name"
            if let Some((name, qty)) = parse_item_with_quantity(piece) {
                items.push(LootItem {
                    item_name: name,
                    quantity: qty,
                    is_speed_bonus: is_bonus,
                    is_primary: false,
                });
            }
        }
    }

    (items, speed_bonus_earned)
}

/// Parse "Item Name x3" or "Item Name" into (name, quantity)
pub fn parse_item_with_quantity(text: &str) -> Option<(String, u32)> {
    // Match "Item Name x3"
    if let Some(x_pos) = text.rfind(" x") {
        let name = text[..x_pos].trim();
        let qty_str = &text[x_pos + 2..];
        if let Ok(qty) = qty_str.parse::<u32>() {
            return Some((name.to_string(), qty));
        }
    }
    // Just "Item Name"
    Some((text.to_string(), 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp_at(instant: &str, time_of_day: &str) -> String {
        to_utc_datetime_at(
            time_of_day,
            None,
            chrono::DateTime::parse_from_rfc3339(instant)
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
    }

    /// A lagging read must not stamp the log line with the ingest date: the line's own
    /// time of day is always behind the clock, so an ahead-of-clock time means
    /// "yesterday". This is the exact line observed in the wild (a 23:31 gift note read
    /// during a 01:27 catch-up was stored as `2026-10-15 23:31:26` — ~24 h in the future).
    #[test]
    fn lagged_read_over_utc_midnight_dates_the_line_yesterday() {
        assert_eq!(
            stamp_at("2026-10-15T01:27:00Z", "23:31:26"),
            "2026-10-14 23:31:26"
        );
        assert_eq!(
            stamp_at("2026-10-15T01:27:00Z", "18:05:00"),
            "2026-10-14 18:05:00"
        );
    }

    /// A real-time read keeps today's date (the normal live path — unchanged).
    #[test]
    fn real_time_read_keeps_todays_utc_date() {
        assert_eq!(
            stamp_at("2026-10-14T23:35:00Z", "23:31:26"),
            "2026-10-14 23:31:26"
        );
        assert_eq!(
            stamp_at("2026-10-15T00:01:00Z", "00:00:58"),
            "2026-10-15 00:00:58"
        );
    }

    /// The midnight edge itself: logged seconds before, read seconds after.
    #[test]
    fn line_logged_seconds_before_midnight_and_read_after_it() {
        assert_eq!(
            stamp_at("2026-10-15T00:00:03Z", "23:59:59"),
            "2026-10-14 23:59:59"
        );
    }

    /// Small skew between the game's clock and ours stays on today's date.
    #[test]
    fn small_clock_skew_is_tolerated() {
        assert_eq!(
            stamp_at("2026-10-15T12:00:00Z", "12:00:04"),
            "2026-10-15 12:00:04"
        );
    }

    /// An explicit base date (replay / old-log reparse) always wins — no heuristic.
    #[test]
    fn explicit_base_date_is_authoritative() {
        assert_eq!(
            to_utc_datetime_at(
                "23:31:26",
                Some(chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()),
                chrono::DateTime::parse_from_rfc3339("2026-10-15T01:27:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
            "2026-10-12 23:31:26"
        );
    }

    /// An unparsable time still yields a usable stamp (the clock itself).
    #[test]
    fn unparsable_time_falls_back_to_the_clock() {
        assert_eq!(
            stamp_at("2026-10-15T01:27:00Z", "not a time"),
            "2026-10-15 01:27:00"
        );
    }

    /// `resolve_at`'s fixed-clock seam must hold on the base-date path too: it
    /// called the real clock on unparsable input (via
    /// `to_utc_datetime_with_base`), so this path could not be tested with a
    /// fixed clock and disagreed with `peek_at`, which already threads `now`.
    #[test]
    fn base_date_path_honours_the_clock_seam() {
        let now = fixed_now("2026-10-15T01:27:00Z");
        let mut dates = LogDateResolver::default();
        dates.set_base_date(chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap());

        assert_eq!(dates.resolve_at("23:31:26", now), "2026-10-12 23:31:26");
        assert_eq!(
            dates.resolve_at("not a time", now),
            "2026-10-15 01:27:00",
            "unparsable input falls back to the injected clock, not the real one"
        );
    }

    fn naive(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn fixed_now(instant: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(instant)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    /// The login line's own UTC instant anchors the date, whatever the clock
    /// says — this is the multi-day-lag case (catch-up reading a log days old).
    #[test]
    fn resolver_anchors_on_the_login_line_not_the_clock() {
        let mut dates = LogDateResolver::default();
        dates.anchor_session(naive("2026-10-14 22:43:41"));
        let now = fixed_now("2026-10-20T05:00:00Z");

        assert_eq!(dates.resolve_at("22:43:41", now), "2026-10-14 22:43:41");
        assert_eq!(dates.resolve_at("23:31:15", now), "2026-10-14 23:31:15");
        // The log crosses UTC midnight: the session date rolls forward once.
        assert_eq!(dates.resolve_at("00:10:00", now), "2026-10-15 00:10:00");
        assert_eq!(dates.resolve_at("00:22:51", now), "2026-10-15 00:22:51");
        // ...and keeps rolling for a later session's login line.
        dates.anchor_session(naive("2026-10-15 21:05:00"));
        assert_eq!(dates.resolve_at("21:06:00", now), "2026-10-15 21:06:00");
    }

    /// A side channel (the coordinator's milking resolve) reads a *later* line
    /// before the batch replay reaches the earlier ones: `peek_at` must report
    /// that line without moving the baseline the in-order scan relies on.
    #[test]
    fn peek_reports_a_later_line_without_rolling_the_in_order_scan() {
        let now = fixed_now("2026-10-15T05:00:00Z");
        let mut dates = LogDateResolver::default();
        dates.anchor_session(naive("2026-10-14 22:43:41"));

        assert_eq!(dates.resolve_at("22:43:41", now), "2026-10-14 22:43:41");

        // Peeked later than the last resolved line, and across UTC midnight.
        assert_eq!(dates.peek_at("23:31:26", now), "2026-10-14 23:31:26");
        assert_eq!(dates.peek_at("00:06:05", now), "2026-10-15 00:06:05");

        // The in-order scan that follows still dates from the session: the
        // peeked lines must not have rolled the day for the batch's earlier
        // events (before the seam this came out one day ahead).
        assert_eq!(dates.resolve_at("22:44:00", now), "2026-10-14 22:44:00");
        assert_eq!(dates.resolve_at("00:06:05", now), "2026-10-15 00:06:05");
        assert_eq!(dates.resolve_at("00:06:40", now), "2026-10-15 00:06:40");
    }

    /// Precedence: explicit base date (replay) > login anchor > clock.
    #[test]
    fn resolver_precedence_is_base_date_then_anchor_then_clock() {
        let now = fixed_now("2026-10-15T01:27:00Z");

        // No anchor, no override → clock, with the one-day rollback.
        let mut dates = LogDateResolver::default();
        assert_eq!(dates.resolve_at("23:31:26", now), "2026-10-14 23:31:26");
        assert_eq!(dates.resolve_at("00:10:00", now), "2026-10-15 00:10:00");

        // An explicit base date wins even with an anchor present.
        let mut dates = LogDateResolver::default();
        dates.set_base_date(chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        dates.anchor_session(naive("2026-10-14 22:43:41"));
        assert_eq!(dates.resolve_at("23:31:26", now), "2026-10-01 23:31:26");
    }

    /// The real login line, including the one that crosses UTC midnight.
    #[test]
    fn parses_the_player_log_login_line() {
        assert_eq!(
            parse_player_log_login_line(
                "[22:43:41] Logged in as character TwinkleofToes. Time UTC=10/14/2026 22:43:41. Timezone Offset -05:00:00"
            ),
            Some(("TwinkleofToes".to_string(), naive("2026-10-14 22:43:41")))
        );
        assert_eq!(
            parse_player_log_login_line(
                "[00:22:51] Logged in as character TwinkleofToes. Time UTC=10/15/2026 00:22:51. Timezone Offset -05:00:00"
            ),
            Some(("TwinkleofToes".to_string(), naive("2026-10-15 00:22:51")))
        );
        // Bracketed name (the older form `log_watchers` matches) has no UTC time.
        assert_eq!(
            parse_player_log_login_line("[12:00:00] Logged in as character [Zenith]"),
            None
        );
        // A time-of-day without a date, or a truncated line, is not a login line.
        assert_eq!(parse_player_log_login_line("Time UTC=10/14/2026"), None);
        assert_eq!(
            parse_player_log_login_line("Logged in as character TwinkleofToes. Time UTC="),
            None
        );
    }

    #[test]
    fn test_parse_loot_simple() {
        let (items, bonus) = parse_loot_items("Fluorite collected!");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_name, "Fluorite");
        assert_eq!(items[0].quantity, 1);
        assert!(items[0].is_primary);
        assert!(!bonus);
    }

    #[test]
    fn test_parse_loot_with_speed_bonus() {
        let (items, bonus) = parse_loot_items(
            "Blue Spinel collected! Also found Rubywall Crystal x2 (speed bonus!)",
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_name, "Blue Spinel");
        assert_eq!(items[0].quantity, 1);
        assert!(items[0].is_primary);
        assert_eq!(items[1].item_name, "Rubywall Crystal");
        assert_eq!(items[1].quantity, 2);
        assert!(items[1].is_speed_bonus);
        assert!(bonus);
    }

    #[test]
    fn test_parse_loot_primary_with_quantity() {
        // Ring bonus can grant extra primary items: "Lapis Lazuli x2 collected!"
        let (items, bonus) =
            parse_loot_items("Lapis Lazuli x2 collected! Also found Azurite (speed bonus!)");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_name, "Lapis Lazuli");
        assert_eq!(items[0].quantity, 2);
        assert!(items[0].is_primary);
        assert_eq!(items[1].item_name, "Azurite");
        assert_eq!(items[1].quantity, 1);
        assert!(items[1].is_speed_bonus);
        assert!(bonus);
    }

    #[test]
    fn test_parse_loot_primary_quantity_no_bonus() {
        let (items, bonus) = parse_loot_items("Obsidian x2 collected!");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_name, "Obsidian");
        assert_eq!(items[0].quantity, 2);
        assert!(items[0].is_primary);
        assert!(!bonus);
    }
}
