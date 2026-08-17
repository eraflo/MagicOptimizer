//! Combo and bracket commands.

use mtg_combo::{assess, BracketAssessment, ComboIndex, ComboMatch};
use mtg_deck::DeckId;
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

/// What the app knows about its combo data.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComboStatus {
    pub loaded: bool,
    pub combos: usize,
    /// When the snapshot was taken, so the UI can say how stale it is.
    pub fetched_at: String,
    pub path: String,
    /// Why loading failed, when it did. The artifact is optional, so this is not an error
    /// state — the UI says the check was skipped rather than that the deck is clean.
    pub error: Option<String>,
}

#[tauri::command]
pub fn combo_status(state: State<'_, AppState>) -> ComboStatus {
    let path = state.combo_path().display().to_string();
    match state.with_combos(|database| (database.len(), database.fetched_at().to_owned())) {
        Some((combos, fetched_at)) => ComboStatus {
            loaded: true,
            combos,
            fetched_at,
            path,
            error: None,
        },
        None => ComboStatus {
            loaded: false,
            combos: 0,
            fetched_at: String::new(),
            path,
            error: state.combo_error(),
        },
    }
}

/// Every combo the deck already contains.
#[tauri::command]
pub fn deck_combos(state: State<'_, AppState>, id: u64) -> CommandResult<Vec<ComboMatch>> {
    let deck = crate::deck_commands::load_deck(&state, DeckId(id))?;
    Ok(state
        .with_combos(|database| ComboIndex::build(database).find_in(&deck))
        .unwrap_or_default())
}

/// Estimates which Commander bracket the deck belongs to.
#[tauri::command]
pub fn deck_bracket(state: State<'_, AppState>, id: u64) -> CommandResult<BracketAssessment> {
    let deck = crate::deck_commands::load_deck(&state, DeckId(id))?;

    // The combo database is optional. `assess` is handed `None` when it is missing and says so
    // in its caveats, rather than reporting a deck as clean on a check that never ran.
    state.with_catalog(|catalog| state.with_combos_ref(|combos| assess(&deck, catalog, combos)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_reports_no_combo_data_without_erroring() {
        // The artifact is an optional download; its absence is a state, not a failure.
        //
        // `without_artifacts` rather than `new`, because `new` falls back to `artifacts/` in the
        // checkout — this test used to pass only because nobody had built the combo snapshot yet.
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::without_artifacts(dir.path()).expect("state");

        assert!(state.with_combos(|db| db.len()).is_none());
        assert!(
            state.combo_error().is_some(),
            "the reason should be recorded"
        );
    }
}
