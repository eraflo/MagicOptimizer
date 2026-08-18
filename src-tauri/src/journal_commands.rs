//! Game log commands.
//!
//! The log is filled in **after** a game — invariant 2 in `CLAUDE.md` — by someone who would
//! rather be shuffling for the next one. That shapes the API: recording a game needs a deck, a
//! date and a result, and everything else is optional.
//!
//! Every rate that crosses this boundary carries its uncertainty with it. `mtg-journal` will not
//! produce a bare percentage, and nothing here is allowed to throw one away on the way out.

use mtg_journal::{
    before_and_after, matchups, BeforeAfter, Game, GameId, Matchup, NewGame, Result_, WinRate,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

/// What the UI sends when recording a game.
///
/// Mirrors [`NewGame`] rather than reusing it, so the wire shape can stay camelCase and this
/// layer keeps its own validation without the domain crate growing serde attributes for a UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInput {
    pub deck_id: u64,
    pub played_at: String,
    #[serde(default)]
    pub format: String,
    /// `win`, `loss` or `draw`.
    pub result: String,
    /// Opponent archetypes, as typed. Blank entries are dropped rather than stored.
    #[serde(default)]
    pub opponents: Vec<String>,
    #[serde(default)]
    pub on_the_play: Option<bool>,
    #[serde(default)]
    pub mulligans: Option<u32>,
    #[serde(default)]
    pub ended_on_turn: Option<u32>,
    #[serde(default)]
    pub notes: String,
}

/// Everything one deck's history says, in one call.
///
/// One command rather than four, because the journal view shows all of it at once and four
/// round trips would only give it four chances to be inconsistent with itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckHistory {
    pub deck_id: u64,
    pub games: Vec<Game>,
    pub overall: WinRate,
    pub matchups: Vec<Matchup>,
    /// Present only when a date was asked about.
    pub change: Option<BeforeAfter>,
}

fn parse_result(value: &str) -> CommandResult<Result_> {
    match value {
        "win" => Ok(Result_::Win),
        "loss" => Ok(Result_::Loss),
        "draw" => Ok(Result_::Draw),
        other => Err(format!(
            "{other:?} is not a result; expected win, loss or draw"
        )),
    }
}

/// Rejects a date the log could not sort.
///
/// `YYYY-MM-DD` is the only shape anything here understands: the store orders on it as a plain
/// string, and `before_and_after` splits on it the same way. A malformed date would not error,
/// it would silently sort into the wrong place — so it is refused at the door.
fn check_date(value: &str) -> CommandResult<()> {
    let bytes = value.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());

    if shaped {
        Ok(())
    } else {
        Err(format!("{value:?} is not a date of the form YYYY-MM-DD"))
    }
}

#[tauri::command]
pub fn journal_add(state: State<'_, AppState>, game: GameInput) -> CommandResult<u64> {
    check_date(&game.played_at)?;
    let result = parse_result(&game.result)?;

    let mut new = NewGame::new(game.deck_id, game.played_at, result);
    new.format = game.format;
    new.on_the_play = game.on_the_play;
    new.mulligans = game.mulligans;
    new.ended_on_turn = game.ended_on_turn;
    new.notes = game.notes;
    for archetype in game.opponents {
        let archetype = archetype.trim();
        if !archetype.is_empty() {
            new = new.against(archetype);
        }
    }

    state
        .journal()
        .add(new)
        .map(|id| id.0)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn journal_remove(state: State<'_, AppState>, id: u64) -> CommandResult<bool> {
    state
        .journal()
        .remove(GameId(id))
        .map_err(|e| e.to_string())
}

/// Every game, most recent first, across all decks.
#[tauri::command]
pub fn journal_list(state: State<'_, AppState>) -> CommandResult<Vec<Game>> {
    state.journal().all().map_err(|e| e.to_string())
}

/// One deck's history and everything derived from it.
#[tauri::command]
pub fn journal_deck_history(
    state: State<'_, AppState>,
    deck_id: u64,
    since: Option<String>,
) -> CommandResult<DeckHistory> {
    let games = state
        .journal()
        .for_deck(deck_id)
        .map_err(|e| e.to_string())?;

    let change = match since {
        Some(date) => {
            check_date(&date)?;
            Some(before_and_after(&games, &date))
        }
        None => None,
    };

    Ok(DeckHistory {
        deck_id,
        overall: WinRate::of(games.iter().map(|game| game.result)),
        matchups: matchups(&games),
        change,
        games,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_the_ui_could_not_have_sent_is_refused() {
        assert!(parse_result("win").is_ok());
        assert!(parse_result("Win").is_err(), "the wire form is lowercase");
        assert!(parse_result("").is_err());
    }

    #[test]
    fn a_malformed_date_is_refused_rather_than_stored() {
        // The store and `before_and_after` both order on this as a plain string. A date in
        // another shape would not fail, it would sort into the wrong place — and a log that
        // silently reorders itself is worse than one that refuses an entry.
        assert!(check_date("2026-08-18").is_ok());
        for bad in [
            "18/08/2026",
            "2026-8-18",
            "2026-08-18T10:00",
            "",
            "yesterday",
        ] {
            assert!(check_date(bad).is_err(), "{bad} was accepted");
        }
    }
}
