//! The card catalog: archive format, loading, and search.
//!
//! The catalog is a single rkyv archive produced by `build-artifacts` and memory-mapped at
//! runtime. There is no database here and no native dependency — see `docs/dev/architecture.md`
//! for why SQLite was ruled out.

mod card;
mod catalog;
mod error;
mod search;

pub use card::{
    legality_to_u8, rarity_to_u8, ArchivedCard, ArchivedCardFace, Card, CardFace, Layout,
    LEGALITY_SLOTS,
};
pub use catalog::{serialize, Catalog, CatalogData, Resolution, FORMAT_VERSION};
pub use error::{CatalogError, Result};
pub use search::Query;
