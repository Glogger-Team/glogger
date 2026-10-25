/// Resolves year-less `"Mon Apr 13 14:29"` stall-log timestamps to real ISO 8601
/// datetimes by walking entries oldest-first and detecting year boundaries.
///
/// The in-game PlayerShopLog book format omits the year, so the Stall Tracker
/// needs a separate inference pass before rows land in SQLite. This module is
/// pure — no I/O, no DB, no app state — so it can be exhaustively unit tested.
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};

/// Parse a raw `"Mon Apr 13 14:29"` stall-log timestamp into `(month, day, hour, minute)`.
/// Day-of-week is ignored (redundant with the date once a year is known).
fn parse_month_day_time(raw: &str) -> Option<(u32, u32, u32, u32)> {
    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() != 4 {
        return None;
    }
    let month = month_num(parts[1])?;
    let day: u32 = parts[2].parse().ok()?;
    let (hour_s, minute_s) = parts[3].split_once(':')?;
    let hour: u32 = hour_s.parse().ok()?;
    let minute: u32 = minute_s.parse().ok()?;
    Some((month, day, hour, minute))
}

fn month_num(mon: &str) -> Option<u32> {
    Some(match mon {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    })
}

fn format_iso(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Option<String> {
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let dt = NaiveDateTime::new(date, chrono::NaiveTime::from_hms_opt(hour, minute, 0)?);
    Some(dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Walks timestamps in **oldest-first order**, assigning a year to each and
/// incrementing when a backward month jump (e.g. Dec → Jan) is detected.
///
/// Caller MUST reverse the book's newest-first content before passing it here,
/// matching the parser's `raw_entries.reverse()` contract.
pub fn resolve_timestamps_oldest_first(timestamps: &[&str], base_year: i32) -> Vec<Option<String>> {
    let mut out = Vec::with_capacity(timestamps.len());
    let mut year = base_year;
    let mut prev_month: Option<u32> = None;

    for raw in timestamps {
        let Some((month, day, hour, minute)) = parse_month_day_time(raw) else {
            out.push(None);
            continue;
        };
        if let Some(pm) = prev_month {
            if month < pm {
                year += 1;
            }
        }
        prev_month = Some(month);
        out.push(format_iso(year, month, day, hour, minute));
    }
    out
}

/// Picks the base year for a live-tailed book: `now.year()`, minus one if the
/// oldest entry's `(month, day)` is **in the future** relative to today. That
/// means the book is still showing entries from the previous calendar year.
pub fn base_year_for_live(oldest_ts: &str) -> i32 {
    let now = Local::now();
    let Some((month, day, _, _)) = parse_month_day_time(oldest_ts) else {
        return now.year();
    };
    let now_m = now.month();
    let now_d = now.day();
    if (month, day) > (now_m, now_d) {
        now.year() - 1
    } else {
        now.year()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_year_book_resolves_to_base_year() {
        let ts = vec!["Mon Apr 13 09:00", "Tue Apr 14 10:30", "Wed Apr 15 11:45"];
        let out = resolve_timestamps_oldest_first(&ts, 2026);
        assert_eq!(
            out,
            vec![
                Some("2026-04-13 09:00:00".to_string()),
                Some("2026-04-14 10:30:00".to_string()),
                Some("2026-04-15 11:45:00".to_string()),
            ]
        );
    }

    #[test]
    fn year_boundary_book_bumps_year_on_backward_month_jump() {
        let ts = vec![
            "Mon Dec 28 09:00",
            "Tue Dec 30 10:00",
            "Fri Jan 02 11:00",
            "Sat Jan 03 12:00",
        ];
        let out = resolve_timestamps_oldest_first(&ts, 2025);
        assert_eq!(
            out,
            vec![
                Some("2025-12-28 09:00:00".to_string()),
                Some("2025-12-30 10:00:00".to_string()),
                Some("2026-01-02 11:00:00".to_string()),
                Some("2026-01-03 12:00:00".to_string()),
            ]
        );
    }

    #[test]
    fn resolver_skips_unparseable_entries_without_panicking() {
        let ts = vec!["garbage", "Mon Apr 13 09:00"];
        let out = resolve_timestamps_oldest_first(&ts, 2026);
        assert_eq!(out[0], None);
        assert_eq!(out[1], Some("2026-04-13 09:00:00".to_string()));
    }

    #[test]
    fn base_year_for_live_in_past_returns_current_year() {
        // Can't construct arbitrary `now`, so test via the pure helper shape:
        // oldest entry in past → base_year == now.year()
        let now = Local::now();
        let past_day = if now.day() > 1 { now.day() - 1 } else { 1 };
        let ts = format!("Mon {} {:02} 09:00", month_abbr(now.month()), past_day);
        assert_eq!(base_year_for_live(&ts), now.year());
    }

    #[test]
    fn base_year_for_live_wraps_to_previous_year_when_oldest_entry_is_in_future() {
        // If the oldest entry's (month, day) is strictly after today,
        // the book must be from the previous calendar year.
        let now = Local::now();
        let (future_month, future_day) = if now.month() == 12 && now.day() == 31 {
            // Edge case: no "future" date in current year — skip assertion.
            return;
        } else if now.day() < 28 {
            (now.month(), now.day() + 1)
        } else {
            (
                if now.month() == 12 {
                    1
                } else {
                    now.month() + 1
                },
                1,
            )
        };
        // Edge: Jan 1 of next month isn't "future in same year" when month wraps,
        // so only assert on non-wrapping case.
        if future_month > now.month() || (future_month == now.month() && future_day > now.day()) {
            let ts = format!("Mon {} {:02} 09:00", month_abbr(future_month), future_day);
            assert_eq!(base_year_for_live(&ts), now.year() - 1);
        }
    }

    fn month_abbr(m: u32) -> &'static str {
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ][(m - 1) as usize]
    }
}
