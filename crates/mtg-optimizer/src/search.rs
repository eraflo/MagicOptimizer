//! Searching for a better version of a deck.
//!
//! Simulated annealing over single-card swaps. Annealing rather than hill climbing because the
//! score surface has local maxima all over it — pulling one land out of a two-colour deck
//! usually makes things worse before a different land makes them better, and a greedy search
//! stops at the first hill it finds.
//!
//! Rather than hand back a rewritten deck, the search returns the **swaps** it made, each with
//! what it changed and why. A deck you did not choose is not much use; a list of "this for
//! that, because your double-black costs were only being met 58% of the time" is.

use std::collections::HashSet;

use mtg_core::{ColorSet, Format};
use mtg_data::ArchivedCard;
use mtg_deck::{Deck, DeckEntry, FormatRules, Zone};
use serde::{Deserialize, Serialize};

use crate::profile::{profile_with_index, CardIndex};
use crate::rng::Rng;
use crate::score::{score, Score, ScoreSettings};

/// Which cards the search is allowed to reach for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CardPool {
    /// Anything legal in the format. Useful for seeing what the deck could be.
    Everything,
    /// Only cards in the given set of oracle ids.
    ///
    /// The caller decides what that means — the collection, the collection plus a wishlist, or
    /// a scanned draft pool. `mtg-optimizer` deliberately knows nothing about collections.
    Only { oracle_ids: HashSet<String> },
}

impl CardPool {
    fn allows(&self, oracle_id: &str) -> bool {
        match self {
            CardPool::Everything => true,
            CardPool::Only { oracle_ids } => oracle_ids.contains(oracle_id),
        }
    }
}

/// How hard to look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchSettings {
    pub score: ScoreSettings,
    pub pool: CardPool,
    /// Swap attempts. A few thousand is enough for a 60-card deck; more mostly costs time.
    pub iterations: u32,
    /// Most swaps to suggest, so the answer stays reviewable.
    pub max_suggestions: usize,
    /// Fixed, so the same deck gives the same advice twice running.
    pub seed: u64,
    /// Ignore cards nobody plays.
    ///
    /// # Why this exists
    ///
    /// The score measures a mana base, a curve and an opening hand. It has **no idea what a
    /// card does** — nothing here reads rules text or knows that a counterspell is better than
    /// a vanilla 2/2. Left unfiltered, the search happily proposes Maze Skullbomb because it
    /// nudges the curve.
    ///
    /// Scryfall's EDHREC rank is a blunt but available proxy: a card with a rank is one people
    /// actually play. It is a Commander statistic, so it is a weaker signal in other formats,
    /// and it is a popularity measure rather than a quality one. It is a stopgap until the
    /// embeddings of phase 8 give the search a real notion of what a card is for.
    ///
    /// It does **not** keep suggestions on colour — see [`candidate_identity`], which is a
    /// separate filter and the one that does that job.
    pub only_played_cards: bool,
    /// With [`SearchSettings::only_played_cards`], how far down the popularity list to go.
    pub popularity_limit: u32,
    /// Keep the deck inside a Commander bracket, if one is asked for.
    ///
    /// # What this enforces, and what it cannot
    ///
    /// Only the **Game Changer count**, which is the one bracket criterion computable from the
    /// catalog alone: Scryfall ships the flag, so it is exact. Brackets 1 and 2 allow none,
    /// bracket 3 allows three, bracket 4 is unbounded.
    ///
    /// It does **not** enforce the other two criteria. Two-card combos need the combo artifact,
    /// which is an optional download, and mass land denial needs rules text — both live in
    /// `mtg-combo`, and reaching for them here would put a 17 ms index build inside a loop that
    /// runs thousands of times. A deck can therefore still sit above its target for a reason
    /// the search cannot see, which is why `BracketPanel` assesses the finished deck properly
    /// rather than trusting this.
    ///
    /// `None` means no constraint.
    pub max_bracket: Option<u8>,
    /// Games per simulation *during the search*.
    ///
    /// Lower than the final figure on purpose: the search runs thousands of evaluations and
    /// only needs to rank them, while the before-and-after report needs to be accurate. The
    /// noise this leaves is why a suggestion is re-checked at full precision before being kept.
    pub games_while_searching: u32,
}

