//! Matching a hash against the known artwork.
//!
//! Brute force over every hash, and deliberately so. Fifty thousand entries at 32 bytes each
//! is 1.6 MB — small enough to sit in cache — and a Hamming distance is a handful of XOR and
//! popcount instructions. An approximate nearest-neighbour index would be more code, more
//! memory and another failure mode, in exchange for microseconds nobody would notice.

use serde::{Deserialize, Serialize};

use crate::hash::{ArtHash, HASH_BITS};

/// Bits of difference still counted as the same artwork.
///
/// Same painting through a phone camera lands within roughly a dozen bits; different paintings
/// land near 128, which is what chance gives. The gap between those is wide, so the exact
/// threshold matters less than it looks — this sits well inside it.
pub const DEFAULT_MAX_DISTANCE: u32 = 28;

/// How much closer the best match must be than the runner-up.
///
/// Two different printings of the same artwork are genuinely indistinguishable here, and that
/// is the point: they *are* the same picture. But two unrelated cards landing equally close
/// means the frame was bad, and returning either would be a guess. This is the check that
/// turns "closest" into "confidently closest".
pub const DEFAULT_MARGIN: u32 = 8;

/// One artwork in the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtEntry {
    pub hash: ArtHash,
    /// Scryfall printing id, which identifies the exact printing this art belongs to.
    pub printing_id: String,
    /// Oracle id, so a match lands straight in a deck or collection.
    pub oracle_id: String,
    pub name: String,
}

/// What a lookup found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    pub printing_id: String,
    pub oracle_id: String,
    pub name: String,
    /// Bits of difference. Lower is better; zero is identical.
    pub distance: u32,
    /// How much worse the next-best *different card* was. Larger means more certain.
    pub margin: u32,
}

/// The artwork database.
#[derive(Debug, Clone, Default)]
pub struct ArtDatabase {
    entries: Vec<ArtEntry>,
}

