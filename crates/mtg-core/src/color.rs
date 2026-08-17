//! Colors and color sets.

/// One of the five colors of Magic.
///
/// Colorless is deliberately absent: it is the empty set of colors, not a sixth variant.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
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

    /// Position in WUBRG order, used for bit indexing.
    const fn index(self) -> u8 {
        match self {
            Color::White => 0,
            Color::Blue => 1,
            Color::Black => 2,
            Color::Red => 3,
            Color::Green => 4,
        }
    }
}

/// A set of colors, packed into the low five bits of a byte.
///
/// Used for a card's colors, its color identity, and a Commander deck's allowed identity.
/// [`ColorSet::is_subset_of`] is what enforces the Commander identity rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ColorSet(u8);

impl ColorSet {
    /// No colors. This is what "colorless" means.
    pub const COLORLESS: ColorSet = ColorSet(0);

    /// All five colors.
    pub const WUBRG: ColorSet = ColorSet(0b1_1111);

    /// Builds a set from an iterator of colors.
    pub fn from_colors(colors: impl IntoIterator<Item = Color>) -> ColorSet {
        let mut set = ColorSet::COLORLESS;
        for color in colors {
            set.insert(color);
        }
        set
    }

    /// Builds a set from mana symbols, e.g. `"WU"`. Unknown characters are ignored,
    /// which keeps parsing tolerant of upstream data we do not model.
    pub fn from_symbols(symbols: &str) -> ColorSet {
        ColorSet::from_colors(symbols.chars().filter_map(Color::from_symbol))
    }

    pub fn insert(&mut self, color: Color) {
        self.0 |= 1 << color.index();
    }

    pub const fn contains(self, color: Color) -> bool {
        self.0 & (1 << color.index()) != 0
    }

    pub const fn is_colorless(self) -> bool {
        self.0 == 0
    }

    /// Number of colors in the set.
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub const fn is_empty(self) -> bool {
        self.is_colorless()
    }

    /// True when every color in `self` is also in `other`.
    ///
    /// This is the Commander color identity rule: a card is legal in a deck when its identity
    /// is a subset of the commander's. Note that the colorless set is a subset of everything,
    /// which is exactly right — colorless cards go in any deck.
    pub const fn is_subset_of(self, other: ColorSet) -> bool {
        self.0 & !other.0 == 0
    }

    pub const fn union(self, other: ColorSet) -> ColorSet {
        ColorSet(self.0 | other.0)
    }

    pub const fn intersection(self, other: ColorSet) -> ColorSet {
        ColorSet(self.0 & other.0)
    }

    /// Iterates the colors present, in WUBRG order.
    pub fn iter(self) -> impl Iterator<Item = Color> {
        Color::ALL.into_iter().filter(move |&c| self.contains(c))
    }

    /// The raw bits, for packing into an archived card.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Rebuilds a set from [`ColorSet::bits`]. Bits outside the low five are discarded.
    pub const fn from_bits(bits: u8) -> ColorSet {
        ColorSet(bits & 0b1_1111)
    }
}

impl std::fmt::Display for ColorSet {
    /// Renders as WUBRG letters, or `C` when colorless.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_colorless() {
            return f.write_str("C");
        }
        for color in self.iter() {
            write!(f, "{}", color.symbol())?;
        }
        Ok(())
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

    #[test]
    fn set_membership_and_size() {
        let azorius = ColorSet::from_symbols("WU");
        assert!(azorius.contains(Color::White));
        assert!(azorius.contains(Color::Blue));
        assert!(!azorius.contains(Color::Black));
        assert_eq!(azorius.len(), 2);
        assert!(!azorius.is_colorless());
    }

    #[test]
    fn colorless_is_a_subset_of_everything() {
        // The rule that lets colorless cards into any Commander deck.
        for set in [
            ColorSet::COLORLESS,
            ColorSet::from_symbols("R"),
            ColorSet::WUBRG,
        ] {
            assert!(ColorSet::COLORLESS.is_subset_of(set));
        }
    }

    #[test]
    fn commander_identity_rule() {
        let commander = ColorSet::from_symbols("WU");

        assert!(ColorSet::from_symbols("W").is_subset_of(commander));
        assert!(ColorSet::from_symbols("WU").is_subset_of(commander));
        // A black card cannot go in an Azorius deck, even alongside legal colors.
        assert!(!ColorSet::from_symbols("B").is_subset_of(commander));
        assert!(!ColorSet::from_symbols("WUB").is_subset_of(commander));
    }

    #[test]
    fn iteration_is_in_wubrg_order() {
        let jund = ColorSet::from_symbols("GBR");
        let symbols: String = jund.iter().map(Color::symbol).collect();
        assert_eq!(symbols, "BRG");
    }

    #[test]
    fn unknown_symbols_are_ignored_when_building_a_set() {
        // Scryfall never sends these, but tolerating them beats panicking on network data.
        assert_eq!(ColorSet::from_symbols("W?U"), ColorSet::from_symbols("WU"));
    }

    #[test]
    fn bits_round_trip() {
        for set in [
            ColorSet::COLORLESS,
            ColorSet::WUBRG,
            ColorSet::from_symbols("BG"),
        ] {
            assert_eq!(ColorSet::from_bits(set.bits()), set);
        }
    }

    #[test]
    fn display_is_readable() {
        assert_eq!(ColorSet::COLORLESS.to_string(), "C");
        assert_eq!(ColorSet::from_symbols("GW").to_string(), "WG");
        assert_eq!(ColorSet::WUBRG.to_string(), "WUBRG");
    }
}