impl SearchSettings {
    pub fn for_deck_size(deck_size: u32) -> SearchSettings {
        SearchSettings {
            score: ScoreSettings::for_deck_size(deck_size),
            pool: CardPool::Everything,
            iterations: 1_200,
            max_suggestions: 12,
            only_played_cards: true,
            popularity_limit: 8_000,
            // Unconstrained by default: most formats have no brackets, and a Commander player
            // who has not said which bracket they are aiming for has not asked to be limited.
            max_bracket: None,
            seed: 0x0B71_D0C0,
            games_while_searching: 1_500,
        }
    }
}

/// One proposed change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub remove_oracle_id: String,
    pub remove_name: String,
    pub add_oracle_id: String,
    pub add_name: String,
    /// Total score before and after this one swap, on top of the ones before it.
    pub score_before: f64,
    pub score_after: f64,
    /// Which named criteria this swap improved, and by how much.
    pub reasons: Vec<String>,
}

impl Suggestion {
    pub fn gain(&self) -> f64 {
        self.score_after - self.score_before
    }
}

/// What the search found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub before: Score,
    /// The score with **every reported suggestion** applied — not with whatever the search
    /// wandered through, so the figure matches what a reader can actually reach.
    pub after: Score,
    pub suggestions: Vec<Suggestion>,
    /// How many cards the search had to choose from. Zero means the filters left nothing, which
    /// is worth saying rather than reporting "no improvements found".
    pub candidates_considered: usize,
}

/// Most Game Changers a deck may hold to stay inside a bracket.
///
/// Mirrors `mtg_combo::assess`, which is the authority: more than three puts a deck in bracket
/// 4, any at all puts it in 3, and brackets 1 and 2 allow none. Duplicated rather than shared
/// because importing `mtg-combo` here would drag the combo artifact into the search loop for
/// one integer.
fn game_changer_allowance(max_bracket: Option<u8>) -> Option<usize> {
    match max_bracket {
        Some(1) | Some(2) => Some(0),
        Some(3) => Some(3),
        _ => None,
    }
}

/// How many Game Changers a deck holds, counting copies.
fn game_changers_in(deck: &Deck, index: &CardIndex<'_>) -> usize {
    deck.entries
        .iter()
        .filter(|entry| entry.zone != Zone::Sideboard)
        .filter(|entry| {
            index
                .get(&entry.oracle_id)
                .is_some_and(|card| card.is_game_changer())
        })
        .map(|entry| entry.quantity as usize)
        .sum()
}

/// Looks for swaps that raise the deck's score.
pub fn search(deck: &Deck, index: &CardIndex<'_>, settings: &SearchSettings) -> SearchResult {
    let rules = FormatRules::for_format(deck.format);
    let identity = candidate_identity(deck, index, &rules);
    let candidates = candidate_cards(deck, index, settings, &rules, identity);

    let full_settings = settings.score;
    let mut fast_settings = settings.score;
    fast_settings.simulation.games = settings.games_while_searching.max(1);

    let before = score(&profile_with_index(deck, index), full_settings);

    if candidates.is_empty() {
        return SearchResult {
            after: before.clone(),
            before,
            suggestions: Vec::new(),
            candidates_considered: 0,
        };
    }

    // A deck already over its target is not made unoptimisable: the ceiling is whichever is
    // higher, so the search can still improve everything else rather than returning nothing.
    let ceiling = game_changer_allowance(settings.max_bracket)
        .map(|allowance| allowance.max(game_changers_in(deck, index)));

    let improved = anneal(
        deck,
        index,
        &candidates,
        &rules,
        settings,
        fast_settings,
        ceiling,
    );
    let (suggestions, after) =
        suggestions_from_diff(deck, &improved, index, settings, full_settings);

    SearchResult {
        before,
        after,
        suggestions,
        candidates_considered: candidates.len(),
    }
}

