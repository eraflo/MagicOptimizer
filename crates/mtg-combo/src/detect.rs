//! Finding the combos a deck already contains.
//!
//! Useful in both directions. You may be looking for one — but more often you have one without
//! knowing, and that changes which bracket your deck belongs in.

use std::collections::{HashMap, HashSet};

use mtg_deck::{Deck, Zone};
use serde::{Deserialize, Serialize};

use crate::combo::{ArchivedCombo, ComboDatabase};

/// A combo found in a deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComboMatch {
    /// Commander Spellbook's identifier, so it can be looked up on their site.
    pub id: String,
    pub card_names: Vec<String>,
    pub produces: Vec<String>,
    pub card_count: usize,
    pub is_infinite: bool,
    pub wins_the_game: bool,
}

/// Combos indexed by the cards they need.
///
/// Built once per database. Checking a deck then only looks at combos that share a card with
/// it, rather than walking every combo in the snapshot — with tens of thousands of combos and
/// a hundred-card deck, the difference is the whole cost.
pub struct ComboIndex<'a> {
    database: &'a ComboDatabase,
    by_card: HashMap<&'a str, Vec<usize>>,
}

impl<'a> ComboIndex<'a> {
    pub fn build(database: &'a ComboDatabase) -> ComboIndex<'a> {
        let mut by_card: HashMap<&str, Vec<usize>> = HashMap::new();
        for (position, combo) in database.iter().enumerate() {
            for oracle_id in combo.oracle_ids.iter() {
                by_card
                    .entry(oracle_id.as_str())
                    .or_default()
                    .push(position);
            }
        }
        ComboIndex { database, by_card }
    }

    pub fn len(&self) -> usize {
        self.database.len()
    }

    pub fn is_empty(&self) -> bool {
        self.database.is_empty()
    }

    /// Every combo fully present in the deck.
    ///
    /// The sideboard is excluded: a combo you cannot assemble in game one is not a combo the
    /// deck has. The command zone counts, since a commander is always available.
    pub fn find_in(&self, deck: &Deck) -> Vec<ComboMatch> {
        let present: HashSet<&str> = deck
            .entries
            .iter()
            .filter(|entry| entry.zone != Zone::Sideboard)
            .map(|entry| entry.oracle_id.as_str())
            .collect();

        self.find_among(&present)
    }

    /// Every combo fully contained in a set of cards.
    pub fn find_among(&self, oracle_ids: &HashSet<&str>) -> Vec<ComboMatch> {
        // Only combos sharing at least one card with the deck are worth testing.
        let mut candidates: HashSet<usize> = HashSet::new();
        for oracle_id in oracle_ids {
            if let Some(positions) = self.by_card.get(oracle_id) {
                candidates.extend(positions);
            }
        }

        let mut found: Vec<ComboMatch> = candidates
            .into_iter()
            .filter_map(|position| self.database.get(position))
            .filter(|combo| {
                // Every piece has to be there. A combo missing one card is not a combo.
                combo
                    .oracle_ids
                    .iter()
                    .all(|id| oracle_ids.contains(id.as_str()))
            })
            .map(describe)
            .collect();

        // Sorted so the same deck always reports its combos in the same order — the candidate
        // set is a HashSet, whose iteration order is not stable between runs.
        found.sort_by(|a, b| {
            a.card_count
                .cmp(&b.card_count)
                .then_with(|| a.card_names.cmp(&b.card_names))
                .then_with(|| a.id.cmp(&b.id))
        });
        found
    }
}

fn describe(combo: &ArchivedCombo) -> ComboMatch {
    ComboMatch {
        id: combo.id().to_owned(),
        card_names: combo.names().into_iter().map(ToOwned::to_owned).collect(),
        produces: combo
            .produces_list()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect(),
        card_count: combo.card_count(),
        is_infinite: combo.is_infinite(),
        wins_the_game: combo.wins_the_game(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combo::tests::{combo, database};
    use mtg_core::Format;
    use mtg_deck::DeckEntry;

    fn deck_of(cards: &[&str]) -> Deck {
        let mut deck = Deck::new("Test", Format::Commander);
        for name in cards {
            deck.add(DeckEntry::new(format!("o-{name}"), *name, 1));
        }
        deck
    }

    fn sample() -> crate::combo::ComboDatabase {
        database(vec![
            combo(
                "a",
                &["Thassa's Oracle", "Demonic Consultation"],
                &["Win the game"],
            ),
            combo(
                "b",
                &["Dramatic Reversal", "Isochron Scepter"],
                &["Infinite colorless mana"],
            ),
            combo(
                "c",
                &["Kiki-Jiki", "Zealous Conscripts", "Ashnod's Altar"],
                &["Infinite colorless mana"],
            ),
        ])
    }

    #[test]
    fn a_complete_combo_is_found() {
        let db = sample();
        let index = ComboIndex::build(&db);
        let found = index.find_in(&deck_of(&[
            "Thassa's Oracle",
            "Demonic Consultation",
            "Island",
        ]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "a");
        assert!(found[0].wins_the_game);
    }

    #[test]
    fn a_combo_missing_a_piece_is_not_reported() {
        // The whole point of checking full inclusion rather than any overlap.
        let db = sample();
        let index = ComboIndex::build(&db);
        assert!(index
            .find_in(&deck_of(&["Thassa's Oracle", "Island"]))
            .is_empty());
        assert!(index
            .find_in(&deck_of(&["Kiki-Jiki", "Zealous Conscripts"]))
            .is_empty());
    }

    #[test]
    fn three_card_combos_work_too() {
        let db = sample();
        let index = ComboIndex::build(&db);
        let found = index.find_in(&deck_of(&[
            "Kiki-Jiki",
            "Zealous Conscripts",
            "Ashnod's Altar",
        ]));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].card_count, 3);
    }

    #[test]
    fn several_combos_are_all_reported_shortest_first() {
        let db = sample();
        let index = ComboIndex::build(&db);
        let found = index.find_in(&deck_of(&[
            "Thassa's Oracle",
            "Demonic Consultation",
            "Kiki-Jiki",
            "Zealous Conscripts",
            "Ashnod's Altar",
        ]));

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].card_count, 2, "two-card combos first");
        assert_eq!(found[1].card_count, 3);
    }

