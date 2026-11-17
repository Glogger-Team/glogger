//! Casino arena fight tracker.
//!
//! Project: Gorgon's casino arena is run by the NPC **Kuzavek**, who narrates
//! every match on the `[NPC Chatter]` chat channel. Two things are recoverable
//! from that narration, with no manual input from the player:
//!
//!   1. **The matchup** — an intro pair announces the two fighters, e.g.
//!      `Introducing our two competitors! On the far side is the mighty ogre OTIS!`
//!      followed by `And on the side nearest me is Leo The Walking Cat!`
//!   2. **The result** — a later line names the winner or the loser, e.g.
//!      `Get that cat some milk. Leo wins!` or `Otis has been defeated!`.
//!
//! The player's own bet is placed via a `Player.log` talk screen (NPC 5712) and
//! is not needed here: this tracker only records *who fought whom and who won*,
//! so the dashboard can rank fighters and predict the favorite in any matchup.
//!
//! Design: a small state machine ([`ArenaTracker`]) fed every parsed chat
//! message. Kuzavek's flowery prose means fighter names are wrapped in flavor
//! text, so we match against a known roster (seeded with the current 7 fighters)
//! by whole-word, case-insensitive comparison. New fighters are auto-discovered
//! from the clean `"<Name> wins!"` / `"<Name> loses!"` result phrasings and
//! folded into the roster so future matchups involving them are recognized.
//!
//! Because a result line is scoped to the *pending* pair (we already know the
//! two fighters from the intro), we don't need the winner's name to sit next to
//! the verb — a line like `Ushug was definitely not expecting this! He's been
//! defeated!` resolves fine: it carries a loss verb and names exactly one of the
//! pending pair (Ushug), so the other fighter is the winner.

/// The fighters known to exist in the arena at time of writing. The tracker
/// seeds its roster from this list; additional names are learned at runtime.
pub const SEED_ROSTER: &[&str] = &["Corrrak", "Dura", "Gloz", "Leo", "Otis", "Ushug", "Vizlark"];

/// Phrases that mark a *win* for the fighter named in the same line.
const WIN_MARKERS: &[&str] = &[" wins!", "is Undefeated!", "victory for"];

/// Phrases that mark a *loss* for the fighter named in the same line. These are
/// deliberately specific so mid-fight taunts don't false-trigger: e.g. `Neither
/// fighter is defeated yet!` contains "is defeated" but none of these markers.
const LOSS_MARKERS: &[&str] = &[
    " loses!",
    "been defeated!",
    "has fallen",
    "has collapsed",
    " is down",
    " is out",
];

/// A fully-resolved arena match ready to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArenaMatch {
    /// Local wall-clock timestamp ("YYYY-MM-DD HH:MM:SS") of the result line.
    pub fought_at: String,
    /// First-announced fighter (the "far side" / "Introducing" fighter).
    pub fighter_a: String,
    /// Second-announced fighter (the "side nearest me" fighter).
    pub fighter_b: String,
    /// Whichever of `fighter_a` / `fighter_b` won.
    pub winner: String,
}

/// Stateful narration tracker. Feed it every parsed chat message via
/// [`ArenaTracker::observe`]; it emits an [`ArenaMatch`] when a match resolves.
#[derive(Debug, Default)]
pub struct ArenaTracker {
    roster: Vec<String>,
    /// First fighter of the match currently being announced/fought, if any.
    pending_a: Option<String>,
    /// Second fighter, once the "And on the side..." line lands.
    pending_b: Option<String>,
}

impl ArenaTracker {
    /// Create a tracker seeded with the known roster.
    pub fn new() -> Self {
        Self {
            roster: SEED_ROSTER.iter().map(|s| s.to_string()).collect(),
            pending_a: None,
            pending_b: None,
        }
    }

