//! Persistence for decks.
//!
//! Same shape as the collection store, and for the same reason: [`redb`] is pure Rust, so it
//! cross-compiles to Android without a C toolchain.

use std::path::Path;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::deck::Deck;

/// Decks, keyed by id, stored as JSON so the user's own data stays readable with any tool.
const DECKS: TableDefinition<u64, &[u8]> = TableDefinition::new("decks");
const META: TableDefinition<&str, u64> = TableDefinition::new("deck_meta");

const NEXT_ID: &str = "next_deck_id";

/// Identifier of a deck within one database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DeckId(pub u64);

impl std::fmt::Display for DeckId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A stored deck: the deck itself plus its identity in the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredDeck {
    pub id: DeckId,
    #[serde(flatten)]
    pub deck: Deck,
}

/// Something went wrong reading or writing a deck.
#[derive(Debug, thiserror::Error)]
pub enum DeckStoreError {
    /// A failure from the underlying database. redb distinguishes a dozen error types by
    /// operation; nothing here can act differently on them, so they are flattened to a message.
    #[error("deck database error: {0}")]
    Database(String),

    #[error("could not encode or decode a deck: {0}")]
    Encoding(String),

    #[error("no deck with id {0}")]
    NotFound(DeckId),
}

pub type Result<T> = std::result::Result<T, DeckStoreError>;

/// A deck database.
pub struct DeckStore {
    db: Database,
}

impl DeckStore {
    /// Opens a deck database, creating it if it does not exist.
    pub fn open(path: impl AsRef<Path>) -> Result<DeckStore> {
        let db = Database::create(path.as_ref()).map_err(db_error)?;
        let store = DeckStore { db };
        store.ensure_tables()?;
        Ok(store)
    }