    #[test]
    fn the_order_is_stable_across_runs() {
        // The candidate set is a HashSet; without an explicit sort the report would shuffle
        // between runs and look like the deck had changed.
        let db = sample();
        let index = ComboIndex::build(&db);
        let deck = deck_of(&[
            "Thassa's Oracle",
            "Demonic Consultation",
            "Dramatic Reversal",
            "Isochron Scepter",
        ]);
        let first = index.find_in(&deck);
        for _ in 0..20 {
            assert_eq!(index.find_in(&deck), first);
        }
    }

    #[test]
    fn the_sideboard_does_not_complete_a_combo() {
        // You cannot assemble it in game one, so the deck does not have it.
        let db = sample();
        let index = ComboIndex::build(&db);

        let mut deck = deck_of(&["Thassa's Oracle"]);
        deck.add(
            DeckEntry::new("o-Demonic Consultation", "Demonic Consultation", 1)
                .in_zone(Zone::Sideboard),
        );

        assert!(index.find_in(&deck).is_empty());
    }

    #[test]
    fn the_command_zone_does_complete_a_combo() {
        // A commander is always available, so it counts.
        let db = sample();
        let index = ComboIndex::build(&db);

        let mut deck = deck_of(&["Demonic Consultation"]);
        deck.add(DeckEntry::new("o-Thassa's Oracle", "Thassa's Oracle", 1).in_zone(Zone::Command));

        assert_eq!(index.find_in(&deck).len(), 1);
    }

    #[test]
    fn an_empty_database_finds_nothing_without_complaining() {
        // What the app has before the optional combo artifact is downloaded.
        let db = database(Vec::new());
        let index = ComboIndex::build(&db);
        assert!(index.is_empty());
        assert!(index.find_in(&deck_of(&["Thassa's Oracle"])).is_empty());
    }

    #[test]
    fn an_empty_deck_finds_nothing() {
        let db = sample();
        let index = ComboIndex::build(&db);
        assert!(index
            .find_in(&Deck::new("Empty", Format::Commander))
            .is_empty());
    }
}
