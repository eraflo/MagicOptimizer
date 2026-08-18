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

use mtg_core::Tag;
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

    /// Conventional **minimum** share of the deck's identifiable spells for each role group.
    ///
    /// A minimum rather than a target, and that asymmetry is the honest part. Nobody agrees on
    /// how much removal is too much — it depends on the format, the metagame and the deck —
    /// but everyone agrees a sixty-card deck with no interaction at all has a problem. Scoring
    /// only the shortfall says the defensible thing and stays quiet about the rest.
    ///
    /// **These numbers are conventional, not derived.** Like the curve targets above, nothing
    /// computes them; they are written here to be inspected and argued with.
    fn minimum_roles(self) -> [(RoleGroup, f64); 3] {
        match self {
            // Aggro interacts less and draws less, because its cards are its plan.
            Archetype::Aggro => [
                (RoleGroup::Interaction, 0.10),
                (RoleGroup::CardAdvantage, 0.05),
                (RoleGroup::Ramp, 0.00),
            ],
            Archetype::Midrange => [
                (RoleGroup::Interaction, 0.18),
                (RoleGroup::CardAdvantage, 0.12),
                (RoleGroup::Ramp, 0.05),
            ],
            // Control lives on answers and refuelling.
            Archetype::Control => [
                (RoleGroup::Interaction, 0.25),
                (RoleGroup::CardAdvantage, 0.20),
                (RoleGroup::Ramp, 0.05),
            ],
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

/// A family of roles a deck needs some of.
///
/// Grouped rather than scored tag by tag: a deck does not need `board-wipe` specifically, it
/// needs *answers*, and which shape they take is the deckbuilder's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleGroup {
    Interaction,
    CardAdvantage,
    Ramp,
}

impl RoleGroup {
    pub const fn label(self) -> &'static str {
        match self {
            RoleGroup::Interaction => "interaction",
            RoleGroup::CardAdvantage => "card advantage",
            RoleGroup::Ramp => "ramp",
        }
    }

    /// The tags that count towards this group.
    ///
    /// Each list starts with the broadest tag, because [`DeckProfile::copies_with_any`] takes
    /// the largest single count rather than a sum — the vocabulary is hierarchical, so the
    /// parent already contains its children.
    pub const fn tags(self) -> &'static [Tag] {
        match self {
            RoleGroup::Interaction => &[
                Tag::Removal,
                Tag::SpotRemoval,
                Tag::BoardWipe,
                Tag::Counterspell,
            ],
            RoleGroup::CardAdvantage => &[Tag::CardAdvantage, Tag::Draw, Tag::Cantrip],
            RoleGroup::Ramp => &[Tag::Ramp, Tag::ManaRock, Tag::ManaDork, Tag::LandRamp],
        }
    }
}

/// Fewer identifiable spells than this and the roles criterion says nothing.
///
/// The tagger covers 72% of the catalog, so a deck can hold cards whose role is simply unknown.
/// Scoring a deck as having no removal when the truth is that nothing in it is tagged would
/// invent a weakness, and the optimizer would then act on it.
const MIN_IDENTIFIABLE_SPELLS: u32 = 8;

/// How much each criterion counts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Weights {
    pub mana_base: f64,
    pub land_drops: f64,
    pub opening_hands: f64,
    pub curve: f64,
    pub roles: f64,
}

