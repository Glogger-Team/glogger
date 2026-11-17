// Pure parsing logic — no Tauri dependencies here

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

/// Parse a `Player.log` login line, returning `(character_name, utc_instant)`.
///
/// Format: `[HH:MM:SS] Logged in as character <Name>. Time UTC=MM/DD/YYYY HH:MM:SS. Timezone Offset ±HH:MM:SS`
///
/// The bracket time equals the `Time UTC=` time (both UTC), so this one line
/// names the character *and* declares the session's UTC instant — the
/// authoritative date anchor for the session that follows it. The coordinator
/// adopts the session's character on catch-up and re-arms a missing live
/// identity from it.
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

    fn naive(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
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
}
