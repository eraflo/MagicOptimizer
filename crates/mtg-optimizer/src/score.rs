//! Scoring a deck.
//!
//! The score is a weighted average of **named** criteria, each in `0..1`, and every one of them
//! is returned alongside the total with a sentence saying what it measured. A single number
//! cannot be acted on; "your mana base makes double blue on turn two 61% of the time" can.
//!
//! Two of the criteria are derived — probabilities computed from the deck itself. The curve
//! criterion is not: comparing a curve to an archetype means having an opinion about what the
//! archetype wants, and that opinion is written down in [`Archetype`] where it can be argued
//! with rather than buried in a formula.

use serde::{Deserialize, Serialize};

use crate::math::probability_castable_on_curve;
use crate::profile::DeckProfile;
use crate::simulate::{simulate, SimulationResult, SimulationSettings};

/// Number of curve buckets, mana value 0 through 7-or-more.
const CURVE_BUCKETS: usize = 8;

/// What a deck is trying to do, and therefore what curve suits it.
///
/// **These targets are conventional, not derived.** They are the shapes deckbuilders generally
/// aim for, written here so they can be inspected and changed. Nothing computes them, and a
/// deck that deviates is not thereby wrong — which is why the curve criterion carries less
/// weight than the two that are actually calculated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Archetype {
    Aggro,
    Midrange,
    Control,
}

impl Archetype {
    pub const ALL: [Archetype; 3] = [Archetype::Aggro, Archetype::Midrange, Archetype::Control];

    pub const fn label(self) -> &'static str {
        match self {
            Archetype::Aggro => "Aggro",
            Archetype::Midrange => "Midrange",
            Archetype::Control => "Control",
        }
    }

    /// Share of non-land cards wanted at each mana value, 0 through 7-or-more.
    fn target_curve(self) -> [f64; CURVE_BUCKETS] {
        match self {
            Archetype::Aggro => [0.02, 0.28, 0.30, 0.22, 0.12, 0.04, 0.01, 0.01],
            Archetype::Midrange => [0.02, 0.14, 0.24, 0.24, 0.18, 0.10, 0.05, 0.03],
            Archetype::Control => [0.02, 0.10, 0.18, 0.20, 0.20, 0.14, 0.09, 0.07],
        }
    }
}

/// How much each criterion counts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Weights {
    pub mana_base: f64,
    pub land_drops: f64,
    pub opening_hands: f64,
    pub curve: f64,
}

impl Default for Weights {
    /// The calculated criteria outweigh the opinionated one, deliberately.
    fn default() -> Weights {
        Weights {
            mana_base: 1.0,
            land_drops: 1.0,
            opening_hands: 0.7,
            curve: 0.6,
        }
    }
}

/// One named component of a score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub name: String,
    /// Always `0..=1`, so criteria are comparable to each other.
    pub score: f64,
    pub weight: f64,
    /// What was measured, in a sentence. This is what the UI shows.
    pub detail: String,
    /// False for the criteria that encode a judgement rather than a calculation.
    pub derived: bool,
}

/// A deck's score, with its reasoning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// Weighted average of the criteria, scaled to 0–100.
    pub total: f64,
    pub criteria: Vec<Criterion>,
    pub simulation: SimulationResult,
    /// False when the deck holds cards the catalog could not resolve, so the numbers describe
    /// only part of it.
    pub reliable: bool,
    pub unresolved_cards: u32,
}

/// Everything scoring needs to know beyond the deck itself.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScoreSettings {
    pub archetype: Archetype,
    pub weights: Weights,
    pub simulation: SimulationSettings,
}

impl ScoreSettings {
    pub fn for_deck_size(deck_size: u32) -> ScoreSettings {
        ScoreSettings {
            archetype: Archetype::Midrange,
            weights: Weights::default(),
            simulation: SimulationSettings::for_deck_size(deck_size),
        }
    }
}

/// Scores a deck.
pub fn score(profile: &DeckProfile, settings: ScoreSettings) -> Score {
    let simulation = simulate(profile, settings.simulation);
    score_with_simulation(profile, settings, simulation)
}

/// Scores a deck against a simulation that has already been run.
///
/// The search loop uses this: simulating is by far the most expensive part, and swapping one
/// card does not always need a fresh ten thousand games.
pub fn score_with_simulation(
    profile: &DeckProfile,
    settings: ScoreSettings,
    simulation: SimulationResult,
) -> Score {
    let criteria = vec![
        mana_base_criterion(profile, settings.weights.mana_base),
        land_drops_criterion(&simulation, settings.weights.land_drops),
        opening_hands_criterion(&simulation, settings.weights.opening_hands),
        curve_criterion(profile, settings.archetype, settings.weights.curve),
    ];

    let total_weight: f64 = criteria.iter().map(|c| c.weight).sum();
    let total = if total_weight <= 0.0 {
        0.0
    } else {
        criteria.iter().map(|c| c.score * c.weight).sum::<f64>() / total_weight * 100.0
    };

    Score {
        total,
        criteria,
        simulation,
        reliable: profile.unresolved == 0,
        unresolved_cards: profile.unresolved,
    }
}

