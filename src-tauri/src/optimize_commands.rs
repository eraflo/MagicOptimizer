//! Optimizer commands.
//!
//! Thin, like the rest. The one thing that genuinely belongs here is turning the UI's choice
//! of card pool into a set of oracle ids: `mtg-optimizer` deliberately knows nothing about
//! collections, so somebody has to bridge the two, and this is the layer that already knows
//! about both.

use std::collections::HashSet;

use mtg_collection::Pool;
use mtg_deck::{DeckEntry, DeckId, Zone};
use mtg_optimizer::{
    profile_with_index, score, search, Archetype, CardIndex, CardPool, Score, SearchResult,
    SearchSettings,
};
use tauri::State;

use crate::deck_commands::DeckView;
use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

fn parse_archetype(archetype: &str) -> CommandResult<Archetype> {
    match archetype {
        "aggro" => Ok(Archetype::Aggro),
        "midrange" => Ok(Archetype::Midrange),
        "control" => Ok(Archetype::Control),
        other => Err(format!("unknown archetype {other:?}")),
    }
}

/// Turns the UI's pool choice into the set of cards the search may reach for.
///
/// This is the "only cards I own" toggle. `owned` spans both collections; `owned_physical`
/// covers the cards actually in front of you, which is the one that matters when you are
/// building something to play tonight.
fn build_pool(state: &AppState, pool: &str) -> CommandResult<CardPool> {
    let restrict_to = match pool {
        "everything" => return Ok(CardPool::Everything),
        "owned" => None,
        "owned_physical" => Some(Pool::Physical),
        "owned_digital" => Some(Pool::Digital),
        other => return Err(format!("unknown card pool {other:?}")),
    };

    let owned = state
        .collection()
        .owned_quantities(restrict_to)
        .map_err(|e| e.to_string())?;

    Ok(CardPool::Only {
        oracle_ids: owned.into_keys().collect::<HashSet<String>>(),
    })
}

/// Scores a deck without searching. Cheap enough to call whenever the deck changes.
#[tauri::command]
pub fn deck_score(state: State<'_, AppState>, id: u64, archetype: String) -> CommandResult<Score> {
    let archetype = parse_archetype(&archetype)?;
    let deck = crate::deck_commands::load_deck(&state, DeckId(id))?;

    state.with_catalog(|catalog| {
        let index = CardIndex::build(catalog);
        let profile = profile_with_index(&deck, &index);
        let mut settings = mtg_optimizer::ScoreSettings::for_deck_size(profile.deck_size());
        settings.archetype = archetype;
        score(&profile, settings)
    })
}

/// Looks for improvements. Returns suggestions rather than a rewritten deck.
#[tauri::command]
pub fn deck_optimize(
    state: State<'_, AppState>,
    id: u64,
    archetype: String,
    pool: String,
    iterations: Option<u32>,
    only_played_cards: Option<bool>,
) -> CommandResult<SearchResult> {
    let archetype = parse_archetype(&archetype)?;
    let card_pool = build_pool(&state, &pool)?;
    let deck = crate::deck_commands::load_deck(&state, DeckId(id))?;

    state.with_catalog(|catalog| {
        let index = CardIndex::build(catalog);
        let deck_size = profile_with_index(&deck, &index).deck_size();

        let mut settings = SearchSettings::for_deck_size(deck_size);
        settings.score.archetype = archetype;
        settings.pool = card_pool;
        // Capped: this runs on the UI thread's command worker, and an unbounded value from
        // the frontend would freeze the window.
        settings.iterations = iterations.unwrap_or(1_200).clamp(50, 5_000);
        if let Some(only_played) = only_played_cards {
            settings.only_played_cards = only_played;
        }

        search(&deck, &index, &settings)
    })
}

