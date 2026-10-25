//! Survey tracker — Phase 5 of the item-provenance overhaul.
//!
//! Thin consumer of the unified provenance pipeline. See:
//!
//! - [docs/plans/survey-tracker-rewrite.md] — full design and implementation plan
//! - [docs/architecture/survey-mechanics.md] — game-side survey behavior reference
//! - [docs/plans/item-provenance-overhaul.md] — parent provenance plan
//!
//! ## Module layout
//!
//! - [`types`] — Public data types shared across the module (sessions, uses,
//!   trigger kinds, status enums).
//! - [`persistence`] — All DB I/O. Pure CRUD, no business logic.
//! - [`multihit_state`] — DB-backed open-multihit-node tracking. State must
//!   survive app restart (the 30-minute window can outlive a session crash).
//! - [`aggregator`] — Subscribes to `PlayerEvent`s, stitches survey-use ↔
//!   mining-context attribution, drives session lifecycle and writes to
//!   the persistence layer.
//! - [`commands`] — Tauri commands the frontend calls.

pub mod aggregator;
pub mod commands;
#[allow(dead_code)] // Scaffolding for in-progress multihit tracking
pub mod multihit_state;
pub mod persistence;
pub mod types;

#[cfg(test)]
mod replay_tests;

#[allow(unused_imports)] // re-exports surface the public types for callers
pub use types::{SessionStartTrigger, SurveySession, SurveyUse, SurveyUseKind, SurveyUseStatus};

/// Parse the zone name out of a survey map's internal name.
///
/// Survey maps follow `GeologySurvey<Zone><N>` (mineral) or
/// `MiningSurvey<Zone><N>X?` (mining) — e.g. `"GeologySurveyEltibule2"`
/// → `"Eltibule"`, `"MiningSurveySouthSerbule1X"` → `"SouthSerbule"`.
///
/// Used by the aggregator as a fallback for `survey_uses.area` when the
/// live tracked area isn't known (which is most of the time today, since
/// the area-tracking signal isn't routed into the aggregator yet).
///
/// Mirrors the same parser the CDN ingestion uses to populate
/// `survey_types.zone`, so the two paths stay aligned.
#[allow(dead_code)] // Used by tests; will be called from aggregator once area-tracking is routed in
pub fn parse_zone_from_internal_name(internal_name: &str) -> Option<String> {
    let rest = internal_name
        .strip_prefix("GeologySurvey")
        .or_else(|| internal_name.strip_prefix("MiningSurvey"))?;
    if rest.is_empty() {
        return None;
    }
    // Strip trailing digits and the "X" suffix used by mining surveys.
    let zone: String = rest
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == 'X')
        .to_string();
    if zone.is_empty() {
        None
    } else {
        Some(zone)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_zone_from_internal_name;

    #[test]
    fn parses_geology_surveys() {
        assert_eq!(
            parse_zone_from_internal_name("GeologySurveyEltibule2"),
            Some("Eltibule".to_string()),
        );
        assert_eq!(
            parse_zone_from_internal_name("GeologySurveyKurMountains3"),
            Some("KurMountains".to_string()),
        );
    }

    #[test]
    fn parses_mining_surveys_including_x_suffix() {
        assert_eq!(
            parse_zone_from_internal_name("MiningSurveySouthSerbule1X"),
            Some("SouthSerbule".to_string()),
        );
        assert_eq!(
            parse_zone_from_internal_name("MiningSurveyPovus7Y"),
            // The 'Y' suffix isn't part of the established convention, so
            // it stays attached to the zone — better to surface a slightly
            // ugly zone name than silently strip an unexpected suffix.
            Some("Povus7Y".to_string()),
        );
    }

    #[test]
    fn returns_none_for_unrecognized_prefixes() {
        assert_eq!(
            parse_zone_from_internal_name("CookingSurveyEltibule1"),
            None
        );
        assert_eq!(parse_zone_from_internal_name("GeologySurvey"), None);
        assert_eq!(parse_zone_from_internal_name(""), None);
    }
}
