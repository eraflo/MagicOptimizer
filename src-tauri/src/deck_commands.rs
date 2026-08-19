//! Deck commands.
//!
//! As thin as the rest: rules, legality and parsing all live in `mtg-deck`. What is here is
//! argument translation and one convenience — every mutation returns the freshly checked deck,
//! so the UI never has to remember to re-check after an edit and cannot drift out of sync.

use mtg_core::Format;
use mtg_deck::{
    check, export, import, Deck, DeckEntry, DeckId, DeckStats, ExportStyle, ImportProblem,
    LegalityReport, StoredDeck, Zone,
};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

/// A deck plus everything the editor draws alongside it.
///
/// Returned as one object because the three are always shown together; splitting them into
/// three commands would only invite them to disagree.
#[derive(Debug, Serialize)]
pub struct DeckView {
    pub id: u64,
    pub deck: Deck,
    pub legality: LegalityReport,
    pub stats: DeckStats,
}

/// What came of importing a decklist.
#[derive(Debug, Serialize)]
pub struct ImportOutcome {
    pub view: DeckView,
    /// Lines the importer could not use. Never empty silently — see `mtg_deck::text`.
    pub problems: Vec<ImportProblem>,
    /// Rendered messages, so the UI does not have to know the shape of each problem.
    pub messages: Vec<String>,
}

fn parse_zone(zone: &str) -> CommandResult<Zone> {
    match zone {
        "main" => Ok(Zone::Main),
        "sideboard" => Ok(Zone::Sideboard),
        "command" => Ok(Zone::Command),
        other => Err(format!("unknown deck zone {other:?}")),
    }
}

fn parse_format(key: &str) -> CommandResult<Format> {
    Format::from_scryfall_key(key).ok_or_else(|| format!("unknown format {key:?}"))
}

fn parse_style(style: &str) -> CommandResult<ExportStyle> {
    match style {
        "plain" => Ok(ExportStyle::Plain),
        "arena" => Ok(ExportStyle::Arena),
        "mtgo" => Ok(ExportStyle::Mtgo),
        other => Err(format!("unknown export style {other:?}")),
    }
}

/// Builds the view for a deck that is already saved.
pub(crate) fn build_view(state: &AppState, id: DeckId, deck: Deck) -> CommandResult<DeckView> {
    let (legality, stats) =
        state.with_catalog(|catalog| (check(&deck, catalog), mtg_deck::stats(&deck, catalog)))?;
    Ok(DeckView {
        id: id.0,
        deck,
        legality,
        stats,
    })
}

pub(crate) fn load_deck(state: &AppState, id: DeckId) -> CommandResult<Deck> {
    state
        .decks()
        .get(id)
        .map_err(|e| e.to_string())?
        .map(|stored| stored.deck)
        .ok_or_else(|| format!("no deck with id {id}"))
}