/// Applies one suggestion: one copy out, one copy in.
///
/// Suggestions are a diff rather than a replay of the search's path, so they can be applied
/// in any order and any subset — see `mtg_optimizer::search`.
#[tauri::command]
pub fn deck_apply_suggestion(
    state: State<'_, AppState>,
    id: u64,
    remove_oracle_id: String,
    add_oracle_id: String,
) -> CommandResult<DeckView> {
    let id = DeckId(id);
    let mut deck = crate::deck_commands::load_deck(&state, id)?;

    // The name comes from the catalog, never from the caller, so a deck cannot end up holding
    // a name that belongs to no card.
    let name = state.with_catalog(|catalog| {
        catalog
            .iter()
            .find(|(_, card)| card.oracle_id() == add_oracle_id)
            .map(|(_, card)| card.name().to_owned())
    })?;
    let name = name.ok_or_else(|| format!("no card with oracle id {add_oracle_id}"))?;

    deck.remove(&remove_oracle_id, Zone::Main, 1);
    deck.add(DeckEntry::new(&add_oracle_id, name, 1));

    state.decks().update(id, &deck).map_err(|e| e.to_string())?;
    crate::deck_commands::build_view(&state, id, deck)
}

/// One choice offered by a dropdown.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Choice {
    pub key: String,
    pub label: String,
}

fn choice(key: &str, label: &str) -> Choice {
    Choice {
        key: key.to_owned(),
        label: label.to_owned(),
    }
}

/// The archetypes and pools the UI offers, served from Rust so the two cannot drift apart.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizerOptions {
    pub archetypes: Vec<Choice>,
    pub pools: Vec<Choice>,
}

#[tauri::command]
pub fn optimizer_options() -> OptimizerOptions {
    OptimizerOptions {
        archetypes: Archetype::ALL
            .iter()
            .map(|a| choice(&a.label().to_lowercase(), a.label()))
            .collect(),
        pools: vec![
            choice("everything", "Any card"),
            choice("owned", "Cards I own"),
            choice("owned_physical", "Physical cards only"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archetype_keys_round_trip_through_the_ui_representation() {
        let options = optimizer_options();
        for choice in &options.archetypes {
            assert!(parse_archetype(&choice.key).is_ok(), "{}", choice.key);
        }
        assert_eq!(options.archetypes.len(), Archetype::ALL.len());
        // Every pool the UI offers must be one build_pool accepts.
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(dir.path()).expect("state");
        for choice in &options.pools {
            assert!(build_pool(&state, &choice.key).is_ok(), "{}", choice.key);
        }
    }

    #[test]
    fn unknown_archetypes_and_pools_are_rejected() {
        assert!(parse_archetype("combo").is_err());

        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(dir.path()).expect("state");
        assert!(build_pool(&state, "wishlist").is_err());
    }

    #[test]
    fn the_everything_pool_needs_no_collection_lookup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(dir.path()).expect("state");
        assert_eq!(
            build_pool(&state, "everything").expect("pool"),
            CardPool::Everything
        );
    }

    #[test]
    fn an_empty_collection_gives_an_empty_pool_rather_than_everything() {
        // The failure that would matter: silently widening "cards I own" to the whole catalog
        // would suggest cards the user cannot play.
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(dir.path()).expect("state");
        match build_pool(&state, "owned").expect("pool") {
            CardPool::Only { oracle_ids } => assert!(oracle_ids.is_empty()),
            CardPool::Everything => panic!("an empty collection must not mean everything"),
        }
    }

    #[test]
    fn the_owned_pool_reflects_the_collection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(dir.path()).expect("state");
        state
            .collection()
            .add(mtg_collection::NewHolding::single(
                Pool::Physical,
                "oracle-sol-ring",
                "Sol Ring",
            ))
            .expect("add");

        match build_pool(&state, "owned").expect("pool") {
            CardPool::Only { oracle_ids } => {
                assert!(oracle_ids.contains("oracle-sol-ring"));
                assert_eq!(oracle_ids.len(), 1);
            }
            CardPool::Everything => panic!("expected a restricted pool"),
        }

        // And the digital pool does not see a physical card.
        match build_pool(&state, "owned_digital").expect("pool") {
            CardPool::Only { oracle_ids } => assert!(oracle_ids.is_empty()),
            CardPool::Everything => panic!("expected a restricted pool"),
        }
    }
}
