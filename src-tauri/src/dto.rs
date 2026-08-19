//! What crosses the boundary to the frontend.
//!
//! These types exist so the UI never sees archived rkyv types or borrowed data. They are plain
//! owned structs, serialized to JSON by Tauri.

use mtg_core::Format;
use mtg_data::ArchivedCard;
use serde::{Deserialize, Serialize};

/// Builds a Scryfall CDN image URL from a printing id.
///
/// **No artwork is stored or redistributed by this project** — images are fetched from
/// Scryfall's CDN on demand, and the app's content security policy allows exactly that one
/// host. See the legal note in `CLAUDE.md`.
///
/// The CDN lays images out by the first two characters of the id.
///
/// `pub(crate)` because the scanner needs it too: a recognised card is presented full-frame over
/// its own artwork, which is the one thing that makes that screen worth looking at.
pub(crate) fn image_url(image_id: &str, size: &str) -> Option<String> {
    let mut chars = image_id.chars();
    let a = chars.next()?;
    let b = chars.next()?;
    Some(format!(
        "https://cards.scryfall.io/{size}/front/{a}/{b}/{image_id}.jpg"
    ))
}

/// One row in a result list.
#[derive(Debug, Clone, Serialize)]
pub struct CardSummary {
    pub oracle_id: String,
    pub name: String,
    pub mana_cost: String,
    pub mana_value: f32,
    pub type_line: String,
    pub colors: String,
    pub color_identity: String,
    pub set_code: String,
    pub collector_number: String,
    pub game_changer: bool,
    pub edhrec_rank: Option<u32>,
    pub faces: usize,
    /// Small image, for list thumbnails.
    pub image_small: Option<String>,
}

impl CardSummary {
    pub fn from_archived(card: &ArchivedCard) -> CardSummary {
        CardSummary {
            oracle_id: card.oracle_id().to_owned(),
            name: card.name().to_owned(),
            mana_cost: card.mana_cost_display().to_owned(),
            mana_value: card.mana_value(),
            type_line: card.type_line().to_owned(),
            colors: card.colors().to_string(),
            color_identity: card.color_identity().to_string(),

            set_code: card.set_code.to_string(),
            collector_number: card.collector_number.to_string(),
            game_changer: card.is_game_changer(),
            edhrec_rank: card.edhrec_rank(),
            faces: card.faces().len(),
            image_small: image_url(&card.image_id, "small"),
        }
    }
}

/// One face of a card, for the detail view.
#[derive(Debug, Clone, Serialize)]
pub struct FaceView {
    pub name: String,
    pub mana_cost: String,
    pub type_line: String,
    pub oracle_text: String,
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
}

/// Everything the detail panel shows.
#[derive(Debug, Clone, Serialize)]
pub struct CardDetails {
    #[serde(flatten)]
    pub summary: CardSummary,
    pub oracle_text: String,
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
    pub keywords: Vec<String>,
    /// What the card does, as readable labels. Empty means **nothing is known**, not that the
    /// card does nothing — the tagger is crowdsourced and its coverage is uneven.
    pub tags: Vec<String>,
    pub rarity: String,
    pub reserved: bool,
    pub layout: String,
    pub released_at: String,
    /// Formats the card is playable in, by display name.
    pub legal_formats: Vec<String>,
    /// Formats where it is banned, so a deck builder is told why rather than just "no".
    pub banned_formats: Vec<String>,
    pub restricted_formats: Vec<String>,
    pub face_views: Vec<FaceView>,
    /// Larger image, for the detail panel.
    pub image_normal: Option<String>,
    /// The artwork alone, without the frame or the rules box.
    ///
    /// A whole card makes a poor background — it carries its own border, title and text, and
    /// anything laid over it collides with them. `art_crop` is the painting on its own, which is
    /// what a full-frame view needs. This is display only; `arthashes.bin` still fingerprints the
    /// `normal` image, and the two must not be confused.
    pub image_art: Option<String>,
}

