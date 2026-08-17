//! Reading and writing decklists as text.
//!
//! There is no decklist standard, only conventions, and every site exports a slightly
//! different one. Rather than ask which tool a list came from, the parser accepts all of them
//! at once — they do not actually conflict:
//!
//! ```text
//! Deck                              Arena and Moxfield use section headers
//! 4 Lightning Bolt (M21) 137        with an optional printing
//! 20 Mountain
//!
//! Sideboard
//! 2 Pyroblast
//!
//! SB: 2 Pyroblast                   MTGO marks the sideboard per line instead
//! 4x Lightning Bolt                 plain text often writes "4x"
//! // a comment                      and .dec comments start with //
//! ```
//!
//! Anything it cannot make sense of is reported rather than dropped, because a decklist that
//! silently imports 58 of 60 cards is worse than one that refuses.

use mtg_core::Format;
use mtg_data::{Catalog, Resolution};
use serde::{Deserialize, Serialize};

use crate::deck::{Deck, DeckEntry, Zone};

/// Something the importer could not do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportProblem {
    /// The line is not blank, not a comment, and not `<quantity> <name>`.
    UnrecognizedLine { line: usize, text: String },
    /// No card of that name is in the catalog.
    UnknownCard { line: usize, name: String },
    /// The name is a face shared by several cards, so the importer will not guess.
    AmbiguousCard {
        line: usize,
        name: String,
        candidates: Vec<String>,
    },
}

impl std::fmt::Display for ImportProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportProblem::UnrecognizedLine { line, text } => {
                write!(f, "line {line}: could not read {text:?}")
            }
            ImportProblem::UnknownCard { line, name } => {
                write!(f, "line {line}: no card named {name:?}")
            }
            ImportProblem::AmbiguousCard {
                line,
                name,
                candidates,
            } => write!(
                f,
                "line {line}: {name:?} could be {} — write the full name",
                candidates.join(" or ")
            ),
        }
    }
}

/// The outcome of importing a decklist.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub deck: Deck,
    /// Empty when everything was understood. Never silently discarded.
    pub problems: Vec<ImportProblem>,
}

