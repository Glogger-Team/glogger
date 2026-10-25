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

// ─────────────────────────────────────────────────────────────────────────────
// Personal betting tracker
// ─────────────────────────────────────────────────────────────────────────────
//
// The player's own arena bets are placed through NPC 5712 (Kuzavek) in
// `Player.log`, entirely separate from the chat-side fight narration above.
// A full bet cycle looks like:
//
//   ProcessTalkScreen(5712, "Confirm Bet", "You are betting <em>7500</em>
//     Councils that Otis defeats Leo in the next arena match. If your fighter is
//     victorious, you will receive <em>14250</em> Councils. ...")
//   ProcessTalkScreen(5712, "", "Success! You have placed your bet for Otis. ...")
//   ProcessScreenText(CombatInfo, "You received 14,250 Councils.")   // win only
//
// Win/loss is fully recoverable from Player.log alone, thanks to the game's
// "one bet per battle, strictly sequential" rule: a `You received <payout>
// Councils.` line before the next bet is placed means the active bet **won**;
// the next bet being placed with no matching payout means the previous bet
// **lost** (a loss produces no payout line at all). The payout amount is read
// from the Confirm screen, so it discriminates arena payouts from unrelated
// council gains (e.g. roulette's `1,800`).

use crate::parsers::{parse_timestamp, to_utc_datetime_with_base};

/// A resolved personal bet, ready to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArenaBet {
    /// Full "YYYY-MM-DD HH:MM:SS" datetime the bet was placed (anchored to the
    /// log's date at confirm time).
    pub placed_at: String,
    /// Fighter the player bet on.
    pub pick: String,
    /// The opposing fighter.
    pub opponent: String,
    /// Councils wagered.
    pub wager: i64,
    /// Councils the player receives if the pick wins.
    pub payout: i64,
    /// Whether the pick won (payout received before the next bet).
    pub won: bool,
}

/// A bet awaiting resolution.
#[derive(Debug, Clone)]
struct PendingBet {
    placed_at: String,
    pick: String,
    opponent: String,
    wager: i64,
    payout: i64,
}

/// Stateful tracker over `Player.log` lines that resolves the player's own bets
/// into win/loss outcomes. Feed it lines in order via [`ArenaBetTracker::observe_line`].
#[derive(Debug, Default)]
pub struct ArenaBetTracker {
    /// Set on a "Confirm Bet" screen, before the "Success!" placement confirms it.
    tentative: Option<PendingBet>,
    /// Set once placement is confirmed, awaiting the fight outcome.
    active: Option<PendingBet>,
}

impl ArenaBetTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one `Player.log` line. `base_date` anchors the line's HH:MM:SS to a
    /// calendar date (the log file's date; `None` = today). Returns a resolved
    /// bet when this line resolves one — either a win (payout received) or the
    /// previous bet's loss (a new bet is being placed with no payout seen).
    pub fn observe_line(
        &mut self,
        line: &str,
        base_date: Option<chrono::NaiveDate>,
    ) -> Option<ArenaBet> {
        // ── Confirm Bet: parse the wager/pick/opponent/payout. ──
        if line.contains("\"Confirm Bet\"")
            && line.contains("You are betting ")
            && line.contains(" in the next arena match")
        {
            if let Some(bet) = parse_confirm_bet(line, base_date) {
                // A new bet being placed means any still-active bet lost (its
                // fight ended without a payout). Emit that loss now; stash the
                // new bet as tentative until "Success!" confirms placement.
                let loss = self.active.take().map(|b| ArenaBet {
                    placed_at: b.placed_at,
                    pick: b.pick,
                    opponent: b.opponent,
                    wager: b.wager,
                    payout: b.payout,
                    won: false,
                });
                self.tentative = Some(bet);
                return loss;
            }
            return None;
        }

        // ── Success! placement confirmed → promote tentative to active. ──
        if line.contains("Success! You have placed your bet for ") {
            if let Some(pick) = parse_placed_pick(line) {
                if let Some(t) = self.tentative.take() {
                    if t.pick.eq_ignore_ascii_case(&pick) {
                        self.active = Some(t);
                    }
                }
            }
            return None;
        }

        // ── Payout: a council gain matching the active bet's payout = win. ──
        if line.contains("ProcessScreenText(")
            && line.contains("You received ")
            && line.contains(" Councils.")
        {
            if let Some(amount) = parse_received_councils(line) {
                if let Some(a) = &self.active {
                    if a.payout == amount {
                        let b = self.active.take().unwrap();
                        return Some(ArenaBet {
                            placed_at: b.placed_at,
                            pick: b.pick,
                            opponent: b.opponent,
                            wager: b.wager,
                            payout: b.payout,
                            won: true,
                        });
                    }
                }
            }
        }

        None
    }
}

