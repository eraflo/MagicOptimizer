//! Deck construction rules, per format.
//!
//! Data rather than code: one struct describes every format, so adding one is a table entry
//! and not a new branch in the legality checker.
//!
//! Ban and restriction lists deliberately do **not** live here. They come from each card's
//! `legalities` in the catalog, which means they update with the card data instead of needing
//! a release whenever Wizards changes one.

use std::ops::RangeInclusive;

use mtg_core::Format;

/// How many cards the main deck must hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckSize {
    /// Exactly this many, as in Commander.
    Exactly(u32),
    /// This many or more, as in the 60-card formats.
    AtLeast(u32),
}

impl DeckSize {
    pub const fn minimum(self) -> u32 {
        match self {
            DeckSize::Exactly(n) | DeckSize::AtLeast(n) => n,
        }
    }
}

/// Rules for formats played with a commander.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommanderRules {
    /// How many cards belong in the command zone. Up to two covers partners, and Oathbreaker's
    /// planeswalker plus signature spell.
    pub count: RangeInclusive<u32>,
    /// Whether every card must sit inside the commander's colour identity.
    pub enforce_color_identity: bool,
    /// Whether the command zone counts towards the deck size. In Commander it does: 99 plus a
    /// commander is a 100-card deck.
    pub counts_towards_deck_size: bool,
}

/// Everything needed to check a deck, for one format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatRules {
    pub format: Format,
    pub deck_size: DeckSize,
    /// Copies of any one card. Basic lands are exempt; the checker knows which cards those are
    /// because it has the catalog and this struct does not.
    pub max_copies: u32,
    pub max_sideboard: u32,
    pub commander: Option<CommanderRules>,
    /// True when the format's rules here are inferred rather than confirmed against an
    /// official document. Surfaced in the UI so a wrong check is not presented as certainty.
    pub approximate: bool,
}

impl FormatRules {
    /// Rules for a format.
    pub fn for_format(format: Format) -> FormatRules {
        match format {
            // Sixty-card constructed. The bulk of the list, and all the same shape.
            Format::Standard
            | Format::Future
            | Format::Historic
            | Format::Timeless
            | Format::Pioneer
            | Format::Modern
            | Format::Legacy
            | Format::Vintage
            | Format::Pauper
            | Format::Penny
            | Format::Alchemy
            | Format::OldSchool
            | Format::Premodern => FormatRules {
                format,
                deck_size: DeckSize::AtLeast(60),
                max_copies: 4,
                max_sideboard: 15,
                commander: None,
                approximate: false,
            },

            // Hundred-card singleton with a commander.
            Format::Commander | Format::Duel | Format::PauperCommander | Format::Predh => {
                FormatRules {
                    format,
                    deck_size: DeckSize::Exactly(100),
                    max_copies: 1,
                    max_sideboard: 0,
                    commander: Some(CommanderRules {
                        // Two for partners, backgrounds and friends.
                        count: 1..=2,
                        enforce_color_identity: true,
                        counts_towards_deck_size: true,
                    }),
                    approximate: false,
                }
            }

            Format::Brawl => FormatRules {
                format,
                deck_size: DeckSize::Exactly(100),
                max_copies: 1,
                max_sideboard: 0,
                commander: Some(CommanderRules {
                    count: 1..=1,
                    enforce_color_identity: true,
                    counts_towards_deck_size: true,
                }),
                approximate: false,
            },

            Format::StandardBrawl => FormatRules {
                format,
                deck_size: DeckSize::Exactly(60),
                max_copies: 1,
                max_sideboard: 0,
                commander: Some(CommanderRules {
                    count: 1..=1,
                    enforce_color_identity: true,
                    counts_towards_deck_size: true,
                }),
                approximate: false,
            },

            // Hundred-card singleton with no commander, so no colour identity restriction.
            Format::Gladiator => FormatRules {
                format,
                deck_size: DeckSize::Exactly(100),
                max_copies: 1,
                max_sideboard: 0,
                commander: None,
                approximate: false,
            },

            // Sixty cards, singleton, with a planeswalker and its signature spell in the
            // command zone — hence a command zone of exactly two.
            Format::Oathbreaker => FormatRules {
                format,
                deck_size: DeckSize::Exactly(60),
                max_copies: 1,
                max_sideboard: 0,
                commander: Some(CommanderRules {
                    count: 2..=2,
                    enforce_color_identity: true,
                    counts_towards_deck_size: true,
                }),
                approximate: false,
            },

            // Not confirmed against an official document. `competitivebrawl` and `tlr` appeared
            // in Scryfall's legality keys without our knowing their construction rules; both
            // look Commander-shaped from their card pools, so they inherit those rules and are
            // flagged rather than presented as certain. See docs/dev/data-pipeline.md.
            Format::CompetitiveBrawl | Format::Tlr => FormatRules {
                format,
                deck_size: DeckSize::Exactly(100),
                max_copies: 1,
                max_sideboard: 0,
                commander: Some(CommanderRules {
                    count: 1..=1,
                    enforce_color_identity: true,
                    counts_towards_deck_size: true,
                }),
                approximate: true,
            },
        }
    }

