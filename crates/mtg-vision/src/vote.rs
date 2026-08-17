//! Turning a stream of per-frame guesses into a decision.
//!
//! A single frame is not evidence. Focus drifts, a hand crosses the lens, the card catches a
//! reflection — any one frame can match the wrong card, and a wrong card added silently to a
//! collection is the failure people would actually notice. Requiring the same answer from
//! several frames in a row costs a fraction of a second and removes nearly all of that.
//!
//! The second job here is *not* re-emitting. Holding a card in front of a camera for two
//! seconds is thirty frames; the scanner should add it once and then wait for the next card.

use std::collections::VecDeque;

use crate::matcher::Match;

/// How many recent frames are remembered.
pub const DEFAULT_WINDOW: usize = 12;

/// How many of them must agree before a card is confirmed.
///
/// Five out of twelve, so a card can be confirmed in five frames when recognition is clean, and
/// still be confirmed when a third of the frames are unusable.
pub const DEFAULT_NEEDED: usize = 5;

/// Frames without any match that mean the card has left.
///
/// Long enough to survive a blink of blur, short enough that scanning a stack is not tedious.
pub const DEFAULT_CLEAR_AFTER: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteSettings {
    pub window: usize,
    pub needed: usize,
    pub clear_after: usize,
}

impl Default for VoteSettings {
    fn default() -> VoteSettings {
        VoteSettings {
            window: DEFAULT_WINDOW,
            needed: DEFAULT_NEEDED,
            clear_after: DEFAULT_CLEAR_AFTER,
        }
    }
}

/// What the scanner has to say about a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing recognisable. Show the viewfinder guide.
    Searching,
    /// A card is matching but has not convinced enough frames yet.
    ///
    /// Worth showing as progress: it tells the user to hold still rather than move on.
    Tracking {
        card: Box<Match>,
        votes: usize,
        needed: usize,
    },
    /// Confirmed. Emitted exactly once per card presented — act on this.
    Confirmed(Box<Match>),
    /// The card just confirmed is still in view. Nothing to do.
    Holding,
}

/// Accumulates per-frame matches into confirmations.
#[derive(Debug, Clone)]
pub struct Voter {
    settings: VoteSettings,
    recent: VecDeque<Option<Match>>,
    /// The card already reported, held until it leaves so it is not counted twice.
    settled: Option<String>,
    misses: usize,
}

impl Default for Voter {
    fn default() -> Voter {
        Voter::new(VoteSettings::default())
    }
}

impl Voter {
    pub fn new(settings: VoteSettings) -> Voter {
        Voter {
            settings,
            recent: VecDeque::with_capacity(settings.window.max(1)),
            settled: None,
            misses: 0,
        }
    }

    /// Forgets everything, as when the camera restarts or the user changes destination.
    pub fn reset(&mut self) {
        self.recent.clear();
        self.settled = None;
        self.misses = 0;
    }