    /// Feed one chat message. `sender`/`channel`/`text` come straight from the
    /// parsed `ChatMessage`; `timestamp` is the local "YYYY-MM-DD HH:MM:SS"
    /// string. Returns `Some(match)` only when a match resolves on this line.
    pub fn observe(
        &mut self,
        channel: Option<&str>,
        sender: Option<&str>,
        text: &str,
        timestamp: &str,
    ) -> Option<ArenaMatch> {
        // Kuzavek is the only arena narrator; ignore everything else.
        if channel != Some("NPC Chatter") || sender != Some("Kuzavek") {
            return None;
        }

        // ── Intro line 1: names the first fighter, resets any half-formed pair.
        if text.starts_with("Introducing") {
            self.pending_a = self.extract_known(text);
            self.pending_b = None;
            return None;
        }

        // ── Intro line 2: names the second fighter, completing the matchup.
        if text.starts_with("And on the side") {
            if self.pending_a.is_some() {
                self.pending_b = self.extract_known(text);
            }
            return None;
        }

        // ── Otherwise, maybe a result line. Learn any newly-seen names first so
        // future intros recognize them, even if this particular result can't be
        // attributed to the current pending pair.
        self.discover(text);

        // Match-resolution trigger: a result line naming exactly one pending fighter
        // resolves the WHOLE match (winner inferred). Personal-bet tracking
        // was retired with the Player.log pipeline: the game's verbose
        // LocalPlayer lines no longer arrive, so only the match outcome is
        // recorded now.
        let (Some(a), Some(b)) = (self.pending_a.clone(), self.pending_b.clone()) else {
            return None;
        };
        // Never resolve a degenerate pair (defensive; intros are distinct).
        // Clear it here so a stale same-name pair can't linger and match a
        // later line's verb.
        if a.eq_ignore_ascii_case(&b) {
            self.pending_a = None;
            self.pending_b = None;
            return None;
        }

        // Lowercase once: the win/loss verb markers are matched
        // case-insensitively (narration capitalizes mid-sentence, e.g.
        // "Leo Wins!" vs "wins!").
        let line_lower = text.to_lowercase();
        let has_win = WIN_MARKERS.iter().any(|m| line_lower.contains(&m.to_lowercase()));
        let has_loss = LOSS_MARKERS.iter().any(|m| line_lower.contains(&m.to_lowercase()));
        let names_a = contains_word(text, &a);
        let names_b = contains_word(text, &b);

        // Resolve only when the verb is unambiguous AND exactly one of the pair
        // is named in the line. Lines naming both (or neither) are skipped; the
        // pending pair survives for a subsequent, clearer line.
        let winner = if has_win && !has_loss {
            match (names_a, names_b) {
                (true, false) => a.clone(),
                (false, true) => b.clone(),
                _ => return None,
            }
        } else if has_loss && !has_win {
            match (names_a, names_b) {
                (true, false) => b.clone(), // a lost → b won
                (false, true) => a.clone(),  // b lost → a won
                _ => return None,
            }
        } else {
            return None;
        };
        self.pending_a = None;
        self.pending_b = None;
        Some(ArenaMatch {
            fought_at: timestamp.to_string(),
            fighter_a: a,
            fighter_b: b,
            winner,
        })
    }

    /// First roster name appearing as a whole word in `text`, canonical-cased.
    fn extract_known(&self, text: &str) -> Option<String> {
        self.roster
            .iter()
            .find(|name| contains_word(text, name))
            .cloned()
    }

    /// Learn a fighter name from a clean result phrasing (`"<Name> wins!"` or
    /// `"<Name> loses!"`), where the name is a single token before the verb.
    /// New names are stored canonical-cased (Titlecase) and matched case-
    /// insensitively thereafter.
    fn discover(&mut self, text: &str) {
        for marker in [" wins!", " loses!"] {
            if let Some(idx) = text.find(marker) {
                let before = &text[..idx];
                if let Some(tok) = before.split_whitespace().last() {
                    let cleaned: String = tok.chars().filter(|c| c.is_ascii_alphabetic()).collect();
                    if cleaned.len() >= 3 {
                        let canon = titlecase(&cleaned);
                        if !self.roster.iter().any(|n| n.eq_ignore_ascii_case(&canon)) {
                            self.roster.push(canon);
                        }
                    }
                }
            }
        }
    }

    /// Current roster (seed + discovered). Exposed for tests/diagnostics.
    #[cfg(test)]
    pub fn roster(&self) -> &[String] {
        &self.roster
    }
}

/// Case-insensitive whole-word containment: is `word` present in `haystack`
/// bounded by non-alphanumeric characters (or string ends)? Guards against
/// `Leo` matching inside a longer token.
fn contains_word(haystack: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let hay = haystack.to_ascii_lowercase();
    let needle = word.to_ascii_lowercase();
    let bytes = hay.as_bytes();
    let mut start = 0;
    while let Some(pos) = hay[start..].find(&needle) {
        let i = start + pos;
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let end = i + needle.len();
        let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = i + 1;
    }
    false
}

