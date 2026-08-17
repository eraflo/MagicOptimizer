//! Combo detection and Commander bracket estimation.
//!
//! The combo data is a snapshot of Commander Spellbook, an unofficial community project. It is
//! an **optional artifact**: everything here degrades to saying what it could not check rather
//! than assuming a deck is clean.

mod bracket;
mod combo;
mod detect;
mod error;

pub use bracket::{assess, BracketAssessment, Marker};
pub use combo::{serialize, ArchivedCombo, Combo, ComboData, ComboDatabase, FORMAT_VERSION};
pub use detect::{ComboIndex, ComboMatch};
pub use error::{ComboError, Result};
