//! Commands exposed to the frontend.
//!
//! These are deliberately thin. Every one of them translates arguments, calls into a crate and
//! converts the result — no rules, no scoring, no persistence logic lives here. That is what
//! keeps the domain testable with `cargo test` and no mobile toolchain; see `CLAUDE.md`.

use std::collections::HashMap;

use mtg_collection::{Holding, HoldingId, NewHolding, Pool, Stats};
use mtg_core::{ColorSet, Format};
use mtg_data::Query;
use tauri::State;

use crate::dto::{CardDetails, CardSummary, CatalogStatus, SearchRequest, SearchResponse};
use crate::state::AppState;

/// Errors are sent to the frontend as plain strings.
///
/// Nothing in the UI branches on an error *kind* — it shows the message — so a typed error
/// across the boundary would be ceremony without a reader.
type CommandResult<T> = Result<T, String>;

fn parse_pool(pool: Option<String>) -> CommandResult<Option<Pool>> {
    match pool.as_deref() {
        None | Some("") | Some("all") => Ok(None),
        Some("physical") => Ok(Some(Pool::Physical)),
        Some("digital") => Ok(Some(Pool::Digital)),
        Some(other) => Err(format!("unknown collection {other:?}")),
    }
}

#[tauri::command]
pub fn catalog_status(state: State<'_, AppState>) -> CatalogStatus {
    let path = state.catalog_path().display().to_string();
    match state.with_catalog(|catalog| (catalog.len(), catalog.source_updated_at().to_owned())) {
        Ok((cards, source_updated_at)) => CatalogStatus {
            loaded: true,
            cards,
            source_updated_at,
            path,
            error: None,
        },
        Err(_) => CatalogStatus {
            loaded: false,
            cards: 0,
            source_updated_at: String::new(),
            path,
            error: state.catalog_error(),
        },
    }
}

/// Re-reads the catalog from disk. Lets the user fix a missing artifact without restarting.
#[tauri::command]
pub fn reload_catalog(state: State<'_, AppState>) -> CatalogStatus {
    state.reload_catalog();
    catalog_status(state)
}