    pub fn is_singleton(&self) -> bool {
        self.max_copies == 1
    }

    pub fn has_commander(&self) -> bool {
        self.commander.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_has_rules() {
        // A missing arm would be a compile error, but this also pins that nothing returns
        // something nonsensical.
        for format in Format::ALL {
            let rules = FormatRules::for_format(format);
            assert_eq!(rules.format, format);
            assert!(rules.deck_size.minimum() >= 40, "{format:?}");
            assert!(rules.max_copies >= 1, "{format:?}");
        }
    }

    #[test]
    fn sixty_card_formats_allow_four_copies_and_a_sideboard() {
        let modern = FormatRules::for_format(Format::Modern);
        assert_eq!(modern.deck_size, DeckSize::AtLeast(60));
        assert_eq!(modern.max_copies, 4);
        assert_eq!(modern.max_sideboard, 15);
        assert!(!modern.has_commander());
        assert!(!modern.is_singleton());
    }

    #[test]
    fn commander_is_a_hundred_card_singleton_with_a_command_zone() {
        let commander = FormatRules::for_format(Format::Commander);
        assert_eq!(commander.deck_size, DeckSize::Exactly(100));
        assert!(commander.is_singleton());
        assert_eq!(commander.max_sideboard, 0);

        let zone = commander.commander.expect("commander rules");
        assert_eq!(zone.count, 1..=2, "partners make two legal");
        assert!(zone.enforce_color_identity);
        assert!(zone.counts_towards_deck_size, "99 + 1 = 100");
    }

    #[test]
    fn oathbreaker_wants_exactly_two_in_the_command_zone() {
        // A planeswalker and its signature spell, unlike partners which are optional.
        let rules = FormatRules::for_format(Format::Oathbreaker);
        assert_eq!(rules.deck_size, DeckSize::Exactly(60));
        assert_eq!(rules.commander.expect("commander rules").count, 2..=2);
    }

    #[test]
    fn gladiator_is_singleton_without_a_commander() {
        // So no colour identity restriction applies, unlike every other singleton format here.
        let rules = FormatRules::for_format(Format::Gladiator);
        assert_eq!(rules.deck_size, DeckSize::Exactly(100));
        assert!(rules.is_singleton());
        assert!(!rules.has_commander());
    }

    #[test]
    fn standard_brawl_is_sixty_cards_not_a_hundred() {
        assert_eq!(
            FormatRules::for_format(Format::StandardBrawl).deck_size,
            DeckSize::Exactly(60)
        );
        assert_eq!(
            FormatRules::for_format(Format::Brawl).deck_size,
            DeckSize::Exactly(100)
        );
    }

    #[test]
    fn only_the_unconfirmed_formats_are_flagged_approximate() {
        let approximate: Vec<Format> = Format::ALL
            .into_iter()
            .filter(|f| FormatRules::for_format(*f).approximate)
            .collect();
        assert_eq!(approximate, [Format::CompetitiveBrawl, Format::Tlr]);
    }
}
