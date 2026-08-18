//! Backup and transfer, through a file.
//!
//! # Why a file, and not a server
//!
//! Invariant 7 in `CLAUDE.md`: no account, no telemetry, nothing leaving the device. That rules
//! out the usual answer to "sync my phone and my PC", and the user guide has always said so —
//! syncing here means writing a file and opening it on the other machine.
//!
//! It also fixes something more pressing than sync. A collection, a deck list and a game log
//! live in three redb files in one directory on one device, and until now there was no way to
//! get them out. A disk failure lost everything, and the game log in particular cannot be
//! rebuilt from anything: nobody remembers last March's games.
//!
//! # Why import refuses by default
//!
//! Merging two collections correctly is a real problem — the same four Lightning Bolts entered
//! on two devices are either four cards or eight, and nothing in the file says which. Rather
//! than guess, an import into a store that already holds something is **refused** unless the
//! caller says explicitly what should happen. A backup that silently doubles a collection is
//! worse than no backup at all.

use mtg_collection::{Holding, NewHolding};
use mtg_deck::StoredDeck;
use mtg_journal::{Game, NewGame};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

/// Bumped when the file's shape changes in a way an older build could not read.
const BACKUP_VERSION: u32 = 1;

/// Everything the app holds that the user created.
///
/// Card data, artwork hashes and the combo snapshot are deliberately absent: they are rebuilt
/// from public sources in minutes, and putting a 26 MB catalog inside a backup would make the
/// backup something people avoid taking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub version: u32,
    /// When it was taken, `YYYY-MM-DD`, so a directory of them is readable at a glance.
    pub taken_at: String,
    pub holdings: Vec<Holding>,
    pub decks: Vec<StoredDeck>,
    pub games: Vec<Game>,
}

/// What an import did, or would have done.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub holdings: usize,
    pub decks: usize,
    pub games: usize,
}

/// What is already in the app, so the UI can warn before an import overwrites nothing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub holdings: usize,
    pub decks: usize,
    pub games: usize,
    pub empty: bool,
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> CommandResult<SyncStatus> {
    let holdings = state
        .collection()
        .list(None)
        .map_err(|e| e.to_string())?
        .len();
    let decks = state.decks().list().map_err(|e| e.to_string())?.len();
    let games = state.journal().all().map_err(|e| e.to_string())?.len();

    Ok(SyncStatus {
        holdings,
        decks,
        games,
        empty: holdings == 0 && decks == 0 && games == 0,
    })
}

/// Everything the user has, as a JSON string for the frontend to save.
///
/// Returned as text rather than written to a path from here: choosing where a file goes is the
/// platform's job, and a command that writes wherever it is told is a command that can be told
/// to write anywhere.
#[tauri::command]
pub fn sync_export(state: State<'_, AppState>) -> CommandResult<String> {
    let backup = Backup {
        version: BACKUP_VERSION,
        taken_at: today(),
        holdings: state.collection().list(None).map_err(|e| e.to_string())?,
        decks: state.decks().list().map_err(|e| e.to_string())?,
        games: state.journal().all().map_err(|e| e.to_string())?,
    };

    // Pretty-printed on purpose. This is the user's own data and the only copy of some of it;
    // being able to read it in any editor is worth more than the bytes.
    serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
}

/// Reads a backup back in.
///
/// `force` is required when anything is already stored. Without it the command refuses and says
/// what is in the way, because the alternative — merging two collections by guessing — produces
/// a plausible-looking result that is quietly wrong.
#[tauri::command]
pub fn sync_import(
    state: State<'_, AppState>,
    contents: String,
    force: bool,
) -> CommandResult<ImportSummary> {
    let backup: Backup = serde_json::from_str(&contents)
        .map_err(|e| format!("this does not look like a MagicOptimizer backup: {e}"))?;

    if backup.version > BACKUP_VERSION {
        return Err(format!(
            "this backup was written by a newer version of the app (format {} against {BACKUP_VERSION})",
            backup.version
        ));
    }

    let status = sync_status(state.clone())?;
    if !status.empty && !force {
        return Err(format!(
            "there is already data here — {} holdings, {} decks, {} games. Importing would add \
             to it rather than replace it, and the same cards entered twice would count twice. \
             Export what is here first if you want to keep it.",
            status.holdings, status.decks, status.games
        ));
    }

    let mut summary = ImportSummary {
        holdings: 0,
        decks: 0,
        games: 0,
    };

    for holding in backup.holdings {
        let new = NewHolding {
            pool: holding.pool,
            oracle_id: holding.oracle_id,
            name: holding.name,
            set_code: holding.set_code,
            collector_number: holding.collector_number,
            language: holding.language,
            finish: holding.finish,
            condition: holding.condition,
            quantity: holding.quantity,
            location: holding.location,
            notes: holding.notes,
        };
        state.collection().add(new).map_err(|e| e.to_string())?;
        summary.holdings += 1;
    }

    for stored in backup.decks {
        // Recreated rather than restored to its old id: ids belong to this database, and two
        // devices will have handed the same number to different decks.
        state
            .decks()
            .create(&stored.deck)
            .map_err(|e| e.to_string())?;
        summary.decks += 1;
    }

    for game in backup.games {
        let mut new = NewGame::new(game.deck_id, game.played_at, game.result);
        new.format = game.format;
        new.opponents = game.opponents;
        new.on_the_play = game.on_the_play;
        new.mulligans = game.mulligans;
        new.ended_on_turn = game.ended_on_turn;
        new.notes = game.notes;
        state.journal().add(new).map_err(|e| e.to_string())?;
        summary.games += 1;
    }

    Ok(summary)
}

/// Today, as `YYYY-MM-DD`.
///
/// The same civil-from-days conversion `build-artifacts` uses, for the same reason: the stamp
/// only has to say roughly when, and that is not worth a date library.
fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let z = (seconds / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;

    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_written_today_carries_a_sortable_date() {
        let stamp = today();
        assert_eq!(stamp.len(), 10);
        assert_eq!(stamp.as_bytes()[4], b'-');
        assert!(stamp.starts_with("20"));
    }

    #[test]
    fn an_empty_backup_round_trips() {
        // The shape has to survive JSON on its own before anything is put in it.
        let backup = Backup {
            version: BACKUP_VERSION,
            taken_at: today(),
            holdings: Vec::new(),
            decks: Vec::new(),
            games: Vec::new(),
        };
        let text = serde_json::to_string(&backup).expect("encode");
        let read: Backup = serde_json::from_str(&text).expect("decode");
        assert_eq!(read.version, BACKUP_VERSION);
    }

    #[test]
    fn something_that_is_not_a_backup_is_refused_with_a_readable_reason() {
        // Someone will point this at the wrong file, and "expected value at line 1" is not an
        // explanation.
        let error = serde_json::from_str::<Backup>("{\"hello\": true}")
            .map_err(|e| format!("this does not look like a MagicOptimizer backup: {e}"))
            .unwrap_err();
        assert!(
            error.contains("does not look like a MagicOptimizer backup"),
            "{error}"
        );
    }
}