/// Runs the annealing and returns the deck it settled on.
fn anneal(
    deck: &Deck,
    index: &CardIndex<'_>,
    candidates: &[(String, String)],
    rules: &FormatRules,
    settings: &SearchSettings,
    fast_settings: ScoreSettings,
    game_changer_ceiling: Option<usize>,
) -> Deck {
    let mut rng = Rng::new(settings.seed);
    let mut current = deck.clone();
    let mut current_score = score(&profile_with_index(&current, index), fast_settings).total;

    for iteration in 0..settings.iterations {
        let Some((removed, added)) = propose_swap(&current, candidates, index, rules, &mut rng)
        else {
            continue;
        };

        let mut trial = current.clone();
        trial.remove(&removed.oracle_id, Zone::Main, 1);
        trial.add(DeckEntry::new(&added.0, &added.1, 1));

        // Checked on the trial rather than filtered out of the candidate list, because the
        // limit is a property of the whole deck: a Game Changer is only forbidden once the deck
        // already holds its quota.
        if let Some(ceiling) = game_changer_ceiling {
            if game_changers_in(&trial, index) > ceiling {
                continue;
            }
        }

        let trial_score = score(&profile_with_index(&trial, index), fast_settings).total;
        let delta = trial_score - current_score;

        // Metropolis: always take an improvement, sometimes take a small loss early on so the
        // search can climb out of a local maximum. Temperature falls to zero by the last
        // iteration, so the tail of the run is pure hill climbing.
        let temperature =
            3.0 * (1.0 - f64::from(iteration) / f64::from(settings.iterations.max(1)));
        if delta > 0.0 || (temperature > 0.0 && rng.unit() < (delta / temperature).exp()) {
            current = trial;
            current_score = trial_score;
        }
    }

    current
}

/// Turns the difference between two decks into swaps a reader can apply one at a time.
///
/// # Why a diff, and not the steps the search took
///
/// Annealing walks a *path*, including downhill steps it later undoes. Reporting that path and
/// letting someone accept a few of its steps does not give them a deck the search ever
/// considered — in testing it produced six copies of a four-of, because a removal earlier in
/// the path was not among the steps applied.
///
/// A diff has none of that. Each entry is a net change from the deck as it stands, so any
/// subset of them applied in any order is a real deck, and never holds more copies of a card
/// than the finished version did.
fn suggestions_from_diff(
    original: &Deck,
    improved: &Deck,
    index: &CardIndex<'_>,
    settings: &SearchSettings,
    full_settings: ScoreSettings,
) -> (Vec<Suggestion>, Score) {
    let mut removals: Vec<(String, String)> = Vec::new();
    let mut additions: Vec<(String, String)> = Vec::new();

    for entry in original.entries_in(Zone::Main) {
        let before = original.copies_of(&entry.oracle_id);
        let after = improved.copies_of(&entry.oracle_id);
        for _ in after..before {
            removals.push((entry.oracle_id.clone(), entry.name.clone()));
        }
    }
    for entry in improved.entries_in(Zone::Main) {
        let before = original.copies_of(&entry.oracle_id);
        let after = improved.copies_of(&entry.oracle_id);
        for _ in before..after {
            additions.push((entry.oracle_id.clone(), entry.name.clone()));
        }
    }

    // Evaluated one at a time on top of everything accepted so far, so the numbers shown add
    // up as a reader works down the list.
    let mut applied = original.clone();
    let mut running = score(&profile_with_index(&applied, index), full_settings);
    let mut suggestions = Vec::new();

    for (removed, added) in removals.into_iter().zip(additions) {
        if suggestions.len() >= settings.max_suggestions {
            break;
        }

        let mut trial = applied.clone();
        trial.remove(&removed.0, Zone::Main, 1);
        trial.add(DeckEntry::new(&added.0, &added.1, 1));

        let trial_score = score(&profile_with_index(&trial, index), full_settings);
        // Re-checked at full precision: the search ran on a smaller, noisier simulation, and a
        // swap it liked does not always survive an honest look.
        if trial_score.total <= running.total {
            continue;
        }

        let reasons = trial_score
            .criteria
            .iter()
            .zip(running.criteria.iter())
            .filter(|(now, then)| now.score > then.score + 0.005)
            .map(|(now, then)| {
                format!(
                    "{}: {:.0}% to {:.0}% — {}",
                    now.name,
                    then.score * 100.0,
                    now.score * 100.0,
                    now.detail
                )
            })
            .collect();

        suggestions.push(Suggestion {
            remove_oracle_id: removed.0,
            remove_name: removed.1,
            add_oracle_id: added.0,
            add_name: added.1,
            score_before: running.total,
            score_after: trial_score.total,
            reasons,
        });

        applied = trial;
        running = trial_score;
    }

    (suggestions, running)
}

