//! Converting Scryfall card objects into the archived catalog model.
//!
//! Everything here is deliberately forgiving. Scryfall adds fields, retires formats and ships
//! layouts we have never seen; a converter that insisted on a fixed shape would break on the
//! next set release. Unknown values fall back to safe defaults, and anything that would
//! silently lose data is *reported* instead — see [`Conversion::unknown_legality_keys`].

use std::collections::{BTreeSet, HashMap};

use mtg_core::{Color, ColorSet, Format, Legality, Rarity};
use mtg_data::{legality_to_u8, rarity_to_u8, Card, CardFace, Layout, LEGALITY_SLOTS};
use serde::Deserialize;

/// A card object as Scryfall sends it. Only the fields we keep are declared.
#[derive(Debug, Deserialize)]
pub struct ScryfallCard {
    pub id: String,
    #[serde(default)]
    pub oracle_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub cmc: Option<f32>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    #[serde(default)]
    pub color_identity: Vec<String>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub legalities: HashMap<String, String>,
    #[serde(default)]
    pub rarity: Option<String>,
    #[serde(default)]
    pub edhrec_rank: Option<u32>,
    #[serde(default)]
    pub game_changer: bool,
    #[serde(default)]
    pub reserved: bool,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub card_faces: Option<Vec<ScryfallFace>>,
    #[serde(default)]
    pub set: Option<String>,
    #[serde(default)]
    pub collector_number: Option<String>,
    #[serde(default)]
    pub released_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScryfallFace {
    pub name: String,
    #[serde(default)]
    pub mana_cost: Option<String>,
    #[serde(default)]
    pub type_line: Option<String>,
    #[serde(default)]
    pub oracle_text: Option<String>,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub loyalty: Option<String>,
    #[serde(default)]
    pub colors: Option<Vec<String>>,
}

/// Layouts that are not real cards and never belong in a deck.
const NON_CARD_LAYOUTS: [&str; 4] = ["token", "double_faced_token", "emblem", "art_series"];

/// Running totals and warnings from a conversion pass.
#[derive(Debug, Default)]
pub struct Conversion {
    pub converted: usize,
    pub skipped_non_cards: usize,
    /// Legality keys Scryfall sent that we do not model.
    ///
    /// Each one means a whole format's legality is being dropped. Reported loudly rather than
    /// swallowed, because the failure is otherwise invisible: cards simply stop showing up in
    /// searches for that format.
    pub unknown_legality_keys: BTreeSet<String>,
    /// Layout strings we collapsed into [`Layout::Other`]. Informational.
    pub unknown_layouts: BTreeSet<String>,
}

impl Conversion {
    /// Converts one Scryfall card, or returns `None` if it is not a real card.
    pub fn convert(&mut self, raw: ScryfallCard) -> Option<Card> {
        let layout_str = raw.layout.unwrap_or_else(|| "normal".to_owned());
        if NON_CARD_LAYOUTS.contains(&layout_str.as_str()) {
            self.skipped_non_cards += 1;
            return None;
        }

        let layout = Layout::from_scryfall_value(&layout_str);
        if layout == Layout::Other && layout_str != "normal" {
            self.unknown_layouts.insert(layout_str);
        }

        let mut legalities = [legality_to_u8(Legality::NotLegal); LEGALITY_SLOTS];
        for (key, value) in &raw.legalities {
            match Format::from_scryfall_key(key) {
                Some(format) => {
                    legalities[format.index()] =
                        legality_to_u8(Legality::from_scryfall_value(value));
                }
                None => {
                    self.unknown_legality_keys.insert(key.clone());
                }
            }
        }

        let type_line = raw.type_line.unwrap_or_default();
        let oracle_text = raw.oracle_text.unwrap_or_default();
        let mana_cost = raw.mana_cost.unwrap_or_default();
        let colors = color_set(raw.colors.as_deref());

        let faces = match raw.card_faces {
            Some(raw_faces) if !raw_faces.is_empty() => raw_faces
                .into_iter()
                .map(|f| CardFace {
                    name: f.name,
                    mana_cost: f.mana_cost.unwrap_or_default(),
                    type_line: f.type_line.unwrap_or_default(),
                    oracle_text: f.oracle_text.unwrap_or_default(),
                    power: f.power,
                    toughness: f.toughness,
                    loyalty: f.loyalty,
                    colors: color_set(f.colors.as_deref()).bits(),
                })
                .collect(),
            // Single-faced cards still get a face, so consumers never branch on face count.
            _ => vec![CardFace {
                name: raw.name.clone(),
                mana_cost: mana_cost.clone(),
                type_line: type_line.clone(),
                oracle_text: oracle_text.clone(),
                power: raw.power.clone(),
                toughness: raw.toughness.clone(),
                loyalty: raw.loyalty.clone(),
                colors: colors.bits(),
            }],
        };

        self.converted += 1;
        Some(Card {
            // Reversible cards carry oracle ids on their faces rather than at the top level;
            // falling back to the printing id keeps every card addressable.
            oracle_id: raw.oracle_id.unwrap_or_else(|| raw.id.clone()),
            name: raw.name,
            mana_cost,
            mana_value: raw.cmc.unwrap_or(0.0),
            colors: colors.bits(),
            color_identity: color_set(Some(&raw.color_identity)).bits(),
            type_line,
            oracle_text,
            power: raw.power,
            toughness: raw.toughness,
            loyalty: raw.loyalty,
            keywords: raw.keywords,
            legalities,
            rarity: rarity_to_u8(
                raw.rarity
                    .as_deref()
                    .map(Rarity::from_scryfall_value)
                    .unwrap_or_default(),
            ),
            edhrec_rank: raw.edhrec_rank,
            game_changer: raw.game_changer,
            reserved: raw.reserved,
            layout,
            faces,
            set_code: raw.set.unwrap_or_default(),
            collector_number: raw.collector_number.unwrap_or_default(),
            released_at: raw.released_at.unwrap_or_default(),
            // The printing id is what builds a Scryfall CDN image URL. No artwork is stored.
            image_id: raw.id,
        })
    }
}

fn color_set(symbols: Option<&[String]>) -> ColorSet {
    let Some(symbols) = symbols else {
        return ColorSet::COLORLESS;
    };
    ColorSet::from_colors(
        symbols
            .iter()
            .filter_map(|s| s.chars().next())
            .filter_map(Color::from_symbol),
    )
}

#[cfg(test)]
mod tests {

