//! Win rates that do not lie about how little they know.
//!
//! # The whole problem in one line
//!
//! Three wins out of three is not a 100% win rate, and a tool that prints "100%" there is
//! actively misleading the person reading it. Magic is high variance, samples are tiny — a
//! Commander night is four games — and the honest answer to "how good is this deck" after an
//! evening is *we can barely tell*.
//!
//! Two things are reported side by side, and neither replaces the other:
//!
//! * the **observed** rate, which is what happened, and
//! * an **adjusted** rate pulled towards even, which is what it is reasonable to believe.
//!
//! The gap between them shrinks as games accumulate, which is exactly the "cautious adjustment
//! that strengthens slowly" the user guide promises. It is also why the recommendation side of
//! this cannot arrive quickly, whatever anyone would prefer.

use serde::{Deserialize, Serialize};

use crate::game::{Game, Result_};

/// How many notional even games the adjustment starts from.
///
/// A Beta(3, 3) prior: it takes about six real games to move the adjusted rate halfway to the
/// observed one, and a few dozen before the two nearly agree. Chosen so a single good evening
/// does not read as a discovery, which is the failure this exists to prevent.
///
/// **This is a judgement, not a calculation.** It is written here to be argued with.
const PRIOR_GAMES: f64 = 6.0;

/// Confidence for the reported interval: the usual 95%, so `1.96` standard deviations.
const Z: f64 = 1.96;

/// A win rate, with everything needed to read it honestly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WinRate {
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    /// Wins over decided games. What happened, with no interpretation.
    pub observed: f64,
    /// Pulled towards even by a prior worth [`PRIOR_GAMES`] even games. What it is reasonable
    /// to believe, given how few games this is.
    pub adjusted: f64,
    /// A 95% Wilson interval on the observed rate. Wide is the point: at three games it spans
    /// most of the range, and that is the true state of knowledge.
    pub low: f64,
    pub high: f64,
}

impl WinRate {
    /// Draws are excluded from the denominator: a draw settles nothing about which deck is
    /// better, and counting it as half a loss would be an invention.
    pub fn from_counts(wins: u32, losses: u32, draws: u32) -> WinRate {
        let decided = wins + losses;
        let n = f64::from(decided);
        let w = f64::from(wins);

        let observed = if decided == 0 { 0.5 } else { w / n };
        let adjusted = (w + PRIOR_GAMES / 2.0) / (n + PRIOR_GAMES);
        let (low, high) = wilson_interval(w, n);

        WinRate {
            games: wins + losses + draws,
            wins,
            losses,
            draws,
            observed,
            adjusted,
            low,
            high,
        }
    }

    pub fn of(games: impl IntoIterator<Item = Result_>) -> WinRate {
        let (mut wins, mut losses, mut draws) = (0, 0, 0);
        for result in games {
            match result {
                Result_::Win => wins += 1,
                Result_::Loss => losses += 1,
                Result_::Draw => draws += 1,
            }
        }
        WinRate::from_counts(wins, losses, draws)
    }

    /// Whether the interval excludes an even record.
    ///
    /// The one question a win rate is usually being asked — "is this deck actually good?" — and
    /// the only honest way to answer it from a handful of games is usually "not yet".
    pub fn is_distinguishable_from_even(&self) -> bool {
        self.low > 0.5 || self.high < 0.5
    }
}

/// The Wilson score interval.
///
/// Chosen over the textbook normal interval because that one is wrong exactly where this data
/// lives: at three games and three wins it gives `1.0 ± 0.0`, claiming certainty from nothing.
/// Wilson stays inside `0..=1` and stays wide when the sample is small.
fn wilson_interval(wins: f64, n: f64) -> (f64, f64) {
    if n <= 0.0 {
        return (0.0, 1.0);
    }
    let p = wins / n;
    let z2 = Z * Z;
    let denominator = 1.0 + z2 / n;
    let centre = p + z2 / (2.0 * n);
    let spread = Z * ((p * (1.0 - p) / n) + z2 / (4.0 * n * n)).sqrt();

    (
        ((centre - spread) / denominator).clamp(0.0, 1.0),
        ((centre + spread) / denominator).clamp(0.0, 1.0),
    )
}

/// A win rate against one kind of opponent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Matchup {
    pub archetype: String,
    pub rate: WinRate,
}

