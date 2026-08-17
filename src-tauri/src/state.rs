//! Application state: the catalog and the collection database.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use mtg_collection::CollectionStore;
use mtg_data::Catalog;

/// Everything the commands need.
///
/// The catalog is optional because the artifact is downloaded rather than bundled: on a fresh
/// install there is nothing to load yet, and the UI has to say so instead of failing.
pub struct AppState {
    catalog: RwLock<Option<Catalog>>,
    /// Reason the last load attempt failed, for the UI to display.
    catalog_error: RwLock<Option<String>>,
    catalog_path: PathBuf,
    collection: CollectionStore,
}

impl AppState {
    pub fn new(data_dir: &Path) -> Result<AppState, String> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| format!("could not create {}: {e}", data_dir.display()))?;

        let collection = CollectionStore::open(data_dir.join("collection.redb"))
            .map_err(|e| format!("could not open the collection: {e}"))?;

        let state = AppState {
            catalog: RwLock::new(None),
            catalog_error: RwLock::new(None),
            catalog_path: locate_catalog(data_dir),
            collection,
        };
        state.reload_catalog();
        Ok(state)
    }

    pub fn catalog_path(&self) -> &Path {
        &self.catalog_path
    }

    /// Attempts to load the catalog, recording the reason on failure.
    pub fn reload_catalog(&self) {
        let result = Catalog::open(&self.catalog_path);
        let (catalog, error) = match result {
            Ok(catalog) => (Some(catalog), None),
            Err(error) => (None, Some(error.to_string())),
        };
        if let Ok(mut slot) = self.catalog.write() {
            *slot = catalog;
        }
        if let Ok(mut slot) = self.catalog_error.write() {
            *slot = error;
        }
    }

    pub fn catalog_error(&self) -> Option<String> {
        self.catalog_error.read().ok().and_then(|e| e.clone())
    }

    /// Runs `f` against the loaded catalog, or reports that there is none.
    pub fn with_catalog<T>(&self, f: impl FnOnce(&Catalog) -> T) -> Result<T, String> {
        let guard = self
            .catalog
            .read()
            .map_err(|_| "the catalog lock was poisoned by an earlier panic".to_owned())?;
        match guard.as_ref() {
            Some(catalog) => Ok(f(catalog)),
            None => Err(match self.catalog_error() {
                Some(error) => format!("no card data loaded: {error}"),
                None => "no card data loaded".to_owned(),
            }),
        }
    }

    pub fn collection(&self) -> &CollectionStore {
        &self.collection
    }
}

/// Decides where the catalog artifact should be read from.
///
/// The app data directory is the real answer once there is a downloader. Until then, a
/// checkout with `artifacts/cards.rkyv` already built is picked up automatically, so
/// `cargo tauri dev` works straight after `cargo run -p build-artifacts`.
fn locate_catalog(data_dir: &Path) -> PathBuf {
    let installed = data_dir.join("cards.rkyv");
    if installed.exists() {
        return installed;
    }

    let in_checkout = Path::new("artifacts/cards.rkyv");
    if in_checkout.exists() {
        return in_checkout.to_path_buf();
    }
    // Running from src-tauri/, as `tauri dev` does.
    let from_src_tauri = Path::new("../artifacts/cards.rkyv");
    if from_src_tauri.exists() {
        return from_src_tauri.to_path_buf();
    }

    installed
}

#[cfg(test)]
mod tests {

    use super::*;
    use mtg_collection::{NewHolding, Pool};
    use mtg_data::Query;

    fn temp_state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(dir.path()).expect("state should initialise");
        (state, dir)
    }

    #[test]
    fn a_fresh_install_starts_with_a_working_collection() {
        let (state, _dir) = temp_state();

        let id = state
            .collection()
            .add(NewHolding::single(Pool::Physical, "oracle-sol-ring", "Sol Ring").quantity(2))
            .unwrap();

        assert_eq!(state.collection().get(id).unwrap().unwrap().quantity, 2);
        assert_eq!(
            state
                .collection()
                .owned_quantities(None)
                .unwrap()
                .get("oracle-sol-ring"),
            Some(&2)
        );
    }

    #[test]
    fn missing_card_data_is_reported_rather_than_crashing() {
        // A fresh install has no catalog: the app has to say so, not fall over. This is the
        // path most likely to be hit by a first-time user.
        let dir = tempfile::tempdir().unwrap();
        let state = AppState {
            catalog: RwLock::new(None),
            catalog_error: RwLock::new(Some("file not found".to_owned())),
            catalog_path: dir.path().join("cards.rkyv"),
            collection: mtg_collection::CollectionStore::open(dir.path().join("c.redb")).unwrap(),
        };

        let error = state.with_catalog(|c| c.len()).unwrap_err();
        assert!(error.contains("no card data"), "{error}");
        assert!(error.contains("file not found"), "{error}");
    }

    /// Exercises the real wiring against a real artifact.
    ///
    /// Skipped when there is none, which is the case in CI: the artifact is 26 MB and is built
    /// from Scryfall rather than committed. Run `cargo run -p build-artifacts` to enable it.
    #[test]
    fn catalog_and_collection_work_together() {
        let (state, _dir) = temp_state();

        if state.catalog_error().is_some() {
            eprintln!(
                "skipping: no catalog at {} — run `cargo run -p build-artifacts`",
                state.catalog_path().display()
            );
            return;
        }

        let card_count = state.with_catalog(|catalog| catalog.len()).unwrap();
        assert!(
            card_count > 30_000,
            "expected a full catalog, got {card_count}"
        );

        // Take a real card out of the catalog and put it in the collection, the way the Browse
        // tab does.
        let (oracle_id, name) = state
            .with_catalog(|catalog| {
                catalog
                    .find_by_name("Sol Ring")
                    .map(|(_, card)| (card.oracle_id().to_owned(), card.name().to_owned()))
            })
            .unwrap()
            .expect("Sol Ring should be in any catalog");

        state
            .collection()
            .add(NewHolding::single(Pool::Physical, &oracle_id, &name))
            .unwrap();

        let owned = state.collection().owned_quantities(None).unwrap();
        assert_eq!(owned.get(&oracle_id), Some(&1));

        // And the "only cards I own" filter finds it.
        let matches = state
            .with_catalog(|catalog| {
                let query = Query::new().name("Sol Ring");
                catalog
                    .iter()
                    .filter(|(_, card)| query.matches(card) && owned.contains_key(card.oracle_id()))
                    .count()
            })
            .unwrap();
        assert_eq!(matches, 1);
    }
}