/// How reliably the deck's coloured costs can be paid on time.
///
/// Every requirement is weighted by the copies that ask for it, so four Counterspells matter
/// four times as much as one. This is the criterion with the most calculation behind it and
/// the least opinion.
fn mana_base_criterion(profile: &DeckProfile, weight: f64) -> Criterion {
    let deck_size = profile.deck_size();

    if profile.pip_requirements.is_empty() {
        return Criterion {
            name: "Mana base".to_owned(),
            // Nothing to fail at. A colourless deck's mana base cannot be wrong about colour.
            score: 1.0,
            weight,
            detail: "no coloured costs to pay".to_owned(),
            derived: true,
        };
    }

    let mut weighted_total = 0.0;
    let mut total_copies = 0.0;
    let mut worst: Option<(f64, String)> = None;

    for requirement in &profile.pip_requirements {
        let sources = profile.sources_of(requirement.color);
        let probability = probability_castable_on_curve(
            deck_size,
            sources,
            requirement.pips,
            requirement.turn,
            true,
        );
        let copies = f64::from(requirement.copies);
        weighted_total += probability * copies;
        total_copies += copies;

        if worst.as_ref().is_none_or(|(p, _)| probability < *p) {
            let symbols: String =
                std::iter::repeat_n(requirement.color.symbol(), requirement.pips as usize)
                    .collect();
            worst = Some((
                probability,
                format!(
                    "{{{symbols}}} on turn {} is met {:.0}% of the time with {sources} sources",
                    requirement.turn,
                    probability * 100.0
                ),
            ));
        }
    }

    // Blended with the worst single requirement rather than left as a plain average. A real
    // Burn list with four Boros Charms and no white sources scored 85% on the mean alone:
    // most of the deck was fine, so the four cards it could not cast at all averaged away.
    // The mean still dominates, because one awkward card should not condemn a deck.
    let mean = if total_copies == 0.0 {
        1.0
    } else {
        weighted_total / total_copies
    };
    let worst_probability = worst.as_ref().map_or(1.0, |(p, _)| *p);
    let score = 0.75 * mean + 0.25 * worst_probability;

    Criterion {
        name: "Mana base".to_owned(),
        score,
        weight,
        detail: worst
            .map(|(_, detail)| detail)
            .unwrap_or_else(|| "no coloured costs to pay".to_owned()),
        derived: true,
    }
}

/// Whether the deck reliably makes its land drops.
///
/// Missing a land drop is what actually loses goldfish games, so this is scored on the run of
/// drops through turn four rather than on a land count. A deck can have the conventional
/// twenty-four lands and still stumble.
fn land_drops_criterion(simulation: &SimulationResult, weight: f64) -> Criterion {
    let through_turn = simulation.land_drops_made.len().min(4);
    let score = simulation
        .land_drops_made
        .get(through_turn.saturating_sub(1))
        .copied()
        .unwrap_or(0.0);

    Criterion {
        name: "Land drops".to_owned(),
        score,
        weight,
        detail: format!(
            "every land drop through turn {through_turn} in {:.0}% of games",
            score * 100.0
        ),
        derived: true,
    }
}

/// How often the opening seven is worth keeping.
fn opening_hands_criterion(simulation: &SimulationResult, weight: f64) -> Criterion {
    Criterion {
        name: "Opening hands".to_owned(),
        score: simulation.keepable_opening_hands,
        weight,
        detail: format!(
            "{:.0}% of opening sevens are keepable, {:.2} mulligans on average",
            simulation.keepable_opening_hands * 100.0,
            simulation.average_mulligans
        ),
        derived: true,
    }
}

