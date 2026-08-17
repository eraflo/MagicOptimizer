//! Camera scanning commands.
//!
//! # Frames come through raw IPC, not as command arguments
//!
//! A 640×480 greyscale frame is 300 KB. Passed as a normal `Vec<u8>` argument it would be
//! serialized as a JSON array of three hundred thousand numbers, ten times a second — the
//! encoding would cost far more than the recognition. [`tauri::ipc::Request`] hands the bytes
//! over untouched, with the dimensions in headers.
//!
//! The frontend also converts to greyscale before sending, which is where the other factor of
//! four comes from. Nothing downstream looks at colour.

use mtg_vision::{Outcome, Quad};
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::State;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

/// An upper bound on a frame, so a wrong header cannot ask for a huge allocation.
///
/// 4096×4096 is far beyond any sensible capture size; the frontend sends 640 wide.
const MAX_PIXELS: u64 = 4096 * 4096;

/// What the app knows about its artwork fingerprints.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub loaded: bool,
    pub artworks: usize,
    pub path: String,
    /// Why loading failed, when it did. The artifact is an optional download, so this is a
    /// state to report rather than an error.
    pub error: Option<String>,
}

/// A card the scanner recognised.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedCard {
    pub oracle_id: String,
    pub printing_id: String,
    pub name: String,
    /// Bits of difference from the reference. Lower is a closer match.
    pub distance: u32,
    /// How much worse the nearest *different* card was. Larger means more certain.
    pub margin: u32,
}

/// The result of one frame.
///
/// Flat rather than a tagged union: the UI switches on `state` and reads what it needs, and a
/// nested shape would only make the Svelte side harder to read.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    /// `searching`, `tracking`, `confirmed` or `holding`.
    pub state: &'static str,
    /// Set on `tracking`, `confirmed` and `holding`.
    pub card: Option<ScannedCard>,
    /// Frames that agree so far, and how many are needed. For a progress ring.
    pub votes: usize,
    pub needed: usize,
    /// The card's outline in frame coordinates, for drawing over the viewfinder. Present
    /// whenever a card was *seen*, even when it could not be named — which is what tells the
    /// user the problem is the database rather than their framing.
    pub quad: Option<[[f32; 2]; 4]>,
}

impl ScanResult {
    fn from(outcome: Outcome, quad: Option<Quad>) -> ScanResult {
        let quad = quad.map(|quad| quad.ordered().corners.map(|(x, y)| [x, y]));

        match outcome {
            Outcome::Searching => ScanResult {
                state: "searching",
                card: None,
                votes: 0,
                needed: 0,
                quad,
            },
            Outcome::Tracking {
                card,
                votes,
                needed,
            } => ScanResult {
                state: "tracking",
                card: Some(scanned(&card)),
                votes,
                needed,
                quad,
            },
            Outcome::Confirmed(card) => ScanResult {
                state: "confirmed",
                card: Some(scanned(&card)),
                votes: 0,
                needed: 0,
                quad,
            },
            Outcome::Holding => ScanResult {
                state: "holding",
                card: None,
                votes: 0,
                needed: 0,
                quad,
            },
        }
    }
}

fn scanned(card: &mtg_vision::Match) -> ScannedCard {
    ScannedCard {
        oracle_id: card.oracle_id.clone(),
        printing_id: card.printing_id.clone(),
        name: card.name.clone(),
        distance: card.distance,
        margin: card.margin,
    }
}

#[tauri::command]
pub fn scan_status(state: State<'_, AppState>) -> ScanStatus {
    let artworks = state.artworks();
    ScanStatus {
        loaded: artworks > 0,
        artworks,
        path: state.art_path().display().to_string(),
        error: state.art_error(),
    }
}

#[tauri::command]
pub fn scan_reload(state: State<'_, AppState>) -> ScanStatus {
    state.reload_artwork();
    scan_status(state)
}

/// Forgets the vote history.
///
/// Called when the camera view opens or the destination changes, so a card confirmed in a
/// previous session does not count towards the next one.
#[tauri::command]
pub fn scan_reset(state: State<'_, AppState>) -> CommandResult<()> {
    state.with_scanner(|scanner| scanner.reset())
}

