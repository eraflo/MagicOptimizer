//! Goldfishing a deck: drawing opening hands and seeing what happens.
//!
//! "Goldfish" because there is no opponent — nothing interacts, nothing gets countered. That
//! makes the numbers optimistic in absolute terms and useful in relative ones: the point is to
//! compare two versions of the same deck, not to predict a win rate.
//!
//! Everything here is seeded. The same deck and seed give the same numbers, which matters
//! because the optimizer compares scores thousands of times and would otherwise chase noise.

use serde::{Deserialize, Serialize};

use crate::profile::DeckProfile;
use crate::rng::Rng;

/// How many games to play. Ten thousand puts the standard error on a probability at well under
/// a percentage point, which is finer than any decision made from it.
pub const DEFAULT_GAMES: u32 = 10_000;

/// How the mulligan is decided.
///
/// The London mulligan draws a fresh seven and bottoms cards equal to the number of mulligans
/// taken, so a hand's *land count* is what decides keepability — the rest is playable or not
/// on its own terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MulliganRule {
    pub minimum_lands: u32,
    pub maximum_lands: u32,
    /// Hands are never mulliganed below this size; past it you keep whatever you have.
    pub keep_at_or_below: u32,
}

impl MulliganRule {
    /// Sensible defaults for a deck size.
    ///
    /// A 100-card singleton deck runs more lands and draws them less reliably, so its keepable
    /// band sits higher than a 60-card deck's.
    pub fn for_deck_size(deck_size: u32) -> MulliganRule {
        if deck_size >= 80 {
            MulliganRule {
                minimum_lands: 3,
                maximum_lands: 6,
                keep_at_or_below: 5,
            }
        } else {
            MulliganRule {
                minimum_lands: 2,
                maximum_lands: 5,
                keep_at_or_below: 5,
            }
        }
    }

    fn keeps(&self, hand_size: u32, lands: u32) -> bool {
        hand_size <= self.keep_at_or_below
            || (lands >= self.minimum_lands && lands <= self.maximum_lands)
    }
}

/// Settings for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationSettings {
    pub games: u32,
    pub on_the_play: bool,
    /// How many turns to play out.
    pub turns: u32,
    pub mulligan: MulliganRule,
    /// Fixed so a score is reproducible. Vary it only to check a result is not a seed artifact.
    pub seed: u64,
}

impl SimulationSettings {
    pub fn for_deck_size(deck_size: u32) -> SimulationSettings {
        SimulationSettings {
            games: DEFAULT_GAMES,
            on_the_play: true,
            turns: 5,
            mulligan: MulliganRule::for_deck_size(deck_size),
            seed: 0x5EED,
        }
    }
}

/// What a run measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    pub games: u32,
    /// Share of games whose first seven were kept.
    pub keepable_opening_hands: f64,
    /// Mean number of mulligans taken.
    pub average_mulligans: f64,
    /// Mean lands in the hand finally kept.
    pub average_opening_lands: f64,
    /// Share of games with a land available every turn up to and including this one, indexed
    /// from turn 1. Missing a land drop is what actually loses goldfish games.
    pub land_drops_made: Vec<f64>,
    /// Share of games able to cast something at each mana value on the turn it comes up —
    /// the deck's ability to "play on curve".
    pub on_curve_by_turn: Vec<f64>,
    /// Share of games with no land at all in the kept hand.
    pub mana_screw: f64,
    /// Share of games where the kept hand was more than half lands.
    pub mana_flood: f64,
}

