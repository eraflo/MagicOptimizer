//! Persistence for the game log, on top of [`redb`].
//!
//! Same shape as the collection store, for the same reason: redb is pure Rust and embedded, so
//! it cross-compiles to Android without a C toolchain. See `docs/dev/architecture.md`.
//!
//! There is no index. A log is a few games a week, and someone playing every day for a decade
//! reaches a few thousand rows — a full scan there costs less than opening the database. An
//! index would be code and a second thing to keep correct, against a cost nobody can measure.

use std::path::Path;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::game::{Game, GameId, NewGame};

/// Games, keyed by id, stored as JSON.
///
/// JSON rather than a compact encoding, exactly as the collection does: this is the user's own
/// history, it cannot be regenerated from anywhere, and being able to read it out with any tool
/// is worth far more than the bytes saved.
const GAMES: TableDefinition<u64, &[u8]> = TableDefinition::new("games");

const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const NEXT_ID: &str = "next_game_id";

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("the game log could not be opened: {0}")]
    Database(String),
    #[error("no game with id {0}")]
    NotFound(u64),
    #[error("a stored game could not be read: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, JournalError>;

pub struct JournalStore {
    db: Database,
}

impl JournalStore {
    pub fn open(path: impl AsRef<Path>) -> Result<JournalStore> {
        let db = Database::create(path.as_ref()).map_err(db_error)?;
        let store = JournalStore { db };
        store.ensure_tables()?;
        Ok(store)
    }