impl ImportResult {
    pub fn is_complete(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Parses a decklist and resolves it against the catalog.
pub fn import(text: &str, name: &str, format: Format, catalog: &Catalog) -> ImportResult {
    let mut deck = Deck::new(name, format);
    let mut problems = Vec::new();

    // Where unmarked lines go. Lists that open straight into cards are main deck; a header
    // changes it from there on.
    let mut zone = Zone::Main;

    for (index, raw) in text.lines().enumerate() {
        let line_number = index + 1;

        match classify(raw) {
            Line::Blank => {}
            Line::Section(section) => zone = section,
            Line::Unrecognized(text) => problems.push(ImportProblem::UnrecognizedLine {
                line: line_number,
                text,
            }),
            Line::Entry(entry) => {
                let target = entry.zone.unwrap_or(zone);
                match catalog.resolve(&entry.name) {
                    Resolution::Found(_, card) => {
                        deck.add(
                            DeckEntry::new(card.oracle_id(), card.name(), entry.quantity)
                                .in_zone(target)
                                .printed_as(entry.set_code, entry.collector_number),
                        );
                    }
                    Resolution::Ambiguous(candidates) => {
                        problems.push(ImportProblem::AmbiguousCard {
                            line: line_number,
                            name: entry.name,
                            candidates: candidates
                                .iter()
                                .map(|(_, c)| c.name().to_owned())
                                .collect(),
                        });
                    }
                    Resolution::NotFound => problems.push(ImportProblem::UnknownCard {
                        line: line_number,
                        name: entry.name,
                    }),
                }
            }
        }
    }

    ImportResult { deck, problems }
}

/// A parsed line, before any catalog lookup.
#[derive(Debug, PartialEq, Eq)]
enum Line {
    Blank,
    Section(Zone),
    Entry(ParsedEntry),
    Unrecognized(String),
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedEntry {
    quantity: u32,
    name: String,
    set_code: String,
    collector_number: String,
    /// Set when the line marked its own zone, as MTGO's `SB:` does.
    zone: Option<Zone>,
}

fn classify(raw: &str) -> Line {
    let mut line = raw.trim();
    if line.is_empty() {
        return Line::Blank;
    }

    // `//` starts a comment in .dec files. It is also the separator inside a card name, but a
    // name never *starts* with it, so only a leading `//` is a comment.
    if let Some(rest) = line.strip_prefix("//") {
        // Exporters often write the section as a comment: "// Sideboard".
        return match section_of(rest.trim()) {
            Some(zone) => Line::Section(zone),
            None => Line::Blank,
        };
    }
    if let Some(rest) = line.strip_prefix('#') {
        return match section_of(rest.trim()) {
            Some(zone) => Line::Section(zone),
            None => Line::Blank,
        };
    }

    // MTGO marks sideboard cards per line rather than with a header.
    let mut zone = None;
    if line.len() >= 3 && line[..3].eq_ignore_ascii_case("sb:") {
        zone = Some(Zone::Sideboard);
        line = line[3..].trim_start();
    }

    match parse_entry(line) {
        Some(mut entry) => {
            entry.zone = zone;
            Line::Entry(entry)
        }
        // A line with no quantity is either a section header or something we cannot use.
        None => match section_of(line) {
            Some(section) => Line::Section(section),
            None => Line::Unrecognized(line.to_owned()),
        },
    }
}

fn section_of(text: &str) -> Option<Zone> {
    let cleaned = text.trim().trim_end_matches(':').trim().to_lowercase();
    match cleaned.as_str() {
        "deck" | "main" | "maindeck" | "mainboard" | "main deck" => Some(Zone::Main),
        // Companions live in the sideboard, which is where every exporter puts them.
        "sideboard" | "side" | "companion" => Some(Zone::Sideboard),
        "commander" | "commanders" | "command zone" | "oathbreaker" => Some(Zone::Command),
        _ => None,
    }
}

/// Parses `4 Lightning Bolt (M21) 137 *F*` into its pieces.
fn parse_entry(line: &str) -> Option<ParsedEntry> {
    let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let quantity: u32 = digits.parse().ok()?;

    let mut rest = line[digits.len()..].trim_start();
    // "4x Lightning Bolt", as plain-text lists tend to write it.
    if let Some(stripped) = rest.strip_prefix(['x', 'X']) {
        if stripped.starts_with(char::is_whitespace) {
            rest = stripped.trim_start();
        }
    }
    if rest.is_empty() {
        return None;
    }

    // Foil and etched markers, which Deckbox and Moxfield append.
    let mut name = rest;
    for marker in ["*F*", "*E*", "*f*", "*e*"] {
        name = name.trim_end().trim_end_matches(marker);
    }

    let (name, set_code, collector_number) = split_printing(name.trim_end());
    if name.is_empty() {
        return None;
    }

    Some(ParsedEntry {
        quantity,
        name: name.to_owned(),
        set_code,
        collector_number,
        zone: None,
    })
}

/// Splits a trailing `(SET) 137` off a card name.
///
/// Deliberately conservative: only a parenthesised group that looks like a set code counts.
/// Card names really do contain parentheses — `Erase (Not the Urza's Legacy One)` is a printed
/// card — and eating those would corrupt the name.
fn split_printing(text: &str) -> (&str, String, String) {
    let Some(open) = text.rfind('(') else {
        return (text.trim_end(), String::new(), String::new());
    };
    let Some(close) = text[open..].find(')').map(|i| open + i) else {
        return (text.trim_end(), String::new(), String::new());
    };

    let inside = &text[open + 1..close];
    let looks_like_a_set_code =
        (2..=6).contains(&inside.len()) && inside.chars().all(|c| c.is_ascii_alphanumeric());
    if !looks_like_a_set_code {
        return (text.trim_end(), String::new(), String::new());
    }

    let collector_number = text[close + 1..].trim().to_owned();
    (
        text[..open].trim_end(),
        inside.to_lowercase(),
        collector_number,
    )
}

/// How to write a decklist out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStyle {
    /// Section headers and bare names. What most sites accept on paste.
    Plain,
    /// Section headers with the printing, as Arena expects.
    Arena,
    /// Flat, with `SB:` prefixes, as MTGO `.dec` expects.
    Mtgo,
}

/// Writes a deck as text.
pub fn export(deck: &Deck, style: ExportStyle) -> String {
    let mut out = String::new();

    if style == ExportStyle::Mtgo {
        // MTGO has no headers and no command zone; commanders go in the main deck, which is
        // the closest the format can represent.
        for zone in [Zone::Command, Zone::Main] {
            for entry in deck.entries_in(zone) {
                out.push_str(&format!("{} {}\n", entry.quantity, entry.name));
            }
        }
        for entry in deck.entries_in(Zone::Sideboard) {
            out.push_str(&format!("SB: {} {}\n", entry.quantity, entry.name));
        }
        return out;
    }

    let mut first = true;
    for zone in [Zone::Command, Zone::Main, Zone::Sideboard] {
        let entries: Vec<&DeckEntry> = deck.entries_in(zone).collect();
        if entries.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;

        out.push_str(zone.label());
        out.push('\n');
        for entry in entries {
            match style {
                ExportStyle::Arena if !entry.set_code.is_empty() => out.push_str(&format!(
                    "{} {} ({}) {}\n",
                    entry.quantity,
                    entry.name,
                    entry.set_code.to_uppercase(),
                    entry.collector_number
                )),
                _ => out.push_str(&format!("{} {}\n", entry.quantity, entry.name)),
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::Legality;
    use mtg_data::{
        legality_to_u8, rarity_to_u8, Card, CardFace, CatalogData, Layout, LEGALITY_SLOTS,
    };

    fn card(name: &str, faces: &[&str]) -> Card {
        let mut card = Card {
            oracle_id: format!("oracle-{name}"),
            name: name.to_owned(),
            mana_cost: String::new(),
            mana_value: 0.0,
            colors: 0,
            color_identity: 0,
            type_line: "Instant".to_owned(),
            oracle_text: String::new(),
            power: None,
            toughness: None,
            loyalty: None,
            keywords: Vec::new(),
            legalities: [legality_to_u8(Legality::Legal); LEGALITY_SLOTS],
            rarity: rarity_to_u8(mtg_core::Rarity::Common),
            edhrec_rank: None,
            game_changer: false,
            reserved: false,
            layout: if faces.len() > 1 {
                Layout::Split
            } else {
                Layout::Normal
            },
            faces: Vec::new(),
            set_code: "tst".to_owned(),
            collector_number: "1".to_owned(),
            released_at: "2026-01-01".to_owned(),
            image_id: String::new(),
        };
        let face_names: Vec<&str> = if faces.is_empty() {
            vec![name]
        } else {
            faces.to_vec()
        };
        card.faces = face_names
            .into_iter()
            .map(|face_name| CardFace {
                name: face_name.to_owned(),
                mana_cost: String::new(),
                type_line: "Instant".to_owned(),
                oracle_text: String::new(),
                power: None,
                toughness: None,
                loyalty: None,
                colors: 0,
            })
            .collect();
        card
    }

    fn catalog() -> Catalog {
        let data = CatalogData {
            format_version: mtg_data::FORMAT_VERSION,
            source_updated_at: String::new(),
            cards: vec![
                card("Lightning Bolt", &[]),
                card("Mountain", &[]),
                card("Pyroblast", &[]),
                card("Krenko, Mob Boss", &[]),
                card(
                    "Bonecrusher Giant // Stomp",
                    &["Bonecrusher Giant", "Stomp"],
                ),
                card("Fire // Ice", &["Fire", "Ice"]),
                card("Start // Fire", &["Start", "Fire"]),
                card("Lim-Dûl's Vault", &[]),
            ],
        };
        Catalog::from_bytes(mtg_data::serialize(&data).unwrap()).unwrap()
    }

    fn import_text(text: &str) -> ImportResult {
        import(text, "Imported", Format::Modern, &catalog())
    }

    #[test]
    fn plain_list_with_no_headers_is_all_main_deck() {
        let result = import_text("4 Lightning Bolt\n20 Mountain\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Main), 24);
        assert_eq!(result.deck.count_in(Zone::Sideboard), 0);
    }

    #[test]
    fn the_x_suffix_is_accepted() {
        let result = import_text("4x Lightning Bolt\n2X Mountain\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Main), 6);
    }

    #[test]
    fn arena_sections_and_printings() {
        let result = import_text(
            "Deck\n4 Lightning Bolt (M21) 137\n20 Mountain (M21) 275\n\nSideboard\n2 Pyroblast (ICE) 213\n",
        );
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Main), 24);
        assert_eq!(result.deck.count_in(Zone::Sideboard), 2);

        let bolt = result
            .deck
            .entries_in(Zone::Main)
            .find(|e| e.name == "Lightning Bolt")
            .expect("bolt");
        assert_eq!(bolt.set_code, "m21");
        assert_eq!(bolt.collector_number, "137");
    }

    #[test]
    fn mtgo_marks_the_sideboard_per_line() {
        let result = import_text("4 Lightning Bolt\nSB: 2 Pyroblast\nsb: 1 Mountain\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Main), 4);
        assert_eq!(result.deck.count_in(Zone::Sideboard), 3);
    }

    #[test]
    fn commander_sections_land_in_the_command_zone() {
        let result = import_text("Commander\n1 Krenko, Mob Boss\n\nDeck\n99 Mountain\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Command), 1);
        assert_eq!(result.deck.count_in(Zone::Main), 99);
    }

    #[test]
    fn comments_are_ignored_but_commented_headers_are_not() {
        let result = import_text("// my burn deck\n4 Lightning Bolt\n// Sideboard\n2 Pyroblast\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Main), 4);
        assert_eq!(result.deck.count_in(Zone::Sideboard), 2);
    }

    #[test]
    fn foil_markers_are_stripped_from_the_name() {
        let result = import_text("1 Lightning Bolt *F*\n1 Mountain *E*\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Main), 2);
    }

    #[test]
    fn multi_part_cards_are_accepted_by_their_front_face() {
        // The common case: no exporter writes "Bonecrusher Giant // Stomp".
        let result = import_text("4 Bonecrusher Giant\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(
            result.deck.entries[0].name, "Bonecrusher Giant // Stomp",
            "stored under the full name"
        );
    }

    #[test]
    fn accents_and_apostrophes_do_not_have_to_match() {
        let result = import_text("1 Lim-Dul's Vault\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.entries[0].name, "Lim-Dûl's Vault");
    }

    #[test]
    fn an_unknown_card_is_reported_with_its_line() {
        let result = import_text("4 Lightning Bolt\n2 Not A Real Card\n");
        assert_eq!(
            result.problems,
            [ImportProblem::UnknownCard {
                line: 2,
                name: "Not A Real Card".to_owned()
            }]
        );
        // The rest of the list still imported.
        assert_eq!(result.deck.count_in(Zone::Main), 4);
    }

    #[test]
    fn an_ambiguous_face_name_is_reported_rather_than_guessed() {
        // "Fire" is a half of both Fire // Ice and Start // Fire.
        let result = import_text("2 Fire\n");
        match &result.problems[..] {
            [ImportProblem::AmbiguousCard {
                line,
                name,
                candidates,
            }] => {
                assert_eq!(*line, 1);
                assert_eq!(name, "Fire");
                assert_eq!(candidates.len(), 2);
            }
            other => panic!("expected ambiguity, got {other:?}"),
        }
        assert!(result.deck.is_empty(), "nothing guessed into the deck");
    }

    #[test]
    fn a_line_that_makes_no_sense_is_reported_not_dropped() {
        // A list that silently imports 58 of 60 cards is worse than one that complains.
        let result = import_text("4 Lightning Bolt\nsome stray text\n");
        assert_eq!(
            result.problems,
            [ImportProblem::UnrecognizedLine {
                line: 2,
                text: "some stray text".to_owned()
            }]
        );
    }

    #[test]
    fn parenthesised_card_names_are_not_mistaken_for_a_printing() {
        // "Erase (Not the Urza's Legacy One)" is a real printed card. The parenthesised part
        // is too long to be a set code, so it stays in the name.
        let (name, set, number) = split_printing("Erase (Not the Urza's Legacy One)");
        assert_eq!(name, "Erase (Not the Urza's Legacy One)");
        assert!(set.is_empty());
        assert!(number.is_empty());

        let (name, set, number) = split_printing("Lightning Bolt (M21) 137");
        assert_eq!(name, "Lightning Bolt");
        assert_eq!(set, "m21");
        assert_eq!(number, "137");
    }

    #[test]
    fn quantities_and_names_survive_odd_spacing() {
        let result = import_text("   4    Lightning Bolt   \n\t2\tMountain\n");
        assert!(result.is_complete(), "{:?}", result.problems);
        assert_eq!(result.deck.count_in(Zone::Main), 6);
    }

    // --- export ------------------------------------------------------------------------

    fn sample_deck() -> Deck {
        let mut deck = Deck::new("Krenko", Format::Commander);
        deck.add(
            DeckEntry::new("oracle-Krenko, Mob Boss", "Krenko, Mob Boss", 1)
                .in_zone(Zone::Command)
                .printed_as("m19", "149"),
        );
        deck.add(DeckEntry::new("oracle-Mountain", "Mountain", 30));
        deck.add(DeckEntry::new("oracle-Pyroblast", "Pyroblast", 2).in_zone(Zone::Sideboard));
        deck
    }

    #[test]
    fn plain_export_uses_section_headers() {
        assert_eq!(
            export(&sample_deck(), ExportStyle::Plain),
            "Commander\n1 Krenko, Mob Boss\n\nDeck\n30 Mountain\n\nSideboard\n2 Pyroblast\n"
        );
    }

    #[test]
    fn arena_export_includes_the_printing_when_known() {
        let text = export(&sample_deck(), ExportStyle::Arena);
        assert!(text.contains("1 Krenko, Mob Boss (M19) 149"), "{text}");
        // Mountain has no recorded printing, so it is written bare rather than with "() ".
        assert!(text.contains("30 Mountain\n"), "{text}");
    }

    #[test]
    fn mtgo_export_is_flat_with_sb_prefixes() {
        assert_eq!(
            export(&sample_deck(), ExportStyle::Mtgo),
            "1 Krenko, Mob Boss\n30 Mountain\nSB: 2 Pyroblast\n"
        );
    }

    #[test]
    fn export_and_import_round_trip() {
        let original =
            import_text("Deck\n4 Lightning Bolt\n20 Mountain\n\nSideboard\n2 Pyroblast\n");
        assert!(original.is_complete());

        for style in [ExportStyle::Plain, ExportStyle::Arena, ExportStyle::Mtgo] {
            let text = export(&original.deck, style);
            let back = import_text(&text);
            assert!(back.is_complete(), "{style:?}: {:?}", back.problems);
            assert_eq!(
                back.deck.count_in(Zone::Main),
                original.deck.count_in(Zone::Main),
                "{style:?}"
            );
            assert_eq!(
                back.deck.count_in(Zone::Sideboard),
                original.deck.count_in(Zone::Sideboard),
                "{style:?}"
            );
        }
    }
}
