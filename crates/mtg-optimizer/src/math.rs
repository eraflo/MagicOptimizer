//! Hypergeometric probability, and what it says about a mana base.
//!
//! Drawing cards is sampling without replacement, so the hypergeometric distribution is the
//! exact answer to "what are the odds I have drawn my land by turn three". No approximation is
//! needed at deck sizes, and an exact answer is cheap.

/// Probability of drawing **exactly** `wanted` of the `successes` in `draws` cards.
///
/// `population` is the deck size, `successes` how many copies of what you want are in it.
pub fn probability_of_exactly(population: u32, successes: u32, draws: u32, wanted: u32) -> f64 {
    if draws > population || successes > population {
        return 0.0;
    }
    if wanted > successes || wanted > draws {
        return 0.0;
    }
    // The remaining draws have to come from the failures; if there are not enough, it cannot
    // happen. Guarding here keeps ln_choose from being handed nonsense.
    if draws - wanted > population - successes {
        return 0.0;
    }

    // C(K,k) · C(N-K,n-k) / C(N,n), built as one running product with the divisions
    // interleaved. Going through logs and back cost about seven digits of accuracy — enough to
    // show up against independently computed reference values — because every `exp` of a sum
    // of large logs throws away precision. Here the running value stays near 1 and each step
    // costs only its own rounding.
    let failures = population - successes;
    let others = draws - wanted;

    let mut result = 1.0f64;
    for i in 0..wanted {
        result *= f64::from(successes - i) / f64::from(i + 1);
    }
    for i in 0..others {
        result *= f64::from(failures - i) / f64::from(i + 1);
    }
    for i in 0..draws {
        result *= f64::from(i + 1) / f64::from(population - i);
    }
    result
}