/// How closely the curve matches the chosen archetype.
///
/// Scored by total variation distance, which is 0 for an exact match and 1 for no overlap at
/// all. Marked as not derived: the target is a convention, not a calculation.
fn curve_criterion(profile: &DeckProfile, archetype: Archetype, weight: f64) -> Criterion {
    let mut actual = [0.0f64; CURVE_BUCKETS];
    let mut spells = 0.0;
    for card in profile.cards.iter().filter(|c| !c.is_land) {
        let bucket = (card.mana_value as usize).min(CURVE_BUCKETS - 1);
        actual[bucket] += 1.0;
        spells += 1.0;
    }

    if spells == 0.0 {
        return Criterion {
            name: "Curve".to_owned(),
            score: 0.0,
            weight,
            detail: "the deck has no spells".to_owned(),
            derived: false,
        };
    }

    let target = archetype.target_curve();
    let distance: f64 = actual
        .iter()
        .zip(target.iter())
        .map(|(count, share)| (count / spells - share).abs())
        .sum::<f64>()
        / 2.0;

    // The bucket furthest from the target, which is the actionable part.
    let (bucket, gap) = actual
        .iter()
        .zip(target.iter())
        .enumerate()
        .map(|(index, (count, share))| (index, count / spells - share))
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .unwrap_or((0, 0.0));

    let direction = if gap > 0.0 { "too many" } else { "too few" };
    let label = if bucket == CURVE_BUCKETS - 1 {
        format!("{bucket}+ drops")
    } else {
        format!("{bucket}-drops")
    };

    Criterion {
        name: "Curve".to_owned(),
        score: (1.0 - distance).clamp(0.0, 1.0),
        weight,
        detail: format!(
            "{direction} {label} for {}: {:.0}% against a target of {:.0}%",
            archetype.label(),
            actual[bucket] / spells * 100.0,
            target[bucket] * 100.0
        ),
        derived: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::PipRequirement;
    use crate::simulate::Card;
    use mtg_core::{Color, ColorSet};

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

    /// A 60-card deck with `lands` lands, all producing blue, and the given spells.
    fn deck(lands: u32, spells: &[(u32, u32)], requirements: Vec<PipRequirement>) -> DeckProfile {
        let mut cards = vec![land(); lands as usize];
        for (mana_value, count) in spells {
            for _ in 0..*count {
                cards.push(spell(*mana_value));
            }
        }
        let mut sources = [0u32; 5];
        sources[1] = lands; // blue
        DeckProfile {
            cards,
            color_sources: sources,
            mana_producers: lands,
            lands,
            creatures: 0,
            pip_requirements: requirements,
            color_identity: ColorSet::from_symbols("U"),
            unresolved: 0,
        }
    }

    fn blue(pips: u32, turn: u32, copies: u32) -> PipRequirement {
        PipRequirement {
            color: Color::Blue,
            pips,
            turn,
            copies,
        }
    }

    fn settings() -> ScoreSettings {
        ScoreSettings::for_deck_size(60)
    }

    fn criterion<'a>(score: &'a Score, name: &str) -> &'a Criterion {
        score
            .criteria
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no criterion named {name}"))
    }

    #[test]
    fn a_total_is_a_weighted_average_scaled_to_a_hundred() {
        let profile = deck(24, &[(2, 20), (3, 16)], vec![blue(1, 2, 20)]);
        let result = score(&profile, settings());

        let total_weight: f64 = result.criteria.iter().map(|c| c.weight).sum();
        let expected: f64 = result
            .criteria
            .iter()
            .map(|c| c.score * c.weight)
            .sum::<f64>()
            / total_weight
            * 100.0;

        assert!((result.total - expected).abs() < 1e-9);
        assert!((0.0..=100.0).contains(&result.total), "{}", result.total);
    }

    #[test]
    fn every_criterion_stays_inside_zero_and_one() {
        // So they remain comparable and the weighting means what it says.
        for profile in [
            deck(24, &[(2, 36)], vec![blue(2, 2, 36)]),
            deck(0, &[(1, 60)], vec![blue(3, 1, 60)]),
            deck(60, &[], vec![]),
        ] {
            for c in score(&profile, settings()).criteria {
                assert!((0.0..=1.0).contains(&c.score), "{}: {}", c.name, c.score);
            }
        }
    }

    #[test]
    fn a_better_mana_base_scores_higher() {
        let requirements = vec![blue(2, 2, 20)];
        let thin = deck(12, &[(2, 20)], requirements.clone());
        let solid = deck(26, &[(2, 20)], requirements);

        let solid = score(&solid, settings());
        let thin = score(&thin, settings());
        assert!(criterion(&solid, "Mana base").score > criterion(&thin, "Mana base").score);
    }

    #[test]
    fn one_uncastable_card_drags_the_mana_base_score_down() {
        // Twenty cards that are fine and four that cannot be cast at all. A plain average
        // would call that 85%; it is not 85%.
        let mut profile = deck(24, &[(2, 24)], vec![blue(1, 2, 20)]);
        profile.pip_requirements.push(PipRequirement {
            color: Color::White,
            pips: 1,
            turn: 2,
            copies: 4,
        });

        let result = score(&profile, settings());
        let mana = criterion(&result, "Mana base");
        assert!(mana.score < 0.75, "{}", mana.score);
        // And the sentence names the card that cannot be cast, not the average.
        assert!(mana.detail.contains("0 sources"), "{}", mana.detail);
    }

    #[test]
    fn double_pips_score_worse_than_single_ones() {
        // The whole reason pips are counted per symbol.
        let single = deck(20, &[(2, 20)], vec![blue(1, 2, 20)]);
        let double = deck(20, &[(2, 20)], vec![blue(2, 2, 20)]);
        let single = score(&single, settings());
        let double = score(&double, settings());
        assert!(criterion(&single, "Mana base").score > criterion(&double, "Mana base").score);
    }

    #[test]
    fn a_colourless_deck_cannot_fail_its_mana_base() {
        let profile = deck(24, &[(2, 36)], vec![]);
        let result = score(&profile, settings());
        let mana = criterion(&result, "Mana base");
        assert_eq!(mana.score, 1.0);
        assert!(mana.detail.contains("no coloured costs"));
    }

    #[test]
    fn the_mana_base_detail_names_the_worst_requirement() {
        // Not the average: the average is not actionable, the worst one is.
        let profile = deck(
            20,
            &[(1, 10), (4, 10)],
            vec![blue(1, 4, 10), blue(3, 1, 10)],
        );
        let result = score(&profile, settings());
        let detail = &criterion(&result, "Mana base").detail;
        assert!(detail.contains("turn 1"), "{detail}");
        assert!(detail.contains("UUU"), "{detail}");
    }

    #[test]
    fn a_deck_with_no_lands_bottoms_out_on_land_drops() {
        let profile = deck(0, &[(1, 60)], vec![]);
        let result = score(&profile, settings());
        assert_eq!(criterion(&result, "Land drops").score, 0.0);
    }

    #[test]
    fn a_curve_matching_its_archetype_scores_near_one() {
        // Built to the midrange target, so the distance should be small.
        let profile = deck(
            24,
            &[(1, 5), (2, 9), (3, 9), (4, 6), (5, 4), (6, 2), (7, 1)],
            vec![],
        );
        let mut config = settings();
        config.archetype = Archetype::Midrange;
        let result = score(&profile, config);
        let curve = criterion(&result, "Curve");
        assert!(curve.score > 0.9, "{}", curve.detail);
    }

    #[test]
    fn the_same_curve_suits_one_archetype_and_not_another() {
        // A pile of one- and two-drops is an aggro curve, whatever else it is.
        let profile = deck(22, &[(1, 16), (2, 16), (3, 6)], vec![]);

        let mut aggro = settings();
        aggro.archetype = Archetype::Aggro;
        let mut control = settings();
        control.archetype = Archetype::Control;

        let aggro = score(&profile, aggro);
        let control = score(&profile, control);
        assert!(criterion(&aggro, "Curve").score > criterion(&control, "Curve").score);
    }

    #[test]
    fn the_curve_criterion_is_flagged_as_a_judgement() {
        // The UI needs to distinguish the calculated criteria from the opinionated one.
        let profile = deck(24, &[(2, 36)], vec![blue(1, 2, 36)]);
        let result = score(&profile, settings());
        assert!(!criterion(&result, "Curve").derived);
        assert!(criterion(&result, "Mana base").derived);
        assert!(criterion(&result, "Land drops").derived);
    }

    #[test]
    fn a_deck_with_no_spells_scores_zero_on_curve_rather_than_dividing_by_zero() {
        let profile = deck(60, &[], vec![]);
        let result = score(&profile, settings());
        let curve = criterion(&result, "Curve");
        assert_eq!(curve.score, 0.0);
        assert!(curve.detail.contains("no spells"));
    }

    #[test]
    fn unresolved_cards_mark_the_score_unreliable() {
        let mut profile = deck(24, &[(2, 36)], vec![blue(1, 2, 36)]);
        profile.unresolved = 3;
        let result = score(&profile, settings());
        assert!(!result.reliable);
        assert_eq!(result.unresolved_cards, 3);
    }

    #[test]
    fn scoring_is_reproducible() {
        // Same deck, same settings, same number — or the search would chase noise.
        let profile = deck(24, &[(2, 20), (3, 16)], vec![blue(2, 2, 20)]);
        assert_eq!(score(&profile, settings()), score(&profile, settings()));
    }

    #[test]
    fn a_deliberately_bad_deck_scores_below_a_sensible_one() {
        // The end-to-end sanity check: twelve lands and a pile of six-drops should lose to a
        // conventional build.
        let sensible = deck(24, &[(2, 14), (3, 12), (4, 10)], vec![blue(1, 2, 14)]);
        let awful = deck(12, &[(6, 48)], vec![blue(3, 6, 48)]);

        let good = score(&sensible, settings()).total;
        let bad = score(&awful, settings()).total;
        assert!(good > bad + 20.0, "{good} vs {bad}");
    }
}
