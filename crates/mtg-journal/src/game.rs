//! What one recorded game holds.
//!
//! Deliberately small. The log is filled in after a game, by someone who would rather be
//! shuffling for the next one, so every field here has to earn the seconds it costs. Anything
//! that can be derived — win rates, matchup tables, the effect of a change — is derived rather
//! than typed in.

use serde::{Deserialize, Serialize};

/// How a game ended, from the point of view of the person keeping the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Result_ {
    Win,
    Loss,
    /// A genuine draw, and also the honest answer for a game abandoned on time.
    Draw,
}

impl Result_ {
    pub const ALL: [Result_; 3] = [Result_::Win, Result_::Loss, Result_::Draw];

    pub const fn label(self) -> &'static str {
        match self {
            Result_::Win => "Win",
            Result_::Loss => "Loss",
            Result_::Draw => "Draw",
        }
    }
}

/// An opponent, as much as anyone bothers to record.
///
/// A free-text label rather than a deck reference: at a table you know "Jean's Atraxa", not a
/// decklist, and demanding more would mean the log stops being filled in.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Opponent {
    /// What the deck was, as you would say it out loud: "Atraxa superfriends", "Burn".
    pub archetype: String,
    /// Whose it was, if that matters to you. Optional, and never required.
    #[serde(default)]
    pub player: String,
}

/// Identifier for a stored game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GameId(pub u64);

/// One game, after the fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    /// The deck played, as `mtg_deck::DeckId`'s inner value.
    ///
    /// Stored as a plain integer rather than the type, so this crate does not have to depend on
    /// `mtg-deck` for one field. A deck that is later deleted leaves its games behind, which is
    /// the right way round: the games happened.
    pub deck_id: u64,
    /// `YYYY-MM-DD`. A string because that is what a date input gives, it sorts correctly, and a
    /// date library is not worth pulling in for a field nobody does arithmetic on.
    pub played_at: String,
    /// Scryfall format key, so a log can span formats.
    #[serde(default)]
    pub format: String,
    pub result: Result_,
    #[serde(default)]
    pub opponents: Vec<Opponent>,
    /// Whether you were on the play. Absent when nobody remembered, which is common.
    #[serde(default)]
    pub on_the_play: Option<bool>,
    /// Mulligans taken. The one number worth typing: it is the single strongest predictor of a
    /// loss, and the optimizer's simulation estimates the same quantity, so the two can be
    /// compared.
    #[serde(default)]
    pub mulligans: Option<u32>,
    /// The turn the game ended on, if anyone counted.
    #[serde(default)]
    pub ended_on_turn: Option<u32>,
    #[serde(default)]
    pub notes: String,
}

/// A game before it has an id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewGame {
    pub deck_id: u64,
    pub played_at: String,
    #[serde(default)]
    pub format: String,
    pub result: Result_,
    #[serde(default)]
    pub opponents: Vec<Opponent>,
    #[serde(default)]
    pub on_the_play: Option<bool>,
    #[serde(default)]
    pub mulligans: Option<u32>,
    #[serde(default)]
    pub ended_on_turn: Option<u32>,
    #[serde(default)]
    pub notes: String,
}

impl NewGame {
    /// The shortest useful record: a deck, a date, and how it went.
    pub fn new(deck_id: u64, played_at: impl Into<String>, result: Result_) -> NewGame {
        NewGame {
            deck_id,
            played_at: played_at.into(),
            format: String::new(),
            result,
            opponents: Vec::new(),
            on_the_play: None,
            mulligans: None,
            ended_on_turn: None,
            notes: String::new(),
        }
    }

    pub fn against(mut self, archetype: impl Into<String>) -> NewGame {
        self.opponents.push(Opponent {
            archetype: archetype.into(),
            player: String::new(),
        });
        self
    }

    pub fn with_id(self, id: GameId) -> Game {
        Game {
            id,
            deck_id: self.deck_id,
            played_at: self.played_at,
            format: self.format,
            result: self.result,
            opponents: self.opponents,
            on_the_play: self.on_the_play,
            mulligans: self.mulligans,
            ended_on_turn: self.ended_on_turn,
            notes: self.notes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortest_record_is_three_fields() {
        // If recording a game is a chore it will not happen, and a log nobody fills in tells
        // you nothing at all.
        let game = NewGame::new(1, "2026-08-18", Result_::Win);
        assert_eq!(game.result, Result_::Win);
        assert!(game.opponents.is_empty());
        assert!(game.mulligans.is_none());
    }

    #[test]
    fn a_game_survives_a_round_trip_through_json() {
        // This is how it is stored, so a field that does not survive is a field that is lost.
        let game = NewGame::new(7, "2026-08-18", Result_::Loss)
            .against("Atraxa superfriends")
            .with_id(GameId(3));
        let json = serde_json::to_string(&game).expect("encode");
        assert_eq!(serde_json::from_str::<Game>(&json).expect("decode"), game);
    }

    #[test]
    fn a_record_written_before_a_field_existed_still_loads() {
        // The log is the user's own history and cannot be regenerated. Every optional field
        // defaults, so adding one later does not strand what is already written.
        let old = r#"{"id":1,"deck_id":2,"played_at":"2026-01-01","result":"win"}"#;
        let game: Game = serde_json::from_str(old).expect("decode");
        assert_eq!(game.result, Result_::Win);
        assert!(game.opponents.is_empty());
        assert!(game.notes.is_empty());
    }

    #[test]
    fn dates_sort_correctly_as_plain_strings() {
        // The reason `played_at` is a string and not a date type: this is the only ordering
        // anything needs, and `YYYY-MM-DD` gives it for free.
        let mut dates = ["2026-08-09", "2025-12-31", "2026-08-10"];
        dates.sort_unstable();
        assert_eq!(dates, ["2025-12-31", "2026-08-09", "2026-08-10"]);
    }
}