#[tauri::command]
pub fn search_cards(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> CommandResult<SearchResponse> {
    let mut query = Query::new();

    if let Some(text) = request
        .text
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        query = query.text(text);
    }
    for kind in request.card_types.iter().filter(|k| !k.trim().is_empty()) {
        query = query.card_type(kind.trim());
    }
    if let Some(identity) = request.identity.as_deref().filter(|i| !i.is_empty()) {
        query = query.identity_within(ColorSet::from_symbols(identity));
    }
    if let Some(format) = request.format.as_deref().filter(|f| !f.is_empty()) {
        let format = Format::from_scryfall_key(format)
            .ok_or_else(|| format!("unknown format {format:?}"))?;
        query = query.legal_in(format);
    }
    if let Some(min) = request.min_mana_value {
        query = query.mana_value_at_least(min);
    }
    if let Some(max) = request.max_mana_value {
        query = query.mana_value_at_most(max);
    }
    if request.game_changers_only {
        query = query.game_changer(true);
    }
    if request.commanders_only {
        query = query.can_be_commander(true);
    }

    // "Owned only" is applied after the catalog query rather than inside it, because the
    // catalog knows nothing about collections and should keep not knowing.
    let owned = if request.owned_only {
        Some(
            state
                .collection()
                .owned_quantities(None)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let limit = request.limit.unwrap_or(60).min(500);

    state.with_catalog(|catalog| {
        let matching = catalog.iter().filter(|(_, card)| {
            query.matches(card)
                && owned
                    .as_ref()
                    .is_none_or(|owned| owned.contains_key(card.oracle_id()))
        });

        let mut total = 0usize;
        let mut cards = Vec::new();
        for (_, card) in matching {
            total += 1;
            if cards.len() < limit {
                cards.push(CardSummary::from_archived(card));
            }
        }
        SearchResponse { total, cards }
    })
}

#[tauri::command]
pub fn card_details(state: State<'_, AppState>, oracle_id: String) -> CommandResult<CardDetails> {
    state.with_catalog(|catalog| {
        catalog
            .iter()
            .find(|(_, card)| card.oracle_id() == oracle_id)
            .map(|(_, card)| CardDetails::from_archived(card))
            .ok_or_else(|| format!("no card with oracle id {oracle_id}"))
    })?
}

#[tauri::command]
pub fn card_by_name(state: State<'_, AppState>, name: String) -> CommandResult<CardDetails> {
    state.with_catalog(|catalog| {
        catalog
            .find_by_name(&name)
            .map(|(_, card)| CardDetails::from_archived(card))
            .ok_or_else(|| format!("no card named {name:?}"))
    })?
}

#[tauri::command]
pub fn collection_list(
    state: State<'_, AppState>,
    pool: Option<String>,
) -> CommandResult<Vec<Holding>> {
    let pool = parse_pool(pool)?;
    state.collection().list(pool).map_err(|e| e.to_string())
}

/// One holding, joined to the catalog so the binder can show it.
///
/// The collection stores what you own — `oracle_id`, a printing, a condition, where it lives.
/// It deliberately stores nothing about what the card *looks* like, because that belongs to the
/// catalog and would go stale in a saved collection. This is that join, done once per view.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BinderCard {
    pub id: u64,
    pub oracle_id: String,
    pub name: String,
    pub quantity: u32,
    pub set_code: String,
    pub collector_number: String,
    pub finish: String,
    pub condition: String,
    /// Where it is kept. Empty when the holding says nothing, which is its own answer.
    pub container: String,
    pub section: String,
    pub slot: Option<u16>,
    pub colors: String,
    pub image_art: Option<String>,
}

/// The collection, joined to the catalog, for the binder pages.
///
/// One catalog scan (~5 ms over 35,306 cards) rather than a lookup per holding, and keyed on
/// `oracle_id` for the reason everything persisted is: `CardId` is a position in one artifact and
/// moves on every rebuild.
#[tauri::command]
pub fn collection_binder(
    state: State<'_, AppState>,
    pool: Option<String>,
) -> CommandResult<Vec<BinderCard>> {
    let pool = parse_pool(pool)?;
    let holdings = state.collection().list(pool).map_err(|e| e.to_string())?;

    let wanted: std::collections::HashSet<&str> =
        holdings.iter().map(|h| h.oracle_id.as_str()).collect();

    let found = state.with_catalog(|catalog| {
        let mut found = std::collections::HashMap::new();
        for (_, card) in catalog.iter() {
            let oracle = card.oracle_id();
            if wanted.contains(oracle) {
                found.insert(
                    oracle.to_owned(),
                    (
                        card.colors().to_string(),
                        crate::dto::image_url(&card.image_id, "art_crop"),
                    ),
                );
            }
        }
        found
    })?;

    Ok(holdings
        .into_iter()
        .map(|h| {
            let known = found.get(&h.oracle_id);
            BinderCard {
                id: h.id.0,
                name: h.name,
                quantity: h.quantity,
                set_code: h.set_code,
                collector_number: h.collector_number,
                finish: format!("{:?}", h.finish).to_lowercase(),
                condition: format!("{:?}", h.condition).to_lowercase(),
                container: h
                    .location
                    .as_ref()
                    .map(|l| l.container.clone())
                    .unwrap_or_default(),
                section: h
                    .location
                    .as_ref()
                    .and_then(|l| l.section.clone())
                    .unwrap_or_default(),
                slot: h.location.as_ref().and_then(|l| l.slot),
                colors: known.map(|k| k.0.clone()).unwrap_or_default(),
                image_art: known.and_then(|k| k.1.clone()),
                oracle_id: h.oracle_id,
            }
        })
        .collect())
}

#[tauri::command]
pub fn collection_add(state: State<'_, AppState>, holding: NewHolding) -> CommandResult<u64> {
    state
        .collection()
        .add(holding)
        .map(|id| id.0)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn collection_set_quantity(
    state: State<'_, AppState>,
    id: u64,
    quantity: u32,
) -> CommandResult<()> {
    state
        .collection()
        .set_quantity(HoldingId(id), quantity)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn collection_update(state: State<'_, AppState>, holding: Holding) -> CommandResult<()> {
    state
        .collection()
        .replace(holding)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn collection_remove(state: State<'_, AppState>, id: u64) -> CommandResult<bool> {
    state
        .collection()
        .remove(HoldingId(id))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn collection_stats(state: State<'_, AppState>, pool: Option<String>) -> CommandResult<Stats> {
    let pool = parse_pool(pool)?;
    state.collection().stats(pool).map_err(|e| e.to_string())
}

/// Owned counts for every card at once.
///
/// The UI calls this once per view and looks results up locally, rather than asking per row —
/// which would rescan the whole collection for every line on screen.
#[tauri::command]
pub fn collection_owned_quantities(
    state: State<'_, AppState>,
    pool: Option<String>,
) -> CommandResult<HashMap<String, u32>> {
    let pool = parse_pool(pool)?;
    state
        .collection()
        .owned_quantities(pool)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn collection_containers(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    state.collection().containers().map_err(|e| e.to_string())
}

/// The formats the UI offers, as (key, display name) pairs.
///
/// Served from `mtg_core::Format` rather than hardcoded in TypeScript, so the list cannot drift
/// away from what the catalog actually stores.
#[tauri::command]
pub fn formats() -> Vec<(String, String)> {
    Format::ALL
        .iter()
        .map(|f| (f.scryfall_key().to_owned(), f.display_name().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_parsing_accepts_the_ui_values() {
        assert_eq!(parse_pool(None).unwrap_or_default(), None);
        assert_eq!(parse_pool(Some("all".into())).unwrap_or_default(), None);
        assert_eq!(
            parse_pool(Some("physical".into())).unwrap_or_default(),
            Some(Pool::Physical)
        );
        assert_eq!(
            parse_pool(Some("digital".into())).unwrap_or_default(),
            Some(Pool::Digital)
        );
    }

    #[test]
    fn unknown_pools_are_rejected() {
        assert!(parse_pool(Some("arena".into())).is_err());
    }

    #[test]
    fn every_format_is_offered_to_the_ui() {
        let offered = formats();
        assert_eq!(offered.len(), Format::COUNT);
        assert!(offered.iter().any(|(key, _)| key == "commander"));
    }
}