    /// Feeds one frame's result and asks what to do about it.
    pub fn observe(&mut self, found: Option<Match>) -> Outcome {
        match &found {
            None => self.misses += 1,
            Some(_) => self.misses = 0,
        }

        // A card that has left the frame stops blocking the next one — including another copy
        // of the same card, which is exactly what scanning a playset looks like.
        if self.settled.is_some() && self.misses >= self.settings.clear_after {
            self.settled = None;
            self.recent.clear();
        }

        self.recent.push_back(found.clone());
        while self.recent.len() > self.settings.window.max(1) {
            self.recent.pop_front();
        }

        let Some(card) = found else {
            return if self.settled.is_some() {
                Outcome::Holding
            } else {
                Outcome::Searching
            };
        };

        if self.settled.as_deref() == Some(card.oracle_id.as_str()) {
            return Outcome::Holding;
        }

        let votes = self
            .recent
            .iter()
            .flatten()
            .filter(|seen| seen.oracle_id == card.oracle_id)
            .count();

        if votes >= self.settings.needed {
            self.settled = Some(card.oracle_id.clone());
            // Cleared so the next card starts from nothing rather than inheriting this one's
            // tally, which otherwise makes a fast second scan confirm on its first frame.
            self.recent.clear();
            return Outcome::Confirmed(Box::new(card));
        }

        Outcome::Tracking {
            card: Box::new(card),
            votes,
            needed: self.settings.needed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str) -> Match {
        Match {
            printing_id: format!("p-{name}"),
            oracle_id: format!("o-{name}"),
            name: name.to_owned(),
            distance: 4,
            margin: 30,
        }
    }

    fn confirmed(outcome: &Outcome) -> Option<&str> {
        match outcome {
            Outcome::Confirmed(card) => Some(card.name.as_str()),
            _ => None,
        }
    }

    #[test]
    fn one_good_frame_is_not_enough() {
        // The whole point: a single frame can match the wrong card.
        let mut voter = Voter::default();
        assert!(matches!(
            voter.observe(Some(card("Sol Ring"))),
            Outcome::Tracking { votes: 1, .. }
        ));
    }

    #[test]
    fn agreement_across_frames_confirms() {
        let mut voter = Voter::default();
        for _ in 0..DEFAULT_NEEDED - 1 {
            assert!(confirmed(&voter.observe(Some(card("Sol Ring")))).is_none());
        }
        assert_eq!(
            confirmed(&voter.observe(Some(card("Sol Ring")))),
            Some("Sol Ring")
        );
    }

    #[test]
    fn a_card_is_confirmed_once_however_long_it_is_held() {
        // Two seconds in front of the lens is thirty frames, not thirty cards.
        let mut voter = Voter::default();
        let mut confirmations = 0;
        for _ in 0..30 {
            if confirmed(&voter.observe(Some(card("Sol Ring")))).is_some() {
                confirmations += 1;
            }
        }
        assert_eq!(confirmations, 1);
    }

    #[test]
    fn taking_the_card_away_and_bringing_another_confirms_the_second() {
        let mut voter = Voter::default();
        for _ in 0..8 {
            voter.observe(Some(card("Sol Ring")));
        }
        for _ in 0..DEFAULT_CLEAR_AFTER {
            voter.observe(None);
        }

        let mut confirmations = Vec::new();
        for _ in 0..DEFAULT_NEEDED {
            if let Some(name) = confirmed(&voter.observe(Some(card("Island")))) {
                confirmations.push(name.to_owned());
            }
        }
        assert_eq!(confirmations, vec!["Island"]);
    }

    #[test]
    fn a_second_copy_of_the_same_card_is_counted_again() {
        // Scanning a playset: four Lightning Bolts must add four, not one.
        let mut voter = Voter::default();
        let mut confirmations = 0;
        for _ in 0..4 {
            for _ in 0..DEFAULT_NEEDED {
                if confirmed(&voter.observe(Some(card("Lightning Bolt")))).is_some() {
                    confirmations += 1;
                }
            }
            for _ in 0..DEFAULT_CLEAR_AFTER {
                voter.observe(None);
            }
        }
        assert_eq!(confirmations, 4);
    }

    #[test]
    fn a_stray_wrong_match_does_not_win() {
        // One bad frame among good ones is exactly what the window is for.
        let mut voter = Voter::default();
        let mut seen = Vec::new();
        let frames = [
            Some(card("Sol Ring")),
            Some(card("Sol Ring")),
            Some(card("Black Lotus")),
            Some(card("Sol Ring")),
            Some(card("Sol Ring")),
            Some(card("Sol Ring")),
        ];
        for frame in frames {
            if let Some(name) = confirmed(&voter.observe(frame)) {
                seen.push(name.to_owned());
            }
        }
        assert_eq!(seen, vec!["Sol Ring"]);
    }

    #[test]
    fn a_brief_loss_of_focus_does_not_restart_the_count() {
        // Hands shake. Losing two frames mid-scan should not send the user back to zero.
        let mut voter = Voter::default();
        let frames = [
            Some(card("Sol Ring")),
            Some(card("Sol Ring")),
            None,
            None,
            Some(card("Sol Ring")),
            Some(card("Sol Ring")),
            Some(card("Sol Ring")),
        ];
        let mut seen = Vec::new();
        for frame in frames {
            if let Some(name) = confirmed(&voter.observe(frame)) {
                seen.push(name.to_owned());
            }
        }
        assert_eq!(seen, vec!["Sol Ring"]);
    }

    #[test]
    fn an_empty_stream_never_confirms_anything() {
        let mut voter = Voter::default();
        for _ in 0..100 {
            assert_eq!(voter.observe(None), Outcome::Searching);
        }
    }

    #[test]
    fn two_cards_alternating_confirm_neither() {
        // A shaky frame flipping between two candidates is not evidence for either.
        let mut voter = Voter::new(VoteSettings {
            window: 6,
            needed: 5,
            clear_after: 6,
        });
        for index in 0..40 {
            let name = if index % 2 == 0 { "Sol Ring" } else { "Island" };
            assert!(confirmed(&voter.observe(Some(card(name)))).is_none());
        }
    }

    #[test]
    fn holding_is_reported_while_the_confirmed_card_is_still_there() {
        // So the UI can show "added" rather than looking like it stopped working.
        let mut voter = Voter::default();
        for _ in 0..DEFAULT_NEEDED {
            voter.observe(Some(card("Sol Ring")));
        }
        assert_eq!(voter.observe(Some(card("Sol Ring"))), Outcome::Holding);
        assert_eq!(voter.observe(None), Outcome::Holding);
    }

    #[test]
    fn resetting_forgets_the_card_in_hand() {
        let mut voter = Voter::default();
        for _ in 0..DEFAULT_NEEDED {
            voter.observe(Some(card("Sol Ring")));
        }
        voter.reset();

        let mut confirmations = 0;
        for _ in 0..DEFAULT_NEEDED {
            if confirmed(&voter.observe(Some(card("Sol Ring")))).is_some() {
                confirmations += 1;
            }
        }
        assert_eq!(confirmations, 1);
    }

    #[test]
    fn a_degenerate_window_still_behaves() {
        // Guarding the arithmetic, not a setting anyone would choose.
        let mut voter = Voter::new(VoteSettings {
            window: 0,
            needed: 1,
            clear_after: 1,
        });
        assert_eq!(
            confirmed(&voter.observe(Some(card("Sol Ring")))),
            Some("Sol Ring")
        );
    }
}
