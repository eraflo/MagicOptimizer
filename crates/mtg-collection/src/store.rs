//! Persistence for collections, on top of [`redb`].
//!
//! redb is pure Rust and embedded, which is the whole reason it is here: it cross-compiles to
//! Android without a C toolchain. See `docs/dev/architecture.md`.

use std::collections::HashMap;
use std::path::Path;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::error::{CollectionError, Result};
use crate::model::{Holding, HoldingId, NewHolding, Pool, StorageLocation};

/// Holdings, keyed by id, stored as JSON.
///
/// JSON rather than a compact binary encoding on purpose: this is the user's own data, it is
/// small, and being able to read it out with any tool is worth more than the bytes saved.
const HOLDINGS: TableDefinition<u64, &[u8]> = TableDefinition::new("holdings");

/// Merge key to holding id.
///
/// The one index that exists, because it is the only O(1) lookup the design actually needs:
/// scanning a binder calls it once per card, and a linear search there would be quadratic.
/// Everything else scans, which is fine at collection sizes.
const MERGE_INDEX: TableDefinition<&str, u64> = TableDefinition::new("merge_index");

/// Small key/value store for the id counter and future settings.
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");

const NEXT_ID: &str = "next_holding_id";

/// A collection database.
pub struct CollectionStore {
    db: Database,
}

impl CollectionStore {
    /// Opens a collection, creating it if it does not exist.
    pub fn open(path: impl AsRef<Path>) -> Result<CollectionStore> {
        let db = Database::create(path.as_ref()).map_err(db_error)?;
        let store = CollectionStore { db };
        store.ensure_tables()?;
        Ok(store)
    }

