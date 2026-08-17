//! Decks: the model, format rules, legality checking and decklist import/export.
//!
//! Decks are stored by Scryfall `oracle_id`, never by [`mtg_core::CardId`] — see the note in
//! [`deck`] for why that distinction matters more than it looks.

mod deck;
mod legality;
mod rules;
mod text;

pub use deck::{Deck, DeckEntry, Zone};
pub use legality::{check, LegalityReport, Violation};
pub use rules::{CommanderRules, DeckSize, FormatRules};
pub use text::{export, import, ExportStyle, ImportProblem, ImportResult};