/// "OTIS" / "otis" → "Otis". ASCII-only; adequate for arena fighter names.
fn titlecase(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CH: Option<&str> = Some("NPC Chatter");
    const KZ: Option<&str> = Some("Kuzavek");
    const TS: &str = "2026-07-20 20:52:59";

    /// Drive a full match through the tracker: intro A, intro B, result.
    fn run(intro_a: &str, intro_b: &str, result: &str) -> Option<ArenaMatch> {
        let mut t = ArenaTracker::new();
        assert!(t.observe(CH, KZ, intro_a, TS).is_none());
        assert!(t.observe(CH, KZ, intro_b, TS).is_none());
        t.observe(CH, KZ, result, TS)
    }

    #[test]
    fn winner_named_directly() {
        let m = run(
            "Introducing our two competitors! On the far side is the mighty ogre OTIS!",
            "And on the side nearest me is Leo The Walking Cat!",
            "Get that cat some milk. Leo wins!",
        )
        .unwrap();
        assert_eq!(m.fighter_a, "Otis");
        assert_eq!(m.fighter_b, "Leo");
        assert_eq!(m.winner, "Leo");
    }

    #[test]
    fn loser_named_directly() {
        let m = run(
            "Introducing our two competitors! On the far side is the mighty ogre OTIS!",
            "And on the side nearest me is Leo The Walking Cat!",
            "What a phenomenal battle! Otis has been defeated!",
        )
        .unwrap();
        assert_eq!(m.winner, "Leo");
    }

    #[test]
    fn loser_named_by_pronoun_line() {
        // The winner isn't named; the loser (Ushug) is, via a loss verb.
        let m = run(
            "Introducing our warriors! On the side farthest from me is Ushug the Undefeatable!",
            "And on the side closer to me is the ferocious Gloz!",
            "Ushug was definitely not expecting this! He's been defeated!",
        )
        .unwrap();
        assert_eq!(m.winner, "Gloz");
    }

    #[test]
    fn is_down_loss_marker() {
        let m = run(
            "Introducing our two competitors! On the far side is Leo The Lion-Man!",
            "And on the side nearest me is the mighty ogre OTIS!",
            "Oh, Leo is down! And he's not getting back up... the fight is over",
        )
        .unwrap();
        assert_eq!(m.winner, "Otis");
    }

    #[test]
    fn ushug_undefeated_is_a_win() {
        let m = run(
            "Introducing our warriors! On the side farthest from me is Ushug the Undefeatable!",
            "And on the side closer to me is the ferocious Gloz!",
            "Ushug has earned his nickname! Ushug is Undefeated!",
        )
        .unwrap();
        assert_eq!(m.winner, "Ushug");
    }

    #[test]
    fn midfight_taunt_does_not_resolve() {
        let mut t = ArenaTracker::new();
        t.observe(
            CH,
            KZ,
            "Introducing our two competitors! On the far side is Corrrak!",
            TS,
        );
        t.observe(
            CH,
            KZ,
            "And on the side nearest me is Vizlark the Violent!",
            TS,
        );
        // Neither a win nor a loss marker → no resolution.
        assert!(t
            .observe(CH, KZ, "Neither fighter is defeated yet!", TS)
            .is_none());
        assert!(t
            .observe(CH, KZ, "The end approaches! Who will stand victorious?", TS)
            .is_none());
        // A real result still resolves afterward.
        let m = t.observe(CH, KZ, "Vizlark wins!", TS).unwrap();
        assert_eq!(m.winner, "Vizlark");
    }

    #[test]
    fn ignores_non_kuzavek_and_other_channels() {
        let mut t = ArenaTracker::new();
        assert!(t
            .observe(Some("Global"), Some("SomePlayer"), "Otis wins!", TS)
            .is_none());
        assert!(t.observe(CH, Some("OtherNpc"), "Otis wins!", TS).is_none());
    }

    #[test]
    fn result_without_intro_is_ignored() {
        // No pending pair → a lone result line resolves nothing.
        let mut t = ArenaTracker::new();
        assert!(t.observe(CH, KZ, "Dura wins!", TS).is_none());
    }

    #[test]
    fn whole_word_matching_guards_substrings() {
        // "Leon" must not match fighter "Leo".
        assert!(!contains_word("Leon the barbarian appears", "Leo"));
        assert!(contains_word("Get that cat some milk. Leo wins!", "Leo"));
        assert!(contains_word("the mighty ogre OTIS!", "Otis"));
    }

    #[test]
    fn discovers_new_fighter_from_clean_result() {
        let mut t = ArenaTracker::new();
        // A brand-new fighter wins with the clean phrasing → learned.
        t.observe(CH, KZ, "Newcomer wins!", TS);
        assert!(t.roster().iter().any(|n| n == "Newcomer"));
    }

    #[test]
    fn timestamp_is_carried_through() {
        let m = run(
            "Introducing our two competitors! On the far side is Otis the Ogre!",
            "And on the side nearest me is Leo The Lion-Man!",
            "Leo wins!",
        )
        .unwrap();
        assert_eq!(m.fought_at, TS);
    }

}
