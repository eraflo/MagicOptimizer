//! Recognising Magic cards in a camera frame.
//!
//! The pipeline is: find the card's outline, square it up, crop the artwork, hash it, look the
//! hash up, and let several frames vote before believing the answer. Hashing the **artwork**
//! rather than the card is what makes this work in any language — see [`hash`].
//!
//! [`Scanner`] is the whole thing behind one method. The modules underneath are public because
//! each is useful on its own — `build-artifacts` needs only [`hash_gray`], for instance.

/// The `arthashes.bin` file format: [`archive::read`] and [`archive::write`].
pub mod archive;

mod detect;
mod geometry;
mod hash;
mod matcher;
mod rectify;
mod scanner;
mod vote;

pub use detect::{
    find_card, DetectSettings, ASPECT_TOLERANCE, DEFAULT_CONTRAST, MIN_AREA_FRACTION, WORKING_WIDTH,
};
pub use geometry::{
    homography_from_quads, rectified_quad, Homography, Point, Quad, CARD_ASPECT_RATIO,
    RECTIFIED_HEIGHT, RECTIFIED_WIDTH,
};
pub use hash::{hash_gray, rgba_to_gray, ArtHash, GrayView, HASH_BITS, HASH_BYTES, HASH_GRID};
pub use matcher::{ArtDatabase, ArtEntry, Match, DEFAULT_MARGIN, DEFAULT_MAX_DISTANCE};
pub use rectify::{crop_artwork, rectify, GrayImage, ART_BOTTOM, ART_LEFT, ART_RIGHT, ART_TOP};
pub use scanner::{ScanSettings, Scanner};
pub use vote::{Outcome, VoteSettings, Voter, DEFAULT_CLEAR_AFTER, DEFAULT_NEEDED, DEFAULT_WINDOW};