/// Picks a card to take out and one to put in.
fn propose_swap(
    deck: &Deck,
    candidates: &[(String, String)],
    index: &CardIndex<'_>,
    rules: &FormatRules,
    rng: &mut Rng,
) -> Option<(DeckEntry, (String, String))> {
    let removable: Vec<&DeckEntry> = deck.entries_in(Zone::Main).collect();
    if removable.is_empty() {
        return None;
    }

    let removed = removable[rng.below(removable.len() as u32) as usize].clone();
    let added = &candidates[rng.below(candidates.len() as u32) as usize];

    if added.0 == removed.oracle_id {
        return None;
    }

    // Adding must not break the copy limit. Basic lands are exempt, which is what lets the
    // search adjust a mana base at all.
    let already = deck.copies_of(&added.0);
    let is_basic = index
        .get(&added.0)
        .is_some_and(|card| card.has_type("Basic") && card.has_type("Land"));
    if !is_basic && already >= rules.max_copies {
        return None;
    }

    Some((removed, added.clone()))
}

/// Every card the search may propose.
fn candidate_cards(
    deck: &Deck,
    index: &CardIndex<'_>,
    settings: &SearchSettings,
    rules: &FormatRules,
    identity: Option<ColorSet>,
) -> Vec<(String, String)> {
    let format = deck.format;
    let mut candidates = Vec::new();

    for oracle_id in index.oracle_ids() {
        let Some(card) = index.get(oracle_id) else {
            continue;
        };
        if !is_candidate(card, format, identity, settings, rules) {
            continue;
        }
        candidates.push((oracle_id.to_owned(), card.name().to_owned()));
    }

    // Sorted so the same deck and seed give the same run: a HashMap's iteration order is not
    // stable between processes, and an unstable candidate order would make the search
    // irreproducible however fixed the seed is.
    candidates.sort_unstable();
    candidates
}

fn is_candidate(
    card: &ArchivedCard,
    format: Format,
    identity: Option<ColorSet>,
    settings: &SearchSettings,
    _rules: &FormatRules,
) -> bool {
    if !card.is_legal_in(format) {
        return false;
    }
    if let Some(identity) = identity {
        if !card.color_identity().is_subset_of(identity) {
            return false;
        }
    }
    if settings.only_played_cards {
        // Basic lands have no rank and must always stay available, or the search loses its
        // only way to adjust a mana base.
        let is_basic = card.has_type("Basic") && card.has_type("Land");
        let played = card
            .edhrec_rank()
            .is_some_and(|rank| rank <= settings.popularity_limit);
        if !is_basic && !played {
            return false;
        }
    }
    settings.pool.allows(card.oracle_id())
}