/// Feeds one greyscale frame.
///
/// The body is the raw pixels, one byte per pixel; `width` and `height` come in as headers.
#[tauri::command]
pub fn scan_frame(state: State<'_, AppState>, request: Request<'_>) -> CommandResult<ScanResult> {
    let InvokeBody::Raw(pixels) = request.body() else {
        return Err("the frame must be sent as raw bytes, not JSON".to_owned());
    };

    let width = header(&request, "width")?;
    let height = header(&request, "height")?;

    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(format!("{width}×{height} is not a plausible frame size"));
    }
    if (pixels.len() as u64) < u64::from(width) * u64::from(height) {
        return Err(format!(
            "the frame is {} bytes, which is short of the {width}×{height} it claims",
            pixels.len()
        ));
    }

    state.with_scanner(|scanner| {
        let outcome = scanner.feed_gray(pixels, width, height);
        ScanResult::from(outcome, scanner.last_quad())
    })
}

fn header(request: &Request<'_>, name: &str) -> CommandResult<u32> {
    request
        .headers()
        .get(name)
        .ok_or_else(|| format!("the frame is missing its {name} header"))?
        .to_str()
        .map_err(|_| format!("the {name} header is not text"))?
        .parse()
        .map_err(|_| format!("the {name} header is not a number"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_vision::Match;

    fn card() -> Match {
        Match {
            printing_id: "p-sol".to_owned(),
            oracle_id: "o-sol".to_owned(),
            name: "Sol Ring".to_owned(),
            distance: 4,
            margin: 30,
        }
    }

    fn quad() -> Quad {
        Quad::new([(10.0, 20.0), (110.0, 20.0), (110.0, 160.0), (10.0, 160.0)])
    }

    #[test]
    fn a_confirmed_card_is_reported_with_its_details() {
        let result = ScanResult::from(Outcome::Confirmed(Box::new(card())), Some(quad()));
        assert_eq!(result.state, "confirmed");
        assert_eq!(result.card.expect("card").name, "Sol Ring");
    }

    #[test]
    fn tracking_carries_the_vote_progress() {
        // So the UI can show a ring filling rather than nothing at all.
        let result = ScanResult::from(
            Outcome::Tracking {
                card: Box::new(card()),
                votes: 3,
                needed: 5,
            },
            Some(quad()),
        );
        assert_eq!(result.state, "tracking");
        assert_eq!((result.votes, result.needed), (3, 5));
    }

    #[test]
    fn the_outline_survives_even_when_no_card_is_named() {
        // A card was seen but not recognised — the overlay must still show, because that is
        // what tells the user their framing is fine and the database is the problem.
        let result = ScanResult::from(Outcome::Searching, Some(quad()));
        assert_eq!(result.state, "searching");
        assert!(result.card.is_none());
        let corners = result.quad.expect("outline");
        assert_eq!(corners[0], [10.0, 20.0]);
        assert_eq!(corners[2], [110.0, 160.0]);
    }

    #[test]
    fn an_empty_scene_has_no_outline() {
        let result = ScanResult::from(Outcome::Searching, None);
        assert!(result.quad.is_none());
    }

    #[test]
    fn the_outline_comes_back_in_a_fixed_corner_order() {
        // The overlay draws a closed path through them, so an unordered quad would render as a
        // bow tie.
        let scrambled = Quad::new([(110.0, 160.0), (10.0, 20.0), (10.0, 160.0), (110.0, 20.0)]);
        let corners = ScanResult::from(Outcome::Searching, Some(scrambled))
            .quad
            .expect("outline");
        assert_eq!(corners[0], [10.0, 20.0], "top left first");
        assert_eq!(corners[1], [110.0, 20.0], "then top right");
        assert_eq!(corners[2], [110.0, 160.0], "then bottom right");
        assert_eq!(corners[3], [10.0, 160.0], "then bottom left");
    }

    #[test]
    fn a_fresh_install_reports_no_artwork_data_without_erroring() {
        // The fingerprints are the heaviest optional download; their absence is a state.
        //
        // `without_artifacts` rather than `new`, so this does not pass or fail depending on
        // whether the developer happens to have run `build-artifacts --art-only`.
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::without_artifacts(dir.path()).expect("state");

        assert_eq!(state.artworks(), 0);
        assert!(state.art_error().is_some(), "the reason should be recorded");
        assert!(state.with_scanner(|_| ()).is_err());
    }
}