/// Win rates by opponent archetype, worst first.
///
/// Worst first because the useful question is "what am I losing to", and a list ordered by
/// success would bury it. Sorted on the **adjusted** rate, so one bad night against a deck you
/// have played once does not top the list.
pub fn matchups<'a>(games: impl IntoIterator<Item = &'a Game>) -> Vec<Matchup> {
    let mut by_archetype: std::collections::BTreeMap<&str, (u32, u32, u32)> = Default::default();

    for game in games {
        for opponent in &game.opponents {
            let label = opponent.archetype.trim();
            if label.is_empty() {
                continue;
            }
            let entry = by_archetype.entry(label).or_default();
            match game.result {
                Result_::Win => entry.0 += 1,
                Result_::Loss => entry.1 += 1,
                Result_::Draw => entry.2 += 1,
            }
        }
    }

    let mut matchups: Vec<Matchup> = by_archetype
        .into_iter()
        .map(|(archetype, (wins, losses, draws))| Matchup {
            archetype: archetype.to_owned(),
            rate: WinRate::from_counts(wins, losses, draws),
        })
        .collect();

    matchups.sort_by(|a, b| {
        a.rate
            .adjusted
            .partial_cmp(&b.rate.adjusted)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.archetype.cmp(&b.archetype))
    });
    matchups
}

/// What changed after a given date.
///
/// The user guide calls this the most telling number the log produces, and it is: people change
/// decks constantly and almost never check whether it helped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeAfter {
    pub since: String,
    pub before: WinRate,
    pub after: WinRate,
    /// Difference in **adjusted** rates. Positive means the change looks like an improvement.
    pub shift: f64,
    /// Whether the two intervals fail to overlap.
    ///
    /// Almost always false, and it should be. Telling someone their five-card swap worked on the
    /// strength of seven games would be exactly the false confidence this module exists to
    /// avoid.
    pub conclusive: bool,
}