/// Plays out the deck and reports what happened.
pub fn simulate(profile: &DeckProfile, settings: SimulationSettings) -> SimulationResult {
    let turns = settings.turns.max(1) as usize;
    let mut rng = Rng::new(settings.seed);

    let mut kept_first_hand = 0u32;
    let mut total_mulligans = 0u64;
    let mut total_opening_lands = 0u64;
    let mut land_drops = vec![0u32; turns];
    let mut on_curve = vec![0u32; turns];
    let mut screwed = 0u32;
    let mut flooded = 0u32;

    // Reused between games so a ten-thousand-game run does not allocate ten thousand decks.
    let mut library: Vec<Card> = profile.cards.clone();

    for _ in 0..settings.games.max(1) {
        let game = play_one(&mut library, &mut rng, &settings, turns);

        if game.mulligans == 0 {
            kept_first_hand += 1;
        }
        total_mulligans += u64::from(game.mulligans);
        total_opening_lands += u64::from(game.opening_lands);
        if game.opening_lands == 0 {
            screwed += 1;
        }
        if game.opening_lands * 2 > game.opening_hand_size {
            flooded += 1;
        }
        for turn in 0..turns {
            if game.made_land_drops[turn] {
                land_drops[turn] += 1;
            }
            if game.on_curve[turn] {
                on_curve[turn] += 1;
            }
        }
    }

    let games = settings.games.max(1);
    let share = |count: u32| f64::from(count) / f64::from(games);

    SimulationResult {
        games,
        keepable_opening_hands: share(kept_first_hand),
        average_mulligans: total_mulligans as f64 / f64::from(games),
        average_opening_lands: total_opening_lands as f64 / f64::from(games),
        land_drops_made: land_drops.iter().copied().map(share).collect(),
        on_curve_by_turn: on_curve.iter().copied().map(share).collect(),
        mana_screw: share(screwed),
        mana_flood: share(flooded),
    }
}

/// One card, reduced to what a goldfish game actually needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub is_land: bool,
    /// Rounded mana value. Lands are 0 and never counted.
    pub mana_value: u32,
}

struct GameOutcome {
    mulligans: u32,
    opening_lands: u32,
    opening_hand_size: u32,
    made_land_drops: Vec<bool>,
    on_curve: Vec<bool>,
}

fn play_one(
    library: &mut [Card],
    rng: &mut Rng,
    settings: &SimulationSettings,
    turns: usize,
) -> GameOutcome {
    const OPENING_HAND: u32 = 7;

    let mut mulligans = 0u32;
    let mut hand_size = OPENING_HAND;
    let mut hand: Vec<Card> = Vec::with_capacity(OPENING_HAND as usize);

    // London: always draw seven, then bottom `mulligans` cards. Bottoming is modelled as
    // discarding the least useful cards, which is what a player does — keeping a hand of seven
    // and putting back the excess lands or the unplayable top end.
    loop {
        rng.shuffle(library);
        hand.clear();
        hand.extend(library.iter().take(OPENING_HAND as usize).copied());

        let lands = hand.iter().filter(|c| c.is_land).count() as u32;
        if settings.mulligan.keeps(hand_size, lands) || hand_size <= 1 {
            break;
        }
        mulligans += 1;
        hand_size = OPENING_HAND.saturating_sub(mulligans).max(1);
    }

    bottom_cards(&mut hand, mulligans, &settings.mulligan);

    let opening_lands = hand.iter().filter(|c| c.is_land).count() as u32;
    let opening_hand_size = hand.len() as u32;

    // Play it out. Drawn cards come off the shuffled library after the opening seven; the
    // cards bottomed above are ignored, which slightly understates the library, and by less
    // than the noise floor at ten thousand games.
    let mut lands_in_play = 0u32;
    let mut hand_lands = opening_lands;
    let mut spells: Vec<u32> = hand
        .iter()
        .filter(|c| !c.is_land)
        .map(|c| c.mana_value)
        .collect();

    let mut made_land_drops = Vec::with_capacity(turns);
    let mut on_curve = Vec::with_capacity(turns);
    let mut still_hitting_drops = true;
    let mut next_draw = OPENING_HAND as usize;

    for turn in 1..=turns as u32 {
        // Draw for the turn, except on turn one when on the play.
        if !(turn == 1 && settings.on_the_play) {
            if let Some(card) = library.get(next_draw) {
                next_draw += 1;
                if card.is_land {
                    hand_lands += 1;
                } else {
                    spells.push(card.mana_value);
                }
            }
        }

        if hand_lands > 0 {
            hand_lands -= 1;
            lands_in_play += 1;
        } else {
            still_hitting_drops = false;
        }
        made_land_drops.push(still_hitting_drops && lands_in_play == turn);

        // "On curve" means something in hand costs exactly this turn's mana and can be cast.
        // Cheaper spells do not count: casting a one-drop on turn four is not being on curve.
        let castable = spells
            .iter()
            .position(|&cost| cost == lands_in_play && cost > 0);
        match castable {
            Some(index) => {
                spells.remove(index);
                on_curve.push(true);
            }
            None => on_curve.push(false),
        }
    }

    GameOutcome {
        mulligans,
        opening_lands,
        opening_hand_size,
        made_land_drops,
        on_curve,
    }
}