    /// Creates the tables so later read transactions never fail on a missing table.
    fn ensure_tables(&self) -> Result<()> {
        let txn = self.db.begin_write().map_err(db_error)?;
        {
            txn.open_table(HOLDINGS).map_err(db_error)?;
            txn.open_table(MERGE_INDEX).map_err(db_error)?;
            txn.open_table(META).map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(())
    }

    /// Adds copies to the collection.
    ///
    /// If an interchangeable holding already exists — same card, printing, language, finish,
    /// condition and location — its quantity goes up instead of a near-duplicate row being
    /// created. That is what keeps a scanned binder from turning into thousands of rows of one.
    pub fn add(&self, new: NewHolding) -> Result<HoldingId> {
        if new.quantity == 0 {
            return Err(CollectionError::ZeroQuantity);
        }

        let key = merge_key(
            new.pool,
            &new.oracle_id,
            &new.set_code,
            &new.collector_number,
            &new.language,
            &format!("{:?}", new.finish),
            &format!("{:?}", new.condition),
            new.location.as_ref(),
        );

        let txn = self.db.begin_write().map_err(db_error)?;
        let id;
        {
            let mut holdings = txn.open_table(HOLDINGS).map_err(db_error)?;
            let mut index = txn.open_table(MERGE_INDEX).map_err(db_error)?;
            let mut meta = txn.open_table(META).map_err(db_error)?;

            let existing = index
                .get(key.as_str())
                .map_err(db_error)?
                .map(|v| v.value());

            match existing {
                Some(existing_id) => {
                    let bytes = holdings
                        .get(existing_id)
                        .map_err(db_error)?
                        .map(|v| v.value().to_vec());
                    // The index can only point at a missing holding if the database was
                    // damaged; fall through to creating a fresh row rather than failing.
                    match bytes {
                        Some(bytes) => {
                            let mut holding: Holding = decode(&bytes)?;
                            holding.quantity = holding.quantity.saturating_add(new.quantity);
                            holdings
                                .insert(existing_id, encode(&holding)?.as_slice())
                                .map_err(db_error)?;
                            id = HoldingId(existing_id);
                        }
                        None => {
                            id = allocate_id(&mut meta)?;
                            let holding = new.into_holding(id);
                            holdings
                                .insert(id.0, encode(&holding)?.as_slice())
                                .map_err(db_error)?;
                            index.insert(key.as_str(), id.0).map_err(db_error)?;
                        }
                    }
                }
                None => {
                    id = allocate_id(&mut meta)?;
                    let holding = new.into_holding(id);
                    holdings
                        .insert(id.0, encode(&holding)?.as_slice())
                        .map_err(db_error)?;
                    index.insert(key.as_str(), id.0).map_err(db_error)?;
                }
            }
        }
        txn.commit().map_err(db_error)?;
        Ok(id)
    }

    pub fn get(&self, id: HoldingId) -> Result<Option<Holding>> {
        let txn = self.db.begin_read().map_err(db_error)?;
        let holdings = txn.open_table(HOLDINGS).map_err(db_error)?;
        let Some(bytes) = holdings.get(id.0).map_err(db_error)? else {
            return Ok(None);
        };
        Ok(Some(decode(bytes.value())?))
    }

    /// Sets a holding's quantity. Setting it to zero removes the holding.
    pub fn set_quantity(&self, id: HoldingId, quantity: u32) -> Result<()> {
        if quantity == 0 {
            return self.remove(id).map(|_| ());
        }
        let Some(mut holding) = self.get(id)? else {
            return Err(CollectionError::NotFound(id));
        };
        holding.quantity = quantity;
        self.replace(holding)
    }

    /// Overwrites a holding.
    ///
    /// Editing the fields that make copies interchangeable moves the holding in the merge
    /// index, which is why this goes through one place rather than being written inline.
    pub fn replace(&self, holding: Holding) -> Result<()> {
        let Some(previous) = self.get(holding.id)? else {
            return Err(CollectionError::NotFound(holding.id));
        };

        let txn = self.db.begin_write().map_err(db_error)?;
        {
            let mut holdings = txn.open_table(HOLDINGS).map_err(db_error)?;
            let mut index = txn.open_table(MERGE_INDEX).map_err(db_error)?;

            let old_key = key_of(&previous);
            let new_key = key_of(&holding);
            if old_key != new_key {
                index.remove(old_key.as_str()).map_err(db_error)?;
                index
                    .insert(new_key.as_str(), holding.id.0)
                    .map_err(db_error)?;
            }
            holdings
                .insert(holding.id.0, encode(&holding)?.as_slice())
                .map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(())
    }

    /// Removes a holding entirely. Returns whether it existed.
    pub fn remove(&self, id: HoldingId) -> Result<bool> {
        let Some(holding) = self.get(id)? else {
            return Ok(false);
        };

        let txn = self.db.begin_write().map_err(db_error)?;
        {
            let mut holdings = txn.open_table(HOLDINGS).map_err(db_error)?;
            let mut index = txn.open_table(MERGE_INDEX).map_err(db_error)?;
            holdings.remove(id.0).map_err(db_error)?;
            index.remove(key_of(&holding).as_str()).map_err(db_error)?;
        }
        txn.commit().map_err(db_error)?;
        Ok(true)
    }

    /// Every holding, optionally restricted to one pool, ordered by id.
    pub fn list(&self, pool: Option<Pool>) -> Result<Vec<Holding>> {
        let txn = self.db.begin_read().map_err(db_error)?;
        let holdings = txn.open_table(HOLDINGS).map_err(db_error)?;

        let mut out = Vec::new();
        for entry in holdings.iter().map_err(db_error)? {
            let (_, value) = entry.map_err(db_error)?;
            let holding: Holding = decode(value.value())?;
            if pool.is_none_or(|p| holding.pool == p) {
                out.push(holding);
            }
        }
        Ok(out)
    }

    /// Every holding of one card.
    pub fn by_oracle_id(&self, pool: Option<Pool>, oracle_id: &str) -> Result<Vec<Holding>> {
        Ok(self
            .list(pool)?
            .into_iter()
            .filter(|h| h.oracle_id == oracle_id)
            .collect())
    }

    /// Total copies owned of one card.
    pub fn owned_quantity(&self, pool: Option<Pool>, oracle_id: &str) -> Result<u32> {
        Ok(self
            .by_oracle_id(pool, oracle_id)?
            .iter()
            .map(|h| h.quantity)
            .sum())
    }

    /// Owned quantity for every card at once, keyed by oracle id.
    ///
    /// Call this once per view rather than [`CollectionStore::owned_quantity`] per row: marking
    /// up a page of search results otherwise rescans the whole collection for every line.
    pub fn owned_quantities(&self, pool: Option<Pool>) -> Result<HashMap<String, u32>> {
        let mut totals: HashMap<String, u32> = HashMap::new();
        for holding in self.list(pool)? {
            *totals.entry(holding.oracle_id).or_default() += holding.quantity;
        }
        Ok(totals)
    }

    /// Distinct container names in use, sorted. Feeds the location picker.
    pub fn containers(&self) -> Result<Vec<String>> {
        let mut names: Vec<String> = self
            .list(Some(Pool::Physical))?
            .into_iter()
            .filter_map(|h| h.location.map(|l| l.container))
            .filter(|c| !c.trim().is_empty())
            .collect();
        names.sort_unstable();
        names.dedup();
        Ok(names)
    }

    /// Headline numbers for one pool.
    pub fn stats(&self, pool: Option<Pool>) -> Result<Stats> {
        let holdings = self.list(pool)?;
        let mut distinct = std::collections::HashSet::new();
        let mut total_copies = 0u64;
        for holding in &holdings {
            distinct.insert(holding.oracle_id.as_str());
            total_copies += u64::from(holding.quantity);
        }
        Ok(Stats {
            holdings: holdings.len(),
            distinct_cards: distinct.len(),
            total_copies,
        })
    }
}

/// Headline collection numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    /// Number of stacks.
    pub holdings: usize,
    /// Number of different cards.
    pub distinct_cards: usize,
    /// Number of physical cards.
    pub total_copies: u64,
}

fn allocate_id(meta: &mut redb::Table<'_, &str, u64>) -> Result<HoldingId> {
    let next = meta
        .get(NEXT_ID)
        .map_err(db_error)?
        .map(|v| v.value())
        .unwrap_or(1);
    meta.insert(NEXT_ID, next + 1).map_err(db_error)?;
    Ok(HoldingId(next))
}

fn key_of(holding: &Holding) -> String {
    let key = holding.merge_key();
    merge_key(
        key.pool,
        key.oracle_id,
        key.set_code,
        key.collector_number,
        key.language,
        &format!("{:?}", key.finish),
        &format!("{:?}", key.condition),
        key.location,
    )
}

#[allow(clippy::too_many_arguments)]
fn merge_key(
    pool: Pool,
    oracle_id: &str,
    set_code: &str,
    collector_number: &str,
    language: &str,
    finish: &str,
    condition: &str,
    location: Option<&StorageLocation>,
) -> String {
    // Newline separated because none of these fields can contain one, so two different sets of
    // values can never produce the same key.
    let location = location.map(ToString::to_string).unwrap_or_default();
    format!(
        "{}\n{oracle_id}\n{set_code}\n{collector_number}\n{language}\n{finish}\n{condition}\n{location}",
        pool.as_str()
    )
}

fn encode(holding: &Holding) -> Result<Vec<u8>> {
    serde_json::to_vec(holding).map_err(|e| CollectionError::Encoding(e.to_string()))
}

fn decode(bytes: &[u8]) -> Result<Holding> {
    serde_json::from_slice(bytes).map_err(|e| CollectionError::Encoding(e.to_string()))
}

fn db_error(error: impl std::fmt::Display) -> CollectionError {
    CollectionError::Database(error.to_string())
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::model::{Condition, Finish};

    struct TempStore {
        store: CollectionStore,
        _dir: tempfile::TempDir,
    }

    impl std::ops::Deref for TempStore {
        type Target = CollectionStore;
        fn deref(&self) -> &CollectionStore {
            &self.store
        }
    }

    fn store() -> TempStore {
        let dir = tempfile::tempdir().unwrap();
        let store = CollectionStore::open(dir.path().join("collection.redb")).unwrap();
        TempStore { store, _dir: dir }
    }

    fn sol_ring() -> NewHolding {
        NewHolding::single(Pool::Physical, "oracle-sol-ring", "Sol Ring")
    }

    #[test]
    fn add_and_read_back() {
        let store = store();
        let id = store.add(sol_ring().quantity(2)).unwrap();

        let holding = store.get(id).unwrap().unwrap();
        assert_eq!(holding.name, "Sol Ring");
        assert_eq!(holding.quantity, 2);
        assert_eq!(holding.pool, Pool::Physical);
    }

    #[test]
    fn identical_copies_merge_into_one_holding() {
        // Scanning the same card four times must not produce four rows.
        let store = store();
        let first = store.add(sol_ring()).unwrap();
        for _ in 0..3 {
            assert_eq!(store.add(sol_ring()).unwrap(), first);
        }

        assert_eq!(store.list(None).unwrap().len(), 1);
        assert_eq!(store.get(first).unwrap().unwrap().quantity, 4);
    }

    #[test]
    fn copies_that_differ_stay_separate() {
        let store = store();
        store.add(sol_ring()).unwrap();
        store.add(sol_ring().finish(Finish::Foil)).unwrap();
        store
            .add(sol_ring().condition(Condition::HeavilyPlayed))
            .unwrap();
        store.add(sol_ring().language("fr")).unwrap();
        store.add(sol_ring().printing("2xm", "263")).unwrap();
        store
            .add(sol_ring().at(StorageLocation::new("Binder 3")))
            .unwrap();

        assert_eq!(store.list(None).unwrap().len(), 6);
        // They are all still the same card.
        assert_eq!(store.owned_quantity(None, "oracle-sol-ring").unwrap(), 6);
    }

    #[test]
    fn pools_are_independent() {
        let store = store();
        store.add(sol_ring().quantity(2)).unwrap();
        store
            .add(NewHolding::single(Pool::Digital, "oracle-sol-ring", "Sol Ring").quantity(5))
            .unwrap();

        assert_eq!(
            store
                .owned_quantity(Some(Pool::Physical), "oracle-sol-ring")
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .owned_quantity(Some(Pool::Digital), "oracle-sol-ring")
                .unwrap(),
            5
        );
        assert_eq!(store.owned_quantity(None, "oracle-sol-ring").unwrap(), 7);
    }

    #[test]
    fn quantity_can_be_changed_and_zero_removes() {
        let store = store();
        let id = store.add(sol_ring().quantity(4)).unwrap();

        store.set_quantity(id, 2).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().quantity, 2);

        store.set_quantity(id, 0).unwrap();
        assert!(store.get(id).unwrap().is_none());
        assert!(store.list(None).unwrap().is_empty());
    }