impl CardDetails {
    pub fn from_archived(card: &ArchivedCard) -> CardDetails {
        let mut legal_formats = Vec::new();
        let mut banned_formats = Vec::new();
        let mut restricted_formats = Vec::new();
        for format in Format::ALL {
            match card.legality(format) {
                mtg_core::Legality::Legal => legal_formats.push(format.display_name().to_owned()),
                mtg_core::Legality::Restricted => {
                    restricted_formats.push(format.display_name().to_owned());
                }
                mtg_core::Legality::Banned => banned_formats.push(format.display_name().to_owned()),
                mtg_core::Legality::NotLegal => {}
            }
        }

        CardDetails {
            summary: CardSummary::from_archived(card),
            oracle_text: card.oracle_text().to_owned(),
            power: card.power.as_ref().map(ToString::to_string),
            toughness: card.toughness.as_ref().map(ToString::to_string),
            loyalty: card.loyalty.as_ref().map(ToString::to_string),
            keywords: card.keywords.iter().map(ToString::to_string).collect(),
            tags: card
                .tags()
                .iter()
                .map(|tag| tag.label().to_owned())
                .collect(),
            rarity: format!("{:?}", card.rarity()),
            reserved: card.reserved,
            layout: format!("{:?}", card.layout),
            released_at: card.released_at.to_string(),
            legal_formats,
            banned_formats,
            restricted_formats,
            face_views: card
                .faces()
                .iter()
                .map(|face| FaceView {
                    name: face.name().to_owned(),
                    mana_cost: face.mana_cost_display().to_owned(),
                    type_line: face.type_line().to_owned(),
                    oracle_text: face.oracle_text().to_owned(),
                    power: face.power.as_ref().map(ToString::to_string),
                    toughness: face.toughness.as_ref().map(ToString::to_string),
                    loyalty: face.loyalty.as_ref().map(ToString::to_string),
                })
                .collect(),
            image_normal: image_url(&card.image_id, "normal"),
            image_art: image_url(&card.image_id, "art_crop"),
        }
    }
}

/// Search filters, as the UI sends them. Every field is optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SearchRequest {
    pub text: Option<String>,
    pub card_types: Vec<String>,
    /// Colour identity to stay within, as WUBRG letters.
    pub identity: Option<String>,
    /// Scryfall format key, e.g. `commander`.
    pub format: Option<String>,
    pub min_mana_value: Option<f32>,
    pub max_mana_value: Option<f32>,
    pub game_changers_only: bool,
    pub commanders_only: bool,
    /// Restrict to cards present in the collection.
    pub owned_only: bool,
    pub limit: Option<usize>,
}

/// A page of results plus the unclipped total, so the UI can say "50 of 480".
#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub total: usize,
    pub cards: Vec<CardSummary>,
}

/// What the app knows about its card data.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatus {
    pub loaded: bool,
    pub cards: usize,
    /// Scryfall's build timestamp for the source bulk file.
    pub source_updated_at: String,
    /// Where the artifact was found, or where it is expected.
    pub path: String,
    /// Set when loading failed, so the UI can explain rather than just showing nothing.
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_urls_follow_the_cdn_layout() {
        assert_eq!(
            image_url("e0deb2a2-3f5e-4b8d-b3d7-6b4d2c8a1f00", "normal").as_deref(),
            Some("https://cards.scryfall.io/normal/front/e/0/e0deb2a2-3f5e-4b8d-b3d7-6b4d2c8a1f00.jpg")
        );
    }

    #[test]
    fn the_art_crop_is_a_different_url_from_the_whole_card() {
        // Full-frame views use `art_crop`; a whole card carries its own border, title and text
        // box, and a name laid over one collides with them. Confusing the two is the single
        // easiest way to undo the direction — see docs/dev/design.md.
        let id = "e0deb2a2-3f5e-4b8d-b3d7-6b4d2c8a1f00";
        let art = image_url(id, "art_crop").expect("a well-formed id has a url");
        let whole = image_url(id, "normal").expect("a well-formed id has a url");
        assert!(art.contains("/art_crop/"));
        assert_ne!(art, whole);
    }

    #[test]
    fn cards_without_an_image_id_have_no_url() {
        // Rather than emitting a broken URL the UI would render as a missing image.
        assert_eq!(image_url("", "normal"), None);
        assert_eq!(image_url("a", "normal"), None);
    }
}