/// Puts `count` cards on the bottom, worst first.
///
/// Excess lands go before spells, then the most expensive spells: those are the cards a real
/// player bottoms. Modelling this matters, because a London mulligan that bottomed at random
/// would make mulliganing look far worse than it is.
fn bottom_cards(hand: &mut Vec<Card>, count: u32, rule: &MulliganRule) {
    for _ in 0..count {
        if hand.len() <= 1 {
            return;
        }
        let lands = hand.iter().filter(|c| c.is_land).count() as u32;

        let victim = if lands > rule.maximum_lands {
            hand.iter().position(|c| c.is_land)
        } else if lands > rule.minimum_lands {
            // Spare a land only while there are enough of them; otherwise the top end goes.
            hand.iter()
                .enumerate()
                .filter(|(_, c)| !c.is_land)
                .max_by_key(|(_, c)| c.mana_value)
                .map(|(index, _)| index)
        } else {
            hand.iter()
                .enumerate()
                .filter(|(_, c)| !c.is_land)
                .max_by_key(|(_, c)| c.mana_value)
                .map(|(index, _)| index)
        };

        match victim {
            Some(index) => {
                hand.remove(index);
            }
            None => {
                hand.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn land() -> Card {
        Card {
            is_land: true,
            mana_value: 0,
        }
    }

    fn spell(mana_value: u32) -> Card {
        Card {
            is_land: false,
            mana_value,
        }
    }

    /// A deck of `lands` lands and spells spread evenly over one to four mana.
    fn deck(lands: u32, spells: u32) -> DeckProfile {
        let mut cards = vec![land(); lands as usize];
        for i in 0..spells {
            cards.push(spell(i % 4 + 1));
        }
        DeckProfile {
            cards,
            ..DeckProfile::default()
        }
    }

    fn run(profile: &DeckProfile) -> SimulationResult {
        let size = profile.cards.len() as u32;
        simulate(profile, SimulationSettings::for_deck_size(size))
    }

    #[test]
    fn the_same_seed_gives_the_same_result() {
        // The property the optimizer depends on: without it, a score wobbles and the search
        // follows noise instead of the deck.
        let profile = deck(24, 36);
        let settings = SimulationSettings::for_deck_size(60);
        assert_eq!(simulate(&profile, settings), simulate(&profile, settings));
    }

    #[test]
    fn a_different_seed_gives_a_close_but_different_result() {
        let profile = deck(24, 36);
        let mut a = SimulationSettings::for_deck_size(60);
        let mut b = a;
        b.seed = a.seed + 1;
        a.games = 10_000;
        b.games = 10_000;

        let first = simulate(&profile, a);
        let second = simulate(&profile, b);
        assert_ne!(first, second, "two seeds should not agree exactly");
        // But they should agree to within sampling noise, or the simulation is not measuring
        // the deck at all.
        assert!(
            (first.keepable_opening_hands - second.keepable_opening_hands).abs() < 0.02,
            "{} vs {}",
            first.keepable_opening_hands,
            second.keepable_opening_hands
        );
    }

    #[test]
    fn an_all_land_deck_never_gets_screwed_and_always_floods() {
        let result = run(&deck(60, 0));
        assert_eq!(result.mana_screw, 0.0);
        assert_eq!(result.mana_flood, 1.0);
        assert_eq!(result.land_drops_made[0], 1.0, "always a land for turn one");
    }

    #[test]
    fn a_deck_with_no_lands_never_makes_a_land_drop() {
        let result = run(&deck(0, 60));
        assert_eq!(result.mana_screw, 1.0);
        assert_eq!(result.mana_flood, 0.0);
        for share in &result.land_drops_made {
            assert_eq!(*share, 0.0);
        }
        for share in &result.on_curve_by_turn {
            assert_eq!(*share, 0.0, "nothing is castable with no mana");
        }
    }

    #[test]
    fn more_lands_means_more_land_drops() {
        // The most basic thing the simulation has to get right.
        let lean = run(&deck(18, 42));
        let rich = run(&deck(28, 32));
        assert!(
            rich.land_drops_made[3] > lean.land_drops_made[3],
            "{} vs {}",
            rich.land_drops_made[3],
            lean.land_drops_made[3]
        );
    }

    #[test]
    fn land_drop_probabilities_only_go_down_with_the_turns() {
        // Making every drop through turn four is strictly harder than through turn three.
        let result = run(&deck(24, 36));
        for window in result.land_drops_made.windows(2) {
            assert!(
                window[1] <= window[0] + 1e-12,
                "{:?}",
                result.land_drops_made
            );
        }
    }

    #[test]
    fn a_reasonable_deck_keeps_most_opening_hands() {
        let result = run(&deck(24, 36));
        assert!(
            result.keepable_opening_hands > 0.75,
            "{}",
            result.keepable_opening_hands
        );
        assert!(
            result.average_mulligans < 0.35,
            "{}",
            result.average_mulligans
        );
    }

    #[test]
    fn a_deck_with_a_terrible_land_count_mulligans_far_more() {
        let sane = run(&deck(24, 36));
        let broken = run(&deck(5, 55));
        assert!(
            broken.average_mulligans > sane.average_mulligans * 2.0,
            "{} vs {}",
            broken.average_mulligans,
            sane.average_mulligans
        );
    }

    #[test]
    fn opening_land_counts_are_near_what_the_maths_predicts() {
        // 24 lands in 60 cards is 2.8 per seven-card hand before mulligans; the mulligan rule
        // pulls that up a little by rejecting the extremes.
        let result = run(&deck(24, 36));
        assert!(
            (result.average_opening_lands - 3.0).abs() < 0.5,
            "{}",
            result.average_opening_lands
        );
    }

    #[test]
    fn commander_sized_decks_use_a_higher_keepable_band() {
        let rule = MulliganRule::for_deck_size(100);
        assert_eq!(rule.minimum_lands, 3);
        assert_eq!(MulliganRule::for_deck_size(60).minimum_lands, 2);
    }

    #[test]
    fn a_small_hand_is_always_kept() {
        // Otherwise the mulligan loop would never terminate on a pathological deck.
        let rule = MulliganRule::for_deck_size(60);
        assert!(
            rule.keeps(5, 0),
            "a five-card hand is kept whatever it holds"
        );
        assert!(!rule.keeps(7, 0));
    }

    #[test]
    fn bottoming_prefers_excess_lands_then_the_top_end() {
        let rule = MulliganRule::for_deck_size(60);

        let mut flooded = vec![land(), land(), land(), land(), land(), land(), spell(2)];
        bottom_cards(&mut flooded, 1, &rule);
        assert_eq!(flooded.iter().filter(|c| c.is_land).count(), 5);

        // With lands already scarce, the expensive spell goes instead.
        let mut lean = vec![land(), land(), spell(1), spell(2), spell(7)];
        bottom_cards(&mut lean, 1, &rule);
        assert_eq!(lean.iter().filter(|c| c.is_land).count(), 2);
        assert!(!lean.iter().any(|c| c.mana_value == 7));
    }

    #[test]
    fn bottoming_never_empties_the_hand() {
        let rule = MulliganRule::for_deck_size(60);
        let mut hand = vec![land(), spell(1)];
        bottom_cards(&mut hand, 10, &rule);
        assert_eq!(hand.len(), 1);
    }

    #[test]
    fn an_empty_deck_does_not_hang_or_panic() {
        // Comes straight from an empty deck in the editor.
        let result = run(&DeckProfile::default());
        assert_eq!(result.mana_screw, 1.0);
        assert!(result.land_drops_made.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn playing_second_draws_an_extra_card() {
        let profile = deck(24, 36);
        let mut on_play = SimulationSettings::for_deck_size(60);
        on_play.on_the_play = true;
        let mut on_draw = on_play;
        on_draw.on_the_play = false;

        let played = simulate(&profile, on_play);
        let drawn = simulate(&profile, on_draw);
        assert!(
            drawn.land_drops_made[3] >= played.land_drops_made[3],
            "the extra card cannot make land drops harder: {} vs {}",
            drawn.land_drops_made[3],
            played.land_drops_made[3]
        );
    }
}
