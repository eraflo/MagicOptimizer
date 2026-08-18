//! Application state: the catalog and the collection database.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use mtg_collection::CollectionStore;
use mtg_combo::ComboDatabase;
use mtg_data::Catalog;
use mtg_deck::DeckStore;
use mtg_journal::JournalStore;
use mtg_vision::{ArtDatabase, Scanner};

/// Everything the commands need.
///
/// The catalog is optional because the artifact is downloaded rather than bundled: on a fresh
/// install there is nothing to load yet, and the UI has to say so instead of failing.
pub struct AppState {
    catalog: RwLock<Option<Catalog>>,
    /// Reason the last load attempt failed, for the UI to display.
    catalog_error: RwLock<Option<String>>,
    catalog_path: PathBuf,
    /// Optional: the combo snapshot is a separate download, and everything that uses it says
    /// what it could not check rather than assuming a deck is clean.
    combos: RwLock<Option<ComboDatabase>>,
    combo_error: RwLock<Option<String>>,
    combo_path: PathBuf,
    /// The live camera session, holding both the artwork fingerprints and the vote history
    /// across frames. A `Mutex` rather than an `RwLock` because feeding a frame mutates it.
    scanner: Mutex<Option<Scanner>>,
    art_error: RwLock<Option<String>>,
    art_path: PathBuf,
    /// Kept separately so the status command does not have to take the scanner lock while a
    /// frame is being processed.
    artworks: RwLock<usize>,
    collection: CollectionStore,
    decks: DeckStore,
    /// The game log. Its own database, because it is the one thing here that is pure history:
    /// a deck can be deleted and rebuilt, but the evenings happened.
    journal: JournalStore,
}

impl AppState {
    pub fn new(data_dir: &Path) -> Result<AppState, String> {
        AppState::with_artifacts_at(data_dir, |name| locate_artifact(data_dir, name))
    }

    /// A state that will not find any artifact, whatever is lying around.
    ///
    /// [`AppState::new`] falls back to `artifacts/` in the checkout so `tauri dev` works right
    /// after a build — which means a test asserting fresh-install behaviour through it passes
    /// or fails depending on whether the developer has run `build-artifacts`. That is exactly
    /// what happened: the combo test passed until `artifacts/combos.rkyv` was first generated.
    ///
    /// This resolves every artifact inside the (empty) data directory instead, so nothing is
    /// found — and, unlike going through `new` and overriding the paths afterwards, it never
    /// loads the real 25 MB catalog and 51 MB combo snapshot just to throw them away.
    #[cfg(test)]
    pub(crate) fn without_artifacts(data_dir: &Path) -> Result<AppState, String> {
        AppState::with_artifacts_at(data_dir, |name| data_dir.join(name))
    }

