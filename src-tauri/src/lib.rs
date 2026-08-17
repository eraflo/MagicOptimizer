//! The MagicOptimizer application shell.
//!
//! Everything of substance lives in the `mtg-*` crates. This layer only wires them to a window.

mod commands;
mod deck_commands;
mod dto;
mod optimize_commands;
mod state;

use tauri::Manager;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
// The one place a panic is the right answer: if the window cannot be created there is nothing
// to show an error in, and no caller to hand it to. Everywhere else the workspace forbids
// `expect` — see the conventions in CLAUDE.md.
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = AppState::new(&data_dir).map_err(std::io::Error::other)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::catalog_status,
            commands::reload_catalog,
            commands::search_cards,
            commands::card_details,
            commands::card_by_name,
            commands::collection_list,
            commands::collection_add,
            commands::collection_set_quantity,
            commands::collection_update,
            commands::collection_remove,
            commands::collection_stats,
            commands::collection_owned_quantities,
            commands::collection_containers,
            commands::formats,
            deck_commands::deck_list,
            deck_commands::deck_get,
            deck_commands::deck_create,
            deck_commands::deck_delete,
            deck_commands::deck_rename,
            deck_commands::deck_add_card,
            deck_commands::deck_remove_card,
            deck_commands::deck_move_card,
            deck_commands::deck_import,
            deck_commands::deck_export,
            deck_commands::deck_zones,
            optimize_commands::deck_score,
            optimize_commands::deck_optimize,
            optimize_commands::deck_apply_suggestion,
            optimize_commands::optimizer_options,
        ])
        .run(tauri::generate_context!())
        .expect("the application failed to start");
}
