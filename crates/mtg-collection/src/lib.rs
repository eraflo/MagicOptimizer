//! Physical and digital card collections.
//!
//! Cards are stored by Scryfall `oracle_id`, never by [`mtg_core::CardId`] — see the note in
//! [`model`] for why that distinction matters more than it looks.

mod error;
mod model;
mod store;

pub use error::{CollectionError, Result};
pub use model::{
    Condition, Finish, Holding, HoldingId, MergeKey, NewHolding, Pool, StorageLocation,
};
pub use store::{CollectionStore, Stats};
