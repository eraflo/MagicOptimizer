//! Mana costs, parsed from Scryfall's brace notation.
//!
//! Scryfall writes costs as `{2}{W}{U}`, with a long tail of special symbols: hybrid `{W/U}`,
//! monocolored hybrid `{2/W}`, Phyrexian `{W/P}`, snow `{S}`, colorless `{C}`, and variables
//! `{X}`. We model all of them rather than collapsing to a number, because the optimizer needs
//! the individual colored pips: Karsten's land formulas count symbols, not mana value.

use crate::{Color, ColorSet};

/// A single symbol inside a mana cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManaSymbol {
    /// `{3}` — generic mana, payable with anything.
    Generic(u8),
    /// `{X}`, `{Y}`, `{Z}` — contributes 0 to mana value on the stack.
    Variable,
    /// `{W}` — a colored pip.
    Colored(Color),
    /// `{C}` — specifically colorless mana, not "no color requirement".
    Colorless,
    /// `{S}` — snow mana.
    Snow,
    /// `{W/U}` — payable with either color.
    Hybrid(Color, Color),
    /// `{2/W}` — two generic or one colored. Mana value 2.
    MonoHybrid(Color),
    /// `{W/P}` — the color or 2 life.
    Phyrexian(Color),
    /// `{W/U/P}` — either color, or 2 life.
    HybridPhyrexian(Color, Color),
}

impl ManaSymbol {
    /// Contribution to mana value (converted mana cost).
    ///
    /// `{X}` counts as 0, which matches the rules everywhere except on the stack.
    pub const fn mana_value(self) -> u32 {
        match self {
            ManaSymbol::Generic(n) => n as u32,
            ManaSymbol::Variable => 0,
            ManaSymbol::MonoHybrid(_) => 2,
            _ => 1,
        }
    }

    /// Colors this symbol can require. Generic, variable, colorless and snow contribute none.
    pub fn colors(self) -> ColorSet {
        match self {
            ManaSymbol::Colored(c) | ManaSymbol::MonoHybrid(c) | ManaSymbol::Phyrexian(c) => {
                ColorSet::from_colors([c])
            }
            ManaSymbol::Hybrid(a, b) | ManaSymbol::HybridPhyrexian(a, b) => {
                ColorSet::from_colors([a, b])
            }
            _ => ColorSet::COLORLESS,
        }
    }
}

/// Why a cost string could not be parsed.
///
/// Scryfall data should never trigger these, but the catalog comes off the network and is
/// treated as untrusted: we return an error rather than panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManaCostError {
    /// A `{` was opened and never closed.
    UnclosedBrace,
    /// Text appeared outside any `{...}` group.
    StrayText(char),
    /// A group whose contents we do not recognize, e.g. `{Q}`.
    UnknownSymbol(String),
}

impl std::fmt::Display for ManaCostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManaCostError::UnclosedBrace => write!(f, "unclosed brace in mana cost"),
            ManaCostError::StrayText(c) => write!(f, "unexpected character {c:?} outside braces"),
            ManaCostError::UnknownSymbol(s) => write!(f, "unknown mana symbol {{{s}}}"),
        }
    }
}

impl std::error::Error for ManaCostError {}

/// A full mana cost, as an ordered list of symbols.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManaCost {
    symbols: Vec<ManaSymbol>,
}

impl ManaCost {
    /// The empty cost, used by lands and by the back faces of transforming cards.
    pub fn empty() -> ManaCost {
        ManaCost {
            symbols: Vec::new(),
        }
    }

    pub fn from_symbols(symbols: Vec<ManaSymbol>) -> ManaCost {
        ManaCost { symbols }
    }

    /// Parses Scryfall brace notation, e.g. `{2}{W}{U}` or `{X}{B/G}{B/G}`.
    ///
    /// An empty string is a valid empty cost — that is how Scryfall represents lands.
    pub fn parse(input: &str) -> Result<ManaCost, ManaCostError> {
        let mut symbols = Vec::new();
        let mut rest = input.trim();

        while !rest.is_empty() {
            let Some(open) = rest.find('{') else {
                // Any leftover text with no brace is malformed.
                let c = rest.chars().next().unwrap_or('?');
                return Err(ManaCostError::StrayText(c));
            };
            if open != 0 {
                let c = rest[..open].chars().next().unwrap_or('?');
                return Err(ManaCostError::StrayText(c));
            }
            let close = rest.find('}').ok_or(ManaCostError::UnclosedBrace)?;
            symbols.push(parse_symbol(&rest[1..close])?);
            rest = &rest[close + 1..];
        }

        Ok(ManaCost { symbols })
    }

