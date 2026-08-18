//! Core types shared across every MagicOptimizer crate.
//!
//! This crate depends on no other crate in the workspace, and on nothing native. See
//! `CLAUDE.md` for why that second point is not negotiable.

mod color;
mod format;
mod mana;
mod tag;

pub use color::{Color, ColorSet};
pub use format::{Format, Legality, Rarity};
pub use mana::{ManaCost, ManaCostError, ManaSymbol};
pub use tag::{Tag, TagSet};

/// Stable identifier for an oracle card: a card as a set of rules, independent of printing.
///
/// This is an index into the card catalog artifact, so it is only meaningful alongside the
/// catalog version that produced it. Distinct printings of the same card share one `CardId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardId(pub u32);

impl CardId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl std::fmt::Display for CardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}