    #[test]
    fn removing_a_holding_frees_its_merge_key() {
        // If the index were left behind, re-adding the card would resurrect a dead row.
        let store = store();
        let first = store.add(sol_ring()).unwrap();
        assert!(store.remove(first).unwrap());

        let second = store.add(sol_ring()).unwrap();
        assert_ne!(first, second);
        assert_eq!(store.list(None).unwrap().len(), 1);
        assert_eq!(store.get(second).unwrap().unwrap().quantity, 1);
    }

    #[test]
    fn editing_a_holding_moves_it_in_the_merge_index() {
        let store = store();
        let id = store.add(sol_ring()).unwrap();

        let mut holding = store.get(id).unwrap().unwrap();
        holding.finish = Finish::Foil;
        store.replace(holding).unwrap();

        // A new non-foil copy must not merge into the now-foil holding.
        let other = store.add(sol_ring()).unwrap();
        assert_ne!(other, id);
        assert_eq!(store.list(None).unwrap().len(), 2);
    }

    #[test]
    fn adding_zero_copies_is_rejected() {
        let store = store();
        assert!(matches!(
            store.add(sol_ring().quantity(0)),
            Err(CollectionError::ZeroQuantity)
        ));
    }

    #[test]
    fn quantities_of_a_missing_holding_report_not_found() {
        let store = store();
        assert!(matches!(
            store.set_quantity(HoldingId(999), 1),
            Err(CollectionError::NotFound(_))
        ));
        assert!(!store.remove(HoldingId(999)).unwrap());
    }