    /// Creates the tables so later read transactions never fail on a missing one.
    fn ensure_tables(&self) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_error)?;
        {
            txn.open_table(GAMES).map_err(db_error)?;
            txn.open_table(META).map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(())
    }

    /// Records a game and returns its id.
    pub fn add(&self, game: NewGame) -> Result<GameId> {
        let txn = self.db.begin_write().map_err(db_error)?;
        let id;
        {
            let mut meta = txn.open_table(META).map_err(db_error)?;
            let next = meta
                .get(NEXT_ID)
                .map_err(db_error)?
                .map(|value| value.value())
                .unwrap_or(1);
            id = GameId(next);
            meta.insert(NEXT_ID, next + 1).map_err(db_error)?;

            let stored = game.with_id(id);
            let encoded =
                serde_json::to_vec(&stored).map_err(|e| JournalError::Corrupt(e.to_string()))?;
            let mut games = txn.open_table(GAMES).map_err(db_error)?;
            games.insert(next, encoded.as_slice()).map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(id)
    }

    pub fn get(&self, id: GameId) -> Result<Option<Game>> {
        let txn = self.db.begin_read().map_err(db_error)?;
        let games = txn.open_table(GAMES).map_err(db_error)?;
        match games.get(id.0).map_err(db_error)? {
            Some(value) => Ok(Some(decode(value.value())?)),
            None => Ok(None),
        }
    }

    /// Every game, most recent first.
    ///
    /// Sorted on the date rather than the id, because games are often entered days after they
    /// were played and a log ordered by when it was typed is not a history of anything.
    pub fn all(&self) -> Result<Vec<Game>> {
        let txn = self.db.begin_read().map_err(db_error)?;
        let games = txn.open_table(GAMES).map_err(db_error)?;

        let mut out = Vec::new();
        for row in games.iter().map_err(db_error)? {
            let (_, value) = row.map_err(db_error)?;
            out.push(decode(value.value())?);
        }
        out.sort_by(|a, b| {
            b.played_at
                .cmp(&a.played_at)
                .then_with(|| b.id.0.cmp(&a.id.0))
        });
        Ok(out)
    }

    /// Every game played with one deck, most recent first.
    pub fn for_deck(&self, deck_id: u64) -> Result<Vec<Game>> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|game| game.deck_id == deck_id)
            .collect())
    }

    /// Replaces a game, keeping its id.
    pub fn update(&self, game: Game) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_error)?;
        {
            let mut games = txn.open_table(GAMES).map_err(db_error)?;
            if games.get(game.id.0).map_err(db_error)?.is_none() {
                return Err(JournalError::NotFound(game.id.0));
            }
            let encoded =
                serde_json::to_vec(&game).map_err(|e| JournalError::Corrupt(e.to_string()))?;
            games
                .insert(game.id.0, encoded.as_slice())
                .map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(())
    }

    pub fn remove(&self, id: GameId) -> Result<bool> {
        let txn = self.db.begin_write().map_err(db_error)?;
        let existed;
        {
            let mut games = txn.open_table(GAMES).map_err(db_error)?;
            existed = games.remove(id.0).map_err(db_error)?.is_some();
        }
        txn.commit().map_err(db_error)?;
        Ok(existed)
    }

    pub fn len(&self) -> Result<usize> {
        Ok(self.all()?.len())
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

fn decode(bytes: &[u8]) -> Result<Game> {
    serde_json::from_slice(bytes).map_err(|e| JournalError::Corrupt(e.to_string()))
}

fn db_error(error: impl std::fmt::Display) -> JournalError {
    JournalError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Result_;

    fn store() -> (JournalStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = JournalStore::open(dir.path().join("journal.redb")).expect("open");
        (store, dir)
    }

    #[test]
    fn a_recorded_game_comes_back() {
        let (store, _dir) = store();
        let id = store
            .add(NewGame::new(1, "2026-08-18", Result_::Win).against("Atraxa"))
            .expect("add");

        let game = store.get(id).expect("get").expect("present");
        assert_eq!(game.result, Result_::Win);
        assert_eq!(game.opponents[0].archetype, "Atraxa");
    }

    #[test]
    fn ids_do_not_get_reused_after_a_deletion() {
        // Reusing one would attach an old game's identity to a new game, and anything holding
        // the id — a note, an export, a screenshot — would quietly point at the wrong evening.
        let (store, _dir) = store();
        let first = store
            .add(NewGame::new(1, "2026-01-01", Result_::Win))
            .expect("add");
        assert!(store.remove(first).expect("remove"));
        let second = store
            .add(NewGame::new(1, "2026-01-02", Result_::Loss))
            .expect("add");
        assert_ne!(first, second);
    }

    #[test]
    fn the_log_is_ordered_by_when_games_were_played_not_when_they_were_typed() {
        // Games get entered days later, in whatever order someone remembers them. A log sorted
        // by insertion is not a history.
        let (store, _dir) = store();
        store
            .add(NewGame::new(1, "2026-03-01", Result_::Win))
            .expect("add");
        store
            .add(NewGame::new(1, "2026-01-01", Result_::Loss))
            .expect("add");
        store
            .add(NewGame::new(1, "2026-02-01", Result_::Draw))
            .expect("add");

        let dates: Vec<String> = store
            .all()
            .expect("all")
            .into_iter()
            .map(|g| g.played_at)
            .collect();
        assert_eq!(dates, ["2026-03-01", "2026-02-01", "2026-01-01"]);
    }

    #[test]
    fn games_are_filtered_by_deck() {
        let (store, _dir) = store();
        store
            .add(NewGame::new(1, "2026-01-01", Result_::Win))
            .expect("add");
        store
            .add(NewGame::new(2, "2026-01-02", Result_::Loss))
            .expect("add");

        assert_eq!(store.for_deck(1).expect("deck 1").len(), 1);
        assert_eq!(store.for_deck(2).expect("deck 2").len(), 1);
        assert!(store.for_deck(99).expect("deck 99").is_empty());
    }

    #[test]
    fn a_games_record_of_a_deleted_deck_survives() {
        // The deck is gone, but the evening happened. Cascading the delete would erase history
        // to tidy up a foreign key.
        let (store, _dir) = store();
        store
            .add(NewGame::new(42, "2026-01-01", Result_::Win))
            .expect("add");
        assert_eq!(store.for_deck(42).expect("games").len(), 1);
    }

    #[test]
    fn updating_a_game_that_does_not_exist_is_an_error_rather_than_an_insert() {
        let (store, _dir) = store();
        let ghost = NewGame::new(1, "2026-01-01", Result_::Win).with_id(GameId(999));
        assert!(matches!(
            store.update(ghost),
            Err(JournalError::NotFound(999))
        ));
    }

    #[test]
    fn a_fresh_log_is_empty_rather_than_an_error() {
        let (store, _dir) = store();
        assert!(store.is_empty().expect("empty"));
        assert!(store.all().expect("all").is_empty());
    }

    #[test]
    fn the_log_survives_being_closed_and_reopened() {
        // It is the user's own history and the only copy of it.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journal.redb");
        {
            let store = JournalStore::open(&path).expect("open");
            store
                .add(NewGame::new(1, "2026-08-18", Result_::Win))
                .expect("add");
        }
        let reopened = JournalStore::open(&path).expect("reopen");
        assert_eq!(reopened.len().expect("len"), 1);
    }
}
