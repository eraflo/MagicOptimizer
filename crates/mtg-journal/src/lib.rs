//! The game log, and what can honestly be concluded from it.
//!
//! Games are recorded **after** they are played — the app is never used at the table, which is
//! invariant 2 in `CLAUDE.md` — and the log's job is to answer "how is this deck actually
//! doing" without overclaiming.
//!
//! That last part is most of the work. See [`stats`]: three wins out of three is not a 100% win
//! rate, and the module is built so nothing here can ever say it is.

mod game;
mod stats;
mod store;

pub use game::{Game, GameId, NewGame, Opponent, Result_};
pub use stats::{before_and_after, matchups, BeforeAfter, Matchup, WinRate};
pub use store::{JournalError, JournalStore, Result};