    #[test]
    fn owned_quantities_aggregates_every_card_in_one_pass() {
        let store = store();
        store.add(sol_ring().quantity(2)).unwrap();
        store.add(sol_ring().finish(Finish::Foil)).unwrap();
        store
            .add(
                NewHolding::single(Pool::Physical, "oracle-counterspell", "Counterspell")
                    .quantity(4),
            )
            .unwrap();

        let totals = store.owned_quantities(Some(Pool::Physical)).unwrap();
        assert_eq!(totals.get("oracle-sol-ring"), Some(&3));
        assert_eq!(totals.get("oracle-counterspell"), Some(&4));
        assert_eq!(totals.len(), 2);
    }

    #[test]
    fn containers_are_listed_once_and_sorted() {
        let store = store();
        store
            .add(sol_ring().at(StorageLocation::new("Binder 3").with_slot(1)))
            .unwrap();
        store
            .add(sol_ring().at(StorageLocation::new("Binder 3").with_slot(2)))
            .unwrap();
        store
            .add(sol_ring().at(StorageLocation::new("Box A")))
            .unwrap();
        store.add(sol_ring()).unwrap();

        assert_eq!(store.containers().unwrap(), ["Binder 3", "Box A"]);
    }

    #[test]
    fn stats_count_stacks_cards_and_copies_separately() {
        let store = store();
        store.add(sol_ring().quantity(2)).unwrap();
        store.add(sol_ring().finish(Finish::Foil)).unwrap();
        store
            .add(
                NewHolding::single(Pool::Physical, "oracle-counterspell", "Counterspell")
                    .quantity(4),
            )
            .unwrap();

        let stats = store.stats(Some(Pool::Physical)).unwrap();
        assert_eq!(stats.holdings, 3);
        assert_eq!(stats.distinct_cards, 2);
        assert_eq!(stats.total_copies, 7);
    }

    #[test]
    fn data_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.redb");

        let id = {
            let store = CollectionStore::open(&path).unwrap();
            store.add(sol_ring().quantity(3)).unwrap()
        };

        let store = CollectionStore::open(&path).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().quantity, 3);

        // Ids keep going up rather than restarting and colliding.
        let next = store
            .add(NewHolding::single(Pool::Physical, "oracle-other", "Other"))
            .unwrap();
        assert!(next.0 > id.0);
    }
}