    pub fn symbols(&self) -> &[ManaSymbol] {
        &self.symbols
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Total mana value (converted mana cost).
    pub fn mana_value(&self) -> u32 {
        self.symbols.iter().map(|s| s.mana_value()).sum()
    }

    /// Every color this cost can require.
    pub fn colors(&self) -> ColorSet {
        self.symbols
            .iter()
            .fold(ColorSet::COLORLESS, |acc, s| acc.union(s.colors()))
    }

    /// How many symbols require `color`.
    ///
    /// This is the input to Karsten's land-count formulas, which care about the number of
    /// colored pips in a cost, not its mana value. `{B}{B}` needs far more black sources
    /// than `{2}{B}` despite both being castable on turn three.
    pub fn pip_count(&self, color: Color) -> u32 {
        self.symbols
            .iter()
            .filter(|s| matches!(s, ManaSymbol::Colored(c) if *c == color))
            .count() as u32
    }

    /// True when the cost contains at least one `{X}`.
    pub fn has_variable(&self) -> bool {
        self.symbols.contains(&ManaSymbol::Variable)
    }
}

fn parse_symbol(body: &str) -> Result<ManaSymbol, ManaCostError> {
    let unknown = || ManaCostError::UnknownSymbol(body.to_owned());

    // Generic costs are the only multi-character numeric symbols.
    if let Ok(n) = body.parse::<u8>() {
        return Ok(ManaSymbol::Generic(n));
    }

    let parts: Vec<&str> = body.split('/').collect();
    match parts.as_slice() {
        ["X" | "Y" | "Z"] => Ok(ManaSymbol::Variable),
        ["C"] => Ok(ManaSymbol::Colorless),
        ["S"] => Ok(ManaSymbol::Snow),
        [single] => single_color(single)
            .map(ManaSymbol::Colored)
            .ok_or_else(unknown),
        [a, "P"] => single_color(a)
            .map(ManaSymbol::Phyrexian)
            .ok_or_else(unknown),
        ["2", c] => single_color(c)
            .map(ManaSymbol::MonoHybrid)
            .ok_or_else(unknown),
        [a, b] => match (single_color(a), single_color(b)) {
            (Some(a), Some(b)) => Ok(ManaSymbol::Hybrid(a, b)),
            _ => Err(unknown()),
        },
        [a, b, "P"] => match (single_color(a), single_color(b)) {
            (Some(a), Some(b)) => Ok(ManaSymbol::HybridPhyrexian(a, b)),
            _ => Err(unknown()),
        },
        _ => Err(unknown()),
    }
}

fn single_color(s: &str) -> Option<Color> {
    let mut chars = s.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Color::from_symbol(first)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn parse(s: &str) -> ManaCost {
        ManaCost::parse(s).unwrap()
    }

    #[test]
    fn empty_cost_is_valid() {
        // Lands have an empty mana cost in Scryfall data.
        let cost = parse("");
        assert!(cost.is_empty());
        assert_eq!(cost.mana_value(), 0);
        assert_eq!(cost.colors(), ColorSet::COLORLESS);
    }

    #[test]
    fn simple_cost() {
        let cost = parse("{2}{W}{U}");
        assert_eq!(cost.mana_value(), 4);
        assert_eq!(cost.colors(), ColorSet::from_symbols("WU"));
        assert_eq!(cost.pip_count(Color::White), 1);
        assert_eq!(cost.pip_count(Color::Black), 0);
    }

    #[test]
    fn double_pips_are_counted_separately_from_mana_value() {
        // The distinction Karsten's formulas depend on: same mana value, very different
        // demands on the mana base.
        let heavy = parse("{B}{B}");
        let light = parse("{1}{B}");
        assert_eq!(heavy.mana_value(), light.mana_value());
        assert_eq!(heavy.pip_count(Color::Black), 2);
        assert_eq!(light.pip_count(Color::Black), 1);
    }

    #[test]
    fn variable_costs_count_as_zero() {
        let cost = parse("{X}{R}");
        assert_eq!(cost.mana_value(), 1);
        assert!(cost.has_variable());
    }

    #[test]
    fn hybrid_symbols() {
        let cost = parse("{B/G}{B/G}");
        assert_eq!(cost.mana_value(), 2);
        assert_eq!(cost.colors(), ColorSet::from_symbols("BG"));
    }

    #[test]
    fn monocolored_hybrid_has_mana_value_two() {
        // {2/W} can be paid with two generic, so its mana value is 2, not 1.
        let cost = parse("{2/W}{2/W}");
        assert_eq!(cost.mana_value(), 4);
        assert_eq!(cost.colors(), ColorSet::from_symbols("W"));
    }

    #[test]
    fn phyrexian_symbols() {
        let cost = parse("{1}{W/P}");
        assert_eq!(cost.mana_value(), 2);
        assert_eq!(cost.colors(), ColorSet::from_symbols("W"));
    }

    #[test]
    fn hybrid_phyrexian_symbols() {
        let cost = parse("{R/W/P}");
        assert_eq!(cost.mana_value(), 1);
        assert_eq!(cost.colors(), ColorSet::from_symbols("RW"));
    }

    #[test]
    fn colorless_and_snow_are_not_colors() {
        let cost = parse("{C}{S}");
        assert_eq!(cost.mana_value(), 2);
        assert!(cost.colors().is_colorless());
    }

    #[test]
    fn large_generic_costs() {
        // Draco costs {16}; two digits must not be read as two symbols.
        let cost = parse("{16}");
        assert_eq!(cost.mana_value(), 16);
    }

    #[test]
    fn malformed_costs_return_errors_rather_than_panicking() {
        assert_eq!(ManaCost::parse("{W"), Err(ManaCostError::UnclosedBrace));
        assert_eq!(ManaCost::parse("W"), Err(ManaCostError::StrayText('W')));
        assert_eq!(ManaCost::parse("{2}X"), Err(ManaCostError::StrayText('X')));
        assert_eq!(
            ManaCost::parse("{Q}"),
            Err(ManaCostError::UnknownSymbol("Q".to_owned()))
        );
    }
}
