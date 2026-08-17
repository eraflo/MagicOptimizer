//! Core types shared across every MagicOptimizer crate.
//!
//! This crate has no dependencies on the other crates in the workspace, and no native
//! dependencies at all. See `CLAUDE.md` for why that second point is not negotiable.

/// Stable identifier for an oracle card (a card as a set of rules, independent of printing).
///
/// Indexes into the card catalog artifact. Distinct printings of the same card share one
/// `CardId`; see `PrintingId` (added in phase 1) to tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardId(pub u32);

/// One of the five colors of Magic.
///
/// Colorless is deliberately absent: it is the empty set of colors, not a sixth variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Color {
    White,
    Blue,
    Black,
    Red,
    Green,
}

impl Color {
    /// Every color, in the canonical WUBRG order used throughout Magic.
    pub const ALL: [Color; 5] = [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ];

    /// The single-letter symbol for this color, as used in mana costs.
    pub const fn symbol(self) -> char {
        match self {
            Color::White => 'W',
            Color::Blue => 'U',
            Color::Black => 'B',
            Color::Red => 'R',
            Color::Green => 'G',
        }
    }

    /// Parses a color from its mana symbol. Case-sensitive, as Scryfall data is.
    pub const fn from_symbol(symbol: char) -> Option<Color> {
        match symbol {
            'W' => Some(Color::White),
            'U' => Some(Color::Blue),
            'B' => Some(Color::Black),
            'R' => Some(Color::Red),
            'G' => Some(Color::Green),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_colors_are_in_wubrg_order() {
        let symbols: String = Color::ALL.iter().map(|c| c.symbol()).collect();
        assert_eq!(symbols, "WUBRG");
    }

    #[test]
    fn symbol_parsing_round_trips() {
        for color in Color::ALL {
            assert_eq!(Color::from_symbol(color.symbol()), Some(color));
        }
    }

    #[test]
    fn unknown_symbols_are_rejected() {
        // 'C' is colorless and 'X' is a generic cost: neither is a color.
        assert_eq!(Color::from_symbol('C'), None);
        assert_eq!(Color::from_symbol('X'), None);
        assert_eq!(Color::from_symbol('w'), None);
    }
}