/// Parse a "Confirm Bet" talk-screen line into a [`PendingBet`].
fn parse_confirm_bet(line: &str, base_date: Option<chrono::NaiveDate>) -> Option<PendingBet> {
    // Strip the emphasis tags so numbers/names sit in plain text.
    let clean = line.replace("<em>", "").replace("</em>", "");

    let after_bet = clean.split("You are betting ").nth(1)?;
    let wager_str = after_bet.split(" Councils that ").next()?;
    let wager: i64 = wager_str.replace(',', "").trim().parse().ok()?;

    let after_that = after_bet.split(" Councils that ").nth(1)?;
    let pick = after_that.split(" defeats ").next()?.trim().to_string();

    let after_defeats = after_that.split(" defeats ").nth(1)?;
    let opponent = after_defeats
        .split(" in the next arena match")
        .next()?
        .trim()
        .to_string();

    let after_receive = clean.split("you will receive ").nth(1)?;
    let payout_str = after_receive.split(" Councils").next()?;
    let payout: i64 = payout_str.replace(',', "").trim().parse().ok()?;

    if pick.is_empty() || opponent.is_empty() {
        return None;
    }

    let placed_at =
        to_utc_datetime_with_base(parse_timestamp(line).as_deref().unwrap_or(""), base_date);

    Some(PendingBet {
        placed_at,
        pick,
        opponent,
        wager,
        payout,
    })
}

/// Parse the picked fighter from a "Success! You have placed your bet for X." line.
fn parse_placed_pick(line: &str) -> Option<String> {
    let after = line
        .split("Success! You have placed your bet for ")
        .nth(1)?;
    let pick = after.split('.').next()?.trim().to_string();
    if pick.is_empty() {
        None
    } else {
        Some(pick)
    }
}

/// Parse the amount from a `... "You received 14,250 Councils." ...` line.
fn parse_received_councils(line: &str) -> Option<i64> {
    let after = line.split("You received ").nth(1)?;
    let num = after.split(" Councils.").next()?;
    num.replace(',', "").trim().parse().ok()
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

    // ── Personal betting tracker ──────────────────────────────────────────
    const CONFIRM: &str = "[16:22:03] LocalPlayer: ProcessTalkScreen(5712, \"Confirm Bet\", \"You are betting <em>7500</em> Councils that Otis defeats Leo in the next arena match. If your fighter is victorious, you will receive <em>14250</em> Councils. If your fighter loses, you will receive <em>0</em> Councils.\", \"\", [201,0,], System.String[], 1, Generic)";
    const SUCCESS: &str = "[16:22:04] LocalPlayer: ProcessTalkScreen(5712, \"\", \"Success! You have placed your bet for Otis. Now please find a spot to watch!\\n\\nThe fight begins in <em>4 minutes 45 seconds</em>.\", \"\", [-1,], System.String[], 1, Generic)";
    const PAYOUT: &str =
        "[16:30:01] LocalPlayer: ProcessScreenText(CombatInfo, \"You received 14,250 Councils.\")";

    #[test]
    fn bet_parsing_extracts_all_fields() {
        let bet = parse_confirm_bet(CONFIRM, None).unwrap();
        assert_eq!(bet.pick, "Otis");
        assert_eq!(bet.opponent, "Leo");
        assert_eq!(bet.wager, 7500);
        assert_eq!(bet.payout, 14250);
    }

    #[test]
    fn winning_bet_resolves_on_matching_payout() {
        let mut t = ArenaBetTracker::new();
        assert!(t.observe_line(CONFIRM, None).is_none());
        assert!(t.observe_line(SUCCESS, None).is_none());
        let bet = t.observe_line(PAYOUT, None).unwrap();
        assert!(bet.won);
        assert_eq!(bet.pick, "Otis");
        assert_eq!(bet.payout, 14250);
    }

    #[test]
    fn non_matching_payout_does_not_resolve() {
        // A roulette payout (1,800) must not resolve the arena bet.
        let mut t = ArenaBetTracker::new();
        t.observe_line(CONFIRM, None);
        t.observe_line(SUCCESS, None);
        let roulette = "[16:29:16] LocalPlayer: ProcessScreenText(CombatInfo, \"You received 1,800 Councils.\")";
        assert!(t.observe_line(roulette, None).is_none());
        // The real payout still resolves it as a win.
        assert!(t.observe_line(PAYOUT, None).unwrap().won);
    }

    #[test]
    fn losing_bet_resolves_when_next_bet_placed() {
        let mut t = ArenaBetTracker::new();
        t.observe_line(CONFIRM, None);
        t.observe_line(SUCCESS, None);
        // No payout arrives; the next bet's Confirm resolves the prior as a loss.
        let next_confirm = CONFIRM.replace("Otis defeats Leo", "Corrrak defeats Otis");
        let loss = t.observe_line(&next_confirm, None).unwrap();
        assert!(!loss.won);
        assert_eq!(loss.pick, "Otis");
    }

    #[test]
    fn cancelled_bet_without_success_is_discarded() {
        let mut t = ArenaBetTracker::new();
        t.observe_line(CONFIRM, None); // confirm shown but never placed
                                       // Payout for that amount must NOT count — no active bet.
        assert!(t.observe_line(PAYOUT, None).is_none());
    }

    #[test]
    fn placed_at_is_dated_from_base_date() {
        use chrono::NaiveDate;
        let base = NaiveDate::from_ymd_opt(2026, 7, 20);
        let bet = parse_confirm_bet(CONFIRM, base).unwrap();
        assert_eq!(bet.placed_at, "2026-07-20 16:22:03");
    }
}