/// Splits a deck's games either side of a date, inclusive of the date itself on the "after".
pub fn before_and_after<'a>(games: impl IntoIterator<Item = &'a Game>, since: &str) -> BeforeAfter {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    for game in games {
        if game.played_at.as_str() < since {
            before.push(game.result);
        } else {
            after.push(game.result);
        }
    }

    let before = WinRate::of(before);
    let after = WinRate::of(after);

    BeforeAfter {
        since: since.to_owned(),
        shift: after.adjusted - before.adjusted,
        conclusive: before.high < after.low || after.high < before.low,
        before,
        after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{GameId, NewGame};

    fn game(deck: u64, date: &str, result: Result_, against: &[&str]) -> Game {
        let mut new = NewGame::new(deck, date, result);
        for archetype in against {
            new = new.against(*archetype);
        }
        new.with_id(GameId(0))
    }

    #[test]
    fn three_wins_out_of_three_is_not_a_hundred_percent() {
        // The failure this whole module exists to prevent.
        let rate = WinRate::from_counts(3, 0, 0);
        assert_eq!(
            rate.observed, 1.0,
            "what happened is still reported plainly"
        );
        assert!(
            rate.adjusted < 0.75,
            "but belief stays cautious: {}",
            rate.adjusted
        );
        assert!(
            rate.low < 0.5,
            "and the interval admits it knows nothing: {}",
            rate.low
        );
        assert!(!rate.is_distinguishable_from_even());
    }

    #[test]
    fn the_adjustment_fades_as_games_accumulate() {
        // "A cautious adjustment that strengthens slowly", which is what the user guide
        // promises and what makes the recommendation side a long game.
        let few = WinRate::from_counts(6, 2, 0);
        let many = WinRate::from_counts(60, 20, 0);
        assert!(
            (many.adjusted - many.observed).abs() < (few.adjusted - few.observed).abs(),
            "eighty games should be trusted more than eight"
        );
        assert!((many.adjusted - 0.75).abs() < 0.02);
    }

    #[test]
    fn a_real_edge_over_many_games_is_eventually_called() {
        let rate = WinRate::from_counts(70, 30, 0);
        assert!(rate.is_distinguishable_from_even(), "{rate:?}");
    }

    #[test]
    fn draws_settle_nothing_and_are_kept_out_of_the_denominator() {
        // Counting a draw as half a loss would be inventing a result nobody achieved.
        let with_draws = WinRate::from_counts(5, 5, 4);
        let without = WinRate::from_counts(5, 5, 0);
        assert_eq!(with_draws.observed, without.observed);
        assert_eq!(
            with_draws.games, 14,
            "they are still counted as games played"
        );
        assert_eq!(with_draws.draws, 4);
    }

    #[test]
    fn a_deck_that_has_never_been_played_reads_as_even_not_as_zero() {
        // Zero wins of zero games is not a 0% win rate, and showing one would be a lie about a
        // deck nobody has tried.
        let rate = WinRate::from_counts(0, 0, 0);
        assert_eq!(rate.observed, 0.5);
        assert_eq!(rate.adjusted, 0.5);
        assert_eq!((rate.low, rate.high), (0.0, 1.0), "it knows nothing at all");
    }

    #[test]
    fn the_interval_never_escapes_zero_to_one() {
        // The normal approximation does, which is why it is not used here.
        for (wins, losses) in [(0, 1), (1, 0), (0, 20), (20, 0), (1, 1)] {
            let rate = WinRate::from_counts(wins, losses, 0);
            assert!(rate.low >= 0.0 && rate.high <= 1.0, "{rate:?}");
            assert!(rate.low <= rate.high, "{rate:?}");
        }
    }

    #[test]
    fn matchups_put_the_worst_first() {
        // The useful question is what you are losing to; ordering by success would bury it.
        let games = vec![
            game(1, "2026-01-01", Result_::Loss, &["Atraxa"]),
            game(1, "2026-01-02", Result_::Loss, &["Atraxa"]),
            game(1, "2026-01-03", Result_::Loss, &["Atraxa"]),
            game(1, "2026-01-04", Result_::Win, &["Burn"]),
            game(1, "2026-01-05", Result_::Win, &["Burn"]),
        ];
        let table = matchups(&games);
        assert_eq!(table[0].archetype, "Atraxa");
        assert_eq!(table[0].rate.losses, 3);
        assert_eq!(table[1].archetype, "Burn");
    }

    #[test]
    fn a_multiplayer_game_counts_against_every_opponent_at_the_table() {
        // Commander is four players. A win is a win against all three, and recording it against
        // only one would quietly halve every matchup number.
        let games = vec![game(
            1,
            "2026-01-01",
            Result_::Win,
            &["Atraxa", "Krenko", "Edgar"],
        )];
        let table = matchups(&games);
        assert_eq!(table.len(), 3);
        assert!(table.iter().all(|m| m.rate.wins == 1));
    }

    #[test]
    fn an_unnamed_opponent_is_skipped_rather_than_grouped_under_nothing() {
        let games = vec![
            game(1, "2026-01-01", Result_::Win, &["  "]),
            game(1, "2026-01-02", Result_::Win, &["Burn"]),
        ];
        let table = matchups(&games);
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].archetype, "Burn");
    }

    #[test]
    fn a_change_is_measured_from_the_date_it_was_made() {
        let games = vec![
            game(1, "2026-01-01", Result_::Loss, &[]),
            game(1, "2026-01-02", Result_::Loss, &[]),
            game(1, "2026-06-01", Result_::Win, &[]),
            game(1, "2026-06-02", Result_::Win, &[]),
        ];
        let split = before_and_after(&games, "2026-06-01");
        assert_eq!(split.before.losses, 2);
        assert_eq!(split.after.wins, 2, "the date itself belongs to the change");
        assert!(split.shift > 0.0);
    }

    #[test]
    fn seven_games_never_prove_a_deck_change_worked() {
        // The exact false confidence the user guide warns about. If this ever starts returning
        // true on a handful of games, the statistics have been broken.
        let games = vec![
            game(1, "2026-01-01", Result_::Loss, &[]),
            game(1, "2026-01-02", Result_::Loss, &[]),
            game(1, "2026-01-03", Result_::Loss, &[]),
            game(1, "2026-06-01", Result_::Win, &[]),
            game(1, "2026-06-02", Result_::Win, &[]),
            game(1, "2026-06-03", Result_::Win, &[]),
            game(1, "2026-06-04", Result_::Win, &[]),
        ];
        let split = before_and_after(&games, "2026-06-01");
        assert!(split.shift > 0.0, "it may still point the right way");
        assert!(
            !split.conclusive,
            "but it must not claim to have proved anything"
        );
    }

    #[test]
    fn a_large_and_real_improvement_is_eventually_called_conclusive() {
        // The other half of the contract: cautious is not the same as useless.
        let mut games = Vec::new();
        for day in 1..=40 {
            games.push(game(1, &format!("2026-01-{day:02}"), Result_::Loss, &[]));
        }
        for day in 1..=40 {
            games.push(game(1, &format!("2026-06-{day:02}"), Result_::Win, &[]));
        }
        let split = before_and_after(&games, "2026-06-01");
        assert!(split.conclusive, "{split:?}");
    }
}
