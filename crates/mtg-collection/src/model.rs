//! What a collection is made of.
//!
//! # Why cards are identified by `oracle_id` and not `CardId`
//!
//! [`mtg_core::CardId`] is a position in one particular catalog artifact. Rebuild the catalog
//! after a set release and every position shifts. A collection keyed on `CardId` would appear
//! to work, then silently turn into a different collection the first time the card data
//! updated — the worst kind of bug, because nothing errors and the damage is only noticed much
//! later.
//!
//! Scryfall's `oracle_id` is stable across printings and across rebuilds, so that is what gets
//! written to disk. `CardId` stays a runtime-only handle.

use serde::{Deserialize, Serialize};

/// Which collection a card belongs to.
///
/// Kept separate because the two answer different questions: physical cards can be traded and
/// have a location in a binder, digital ones cannot. Importing a digital collection is not
/// possible today (there is no Arena API), but the split exists so that adding it later does
/// not mean migrating everyone's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    Physical,
    Digital,
}

impl Pool {
    pub const ALL: [Pool; 2] = [Pool::Physical, Pool::Digital];

    pub const fn as_str(self) -> &'static str {
        match self {
            Pool::Physical => "physical",
            Pool::Digital => "digital",
        }
    }
}

/// How a printed card is finished.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    #[default]
    Nonfoil,
    Foil,
    Etched,
}

/// Condition of a physical card, using the grades every marketplace uses.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    #[default]
    NearMint,
    LightlyPlayed,
    ModeratelyPlayed,
    HeavilyPlayed,
    Damaged,
}

impl Condition {
    pub const ALL: [Condition; 5] = [
        Condition::NearMint,
        Condition::LightlyPlayed,
        Condition::ModeratelyPlayed,
        Condition::HeavilyPlayed,
        Condition::Damaged,
    ];

    /// The abbreviation used on price lists and trade binders.
    pub const fn short_code(self) -> &'static str {
        match self {
            Condition::NearMint => "NM",
            Condition::LightlyPlayed => "LP",
            Condition::ModeratelyPlayed => "MP",
            Condition::HeavilyPlayed => "HP",
            Condition::Damaged => "DMG",
        }
    }
}

/// Where a physical card lives.
///
/// Free-form on purpose: people organise by binder, by box, by deck, by shoebox. Imposing a
/// scheme would just mean fighting it. During a scan session the location is sticky and the
/// slot increments, so nothing has to be typed per card.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StorageLocation {
    /// e.g. "Binder 3", "Blue deck box".
    pub container: String,
    /// e.g. "page 12".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Position within the section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u16>,
}

impl StorageLocation {
    pub fn new(container: impl Into<String>) -> StorageLocation {
        StorageLocation {
            container: container.into(),
            section: None,
            slot: None,
        }
    }

    pub fn with_section(mut self, section: impl Into<String>) -> StorageLocation {
        self.section = Some(section.into());
        self
    }

    pub fn with_slot(mut self, slot: u16) -> StorageLocation {
        self.slot = Some(slot);
        self
    }

    /// The next slot in the same section, for sticky scanning.
    pub fn next_slot(&self) -> StorageLocation {
        StorageLocation {
            container: self.container.clone(),
            section: self.section.clone(),
            slot: Some(self.slot.map_or(1, |s| s.saturating_add(1))),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.container.trim().is_empty() && self.section.is_none() && self.slot.is_none()
    }
}

impl std::fmt::Display for StorageLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.container)?;
        if let Some(section) = &self.section {
            write!(f, ", {section}")?;
        }
        if let Some(slot) = self.slot {
            write!(f, " #{slot}")?;
        }
        Ok(())
    }
}

/// Identifier of a holding, unique within one collection database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HoldingId(pub u64);

impl std::fmt::Display for HoldingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A stack of interchangeable copies of one card.
///
/// Two copies of the same card in different binders, or in different conditions, are two
/// holdings. Copies that differ in nothing worth tracking share one holding and a quantity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holding {
    pub id: HoldingId,
    pub pool: Pool,
    /// Scryfall oracle id — stable across catalog rebuilds. See the module docs.
    pub oracle_id: String,
    /// Denormalised so a collection stays readable even without a catalog loaded.
    pub name: String,
    /// Which printing, when known. Empty means "unspecified".
    #[serde(default)]
    pub set_code: String,
    #[serde(default)]
    pub collector_number: String,
    /// Two-letter Scryfall language code. `"en"` by default; French cards are `"fr"`.
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub finish: Finish,
    #[serde(default)]
    pub condition: Condition,
    pub quantity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<StorageLocation>,
    #[serde(default)]
    pub notes: String,
}

fn default_language() -> String {
    "en".to_owned()
}

impl Holding {
    /// Fields that make two stacks interchangeable.
    ///
    /// Used to merge an addition into an existing holding instead of creating a near-duplicate
    /// row every time the same card is scanned.
    pub fn merge_key(&self) -> MergeKey<'_> {
        MergeKey {
            pool: self.pool,
            oracle_id: &self.oracle_id,
            set_code: &self.set_code,
            collector_number: &self.collector_number,
            language: &self.language,
            finish: self.finish,
            condition: self.condition,
            location: self.location.as_ref(),
        }
    }
}

/// Borrowed view of the fields that identify interchangeable copies.
#[derive(Debug, PartialEq, Eq)]
pub struct MergeKey<'a> {
    pub pool: Pool,
    pub oracle_id: &'a str,
    pub set_code: &'a str,
    pub collector_number: &'a str,
    pub language: &'a str,
    pub finish: Finish,
    pub condition: Condition,
    pub location: Option<&'a StorageLocation>,
}