/// The colour identity the deck is bound to, if its format binds one.
/// The colours the search is allowed to reach for.
///
/// In Commander this is a rule: the deck's colour identity comes from its commander, and a card
/// outside it is illegal. Everywhere else there is no such rule — a Modern deck may legally play
/// any card of any colour — but proposing one is still wrong, and measurement said so loudly.
///
/// Run against the real catalog, a mono-red burn deck was offered Horizon Canopy, Yavimaya Coast
/// and Nomad Outpost: eight of twelve suggestions were off colour. The score cannot see the
/// mistake, because a land it cannot use still counts as a land drop. The `only_played_cards`
/// gate does not catch it either — popularity says nothing about colour, and an earlier version
/// of this file claimed otherwise.
///
/// So outside Commander the identity is derived from the deck itself. The cost is that the
/// search will never suggest a splash or a colour change; that is the right trade, because a
/// scorer with no idea what a card does has no business proposing one.
fn candidate_identity(deck: &Deck, index: &CardIndex<'_>, rules: &FormatRules) -> Option<ColorSet> {
    if let Some(commander) = rules.commander.as_ref() {
        if commander.enforce_color_identity {
            let mut identity = ColorSet::COLORLESS;
            for entry in deck.entries_in(Zone::Command) {
                if let Some(card) = index.get(&entry.oracle_id) {
                    identity = identity.union(card.color_identity());
                }
            }
            return Some(identity);
        }
    }

    let mut identity = ColorSet::COLORLESS;
    for entry in &deck.entries {
        if entry.zone == Zone::Sideboard {
            continue;
        }
        if let Some(card) = index.get(&entry.oracle_id) {
            identity = identity.union(card.color_identity());
        }
    }

    // An empty deck, or one built entirely of colourless cards, constrains nothing — returning
    // `Some(COLORLESS)` there would leave the search with basics and artifacts to choose from.
    (!identity.is_empty()).then_some(identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{Legality, Rarity};
    use mtg_data::{
        legality_to_u8, rarity_to_u8, Card as CatalogCard, CardFace, Catalog, CatalogData, Layout,
        LEGALITY_SLOTS,
    };

    fn card(name: &str, type_line: &str, mana_cost: &str, produced: &str) -> CatalogCard {
        let cost = mtg_core::ManaCost::parse(mana_cost).unwrap_or_default();
        let mut built = CatalogCard {
            oracle_id: format!("o-{name}"),
            name: name.to_owned(),
            mana_cost: mana_cost.to_owned(),
            mana_value: cost.mana_value() as f32,
            colors: cost.colors().bits(),
            color_identity: cost.colors().union(ColorSet::from_symbols(produced)).bits(),
            produced_mana: ColorSet::from_symbols(produced).bits(),
            type_line: type_line.to_owned(),
            oracle_text: String::new(),
            power: None,
            toughness: None,
            loyalty: None,
            keywords: Vec::new(),
            legalities: [legality_to_u8(Legality::Legal); LEGALITY_SLOTS],
            rarity: rarity_to_u8(Rarity::Common),
            edhrec_rank: None,
            game_changer: false,
            tags: 0,
            reserved: false,
            layout: Layout::Normal,
            faces: Vec::new(),
            set_code: "tst".to_owned(),
            collector_number: "1".to_owned(),
            released_at: "2026-01-01".to_owned(),
            image_id: String::new(),
        };
        built.faces.push(CardFace {
            name: built.name.clone(),
            mana_cost: built.mana_cost.clone(),
            type_line: built.type_line.clone(),
            oracle_text: String::new(),
            power: None,
            toughness: None,
            loyalty: None,
            colors: built.colors,
        });
        built
    }

    fn catalog() -> Catalog {
        let data = CatalogData {
            format_version: mtg_data::FORMAT_VERSION,
            source_updated_at: String::new(),
            cards: vec![
                card("Island", "Basic Land — Island", "", "U"),
                card("Swamp", "Basic Land — Swamp", "", "B"),
                card("Forest", "Basic Land — Forest", "", "G"),
                card("Counterspell", "Instant", "{U}{U}", ""),
                card("Doom Blade", "Instant", "{1}{B}", ""),
                card("Ponder", "Sorcery", "{U}", ""),
                card("Divination", "Sorcery", "{2}{U}", ""),
                card("Colossus", "Creature — Golem", "{7}", ""),
            ],
        };
        Catalog::from_bytes(mtg_data::serialize(&data).unwrap()).unwrap()
    }

    fn deck_of(entries: &[(&str, u32)]) -> Deck {
        let mut deck = Deck::new("Test", Format::Modern);
        for (name, quantity) in entries {
            deck.add(DeckEntry::new(format!("o-{name}"), *name, *quantity));
        }
        deck
    }

    fn settings() -> SearchSettings {
        let mut settings = SearchSettings::for_deck_size(60);
        // Small and fast: these tests check behaviour, not convergence quality.
        // The fixtures carry no EDHREC rank, so the popularity filter would reject all of
        // them; it has a test of its own below.
        settings.only_played_cards = false;
        settings.iterations = 250;
        settings.games_while_searching = 400;
        settings.score.simulation.games = 1_500;
        settings
    }

    #[test]
    fn the_same_seed_gives_the_same_advice() {
        // Otherwise "optimize" would say something different every time it was pressed.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 10), ("Colossus", 50)]);

        let first = search(&deck, &index, &settings());
        let second = search(&deck, &index, &settings());
        assert_eq!(first.suggestions, second.suggestions);
        assert_eq!(first.before, second.before);
    }

    #[test]
    fn a_bad_deck_gets_suggestions_that_improve_it() {
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        // Ten lands and fifty seven-drops: about as bad as a deck can be.
        let deck = deck_of(&[("Island", 10), ("Colossus", 50)]);

        let result = search(&deck, &index, &settings());
        assert!(!result.suggestions.is_empty(), "nothing suggested");
        assert!(
            result.after.total > result.before.total,
            "{} to {}",
            result.before.total,
            result.after.total
        );
    }

    #[test]
    fn every_suggestion_carries_its_own_gain_and_a_reason() {
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 10), ("Colossus", 50)]);

        for suggestion in search(&deck, &index, &settings()).suggestions {
            assert!(suggestion.gain() > 0.0, "{suggestion:?}");
            assert!(!suggestion.reasons.is_empty(), "{suggestion:?}");
            assert_ne!(suggestion.remove_oracle_id, suggestion.add_oracle_id);
        }
    }

    #[test]
    fn suggestions_are_capped_so_the_answer_stays_reviewable() {
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 10), ("Colossus", 50)]);

        let mut config = settings();
        config.max_suggestions = 3;
        assert!(search(&deck, &index, &config).suggestions.len() <= 3);
    }

    #[test]
    fn a_restricted_pool_is_respected() {
        // The whole point of the "only cards I own" toggle.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 10), ("Colossus", 50)]);

        let mut config = settings();
        config.pool = CardPool::Only {
            oracle_ids: ["o-Island".to_owned(), "o-Ponder".to_owned()]
                .into_iter()
                .collect(),
        };

        let result = search(&deck, &index, &config);
        for suggestion in &result.suggestions {
            assert!(
                ["o-Island", "o-Ponder"].contains(&suggestion.add_oracle_id.as_str()),
                "{suggestion:?}"
            );
        }
        assert_eq!(result.candidates_considered, 2);
    }

    #[test]
    fn an_empty_pool_says_so_rather_than_reporting_no_improvements() {
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 10), ("Colossus", 50)]);

        let mut config = settings();
        config.pool = CardPool::Only {
            oracle_ids: HashSet::new(),
        };

        let result = search(&deck, &index, &config);
        assert_eq!(result.candidates_considered, 0);
        assert!(result.suggestions.is_empty());
        assert_eq!(result.before.total, result.after.total);
    }

    #[test]
    fn the_copy_limit_is_not_broken() {
        // Started from a legal deck on purpose. An earlier version of this test began with
        // forty copies of one card and then asserted the result obeyed the four-copy limit,
        // which the search was never going to fix — it improves decks, it does not legalise
        // them.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[
            ("Island", 24),
            ("Swamp", 20),
            ("Counterspell", 4),
            ("Ponder", 4),
            ("Divination", 4),
            ("Doom Blade", 4),
        ]);

        let result = search(&deck, &index, &settings());
        let mut final_deck = deck.clone();
        for suggestion in &result.suggestions {
            final_deck.remove(&suggestion.remove_oracle_id, Zone::Main, 1);
            final_deck.add(DeckEntry::new(
                &suggestion.add_oracle_id,
                &suggestion.add_name,
                1,
            ));
        }

        for entry in final_deck.entries_in(Zone::Main) {
            let card = index.get(&entry.oracle_id).expect("card");
            let is_basic = card.has_type("Basic") && card.has_type("Land");
            assert!(
                is_basic || entry.quantity <= 4,
                "{} has {} copies",
                entry.name,
                entry.quantity
            );
        }
    }

    #[test]
    fn a_deck_already_over_the_limit_is_not_pushed_further_over() {
        // The search is not a legaliser, but it must not make an existing violation worse.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 20), ("Colossus", 40)]);

        let result = search(&deck, &index, &settings());
        let mut final_deck = deck.clone();
        for suggestion in &result.suggestions {
            final_deck.remove(&suggestion.remove_oracle_id, Zone::Main, 1);
            final_deck.add(DeckEntry::new(
                &suggestion.add_oracle_id,
                &suggestion.add_name,
                1,
            ));
        }

        for entry in final_deck.entries_in(Zone::Main) {
            let started_with = deck
                .entries_in(Zone::Main)
                .find(|e| e.oracle_id == entry.oracle_id)
                .map_or(0, |e| e.quantity);
            let card = index.get(&entry.oracle_id).expect("card");
            let is_basic = card.has_type("Basic") && card.has_type("Land");
            assert!(
                is_basic || entry.quantity <= started_with.max(4),
                "{} went from {started_with} to {} copies",
                entry.name,
                entry.quantity
            );
        }
    }

    #[test]
    fn colour_identity_is_respected_in_commander() {
        let catalog = catalog();
        let index = CardIndex::build(&catalog);

        let mut deck = Deck::new("Mono blue", Format::Commander);
        deck.add(DeckEntry::new("o-Ponder", "Ponder", 1).in_zone(Zone::Command));
        deck.add(DeckEntry::new("o-Colossus", "Colossus", 99));

        let result = search(&deck, &index, &settings());
        for suggestion in &result.suggestions {
            let card = index.get(&suggestion.add_oracle_id).expect("card");
            assert!(
                card.color_identity()
                    .is_subset_of(ColorSet::from_symbols("U")),
                "{} is outside the identity",
                suggestion.add_name
            );
        }
    }

    #[test]
    fn the_popularity_filter_keeps_out_cards_nobody_plays() {
        // Without it the search proposed cards nobody plays, such as a Maze filter land,
        // because the score can measure a land drop but not what a card is for.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 10), ("Colossus", 50)]);

        let mut config = settings();
        config.only_played_cards = true;

        let result = search(&deck, &index, &config);
        // Only the basics survive: nothing in the fixture catalog has a rank.
        for suggestion in &result.suggestions {
            let card = index.get(&suggestion.add_oracle_id).expect("card");
            assert!(
                card.has_type("Basic"),
                "{} has no rank and should have been filtered",
                suggestion.add_name
            );
        }
    }

    /// The candidate list by name, which is what the colour and popularity rules act on.
    fn candidates_for(deck: &Deck, index: &CardIndex<'_>, config: &SearchSettings) -> Vec<String> {
        let rules = FormatRules::for_format(deck.format);
        let identity = candidate_identity(deck, index, &rules);
        candidate_cards(deck, index, config, &rules, identity)
            .into_iter()
            .map(|(_, name)| name)
            .collect()
    }

    /// A catalog where some cards are flagged as Game Changers.
    fn bracket_catalog() -> Catalog {
        let mut island = card("Island", "Basic Land — Island", "", "U");
        island.edhrec_rank = Some(1);
        let mut plain_spell = card("Ponder", "Sorcery", "{U}", "");
        plain_spell.edhrec_rank = Some(2);

        let mut cards = vec![island, plain_spell];
        for name in [
            "Rhystic Study",
            "Cyclonic Rift",
            "Mystical Tutor",
            "Fierce Guardianship",
        ] {
            let mut changer = card(name, "Instant", "{U}", "");
            changer.game_changer = true;
            changer.edhrec_rank = Some(3);
            cards.push(changer);
        }

        let data = CatalogData {
            format_version: mtg_data::FORMAT_VERSION,
            source_updated_at: String::new(),
            cards,
        };
        Catalog::from_bytes(mtg_data::serialize(&data).unwrap()).unwrap()
    }

    #[test]
    fn the_allowance_matches_what_assess_would_say() {
        // These mirror `mtg_combo::assess`, which is the authority. If the two ever disagree
        // the optimizer would build a deck the bracket panel then calls out.
        assert_eq!(game_changer_allowance(Some(1)), Some(0));
        assert_eq!(game_changer_allowance(Some(2)), Some(0));
        assert_eq!(game_changer_allowance(Some(3)), Some(3));
        assert_eq!(
            game_changer_allowance(Some(4)),
            None,
            "bracket 4 is unbounded"
        );
        assert_eq!(
            game_changer_allowance(None),
            None,
            "and so is asking for nothing"
        );
    }

    #[test]
    fn a_bracket_two_search_never_adds_a_game_changer() {
        let catalog = bracket_catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 40), ("Ponder", 20)]);

        let mut config = settings();
        config.max_bracket = Some(2);
        let result = search(&deck, &index, &config);

        for suggestion in &result.suggestions {
            let card = index.get(&suggestion.add_oracle_id).expect("candidate");
            assert!(
                !card.is_game_changer(),
                "{} would push the deck out of bracket 2",
                suggestion.add_name
            );
        }
    }

    #[test]
    fn a_deck_already_over_its_target_is_still_optimisable() {
        // A constraint that returns nothing helps nobody. The ceiling is whichever is higher —
        // the allowance or what the deck already holds — so everything else can still improve.
        let catalog = bracket_catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[
            ("Island", 34),
            ("Rhystic Study", 4),
            ("Cyclonic Rift", 4),
            ("Mystical Tutor", 4),
            ("Fierce Guardianship", 4),
            ("Ponder", 10),
        ]);

        let mut config = settings();
        config.max_bracket = Some(2);
        let result = search(&deck, &index, &config);
        assert!(
            result.candidates_considered > 0,
            "the search must still run"
        );
    }

    #[test]
    fn an_unconstrained_search_is_free_to_use_game_changers() {
        // The other half: the constraint only applies when a bracket was actually asked for.
        let catalog = bracket_catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 40), ("Ponder", 20)]);

        assert_eq!(settings().max_bracket, None);
        let ceiling = game_changer_allowance(settings().max_bracket)
            .map(|allowance| allowance.max(game_changers_in(&deck, &index)));
        assert_eq!(ceiling, None);
    }

    #[test]
    fn game_changers_are_counted_per_copy_and_ignore_the_sideboard() {
        // Four copies of one Game Changer are four Game Changers, and a card on the bench is
        // not in the deck at all.
        let catalog = bracket_catalog();
        let index = CardIndex::build(&catalog);

        let mut deck = deck_of(&[("Rhystic Study", 3)]);
        let mut benched = DeckEntry::new("o-Cyclonic Rift", "Cyclonic Rift", 5);
        benched.zone = Zone::Sideboard;
        deck.add(benched);

        assert_eq!(game_changers_in(&deck, &index), 3);
    }

    #[test]
    fn basic_lands_survive_the_popularity_filter() {
        // They carry no rank either, and excluding them would leave the search unable to
        // adjust a mana base at all.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        // All three basics, so the deck's own colours do not narrow the answer — this test is
        // about the popularity gate, not about colour.
        let deck = deck_of(&[("Island", 4), ("Swamp", 4), ("Forest", 4), ("Colossus", 48)]);

        let mut config = settings();
        config.only_played_cards = true;
        let names = candidates_for(&deck, &index, &config);

        for basic in ["Island", "Swamp", "Forest"] {
            assert!(
                names.iter().any(|name| name == basic),
                "{basic} was filtered out: {names:?}"
            );
        }
        // Everything else in the fixture is unranked and not basic, so nothing else survives.
        assert_eq!(names.len(), 3, "{names:?}");
    }

    #[test]
    fn the_search_stays_inside_the_decks_own_colours() {
        // Measured against the real catalog, a mono-red deck was offered Horizon Canopy,
        // Yavimaya Coast and Nomad Outpost — eight of twelve suggestions off colour. Nothing in
        // the score can see that mistake: a land it cannot use still counts as a land drop.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Island", 20), ("Counterspell", 40)]);

        let names = candidates_for(&deck, &index, &settings());

        assert!(names.iter().any(|name| name == "Ponder"), "{names:?}");
        assert!(
            names.iter().any(|name| name == "Colossus"),
            "colourless is always allowed"
        );
        for off_colour in ["Swamp", "Forest", "Doom Blade"] {
            assert!(
                !names.iter().any(|name| name == off_colour),
                "{off_colour} is outside the deck's colours: {names:?}"
            );
        }
    }

    #[test]
    fn a_colourless_deck_is_not_locked_out_of_every_colour() {
        // An all-artifact list constrains nothing, and returning an empty identity would leave
        // the search with almost nothing to choose from.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let deck = deck_of(&[("Colossus", 60)]);

        let names = candidates_for(&deck, &index, &settings());
        assert!(names.iter().any(|name| name == "Swamp"), "{names:?}");
        assert!(names.iter().any(|name| name == "Counterspell"), "{names:?}");
    }

    #[test]
    fn a_commanders_identity_still_wins_over_the_decks_own_cards() {
        // In Commander the identity is a rule, not a heuristic: a card outside the commander's
        // colours is illegal however many of them the ninety-nine happen to contain.
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let rules = FormatRules::for_format(Format::Commander);

        let mut deck = Deck::new("EDH", Format::Commander);
        let mut commander = DeckEntry::new("o-Ponder", "Ponder", 1);
        commander.zone = Zone::Command;
        deck.add(commander);
        deck.add(DeckEntry::new("o-Swamp", "Swamp", 40));

        let identity = candidate_identity(&deck, &index, &rules).expect("an identity");
        assert!(identity.contains(mtg_core::Color::Blue));
        assert!(
            !identity.contains(mtg_core::Color::Black),
            "the Swamps in the deck must not widen the commander's identity"
        );
    }

    #[test]
    fn an_empty_deck_does_not_panic() {
        let catalog = catalog();
        let index = CardIndex::build(&catalog);
        let result = search(&Deck::new("Empty", Format::Modern), &index, &settings());
        assert!(result.suggestions.is_empty());
    }

    #[test]
    fn a_deck_the_search_cannot_improve_returns_nothing_rather_than_churn() {
        let catalog = catalog();
        let index = CardIndex::build(&catalog);

        // Nothing to swap towards: the pool holds only what is already in the deck.
        let deck = deck_of(&[("Island", 24), ("Ponder", 36)]);
        let mut config = settings();
        config.pool = CardPool::Only {
            oracle_ids: ["o-Island".to_owned(), "o-Ponder".to_owned()]
                .into_iter()
                .collect(),
        };

        let result = search(&deck, &index, &config);
        assert!(result.after.total >= result.before.total - 1e-9);
    }
}