    /// The shared body of the two constructors: only where artifacts are looked for differs.
    fn with_artifacts_at(
        data_dir: &Path,
        locate: impl Fn(&str) -> PathBuf,
    ) -> Result<AppState, String> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| format!("could not create {}: {e}", data_dir.display()))?;

        let collection = CollectionStore::open(data_dir.join("collection.redb"))
            .map_err(|e| format!("could not open the collection: {e}"))?;
        let decks = DeckStore::open(data_dir.join("decks.redb"))
            .map_err(|e| format!("could not open the deck database: {e}"))?;
        let journal = JournalStore::open(data_dir.join("journal.redb"))
            .map_err(|e| format!("could not open the game log: {e}"))?;

        let state = AppState {
            catalog: RwLock::new(None),
            catalog_error: RwLock::new(None),
            catalog_path: locate("cards.rkyv"),
            combos: RwLock::new(None),
            combo_error: RwLock::new(None),
            combo_path: locate("combos.rkyv"),
            scanner: Mutex::new(None),
            art_error: RwLock::new(None),
            art_path: locate("arthashes.bin"),
            artworks: RwLock::new(0),
            collection,
            decks,
            journal,
        };
        state.reload_catalog();
        state.reload_combos();
        state.reload_artwork();
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

    pub fn combo_path(&self) -> &Path {
        &self.combo_path
    }

    /// Attempts to load the combo snapshot, recording the reason on failure.
    pub fn reload_combos(&self) {
        let (database, error) = match ComboDatabase::open(&self.combo_path) {
            Ok(database) => (Some(database), None),
            Err(error) => (None, Some(error.to_string())),
        };
        if let Ok(mut slot) = self.combos.write() {
            *slot = database;
        }
        if let Ok(mut slot) = self.combo_error.write() {
            *slot = error;
        }
    }

    pub fn combo_error(&self) -> Option<String> {
        self.combo_error.read().ok().and_then(|e| e.clone())
    }

    /// Runs `f` against the combo database, or returns `None` when there is none.
    pub fn with_combos<T>(&self, f: impl FnOnce(&ComboDatabase) -> T) -> Option<T> {
        let guard = self.combos.read().ok()?;
        guard.as_ref().map(f)
    }

    /// Runs `f` with the combo database as an `Option`, so a caller can distinguish "no
    /// combos found" from "the combo check never ran".
    pub fn with_combos_ref<T>(&self, f: impl FnOnce(Option<&ComboDatabase>) -> T) -> T {
        match self.combos.read() {
            Ok(guard) => f(guard.as_ref()),
            Err(_) => f(None),
        }
    }

    pub fn art_path(&self) -> &Path {
        &self.art_path
    }

    pub fn art_error(&self) -> Option<String> {
        self.art_error.read().ok().and_then(|e| e.clone())
    }

    /// How many artworks the scanner can recognise. Zero means the artifact is not installed.
    pub fn artworks(&self) -> usize {
        self.artworks.read().map(|count| *count).unwrap_or(0)
    }

    /// Loads the artwork fingerprints and builds a fresh scanner around them.
    ///
    /// The heaviest optional artifact, and the one most likely to be absent: someone who never
    /// scans cards should not have to download 6 MB of fingerprints. Its absence is a state the
    /// UI reports, not an error.
    pub fn reload_artwork(&self) {
        let (database, error) = match std::fs::File::open(&self.art_path) {
            Ok(file) => {
                let mut reader = std::io::BufReader::with_capacity(1 << 16, file);
                match mtg_vision::archive::read(&mut reader) {
                    Ok(database) => (Some(database), None),
                    Err(error) => (None, Some(error.to_string())),
                }
            }
            Err(error) => (None, Some(error.to_string())),
        };

        let count = database.as_ref().map(ArtDatabase::len).unwrap_or(0);
        // A scanner is built even when the fingerprints are missing, around an empty database.
        // Detection, rectification and the outline all still work — only naming a card does not
        // — and refusing to start the camera would make the one thing worth testing on a fresh
        // install impossible to test. `mtg-vision` has a test for exactly this case.
        if let Ok(mut slot) = self.scanner.lock() {
            *slot = Some(Scanner::new(database.unwrap_or_default()));
        }
        if let Ok(mut slot) = self.art_error.write() {
            *slot = error;
        }
        if let Ok(mut slot) = self.artworks.write() {
            *slot = count;
        }
    }

    /// Runs `f` against the live scanner, or reports that there is none.
    ///
    /// Takes `&mut` because feeding a frame advances the vote history — that history is the
    /// whole reason the scanner is a long-lived object rather than a function.
    pub fn with_scanner<T>(&self, f: impl FnOnce(&mut Scanner) -> T) -> Result<T, String> {
        let mut guard = self
            .scanner
            .lock()
            .map_err(|_| "the scanner lock was poisoned by an earlier panic".to_owned())?;
        match guard.as_mut() {
            Some(scanner) => Ok(f(scanner)),
            None => Err(match self.art_error() {
                Some(error) => format!("no artwork data loaded: {error}"),
                None => "no artwork data loaded".to_owned(),
            }),
        }
    }

    pub fn collection(&self) -> &CollectionStore {
        &self.collection
    }

    pub fn decks(&self) -> &DeckStore {
        &self.decks
    }

    pub fn journal(&self) -> &JournalStore {
        &self.journal
    }
}

/// Finds an artifact by name, preferring the installed copy.
///
/// The app data directory is the real answer once there is a downloader. Until then, a checkout
/// with the artifacts already built is picked up automatically, so `cargo tauri dev` works
/// straight after `cargo run -p build-artifacts`.
fn locate_artifact(data_dir: &Path, name: &str) -> PathBuf {
    let installed = data_dir.join(name);
    if installed.exists() {
        return installed;
    }

    let in_checkout = Path::new("artifacts").join(name);
    if in_checkout.exists() {
        return in_checkout;
    }
    // Running from src-tauri/, as `tauri dev` does.
    let from_src_tauri = Path::new("../artifacts").join(name);
    if from_src_tauri.exists() {
        return from_src_tauri;
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
            combos: RwLock::new(None),
            combo_error: RwLock::new(None),
            combo_path: dir.path().join("combos.rkyv"),
            scanner: Mutex::new(None),
            art_error: RwLock::new(None),
            art_path: dir.path().join("arthashes.bin"),
            artworks: RwLock::new(0),
            collection: mtg_collection::CollectionStore::open(dir.path().join("c.redb")).unwrap(),
            decks: DeckStore::open(dir.path().join("d.redb")).unwrap(),
            journal: JournalStore::open(dir.path().join("j.redb")).unwrap(),
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