/// Probability of drawing **at least** `wanted` of the `successes` in `draws` cards.
///
/// This is the one that answers real questions: "will I have two lands in my opener", "will I
/// have a red source by turn three".
pub fn probability_of_at_least(population: u32, successes: u32, draws: u32, wanted: u32) -> f64 {
    if wanted == 0 {
        return 1.0;
    }
    if wanted > successes || wanted > draws {
        return 0.0;
    }

    // Summed from the tail with fewer terms. The complement (1 - P(fewer)) would lose
    // precision exactly where the answer matters most, near probability 1.
    let highest = successes.min(draws);
    (wanted..=highest)
        .map(|k| probability_of_exactly(population, successes, draws, k))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// How many cards you have seen by the start of turn `turn`.
///
/// Seven for the opening hand, plus one per draw step. On the play you skip the first draw,
/// which is the whole disadvantage of choosing to go first.
pub fn cards_seen_by_turn(turn: u32, on_the_play: bool) -> u32 {
    const OPENING_HAND: u32 = 7;
    let draws = turn.saturating_sub(u32::from(on_the_play));
    OPENING_HAND + draws
}

/// Probability that a cost's coloured requirements are met on time.
///
/// `sources` is how many cards in the deck produce the colour, `pips` how many symbols of it
/// the spell asks for.
pub fn probability_castable_on_curve(
    deck_size: u32,
    sources: u32,
    pips: u32,
    turn: u32,
    on_the_play: bool,
) -> f64 {
    if pips == 0 {
        return 1.0;
    }
    probability_of_at_least(
        deck_size,
        sources,
        cards_seen_by_turn(turn, on_the_play),
        pips,
    )
}

/// Smallest number of sources reaching `confidence` for a cost on curve.
///
/// # This is not Frank Karsten's published number
///
/// Karsten's widely-cited tables — 14 sources for a single pip on turn one in a 60-card deck —
/// are computed **conditional on hands you keep**, so mulliganing away unkeepable openers is
/// folded in. This function is unconditional over all opening hands, and therefore asks for a
/// few more sources: 16 for that same case, converging with his numbers by turn four.
///
/// Scoring deliberately uses [`probability_castable_on_curve`] instead of this. A probability
/// is a smooth signal an optimizer can follow; a threshold is a cliff, and it would also be
/// asserting a number this code cannot derive.
pub fn sources_for_confidence(
    deck_size: u32,
    pips: u32,
    turn: u32,
    on_the_play: bool,
    confidence: f64,
) -> Option<u32> {
    // Compared with a tolerance: summing the tail of a distribution that is mathematically
    // exactly 1 lands a bit under it in floating point, so an honest request for certainty
    // would otherwise never be satisfied.
    (pips..=deck_size).find(|&sources| {
        probability_castable_on_curve(deck_size, sources, pips, turn, on_the_play)
            >= confidence - 1e-12
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values computed independently with exact rational arithmetic, to catch a wrong formula
    /// rather than a wrong intuition.
    ///
    /// They are quoted to full `f64` precision on purpose. An earlier version of this test
    /// carried six real decimals padded out by hand, which made a correct implementation look
    /// broken and sent the search for the bug in entirely the wrong direction.
    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn exact_probabilities_match_reference_values() {
        // Three lands in an opening seven from a 24-land deck.
        assert_close(
            probability_of_exactly(60, 24, 7, 3),
            0.308_704_256_257_241_57,
        );
        // One ace in a five-card poker hand — the textbook case, from a different domain.
        assert_close(probability_of_exactly(52, 4, 5, 1), 0.299_473_635_608_089_4);
        assert_close(probability_of_exactly(10, 5, 3, 2), 0.416_666_666_666_666_7);
    }

    #[test]
    fn at_least_matches_reference_values() {
        assert_close(
            probability_of_at_least(60, 24, 7, 3),
            0.587_929_496_447_137_8,
        );
        assert_close(
            probability_of_at_least(99, 38, 7, 3),
            0.548_030_429_228_732_5,
        );
        assert_close(
            probability_of_at_least(40, 17, 7, 2),
            0.894_802_494_802_494_9,
        );
        assert_close(
            probability_of_at_least(60, 4, 7, 1),
            0.399_499_625_744_665_6,
        );
    }

    #[test]
    fn a_full_distribution_sums_to_one() {
        for (population, successes, draws) in [(60, 24, 7), (99, 38, 7), (40, 17, 9)] {
            let total: f64 = (0..=successes.min(draws))
                .map(|k| probability_of_exactly(population, successes, draws, k))
                .sum();
            assert_close(total, 1.0);
        }
    }

    #[test]
    fn at_least_zero_is_certain_and_at_least_everything_is_not() {
        assert_close(probability_of_at_least(60, 24, 7, 0), 1.0);
        // More copies wanted than exist.
        assert_eq!(probability_of_at_least(60, 3, 7, 4), 0.0);
        // More copies wanted than cards drawn.
        assert_eq!(probability_of_at_least(60, 24, 3, 4), 0.0);
    }

    #[test]
    fn drawing_the_whole_deck_finds_everything() {
        assert_close(probability_of_at_least(60, 24, 60, 24), 1.0);
        assert_close(probability_of_exactly(60, 24, 60, 24), 1.0);
    }

    #[test]
    fn impossible_inputs_are_zero_rather_than_a_panic() {
        // These come from user decks, so they have to be handled rather than asserted away.
        assert_eq!(
            probability_of_exactly(10, 20, 5, 1),
            0.0,
            "more successes than cards"
        );
        assert_eq!(
            probability_of_exactly(10, 5, 20, 1),
            0.0,
            "more draws than cards"
        );
        assert_eq!(
            probability_of_exactly(60, 24, 7, 8),
            0.0,
            "more wanted than drawn"
        );
    }

    #[test]
    fn more_sources_never_lowers_the_odds() {
        let mut previous = 0.0;
        for sources in 0..=40 {
            let probability = probability_castable_on_curve(60, sources, 1, 3, true);
            assert!(probability >= previous - 1e-12, "{sources} sources");
            previous = probability;
        }
        // Not exactly 1: 40 sources in 60 cards still leaves a one-in-a-hundred-thousand
        // opener with none of them. Certainty needs the whole deck.
        assert!(previous > 0.9999 && previous < 1.0, "{previous}");
        assert_close(probability_castable_on_curve(60, 60, 1, 3, true), 1.0);
    }

    #[test]
    fn going_second_sees_one_more_card() {
        assert_eq!(cards_seen_by_turn(1, true), 7);
        assert_eq!(cards_seen_by_turn(1, false), 8);
        assert_eq!(cards_seen_by_turn(4, true), 10);
        assert_eq!(cards_seen_by_turn(4, false), 11);
    }

    #[test]
    fn a_colourless_cost_is_always_castable() {
        assert_close(probability_castable_on_curve(60, 0, 0, 1, true), 1.0);
    }

    #[test]
    fn double_pips_need_far_more_sources_than_one() {
        // The reason pips are counted per symbol rather than per card.
        let single = probability_castable_on_curve(60, 16, 1, 2, true);
        let double = probability_castable_on_curve(60, 16, 2, 2, true);
        assert!(single > 0.9, "{single}");
        assert!(double < 0.7, "{double}");
    }

    #[test]
    fn source_counts_are_the_unconditional_ones_not_karstens() {
        // Pinned so the difference stays visible: Karsten publishes 14 for this case because
        // his model conditions on kept hands. Ours does not, and says 16.
        assert_eq!(sources_for_confidence(60, 1, 1, true, 0.90), Some(16));
        assert_eq!(sources_for_confidence(60, 1, 2, true, 0.90), Some(15));
        assert_eq!(sources_for_confidence(60, 1, 4, true, 0.90), Some(12));
        // Double pips, where the gap to his tables is wider still.
        assert_eq!(sources_for_confidence(60, 2, 3, true, 0.90), Some(22));
    }

    #[test]
    fn an_unreachable_confidence_returns_nothing() {
        // Eight pips cannot be met from a seven-card opener, whatever the mana base.
        assert_eq!(sources_for_confidence(60, 8, 1, true, 0.9), None);
    }

    #[test]
    fn enough_sources_does_make_it_certain() {
        // Worth pinning, because it is where an earlier version of the test above was wrong:
        // with only four non-sources left, a seven-card hand cannot miss three of them.
        assert_eq!(sources_for_confidence(60, 3, 1, true, 1.0), Some(56));
    }
}