impl Default for Weights {
    /// The calculated criteria outweigh the opinionated one, deliberately.
    fn default() -> Weights {
        Weights {
            mana_base: 1.0,
            land_drops: 1.0,
            opening_hands: 0.7,
            curve: 0.6,
            // Between the calculated criteria and the curve. The thresholds are conventional,
            // but the thing being measured is real and nothing else in the score can see it:
            // without this, cutting the deck's removal costs nothing.
            roles: 0.8,
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
        land_drops_criterion(profile, &simulation, settings.weights.land_drops),
        opening_hands_criterion(&simulation, settings.weights.opening_hands),
        curve_criterion(profile, settings.archetype, settings.weights.curve),
        roles_criterion(profile, settings.archetype, settings.weights.roles),
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

/// Share of a deck's spells that should be castable by the horizon.
///
/// Not all of them: a control deck's single seven-drop should not drag the whole criterion out
/// to turn seven, and a deck's top end is usually a card it is happy to draw late.
const CASTABLE_BY_HORIZON: f64 = 0.90;

/// The earliest horizon considered, whatever the curve says.
///
/// Even a deck of nothing but one-drops wants a second land — to double-spell, and because a
/// one-land hand is a mulligan waiting to happen. Below this the criterion would measure
/// almost nothing.
const MIN_HORIZON: usize = 2;

/// The turn by which the deck actually needs its lands.
///
/// The smallest turn at which [`CASTABLE_BY_HORIZON`] of the deck's spells are affordable.
///
/// This used to be a flat turn four for every deck, and that was wrong in a way that showed up
/// in real advice: a Modern burn list topping out at three mana scored 0.62 here — punished for
/// missing a land drop it has no use for — and the search answered by trading burn spells for
/// lands. The horizon belongs to the deck, not to the criterion.
fn land_drop_horizon(profile: &DeckProfile) -> usize {
    let spells: Vec<u32> = profile
        .cards
        .iter()
        .filter(|card| !card.is_land)
        .map(|card| card.mana_value)
        .collect();

    if spells.is_empty() {
        return MIN_HORIZON;
    }

    let wanted = (spells.len() as f64 * CASTABLE_BY_HORIZON).ceil() as usize;
    let mut turn = MIN_HORIZON;
    loop {
        let affordable = spells
            .iter()
            .filter(|value| **value as usize <= turn)
            .count();
        if affordable >= wanted {
            return turn;
        }
        // The caller clamps to the turns actually simulated, so this cannot run away: a deck of
        // nothing but Emrakuls stops here and the clamp does the rest.
        if turn >= 20 {
            return turn;
        }
        turn += 1;
    }
}

/// Whether the deck reliably makes the land drops it needs.
///
/// Missing a land drop is what actually loses goldfish games, so this is scored on the run of
/// drops rather than on a land count — a deck can have the conventional twenty-four lands and
/// still stumble.
///
/// How far that run has to go comes from [`land_drop_horizon`], and therefore from the deck's
/// own curve. Asking every deck for a turn-four land was the same mistake as scoring a burn
/// deck against a control deck's curve.
fn land_drops_criterion(
    profile: &DeckProfile,
    simulation: &SimulationResult,
    weight: f64,
) -> Criterion {
    let simulated = simulation.land_drops_made.len();
    if simulated == 0 {
        return Criterion {
            name: "Land drops".to_owned(),
            score: 0.0,
            weight: 0.0,
            detail: "no turns were simulated".to_owned(),
            derived: true,
        };
    }

    let horizon = land_drop_horizon(profile).min(simulated);
    let score = simulation
        .land_drops_made
        .get(horizon - 1)
        .copied()
        .unwrap_or(0.0);

    Criterion {
        name: "Land drops".to_owned(),
        score,
        weight,
        detail: format!(
            "every land drop through turn {horizon} in {:.0}% of games              (turn {horizon} is where {:.0}% of this deck is castable)",
            score * 100.0,
            CASTABLE_BY_HORIZON * 100.0
        ),
        derived: true,
    }
}

/// Whether the deck carries the roles a deck of its kind needs.
///
/// This is the only criterion that can see what a card *does*. Everything else measures a mana
/// base, a curve or an opening hand, all of which are blind to effect — which is how the search
/// came to offer a burn deck a Mountain in exchange for Lightning Bolt. Nothing in the score
/// could tell that anything had been lost.
///
/// Scored as shortfall only: a deck at or above the conventional minimum for a group scores
/// full marks for it, and a deck at half the minimum scores half. Exceeding it is neither
/// rewarded nor punished, because there is no defensible number for "too much removal".
///
/// The share is taken over the spells whose role is **known**, not over all spells. The tagger
/// covers 72% of the catalog; treating an untagged card as roleless would manufacture a
/// weakness, and this criterion exists precisely so the search acts on what it says.
fn roles_criterion(profile: &DeckProfile, archetype: Archetype, weight: f64) -> Criterion {
    let identifiable = profile.with_roles;

    if identifiable < MIN_IDENTIFIABLE_SPELLS {
        // Weight zero rather than a middling score: an unmeasurable criterion must not drag
        // the total towards the middle, and must not give the search anything to chase.
        return Criterion {
            name: "Roles".to_owned(),
            score: 0.0,
            weight: 0.0,
            detail: format!(
                "only {identifiable} card(s) have a known role, too few to judge —                  the tag data covers about 72% of cards"
            ),
            derived: false,
        };
    }

    let mut total = 0.0;
    let mut parts = Vec::new();
    let minimums = archetype.minimum_roles();

    for (group, minimum) in minimums {
        let copies = profile.copies_with_any(group.tags());
        let share = f64::from(copies) / f64::from(identifiable);

        let met = if minimum <= 0.0 {
            1.0
        } else {
            (share / minimum).min(1.0)
        };
        total += met;

        if minimum > 0.0 {
            parts.push(format!(
                "{copies} {} ({:.0}% of {:.0}% wanted)",
                group.label(),
                share * 100.0,
                minimum * 100.0
            ));
        }
    }

    let score = total / minimums.len() as f64;
    let unknown = profile.without_roles;
    let caveat = if unknown > 0 {
        format!("; {unknown} card(s) have no tag data")
    } else {
        String::new()
    };

    Criterion {
        name: "Roles".to_owned(),
        score,
        weight,
        detail: format!("{}{caveat}", parts.join(", ")),
        derived: false,
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
            // No role data: these fixtures exercise the mana and curve criteria, and the roles
            // criterion correctly reports itself unmeasurable on them.
            ..DeckProfile::default()
        }
    }

    #[test]
    fn an_aggressive_curve_asks_for_fewer_lands_than_a_slow_one() {
        // The whole point of the change. A burn deck topping out at three mana has no use for
        // its turn-four land, and used to be marked down for missing it.
        let burn = deck(20, &[(1, 16), (2, 12), (3, 12)], Vec::new());
        let control = deck(26, &[(2, 8), (3, 8), (4, 8), (5, 6), (6, 4)], Vec::new());

        assert_eq!(land_drop_horizon(&burn), 3);
        assert!(
            land_drop_horizon(&control) > land_drop_horizon(&burn),
            "control wants its lands for longer"
        );
    }

    #[test]
    fn one_expensive_card_does_not_drag_the_horizon_out() {
        // A single seven-drop is a card you are happy to draw late, not a reason to demand a
        // seventh land drop from a deck that otherwise curves out at two.
        let mostly_cheap = deck(24, &[(1, 18), (2, 17), (7, 1)], Vec::new());
        assert_eq!(land_drop_horizon(&mostly_cheap), MIN_HORIZON);
    }

    #[test]
    fn a_deck_of_nothing_but_lands_does_not_panic() {
        // There are no spells to derive a horizon from, and dividing by that would be the kind
        // of thing that only shows up on someone else's deck.
        let all_lands = deck(60, &[], Vec::new());
        assert_eq!(land_drop_horizon(&all_lands), MIN_HORIZON);
    }

    #[test]
    fn the_horizon_never_exceeds_the_turns_actually_simulated() {
        // Asking for turn seven when five were played would read a missing entry as zero, and
        // the deck would score nothing for a land drop nobody measured.
        let expensive = deck(30, &[(7, 15), (8, 15)], Vec::new());
        let mut settings = ScoreSettings::for_deck_size(60);
        settings.simulation.games = 300;
        settings.simulation.turns = 5;

        let criterion = score(&expensive, settings)
            .criteria
            .into_iter()
            .find(|criterion| criterion.name == "Land drops")
            .expect("land drops");
        assert!(criterion.score > 0.0, "{criterion:?}");
        assert!(
            criterion.detail.contains("through turn 5"),
            "{}",
            criterion.detail
        );
    }

    #[test]
    fn an_aggressive_deck_scores_better_than_it_did_under_a_flat_turn_four() {
        // The regression this change exists for, stated as a comparison rather than a constant:
        // the same deck, judged at the turn it cares about instead of turn four.
        let burn = deck(20, &[(1, 16), (2, 12), (3, 12)], Vec::new());
        let mut settings = ScoreSettings::for_deck_size(60);
        settings.simulation.games = 2_000;
        let simulation = crate::simulate::simulate(&burn, settings.simulation);

        let at_its_own_horizon = land_drops_criterion(&burn, &simulation, 1.0).score;
        let at_turn_four = simulation.land_drops_made[3];
        assert!(
            at_its_own_horizon > at_turn_four,
            "{at_its_own_horizon} should beat the old {at_turn_four}"
        );
    }

    /// A profile carrying only role data, which is all the roles criterion reads.
    fn with_roles(spells: u32, groups: &[(RoleGroup, u32)]) -> DeckProfile {
        let mut profile = DeckProfile {
            with_roles: spells,
            ..DeckProfile::default()
        };
        for (group, copies) in groups {
            // The first tag of a group is its broadest, and `copies_with_any` takes the largest
            // single count — so setting that one is what a real catalog would produce.
            profile.roles[group.tags()[0] as usize] = *copies;
        }
        profile
    }

    #[test]
    fn a_deck_meeting_every_minimum_scores_full_marks() {
        let profile = with_roles(
            40,
            &[
                (RoleGroup::Interaction, 8),   // 20%, above the 18% midrange wants
                (RoleGroup::CardAdvantage, 6), // 15%, above 12%
                (RoleGroup::Ramp, 4),          // 10%, above 5%
            ],
        );
        let criterion = roles_criterion(&profile, Archetype::Midrange, 1.0);
        assert!((criterion.score - 1.0).abs() < 1e-9, "{criterion:?}");
    }

    #[test]
    fn a_deck_with_no_interaction_at_all_is_marked_down() {
        // The failure this criterion exists for. Nothing else in the score can see it.
        let profile = with_roles(40, &[(RoleGroup::CardAdvantage, 6), (RoleGroup::Ramp, 4)]);
        let criterion = roles_criterion(&profile, Archetype::Midrange, 1.0);
        assert!(criterion.score < 0.7, "{}", criterion.score);
        assert!(
            criterion.detail.contains("0 interaction"),
            "{}",
            criterion.detail
        );
    }

    #[test]
    fn exceeding_a_minimum_is_not_rewarded() {
        // There is no defensible number for "too much removal", so the criterion says nothing
        // above the threshold rather than inventing one — and a search cannot farm it by
        // stuffing a deck with removal.
        let modest = with_roles(
            40,
            &[(RoleGroup::Interaction, 8), (RoleGroup::CardAdvantage, 5)],
        );
        let loaded = with_roles(
            40,
            &[(RoleGroup::Interaction, 30), (RoleGroup::CardAdvantage, 5)],
        );
        assert_eq!(
            roles_criterion(&modest, Archetype::Midrange, 1.0).score,
            roles_criterion(&loaded, Archetype::Midrange, 1.0).score
        );
    }

    #[test]
    fn an_archetype_that_wants_none_of_a_role_does_not_punish_its_absence() {
        // Aggro asks for no ramp. A burn deck with zero mana rocks is not thereby worse.
        let profile = with_roles(
            40,
            &[(RoleGroup::Interaction, 8), (RoleGroup::CardAdvantage, 4)],
        );
        let criterion = roles_criterion(&profile, Archetype::Aggro, 1.0);
        assert!((criterion.score - 1.0).abs() < 1e-9, "{criterion:?}");
    }

    #[test]
    fn a_deck_nothing_is_known_about_is_declared_unmeasurable() {
        // The important half. The tagger covers 72% of cards, so "no removal found" and "no
        // data" are different states, and scoring the second as the first would invent a
        // weakness the optimizer would then act on.
        let profile = with_roles(3, &[]);
        let criterion = roles_criterion(&profile, Archetype::Midrange, 1.0);
        assert_eq!(
            criterion.weight, 0.0,
            "an unmeasurable criterion must not count"
        );
        assert!(
            criterion.detail.contains("too few to judge"),
            "{}",
            criterion.detail
        );
    }

    #[test]
    fn an_unmeasurable_roles_criterion_does_not_move_the_total() {
        // Weight zero has to mean weight zero all the way through the weighted average.
        let mut settings = ScoreSettings::for_deck_size(60);
        settings.simulation.games = 300;
        let profile = deck(24, &[(2, 20), (3, 16)], vec![blue(1, 2, 12)]);

        let score = score(&profile, settings);
        let roles = score
            .criteria
            .iter()
            .find(|criterion| criterion.name == "Roles")
            .expect("the roles criterion should still be reported");
        assert_eq!(roles.weight, 0.0);
        assert!(
            score.total > 0.0,
            "the rest of the score should be unaffected"
        );
    }

    #[test]
    fn the_share_is_taken_over_known_cards_not_the_whole_deck() {
        // The numbers are chosen so the two denominators disagree. Ten cards are known, thirty
        // are not. Over the known ten, every minimum is met; over all forty it would look like
        // a deck with almost no interaction — a weakness that is not there, and one the search
        // would then try to fix by cutting real cards.
        let mostly_unknown = DeckProfile {
            with_roles: 10,
            without_roles: 30,
            ..with_roles(
                10,
                &[
                    (RoleGroup::Interaction, 2),   // 20% of 10, but only 5% of 40
                    (RoleGroup::CardAdvantage, 2), // 20% of 10, but only 5% of 40
                    (RoleGroup::Ramp, 1),          // 10% of 10, but only 2.5% of 40
                ],
            )
        };
        let criterion = roles_criterion(&mostly_unknown, Archetype::Midrange, 1.0);
        assert!(
            (criterion.score - 1.0).abs() < 1e-9,
            "every minimum is met among the cards actually known: {criterion:?}"
        );
        assert!(
            criterion.detail.contains("30 card(s) have no tag data"),
            "the caveat has to be visible: {}",
            criterion.detail
        );
    }

    #[test]
    fn one_card_with_two_roles_in_a_group_is_counted_once() {
        // Lightning Bolt is both `removal` and `spot-removal`. Summing them would make four
        // copies look like eight pieces of interaction.
        let mut profile = with_roles(40, &[]);
        profile.roles[Tag::Removal as usize] = 4;
        profile.roles[Tag::SpotRemoval as usize] = 4;
        assert_eq!(profile.copies_with_any(RoleGroup::Interaction.tags()), 4);
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