#[tauri::command]
pub fn deck_list(state: State<'_, AppState>) -> CommandResult<Vec<StoredDeck>> {
    state.decks().list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn deck_get(state: State<'_, AppState>, id: u64) -> CommandResult<DeckView> {
    let id = DeckId(id);
    let deck = load_deck(&state, id)?;
    build_view(&state, id, deck)
}

#[tauri::command]
pub fn deck_create(state: State<'_, AppState>, name: String, format: String) -> CommandResult<u64> {
    let deck = Deck::new(name, parse_format(&format)?);
    state
        .decks()
        .create(&deck)
        .map(|id| id.0)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn deck_delete(state: State<'_, AppState>, id: u64) -> CommandResult<bool> {
    state.decks().delete(DeckId(id)).map_err(|e| e.to_string())
}

/// Renames a deck or changes its format.
#[tauri::command]
pub fn deck_rename(
    state: State<'_, AppState>,
    id: u64,
    name: String,
    format: String,
) -> CommandResult<DeckView> {
    let id = DeckId(id);
    let mut deck = load_deck(&state, id)?;
    deck.name = name;
    deck.format = parse_format(&format)?;
    state.decks().update(id, &deck).map_err(|e| e.to_string())?;
    build_view(&state, id, deck)
}

#[tauri::command]
pub fn deck_add_card(
    state: State<'_, AppState>,
    id: u64,
    oracle_id: String,
    quantity: u32,
    zone: String,
) -> CommandResult<DeckView> {
    let id = DeckId(id);
    let zone = parse_zone(&zone)?;
    let mut deck = load_deck(&state, id)?;

    // The name is denormalised into the entry, so it has to come from the catalog rather than
    // from the caller — otherwise a deck could hold a name that belongs to no card.
    let name = state.with_catalog(|catalog| {
        catalog
            .iter()
            .find(|(_, card)| card.oracle_id() == oracle_id)
            .map(|(_, card)| card.name().to_owned())
    })?;
    let name = name.ok_or_else(|| format!("no card with oracle id {oracle_id}"))?;

    deck.add(DeckEntry::new(oracle_id, name, quantity).in_zone(zone));
    state.decks().update(id, &deck).map_err(|e| e.to_string())?;
    build_view(&state, id, deck)
}

#[tauri::command]
pub fn deck_remove_card(
    state: State<'_, AppState>,
    id: u64,
    oracle_id: String,
    quantity: u32,
    zone: String,
) -> CommandResult<DeckView> {
    let id = DeckId(id);
    let zone = parse_zone(&zone)?;
    let mut deck = load_deck(&state, id)?;
    deck.remove(&oracle_id, zone, quantity);
    state.decks().update(id, &deck).map_err(|e| e.to_string())?;
    build_view(&state, id, deck)
}

/// Moves copies between zones, e.g. promoting a card to the command zone.
#[tauri::command]
pub fn deck_move_card(
    state: State<'_, AppState>,
    id: u64,
    oracle_id: String,
    quantity: u32,
    from: String,
    to: String,
) -> CommandResult<DeckView> {
    let id = DeckId(id);
    let from = parse_zone(&from)?;
    let to = parse_zone(&to)?;
    let mut deck = load_deck(&state, id)?;

    let Some(entry) = deck
        .entries_in(from)
        .find(|e| e.oracle_id == oracle_id)
        .cloned()
    else {
        return Err(format!("{oracle_id} is not in the {} zone", from.label()));
    };
    let moved = quantity.min(entry.quantity);

    deck.remove(&oracle_id, from, moved);
    deck.add(DeckEntry::new(&entry.oracle_id, &entry.name, moved).in_zone(to));
    state.decks().update(id, &deck).map_err(|e| e.to_string())?;
    build_view(&state, id, deck)
}

#[tauri::command]
pub fn deck_import(
    state: State<'_, AppState>,
    text: String,
    name: String,
    format: String,
) -> CommandResult<ImportOutcome> {
    let format = parse_format(&format)?;
    let result = state.with_catalog(|catalog| import(&text, &name, format, catalog))?;

    let id = state
        .decks()
        .create(&result.deck)
        .map_err(|e| e.to_string())?;

    Ok(ImportOutcome {
        view: build_view(&state, id, result.deck)?,
        messages: result.problems.iter().map(ToString::to_string).collect(),
        problems: result.problems,
    })
}

#[tauri::command]
pub fn deck_export(state: State<'_, AppState>, id: u64, style: String) -> CommandResult<String> {
    let deck = load_deck(&state, DeckId(id))?;
    Ok(export(&deck, parse_style(&style)?))
}

/// The deck zones the UI offers, as (key, label) pairs, served from Rust so the two cannot
/// drift apart.
/// One card of a deck, with what the board needs to place it.
///
/// The deck store keeps `oracle_id`, a name and a quantity — nothing about what the card costs
/// or looks like, and rightly so: that belongs to the catalog and would go stale in a saved deck.
/// This is the join, done at the moment the editor opens it.
#[derive(Debug, Serialize)]
pub struct BoardCard {
    pub oracle_id: String,
    pub name: String,
    pub quantity: u32,
    pub zone: String,
    /// The column this card belongs in. `None` when the catalog does not know the card.
    pub mana_value: Option<f32>,
    pub colors: String,
    pub type_line: String,
    /// True for lands, which get a column of their own — their cost says nothing about when
    /// they are played.
    pub is_land: bool,
    pub image_art: Option<String>,
}

/// A deck laid out for the board: every card, joined to the catalog.
///
/// Resolved by scanning the catalog once and keeping only the deck's oracle ids, rather than
/// looking each card up by name. Names are not keys — Scryfall has several cards sharing one —
/// and `CardId` shifts on every catalog rebuild, which is exactly why decks persist `oracle_id`
/// in the first place. One scan is ~5 ms over 35,306 cards and runs once per deck opened.
#[tauri::command]
pub fn deck_board(state: State<'_, AppState>, id: u64) -> CommandResult<Vec<BoardCard>> {
    let stored = state
        .decks()
        .get(DeckId(id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("there is no deck {id}"))?;

    let wanted: std::collections::HashSet<&str> = stored
        .deck
        .entries
        .iter()
        .map(|entry| entry.oracle_id.as_str())
        .collect();

    let found = state.with_catalog(|catalog| {
        let mut found = std::collections::HashMap::new();
        for (_, card) in catalog.iter() {
            let oracle = card.oracle_id();
            if wanted.contains(oracle) {
                found.insert(
                    oracle.to_owned(),
                    (
                        card.mana_value(),
                        card.colors().to_string(),
                        card.type_line().to_owned(),
                        card.type_line().contains("Land"),
                        crate::dto::image_url(&card.image_id, "art_crop"),
                    ),
                );
            }
        }
        found
    })?;

    Ok(stored
        .deck
        .entries
        .iter()
        .map(|entry| {
            let known = found.get(&entry.oracle_id);
            BoardCard {
                oracle_id: entry.oracle_id.clone(),
                name: entry.name.clone(),
                quantity: entry.quantity,
                zone: match entry.zone {
                    Zone::Main => "main",
                    Zone::Sideboard => "sideboard",
                    Zone::Command => "command",
                }
                .to_owned(),
                mana_value: known.map(|k| k.0),
                colors: known.map(|k| k.1.clone()).unwrap_or_default(),
                type_line: known.map(|k| k.2.clone()).unwrap_or_default(),
                is_land: known.is_some_and(|k| k.3),
                image_art: known.and_then(|k| k.4.clone()),
            }
        })
        .collect())
}

#[tauri::command]
pub fn deck_zones() -> Vec<(String, String)> {
    Zone::ALL
        .iter()
        .map(|zone| {
            let key = match zone {
                Zone::Main => "main",
                Zone::Sideboard => "sideboard",
                Zone::Command => "command",
            };
            (key.to_owned(), zone.label().to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_keys_round_trip_through_the_ui_representation() {
        for (key, _) in deck_zones() {
            assert!(parse_zone(&key).is_ok(), "{key}");
        }
        assert_eq!(deck_zones().len(), Zone::ALL.len());
    }

    #[test]
    fn unknown_zones_and_styles_are_rejected() {
        assert!(parse_zone("graveyard").is_err());
        assert!(parse_style("xml").is_err());
        assert!(parse_format("explorer").is_err());
    }
}