/// What to add to a collection. `quantity` is added to any matching holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewHolding {
    pub pool: Pool,
    pub oracle_id: String,
    pub name: String,
    #[serde(default)]
    pub set_code: String,
    #[serde(default)]
    pub collector_number: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub finish: Finish,
    #[serde(default)]
    pub condition: Condition,
    pub quantity: u32,
    #[serde(default)]
    pub location: Option<StorageLocation>,
    #[serde(default)]
    pub notes: String,
}

impl NewHolding {
    /// A single English non-foil copy in near-mint condition, with no printing specified.
    pub fn single(pool: Pool, oracle_id: impl Into<String>, name: impl Into<String>) -> NewHolding {
        NewHolding {
            pool,
            oracle_id: oracle_id.into(),
            name: name.into(),
            set_code: String::new(),
            collector_number: String::new(),
            language: default_language(),
            finish: Finish::default(),
            condition: Condition::default(),
            quantity: 1,
            location: None,
            notes: String::new(),
        }
    }

    pub fn quantity(mut self, quantity: u32) -> NewHolding {
        self.quantity = quantity;
        self
    }

    pub fn printing(
        mut self,
        set_code: impl Into<String>,
        collector_number: impl Into<String>,
    ) -> NewHolding {
        self.set_code = set_code.into();
        self.collector_number = collector_number.into();
        self
    }

    pub fn language(mut self, language: impl Into<String>) -> NewHolding {
        self.language = language.into();
        self
    }

    pub fn finish(mut self, finish: Finish) -> NewHolding {
        self.finish = finish;
        self
    }

    pub fn condition(mut self, condition: Condition) -> NewHolding {
        self.condition = condition;
        self
    }

    pub fn at(mut self, location: StorageLocation) -> NewHolding {
        self.location = Some(location);
        self
    }

    pub(crate) fn into_holding(self, id: HoldingId) -> Holding {
        Holding {
            id,
            pool: self.pool,
            oracle_id: self.oracle_id,
            name: self.name,
            set_code: self.set_code,
            collector_number: self.collector_number,
            language: self.language,
            finish: self.finish,
            condition: self.condition,
            quantity: self.quantity,
            location: self.location,
            notes: self.notes,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn location_display() {
        assert_eq!(StorageLocation::new("Binder 3").to_string(), "Binder 3");
        assert_eq!(
            StorageLocation::new("Binder 3")
                .with_section("page 12")
                .to_string(),
            "Binder 3, page 12"
        );
        assert_eq!(
            StorageLocation::new("Binder 3")
                .with_section("page 12")
                .with_slot(4)
                .to_string(),
            "Binder 3, page 12 #4"
        );
    }

    #[test]
    fn sticky_scanning_increments_the_slot() {
        let start = StorageLocation::new("Binder 3").with_section("page 12");
        let first = start.next_slot();
        assert_eq!(first.slot, Some(1));

        let second = first.next_slot();
        assert_eq!(second.slot, Some(2));
        assert_eq!(second.container, "Binder 3");
        assert_eq!(second.section.as_deref(), Some("page 12"));
    }

    #[test]
    fn slot_increment_saturates_rather_than_wrapping() {
        // Nobody has 65,535 cards on one page, but wrapping to 0 would silently scramble
        // locations, and saturating is free.
        let last = StorageLocation::new("Box").with_slot(u16::MAX);
        assert_eq!(last.next_slot().slot, Some(u16::MAX));
    }

    #[test]
    fn condition_codes_are_the_usual_ones() {
        let codes: Vec<&str> = Condition::ALL.iter().map(|c| c.short_code()).collect();
        assert_eq!(codes, ["NM", "LP", "MP", "HP", "DMG"]);
    }

    #[test]
    fn new_holdings_default_to_one_english_near_mint_copy() {
        let holding = NewHolding::single(Pool::Physical, "oracle-1", "Sol Ring");
        assert_eq!(holding.quantity, 1);
        assert_eq!(holding.language, "en");
        assert_eq!(holding.finish, Finish::Nonfoil);
        assert_eq!(holding.condition, Condition::NearMint);
    }

    #[test]
    fn holdings_serialize_round_trip() {
        let holding = NewHolding::single(Pool::Physical, "oracle-1", "Sol Ring")
            .quantity(3)
            .printing("2xm", "263")
            .language("fr")
            .finish(Finish::Foil)
            .condition(Condition::LightlyPlayed)
            .at(StorageLocation::new("Binder 3")
                .with_section("page 12")
                .with_slot(4))
            .into_holding(HoldingId(7));

        let json = serde_json::to_vec(&holding).expect("serialize");
        let back: Holding = serde_json::from_slice(&json).expect("deserialize");
        assert_eq!(back, holding);
    }

    #[test]
    fn missing_optional_fields_deserialize_to_defaults() {
        // Older databases will not have every field. Reading one must not fail.
        let json = r#"{"id":1,"pool":"physical","oracle_id":"o","name":"X","quantity":2}"#;
        let holding: Holding = serde_json::from_str(json).expect("deserialize");
        assert_eq!(holding.language, "en");
        assert_eq!(holding.finish, Finish::Nonfoil);
        assert_eq!(holding.condition, Condition::NearMint);
        assert!(holding.location.is_none());
    }
}
