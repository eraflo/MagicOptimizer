//! The deck model.
//!
//! Like collections, decks are stored by Scryfall `oracle_id` rather than by
//! [`mtg_core::CardId`]. A `CardId` is a position in one catalog artifact and shifts on every
//! rebuild, so a deck keyed on it would quietly become a different deck after the next set
//! release. See the note in `mtg_collection::model`.

use mtg_core::Format;
use serde::{Deserialize, Serialize};

/// Which part of a deck a card belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    /// The deck proper.
    Main,
    /// The sideboard, where the format has one.
    Sideboard,
    /// Commanders, and anything else in the command zone.
    Command,
}

impl Zone {
    pub const ALL: [Zone; 3] = [Zone::Main, Zone::Sideboard, Zone::Command];

    pub const fn label(self) -> &'static str {
        match self {
            Zone::Main => "Deck",
            Zone::Sideboard => "Sideboard",
            Zone::Command => "Commander",
        }
    }
}

/// One line of a decklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckEntry {
    /// Scryfall oracle id — stable across catalog rebuilds.
    pub oracle_id: String,
    /// Denormalised so a deck stays readable without a catalog loaded.
    pub name: String,
    pub quantity: u32,
    pub zone: Zone,
    /// Optional printing preference. Empty means "any".
    #[serde(default)]
    pub set_code: String,
    #[serde(default)]
    pub collector_number: String,
}

impl DeckEntry {
    pub fn new(oracle_id: impl Into<String>, name: impl Into<String>, quantity: u32) -> DeckEntry {
        DeckEntry {
            oracle_id: oracle_id.into(),
            name: name.into(),
            quantity,
            zone: Zone::Main,
            set_code: String::new(),
            collector_number: String::new(),
        }
    }

    pub fn in_zone(mut self, zone: Zone) -> DeckEntry {
        self.zone = zone;
        self
    }

    pub fn printed_as(
        mut self,
        set_code: impl Into<String>,
        collector_number: impl Into<String>,
    ) -> DeckEntry {
        self.set_code = set_code.into();
        self.collector_number = collector_number.into();
        self
    }
}

/// A deck, in one format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deck {
    pub name: String,
    pub format: Format,
    /// One flat list rather than a struct per zone: almost every operation — counting,
    /// legality, the mana curve, export — walks all the entries anyway, and a flat list means
    /// adding a zone later does not change the shape of everything that reads a deck.
    pub entries: Vec<DeckEntry>,
    #[serde(default)]
    pub notes: String,
}

impl Deck {
    pub fn new(name: impl Into<String>, format: Format) -> Deck {
        Deck {
            name: name.into(),
            format,
            entries: Vec::new(),
            notes: String::new(),
        }
    }

    /// Adds cards, merging into an existing entry for the same card in the same zone.
    pub fn add(&mut self, entry: DeckEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.oracle_id == entry.oracle_id && e.zone == entry.zone)
        {
            existing.quantity = existing.quantity.saturating_add(entry.quantity);
            return;
        }
        self.entries.push(entry);
    }

    /// Removes copies. Removing them all drops the entry.
    pub fn remove(&mut self, oracle_id: &str, zone: Zone, quantity: u32) {
        let Some(position) = self
            .entries
            .iter()
            .position(|e| e.oracle_id == oracle_id && e.zone == zone)
        else {
            return;
        };
        let entry = &mut self.entries[position];
        if entry.quantity <= quantity {
            self.entries.remove(position);
        } else {
            entry.quantity -= quantity;
        }
    }

    pub fn entries_in(&self, zone: Zone) -> impl Iterator<Item = &DeckEntry> {
        self.entries.iter().filter(move |e| e.zone == zone)
    }

    /// Number of physical cards in a zone.
    pub fn count_in(&self, zone: Zone) -> u32 {
        self.entries_in(zone).map(|e| e.quantity).sum()
    }

    /// Total copies of one card across the main deck and sideboard.
    ///
    /// Commanders are excluded because the copy limit counts them separately in every format
    /// that has them.
    pub fn copies_of(&self, oracle_id: &str) -> u32 {
        self.entries
            .iter()
            .filter(|e| e.oracle_id == oracle_id && e.zone != Zone::Command)
            .map(|e| e.quantity)
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> Deck {
        Deck::new("Test", Format::Commander)
    }

    #[test]
    fn adding_the_same_card_twice_merges() {
        let mut deck = deck();
        deck.add(DeckEntry::new("o1", "Lightning Bolt", 2));
        deck.add(DeckEntry::new("o1", "Lightning Bolt", 2));

        assert_eq!(deck.entries.len(), 1);
        assert_eq!(deck.count_in(Zone::Main), 4);
    }

    #[test]
    fn the_same_card_in_two_zones_stays_separate() {
        let mut deck = deck();
        deck.add(DeckEntry::new("o1", "Pyroblast", 1));
        deck.add(DeckEntry::new("o1", "Pyroblast", 2).in_zone(Zone::Sideboard));

        assert_eq!(deck.entries.len(), 2);
        assert_eq!(deck.count_in(Zone::Main), 1);
        assert_eq!(deck.count_in(Zone::Sideboard), 2);
        // The copy limit spans both zones.
        assert_eq!(deck.copies_of("o1"), 3);
    }

    #[test]
    fn commanders_do_not_count_towards_the_copy_limit() {
        // Otherwise a commander plus a copy in the deck would read as two of the same card,
        // which singleton would then reject.
        let mut deck = deck();
        deck.add(DeckEntry::new("o1", "Krenko", 1).in_zone(Zone::Command));
        deck.add(DeckEntry::new("o1", "Krenko", 1));

        assert_eq!(deck.copies_of("o1"), 1);
        assert_eq!(deck.count_in(Zone::Command), 1);
    }

    #[test]
    fn removing_fewer_copies_than_present_keeps_the_entry() {
        let mut deck = deck();
        deck.add(DeckEntry::new("o1", "Island", 10));
        deck.remove("o1", Zone::Main, 4);

        assert_eq!(deck.count_in(Zone::Main), 6);
        assert_eq!(deck.entries.len(), 1);
    }

    #[test]
    fn removing_every_copy_drops_the_entry() {
        let mut deck = deck();
        deck.add(DeckEntry::new("o1", "Island", 3));
        deck.remove("o1", Zone::Main, 3);
        assert!(deck.is_empty());

        // Over-removing is not an error; the entry is simply gone.
        deck.add(DeckEntry::new("o2", "Forest", 2));
        deck.remove("o2", Zone::Main, 99);
        assert!(deck.is_empty());
    }

    #[test]
    fn removing_a_card_that_is_not_there_does_nothing() {
        let mut deck = deck();
        deck.add(DeckEntry::new("o1", "Island", 3));
        deck.remove("o2", Zone::Main, 1);
        deck.remove("o1", Zone::Sideboard, 1);
        assert_eq!(deck.count_in(Zone::Main), 3);
    }

    #[test]
    fn decks_serialize_round_trip() {
        let mut deck = Deck::new("Krenko goblins", Format::Commander);
        deck.add(DeckEntry::new("o1", "Krenko, Mob Boss", 1).in_zone(Zone::Command));
        deck.add(DeckEntry::new("o2", "Goblin Chieftain", 1).printed_as("m10", "142"));
        deck.notes = "needs more removal".to_owned();

        let json = serde_json::to_string(&deck).expect("serialize");
        let back: Deck = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, deck);
    }
}