    fn ensure_tables(&self) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_error)?;
        {
            txn.open_table(DECKS).map_err(db_error)?;
            txn.open_table(META).map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(())
    }

    /// Saves a new deck and returns its id.
    pub fn create(&self, deck: &Deck) -> Result<DeckId> {
        let txn = self.db.begin_write().map_err(db_error)?;
        let id;
        {
            let mut decks = txn.open_table(DECKS).map_err(db_error)?;
            let mut meta = txn.open_table(META).map_err(db_error)?;

            let next = meta
                .get(NEXT_ID)
                .map_err(db_error)?
                .map(|v| v.value())
                .unwrap_or(1);
            meta.insert(NEXT_ID, next + 1).map_err(db_error)?;
            id = DeckId(next);

            decks
                .insert(id.0, encode(deck)?.as_slice())
                .map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(id)
    }

    pub fn get(&self, id: DeckId) -> Result<Option<StoredDeck>> {
        let txn = self.db.begin_read().map_err(db_error)?;
        let decks = txn.open_table(DECKS).map_err(db_error)?;
        let Some(bytes) = decks.get(id.0).map_err(db_error)? else {
            return Ok(None);
        };
        Ok(Some(StoredDeck {
            id,
            deck: decode(bytes.value())?,
        }))
    }

    /// Overwrites an existing deck.
    pub fn update(&self, id: DeckId, deck: &Deck) -> Result<()> {
        if self.get(id)?.is_none() {
            return Err(DeckStoreError::NotFound(id));
        }
        let txn = self.db.begin_write().map_err(db_error)?;
        {
            let mut decks = txn.open_table(DECKS).map_err(db_error)?;
            decks
                .insert(id.0, encode(deck)?.as_slice())
                .map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(())
    }

    /// Deletes a deck. Returns whether it existed.
    pub fn delete(&self, id: DeckId) -> Result<bool> {
        let txn = self.db.begin_write().map_err(db_error)?;
        let existed;
        {
            let mut decks = txn.open_table(DECKS).map_err(db_error)?;
            existed = decks.remove(id.0).map_err(db_error)?.is_some();
        }
        txn.commit().map_err(db_error)?;
        Ok(existed)
    }

    /// Every deck, ordered by id.
    pub fn list(&self) -> Result<Vec<StoredDeck>> {
        let txn = self.db.begin_read().map_err(db_error)?;
        let decks = txn.open_table(DECKS).map_err(db_error)?;

        let mut out = Vec::new();
        for entry in decks.iter().map_err(db_error)? {
            let (key, value) = entry.map_err(db_error)?;
            out.push(StoredDeck {
                id: DeckId(key.value()),
                deck: decode(value.value())?,
            });
        }
        Ok(out)
    }
}

fn encode(deck: &Deck) -> Result<Vec<u8>> {
    serde_json::to_vec(deck).map_err(|e| DeckStoreError::Encoding(e.to_string()))
}

fn decode(bytes: &[u8]) -> Result<Deck> {
    serde_json::from_slice(bytes).map_err(|e| DeckStoreError::Encoding(e.to_string()))
}

fn db_error(error: impl std::fmt::Display) -> DeckStoreError {
    DeckStoreError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::{DeckEntry, Zone};
    use mtg_core::Format;

    struct TempStore {
        store: DeckStore,
        _dir: tempfile::TempDir,
    }

    impl std::ops::Deref for TempStore {
        type Target = DeckStore;
        fn deref(&self) -> &DeckStore {
            &self.store
        }
    }

    fn store() -> TempStore {
        let dir = tempfile::tempdir().unwrap();
        let store = DeckStore::open(dir.path().join("decks.redb")).unwrap();
        TempStore { store, _dir: dir }
    }

    fn krenko() -> Deck {
        let mut deck = Deck::new("Krenko", Format::Commander);
        deck.add(DeckEntry::new("o-krenko", "Krenko, Mob Boss", 1).in_zone(Zone::Command));
        deck.add(DeckEntry::new("o-mountain", "Mountain", 99));
        deck
    }

    #[test]
    fn create_and_read_back() {
        let store = store();
        let id = store.create(&krenko()).unwrap();

        let stored = store.get(id).unwrap().unwrap();
        assert_eq!(stored.id, id);
        assert_eq!(stored.deck.name, "Krenko");
        assert_eq!(stored.deck.format, Format::Commander);
        assert_eq!(stored.deck.count_in(Zone::Main), 99);
    }

    #[test]
    fn ids_do_not_repeat() {
        let store = store();
        let first = store.create(&krenko()).unwrap();
        let second = store.create(&krenko()).unwrap();
        assert_ne!(first, second);
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn update_replaces_the_deck() {
        let store = store();
        let id = store.create(&krenko()).unwrap();

        let mut edited = krenko();
        edited.name = "Krenko, upgraded".to_owned();
        edited.add(DeckEntry::new("o-sol-ring", "Sol Ring", 1));
        store.update(id, &edited).unwrap();

        let stored = store.get(id).unwrap().unwrap();
        assert_eq!(stored.deck.name, "Krenko, upgraded");
        assert_eq!(stored.deck.count_in(Zone::Main), 100);
        assert_eq!(store.list().unwrap().len(), 1, "updating is not creating");
    }

    #[test]
    fn updating_a_missing_deck_is_an_error() {
        let store = store();
        assert!(matches!(
            store.update(DeckId(99), &krenko()),
            Err(DeckStoreError::NotFound(_))
        ));
    }

    #[test]
    fn delete_reports_whether_it_existed() {
        let store = store();
        let id = store.create(&krenko()).unwrap();

        assert!(store.delete(id).unwrap());
        assert!(store.get(id).unwrap().is_none());
        assert!(!store.delete(id).unwrap());
    }

    #[test]
    fn decks_survive_reopening_and_ids_keep_climbing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decks.redb");

        let id = {
            let store = DeckStore::open(&path).unwrap();
            store.create(&krenko()).unwrap()
        };

        let store = DeckStore::open(&path).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().deck.name, "Krenko");

        let next = store.create(&Deck::new("Other", Format::Modern)).unwrap();
        assert!(next.0 > id.0, "ids must not restart and collide");
    }

    #[test]
    fn an_unknown_format_in_stored_json_fails_loudly() {
        // Format deserialises from its Scryfall key. If a stored deck names a format this
        // build does not know, it must be an error rather than silently becoming a default —
        // checking a Commander deck against Standard rules would be nonsense.
        let store = store();
        let id = store.create(&krenko()).unwrap();

        let txn = store.db.begin_write().unwrap();
        {
            let mut decks = txn.open_table(DECKS).unwrap();
            let corrupted = br#"{"name":"X","format":"explorer","entries":[],"notes":""}"#;
            decks.insert(id.0, corrupted.as_slice()).unwrap();
        }
        txn.commit().unwrap();

        assert!(matches!(store.get(id), Err(DeckStoreError::Encoding(_))));
    }
}
