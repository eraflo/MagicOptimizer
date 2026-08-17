//! Scoring, simulation and search for decks.

mod math;
mod profile;
mod rng;
mod score;
mod search;
mod simulate;

pub use math::{
    cards_seen_by_turn, probability_castable_on_curve, probability_of_at_least,
    probability_of_exactly, sources_for_confidence,
};
pub use profile::{profile, profile_with_index, CardIndex, DeckProfile, PipRequirement};
pub use rng::Rng;
pub use score::{
    score, score_with_simulation, Archetype, Criterion, Score, ScoreSettings, Weights,
};
pub use search::{search, CardPool, SearchResult, SearchSettings, Suggestion};
pub use simulate::{
    simulate, Card, MulliganRule, SimulationResult, SimulationSettings, DEFAULT_GAMES,
};