    use super::*;

    fn parse(json: &str) -> ScryfallCard {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn converts_a_plain_card() {
        // Trimmed from the real Mana Vault object.
        let raw = parse(
            r#"{
                "id": "e0deb2a2-3f5e-4b8d-b3d7-6b4d2c8a1f00",
                "oracle_id": "1c78ac6f-2a0f-4d8e-8f2f-3b6d1a2c9e77",
                "name": "Mana Vault",
                "mana_cost": "{1}",
                "cmc": 1.0,
                "colors": [],
                "color_identity": [],
                "type_line": "Artifact",
                "oracle_text": "Mana Vault doesn't untap during your untap step.",
                "keywords": [],
                "legalities": {"commander": "legal", "modern": "not_legal", "vintage": "restricted"},
                "rarity": "rare",
                "edhrec_rank": 146,
                "game_changer": true,
                "reserved": false,
                "layout": "normal",
                "set": "2xm",
                "collector_number": "263",
                "released_at": "2020-08-07"
            }"#,
        );

        let mut conversion = Conversion::default();
        let card = conversion.convert(raw).unwrap();

        assert_eq!(card.name, "Mana Vault");
        assert_eq!(card.mana_value, 1.0);
        assert!(card.game_changer, "Scryfall flags Game Changers itself");
        assert_eq!(card.edhrec_rank, Some(146));
        assert_eq!(
            card.legalities[Format::Commander.index()],
            legality_to_u8(Legality::Legal)
        );
        assert_eq!(
            card.legalities[Format::Vintage.index()],
            legality_to_u8(Legality::Restricted)
        );
        assert_eq!(
            card.legalities[Format::Modern.index()],
            legality_to_u8(Legality::NotLegal)
        );
        // Formats absent from the object default to not legal, never to legal.
        assert_eq!(
            card.legalities[Format::Legacy.index()],
            legality_to_u8(Legality::NotLegal)
        );
        assert_eq!(card.faces.len(), 1, "single-faced cards still get one face");
        assert_eq!(conversion.converted, 1);
    }

