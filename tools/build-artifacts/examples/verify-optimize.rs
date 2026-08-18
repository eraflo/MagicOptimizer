//! Runs the optimizer against the real catalog, on a deck made of real cards.
//!
//! The unit tests score hand-built profiles. This asks the question those cannot: given 35,306
//! actual cards to choose from, does the search propose things that belong in the deck?
//!
//! It exists because the search **has no idea what a card does** — it scores a mana base, a
//! curve and an opening hand, and nothing reads rules text. `only_played_cards` gates candidates
//! on EDHREC rank as a stopgap. Whether that gate is doing its job is a question about real
//! data, so it gets asked against real data.
//!
//! ```bash
//! cargo run --release -p build-artifacts --example verify-optimize
//! ```

use anyhow::{Context, Result};
use mtg_core::Format;
use mtg_data::Catalog;
use mtg_deck::{Deck, DeckEntry};
use mtg_optimizer::{search, CardIndex, CardPool, SearchSettings};

/// A real, legal mono-red burn deck. Every card is one people actually play.
const BURN: &[(&str, u32)] = &[
    ("Lightning Bolt", 4),
    ("Monastery Swiftspear", 4),
    ("Goblin Guide", 4),
    ("Lava Spike", 4),
    ("Rift Bolt", 4),
    ("Skewer the Critics", 4),
    ("Boros Charm", 4),
    ("Eidolon of the Great Revel", 4),
    ("Searing Blaze", 4),
    ("Mountain", 20),
];

fn main() -> Result<()> {
    let catalog = Catalog::open("artifacts/cards.rkyv").context("opening artifacts/cards.rkyv")?;
    println!("Catalog: {} cards\n", catalog.len());

    let mut deck = Deck::new("Burn", Format::Modern);
    let mut missing = Vec::new();
    for (name, quantity) in BURN {
        match catalog
            .iter()
            .find(|(_, card)| card.name().eq_ignore_ascii_case(name))
        {
            Some((_, card)) => deck.add(DeckEntry::new(
                card.oracle_id().to_owned(),
                card.name().to_owned(),
                *quantity,
            )),
            None => missing.push(*name),
        }
    }
    if !missing.is_empty() {
        println!("not in the catalog: {}\n", missing.join(", "));
    }

    let index = CardIndex::build(&catalog);

    // The deck's own identity, not an assumption about it. An earlier version of this check
    // hardcoded mono-red and then flagged white lands as wrong — but four Boros Charm make this
    // deck Boros, and white sources are exactly what it needs.
    let mut deck_identity = mtg_core::ColorSet::COLORLESS;
    for entry in &deck.entries {
        if let Some((_, card)) = catalog
            .iter()
            .find(|(_, card)| card.oracle_id() == entry.oracle_id)
        {
            deck_identity = deck_identity.union(card.color_identity());
        }
    }
    let deck_symbols: String = deck_identity.iter().map(|colour| colour.symbol()).collect();
    println!(
        "Deck identity: {deck_symbols}
"
    );

    for gated in [true, false] {
        let settings = SearchSettings {
            pool: CardPool::Everything,
            only_played_cards: gated,
            ..SearchSettings::for_deck_size(60)
        };
        let result = search(&deck, &index, &settings);

        println!(
            "── only_played_cards = {gated}   ({} candidates, score {:.3} -> {:.3})",
            result.candidates_considered, result.before.total, result.after.total
        );
        if result.suggestions.is_empty() {
            println!("   no swaps proposed");
        }
        for suggestion in &result.suggestions {
            // The question the gate exists to answer: is the proposed card even castable here?
            let identity = catalog
                .iter()
                .find(|(_, card)| card.oracle_id() == suggestion.add_oracle_id)
                .map(|(_, card)| card.color_identity())
                .unwrap_or(mtg_core::ColorSet::COLORLESS);
            let symbols: String = identity.iter().map(|colour| colour.symbol()).collect();
            let off_colour = !identity.is_subset_of(deck_identity);
            println!(
                "   {} -> {:<32} [{}]{}",
                suggestion.remove_name,
                suggestion.add_name,
                if symbols.is_empty() {
                    "colourless".to_owned()
                } else {
                    symbols
                },
                if off_colour { "   <-- OFF COLOUR" } else { "" }
            );
        }
        println!();
    }
    Ok(())
}