impl ArtDatabase {
    pub fn new(entries: Vec<ArtEntry>) -> ArtDatabase {
        ArtDatabase { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[ArtEntry] {
        &self.entries
    }

    /// Finds the artwork a hash belongs to, or nothing if the answer is not clear.
    ///
    /// Returns `None` rather than a best guess when the closest entry is too far away, or when
    /// a *different card* is nearly as close. A wrong card added silently to a collection is
    /// worse than a card the scanner declines to name.
    pub fn best_match(&self, hash: &ArtHash) -> Option<Match> {
        self.best_match_with(hash, DEFAULT_MAX_DISTANCE, DEFAULT_MARGIN)
    }

    /// As [`ArtDatabase::best_match`], with the thresholds spelled out.
    ///
    /// Two passes rather than one. Tracking the runner-up while hunting for the best in a
    /// single loop needs care — the runner-up has to be recomputed whenever the best changes
    /// oracle id — and an earlier version of that was correct but nearly impossible to check by
    /// reading. This decides whether a wrong card can be named, so it is written to be obvious.
    /// The second pass costs another 50,000 popcounts, and only runs when there is a candidate
    /// worth confirming at all.
    pub fn best_match_with(&self, hash: &ArtHash, max_distance: u32, margin: u32) -> Option<Match> {
        let (entry, distance) = self
            .entries
            .iter()
            .map(|entry| (entry, hash.distance(&entry.hash)))
            .min_by_key(|(_, distance)| *distance)?;

        if distance > max_distance {
            return None;
        }

        // Only a *different card* counts as a rival. Two printings of the same artwork sit at
        // the same distance by definition, and letting them compete would make every reprinted
        // card unrecognisable.
        let runner_up = self
            .entries
            .iter()
            .filter(|other| other.oracle_id != entry.oracle_id)
            .map(|other| hash.distance(&other.hash))
            .min();

        // No rival at all — a one-card database, or every entry a printing of the same card.
        // There is nothing to be ambiguous with, so the margin check does not apply.
        let gap = match runner_up {
            Some(rival) => {
                let gap = rival.saturating_sub(distance);
                if gap < margin {
                    return None;
                }
                gap
            }
            None => HASH_BITS as u32,
        };

        Some(Match {
            printing_id: entry.printing_id.clone(),
            oracle_id: entry.oracle_id.clone(),
            name: entry.name.clone(),
            distance,
            margin: gap,
        })
    }

    /// Every printing whose artwork matches, closest first.
    ///
    /// A reprint shares its painting with the original, so the hash cannot tell them apart —
    /// by design, since they are the same picture. This is what lets the UI offer the choice
    /// rather than picking a printing the user never saw.
    pub fn printings_of(&self, oracle_id: &str) -> Vec<&ArtEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.oracle_id == oracle_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(bits: &[usize]) -> ArtHash {
        let mut bytes = [0u8; crate::hash::HASH_BYTES];
        for bit in bits {
            bytes[bit / 8] |= 1 << (bit % 8);
        }
        ArtHash(bytes)
    }

    fn entry(name: &str, oracle: &str, printing: &str, bits: &[usize]) -> ArtEntry {
        ArtEntry {
            hash: hash_of(bits),
            printing_id: printing.to_owned(),
            oracle_id: oracle.to_owned(),
            name: name.to_owned(),
        }
    }

    /// Cards far apart from each other, as real artwork is.
    fn database() -> ArtDatabase {
        ArtDatabase::new(vec![
            entry(
                "Forest",
                "o-forest",
                "p-forest-1",
                &(0..40).collect::<Vec<_>>(),
            ),
            entry(
                "Island",
                "o-island",
                "p-island-1",
                &(80..130).collect::<Vec<_>>(),
            ),
            entry(
                "Mountain",
                "o-mountain",
                "p-mountain-1",
                &(160..210).collect::<Vec<_>>(),
            ),
        ])
    }

    #[test]
    fn an_exact_hash_matches_its_card() {
        let db = database();
        let found = db
            .best_match(&hash_of(&(0..40).collect::<Vec<_>>()))
            .expect("match");
        assert_eq!(found.name, "Forest");
        assert_eq!(found.distance, 0);
    }

    #[test]
    fn a_slightly_noisy_hash_still_matches() {
        // A camera never reproduces a hash exactly; a handful of flipped bits is normal.
        let db = database();
        let mut bits: Vec<usize> = (0..40).collect();
        bits.extend([250, 251, 252, 253, 254]);

        let found = db.best_match(&hash_of(&bits)).expect("match");
        assert_eq!(found.name, "Forest");
        assert_eq!(found.distance, 5);
    }

    #[test]
    fn a_hash_that_matches_nothing_returns_nothing() {
        // A blurry frame, a thumb over the card, a coaster. Naming a card here would be worse
        // than declining to.
        let db = database();
        assert!(db
            .best_match(&hash_of(&(200..256).collect::<Vec<_>>()))
            .is_none());
    }

    #[test]
    fn an_ambiguous_hash_is_refused_rather_than_guessed() {
        // Two unrelated cards equally close means the frame was bad.
        let db = ArtDatabase::new(vec![
            entry("A", "o-a", "p-a", &[0, 1, 2, 3]),
            entry("B", "o-b", "p-b", &[4, 5, 6, 7]),
        ]);
        // Four bits from each: equidistant.
        assert!(db.best_match(&hash_of(&[0, 1, 4, 5])).is_none());
    }

    #[test]
    fn two_printings_of_the_same_art_do_not_block_each_other() {
        // They are the same painting, so they sit at the same distance. Treating that as
        // ambiguity would make every reprinted card unrecognisable.
        let db = ArtDatabase::new(vec![
            entry("Sol Ring", "o-sol", "p-sol-a", &[0, 1, 2, 3]),
            entry("Sol Ring", "o-sol", "p-sol-b", &[0, 1, 2, 3]),
            entry(
                "Island",
                "o-island",
                "p-island",
                &(100..150).collect::<Vec<_>>(),
            ),
        ]);

        let found = db.best_match(&hash_of(&[0, 1, 2, 3])).expect("match");
        assert_eq!(found.oracle_id, "o-sol");
    }

    #[test]
    fn every_printing_of_a_card_can_be_listed() {
        // What the UI offers when a reprint cannot be told from the original — because it
        // genuinely cannot.
        let db = ArtDatabase::new(vec![
            entry("Sol Ring", "o-sol", "p-sol-a", &[0]),
            entry("Sol Ring", "o-sol", "p-sol-b", &[0]),
            entry("Island", "o-island", "p-island", &[100]),
        ]);

        let printings = db.printings_of("o-sol");
        assert_eq!(printings.len(), 2);
        assert!(db.printings_of("o-nothing").is_empty());
    }

    #[test]
    fn the_verdict_does_not_depend_on_the_order_of_the_database() {
        // The check that matters most here. Deciding "is the runner-up too close?" while still
        // hunting for the best is where an ordering bug would hide, and the symptom would be a
        // wrong card named for one database ordering and declined for another.
        let sol_a = entry("Sol Ring", "o-sol", "p-sol-a", &[0, 1, 2, 3]);
        let sol_b = entry("Sol Ring", "o-sol", "p-sol-b", &[0, 1, 2, 3, 4, 5]);
        // Ten bits away: comfortably outside DEFAULT_MARGIN, so the answer is a match rather
        // than a refusal, and the reported margin is a number worth asserting on.
        let mut rival_bits: Vec<usize> = vec![0, 1, 2, 3];
        rival_bits.extend(100..110);
        let rival = entry("Mox", "o-mox", "p-mox", &rival_bits);
        let query = hash_of(&[0, 1, 2, 3]);

        let orderings = [
            vec![sol_a.clone(), sol_b.clone(), rival.clone()],
            vec![rival.clone(), sol_a.clone(), sol_b.clone()],
            vec![sol_b.clone(), rival.clone(), sol_a.clone()],
            vec![rival.clone(), sol_b.clone(), sol_a.clone()],
        ];

        for entries in orderings {
            let found = ArtDatabase::new(entries).best_match(&query).expect("match");
            assert_eq!(found.oracle_id, "o-sol");
            assert_eq!(found.distance, 0);
            // The Mox is ten bits away, and it is the nearest *different* card.
            assert_eq!(
                found.margin, 10,
                "the runner-up should be the Mox, whatever the order"
            );
        }
    }

    #[test]
    fn a_lone_card_is_named_without_a_rival_to_compare_against() {
        // A database of one, or of several printings of one card: there is nothing to be
        // ambiguous with, so the margin rule must not reject the only answer there is.
        let db = ArtDatabase::new(vec![
            entry("Sol Ring", "o-sol", "p-sol-a", &[0, 1, 2, 3]),
            entry("Sol Ring", "o-sol", "p-sol-b", &[0, 1, 2, 3]),
        ]);
        let found = db.best_match(&hash_of(&[0, 1, 2, 3])).expect("match");
        assert_eq!(found.oracle_id, "o-sol");
    }

    #[test]
    fn an_empty_database_matches_nothing_without_complaining() {
        // What the app has before the optional artwork hashes are downloaded.
        let db = ArtDatabase::default();
        assert!(db.is_empty());
        assert!(db.best_match(&hash_of(&[1, 2, 3])).is_none());
    }

    #[test]
    fn the_thresholds_can_be_tightened_or_loosened() {
        let db = database();
        let mut bits: Vec<usize> = (0..40).collect();
        bits.extend(240..250);

        // Ten bits out: inside the default, outside a strict threshold.
        assert!(db.best_match_with(&hash_of(&bits), 28, 8).is_some());
        assert!(db.best_match_with(&hash_of(&bits), 5, 8).is_none());
    }

    #[test]
    fn a_match_reports_how_certain_it_was() {
        // So the caller can require more confidence than the defaults if it wants.
        let db = database();
        let found = db
            .best_match(&hash_of(&(0..40).collect::<Vec<_>>()))
            .expect("match");
        assert!(found.margin > 0, "the runner-up should be measurably worse");
    }
}