    #[test]
    fn converts_a_transforming_card() {
        // Transform cards have no top-level mana cost and no top-level colors.
        let raw = parse(
            r#"{
                "id": "aaaa",
                "oracle_id": "bbbb",
                "name": "Delver of Secrets // Insectile Aberration",
                "cmc": 1.0,
                "color_identity": ["U"],
                "layout": "transform",
                "legalities": {"modern": "legal"},
                "rarity": "common",
                "card_faces": [
                    {"name": "Delver of Secrets", "mana_cost": "{U}", "type_line": "Creature — Human Wizard", "oracle_text": "...", "colors": ["U"]},
                    {"name": "Insectile Aberration", "mana_cost": "", "type_line": "Creature — Human Insect", "oracle_text": "Flying", "colors": ["U"]}
                ]
            }"#,
        );

        let mut conversion = Conversion::default();
        let card = conversion.convert(raw).unwrap();

        assert_eq!(card.layout, Layout::Transform);
        assert_eq!(card.faces.len(), 2);
        assert_eq!(card.faces[1].oracle_text, "Flying");
        assert!(card.mana_cost.is_empty());
        assert_eq!(
            ColorSet::from_bits(card.color_identity),
            ColorSet::from_symbols("U")
        );
    }

    #[test]
    fn unknown_legality_keys_are_reported_not_swallowed() {
        // The whole point: when Scryfall adds a format, we find out at build time.
        let raw = parse(
            r#"{
                "id": "a", "name": "X", "cmc": 0.0, "color_identity": [],
                "legalities": {"commander": "legal", "some_new_format": "legal"},
                "rarity": "common", "layout": "normal"
            }"#,
        );

        let mut conversion = Conversion::default();
        conversion.convert(raw).unwrap();

        assert!(conversion.unknown_legality_keys.contains("some_new_format"));
        assert_eq!(conversion.unknown_legality_keys.len(), 1);
    }

    #[test]
    fn unknown_legality_values_are_not_treated_as_legal() {
        let raw = parse(
            r#"{
                "id": "a", "name": "X", "cmc": 0.0, "color_identity": [],
                "legalities": {"commander": "some_new_status"},
                "rarity": "common", "layout": "normal"
            }"#,
        );

        let mut conversion = Conversion::default();
        let card = conversion.convert(raw).unwrap();
        assert_eq!(
            card.legalities[Format::Commander.index()],
            legality_to_u8(Legality::NotLegal)
        );
    }

    #[test]
    fn tokens_and_emblems_are_skipped() {
        for layout in NON_CARD_LAYOUTS {
            let raw = parse(&format!(
                r#"{{"id": "a", "name": "X", "color_identity": [], "layout": "{layout}"}}"#
            ));
            let mut conversion = Conversion::default();
            assert!(conversion.convert(raw).is_none(), "{layout}");
            assert_eq!(conversion.skipped_non_cards, 1);
        }
    }

    #[test]
    fn unmodelled_layouts_are_recorded_and_kept() {
        let raw = parse(
            r#"{"id": "a", "name": "X", "color_identity": [], "layout": "planar", "rarity": "common"}"#,
        );
        let mut conversion = Conversion::default();
        let card = conversion.convert(raw).unwrap();

        assert_eq!(card.layout, Layout::Other);
        assert!(conversion.unknown_layouts.contains("planar"));
    }

    #[test]
    fn missing_optional_fields_do_not_fail_the_conversion() {
        // The minimum an object could plausibly shrink to. A converter that required fields
        // would break the whole build on one odd card.
        let raw = parse(r#"{"id": "a", "name": "Nameless", "color_identity": []}"#);
        let mut conversion = Conversion::default();
        let card = conversion.convert(raw).unwrap();

        assert_eq!(card.name, "Nameless");
        assert_eq!(card.mana_value, 0.0);
        assert_eq!(card.layout, Layout::Normal);
        assert_eq!(card.oracle_id, "a", "falls back to the printing id");
    }

    #[test]
    fn unknown_fields_in_the_payload_are_ignored() {
        // Scryfall adds fields regularly; that must never break the build.
        let raw = parse(
            r#"{"id": "a", "name": "X", "color_identity": [], "a_field_from_2027": {"nested": true}}"#,
        );
        let mut conversion = Conversion::default();
        assert!(conversion.convert(raw).is_some());
    }
}
