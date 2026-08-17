//! The whole pipeline, one frame in and one decision out.
//!
//! Detect the card, straighten it, cut out the artwork, hash it, match it, and let several
//! frames vote. Each of those steps is tested on its own; this is the wiring, and the reason it
//! is a struct rather than a function is that a video stream should not allocate a greyscale
//! buffer thirty times a second.

use crate::detect::{find_card, DetectSettings};
use crate::geometry::Quad;
use crate::hash::{hash_gray, rgba_to_gray, GrayView};
use crate::matcher::ArtDatabase;
use crate::rectify::{crop_artwork, rectify};
use crate::vote::{Outcome, VoteSettings, Voter};

/// Everything the scanner can be tuned by, in one place.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScanSettings {
    pub detect: DetectSettings,
    pub vote: VoteSettings,
    /// Bits of difference still counted as the same artwork.
    pub max_distance: u32,
    /// How much closer the best match must be than the nearest different card.
    pub margin: u32,
}

impl ScanSettings {
    pub fn tuned() -> ScanSettings {
        ScanSettings {
            detect: DetectSettings::default(),
            vote: VoteSettings::default(),
            max_distance: crate::matcher::DEFAULT_MAX_DISTANCE,
            margin: crate::matcher::DEFAULT_MARGIN,
        }
    }
}

/// A live scanning session.
pub struct Scanner {
    database: ArtDatabase,
    settings: ScanSettings,
    voter: Voter,
    /// Reused across frames so a video stream does not allocate per frame.
    gray: Vec<u8>,
    last_quad: Option<Quad>,
}

impl Scanner {
    pub fn new(database: ArtDatabase) -> Scanner {
        Scanner::with_settings(database, ScanSettings::tuned())
    }

    pub fn with_settings(database: ArtDatabase, settings: ScanSettings) -> Scanner {
        Scanner {
            database,
            settings,
            voter: Voter::new(settings.vote),
            gray: Vec::new(),
            last_quad: None,
        }
    }

    /// Where the card was in the last frame, for drawing the outline over the viewfinder.
    ///
    /// Showing this is what makes a scanner feel responsive instead of broken: the user can see
    /// it is looking at the card even while it is still gathering votes.
    pub fn last_quad(&self) -> Option<Quad> {
        self.last_quad
    }

    /// Clears the vote history, as when the camera restarts or the destination changes.
    pub fn reset(&mut self) {
        self.voter.reset();
        self.last_quad = None;
    }

    /// Feeds one frame of RGBA, as a canvas hands it over.
    pub fn feed_rgba(&mut self, rgba: &[u8], width: u32, height: u32) -> Outcome {
        // Lifted out of `self` for the duration so the conversion buffer can be reused across
        // frames without holding a borrow on the scanner while it works.
        let mut gray = std::mem::take(&mut self.gray);
        rgba_to_gray(rgba, &mut gray);
        let outcome = self.feed_gray(&gray, width, height);
        self.gray = gray;
        outcome
    }

    /// Feeds one frame that is already greyscale.
    pub fn feed_gray(&mut self, gray: &[u8], width: u32, height: u32) -> Outcome {
        match GrayView::new(gray, width, height) {
            Some(view) => self.feed_gray_view(&view),
            None => {
                self.last_quad = None;
                self.voter.observe(None)
            }
        }
    }

    fn feed_gray_view(&mut self, view: &GrayView<'_>) -> Outcome {
        let found = self.recognise(view);
        self.voter.observe(found)
    }

    /// The per-frame half: everything up to, but not including, the vote.
    ///
    /// Separate so it can be tested without a stream, and so a still photo can use it directly.
    fn recognise(&mut self, view: &GrayView<'_>) -> Option<crate::matcher::Match> {
        let quad = find_card(view, self.settings.detect);
        self.last_quad = quad;

        let quad = quad?;
        let card = rectify(view, &quad)?;
        let art = crop_artwork(&card.view());
        let hash = hash_gray(&art.view());

        self.database
            .best_match_with(&hash, self.settings.max_distance, self.settings.margin)
    }

    /// Recognises a single still image, skipping the voting entirely.
    ///
    /// For the "scan a whole deck from photos" flow, where there is no stream to vote over and
    /// the user reviews the list afterwards anyway.
    pub fn recognise_still(
        &mut self,
        gray: &[u8],
        width: u32,
        height: u32,
    ) -> Option<crate::matcher::Match> {
        let view = GrayView::new(gray, width, height)?;
        self.recognise(&view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{RECTIFIED_HEIGHT, RECTIFIED_WIDTH};
    use crate::matcher::ArtEntry;
    use crate::rectify::GrayImage;
    use crate::vote::DEFAULT_NEEDED;

    /// A synthetic card: smooth artwork in the art box, a distinct border, plain text area.
    ///
    /// Smooth on purpose — real artwork is low-frequency, and a high-frequency pattern would be
    /// testing the hasher's worst case rather than its job.
    fn card_image(seed: u32) -> GrayImage {
        let mut image = GrayImage::new(RECTIFIED_WIDTH, RECTIFIED_HEIGHT);
        let w = RECTIFIED_WIDTH as f32;
        let h = RECTIFIED_HEIGHT as f32;

        for y in 0..RECTIFIED_HEIGHT {
            for x in 0..RECTIFIED_WIDTH {
                let fx = x as f32 / w;
                let fy = y as f32 / h;

                let value = if !(0.11..=0.56).contains(&fy) {
                    // Frame and text box: near-identical between cards, as they really are.
                    200.0
                } else {
                    // Broad blobs, placed differently per card.
                    let a = ((fx * 3.0 + seed as f32 * 0.7).sin() + 1.0) * 0.5;
                    let b = ((fy * 4.0 + seed as f32 * 1.3).cos() + 1.0) * 0.5;
                    40.0 + 170.0 * (a * 0.6 + b * 0.4)
                };
                image.pixels[(y * RECTIFIED_WIDTH + x) as usize] = value as u8;
            }
        }
        image
    }

    /// Places a card image into a larger frame, at an angle if asked.
    fn frame_with(card: &GrayImage, quad: &Quad, width: u32, height: u32) -> GrayImage {
        let mut frame = GrayImage::new(width, height);
        // Background: dark and plain, which is what the user guide asks for.
        frame.pixels.iter_mut().for_each(|pixel| *pixel = 20);

        let source = Quad::new([
            (0.0, 0.0),
            (card.width as f32 - 1.0, 0.0),
            (card.width as f32 - 1.0, card.height as f32 - 1.0),
            (0.0, card.height as f32 - 1.0),
        ]);
        let Some(homography) = crate::geometry::homography_from_quads(quad, &source) else {
            return frame;
        };

        for y in 0..height {
            for x in 0..width {
                let (sx, sy) = homography.apply((x as f32 + 0.5, y as f32 + 0.5));
                if sx >= 0.0 && sy >= 0.0 && sx < card.width as f32 && sy < card.height as f32 {
                    frame.pixels[(y * width + x) as usize] = card.view().at(sx as u32, sy as u32);
                }
            }
        }
        frame
    }

    /// A database built by running the pipeline's own hasher over reference images, which is
    /// exactly how `arthashes.bin` is produced.
    fn database(seeds: &[u32]) -> ArtDatabase {
        ArtDatabase::new(
            seeds
                .iter()
                .map(|&seed| {
                    let art = crop_artwork(&card_image(seed).view());
                    ArtEntry {
                        hash: hash_gray(&art.view()),
                        printing_id: format!("p-{seed}"),
                        oracle_id: format!("o-{seed}"),
                        name: format!("Card {seed}"),
                    }
                })
                .collect(),
        )
    }

    #[test]
    fn a_card_held_in_front_of_the_camera_is_recognised() {
        let mut scanner = Scanner::new(database(&[1, 2, 3, 4, 5]));
        let quad = Quad::new([
            (160.0, 150.0),
            (480.0, 150.0),
            (480.0, 597.0),
            (160.0, 597.0),
        ]);
        let frame = frame_with(&card_image(3), &quad, 640, 900);

        let mut confirmed = None;
        for _ in 0..DEFAULT_NEEDED {
            if let Outcome::Confirmed(card) = scanner.feed_gray(&frame.pixels, 640, 900) {
                confirmed = Some(card.name.clone());
            }
        }
        assert_eq!(confirmed.as_deref(), Some("Card 3"));
    }

    #[test]
    fn a_tilted_card_is_recognised_too() {
        // Rectification's entire reason for existing: nobody holds a card square to the lens.
        let mut scanner = Scanner::new(database(&[1, 2, 3, 4, 5]));
        let quad = Quad::new([
            (170.0, 160.0),
            (490.0, 190.0),
            (465.0, 620.0),
            (145.0, 585.0),
        ]);
        let frame = frame_with(&card_image(2), &quad, 640, 900);

        let found = scanner
            .recognise_still(&frame.pixels, 640, 900)
            .expect("recognised");
        assert_eq!(found.name, "Card 2");
    }

    #[test]
    fn a_card_the_database_has_never_seen_is_not_named() {
        // Someone scanning a token, a proxy, or a card from a set that was not downloaded.
        let mut scanner = Scanner::new(database(&[1, 2, 3]));
        let quad = Quad::new([
            (160.0, 150.0),
            (480.0, 150.0),
            (480.0, 597.0),
            (160.0, 597.0),
        ]);
        let frame = frame_with(&card_image(77), &quad, 640, 900);
        assert!(scanner.recognise_still(&frame.pixels, 640, 900).is_none());
    }

    #[test]
    fn an_empty_scene_never_confirms_anything() {
        let mut scanner = Scanner::new(database(&[1, 2, 3]));
        let frame = GrayImage::new(640, 900);
        for _ in 0..40 {
            assert_eq!(
                scanner.feed_gray(&frame.pixels, 640, 900),
                Outcome::Searching
            );
        }
    }

    #[test]
    fn the_outline_is_available_for_the_viewfinder() {
        let mut scanner = Scanner::new(database(&[1]));
        let quad = Quad::new([
            (160.0, 150.0),
            (480.0, 150.0),
            (480.0, 597.0),
            (160.0, 597.0),
        ]);
        let frame = frame_with(&card_image(1), &quad, 640, 900);

        scanner.feed_gray(&frame.pixels, 640, 900);
        let outline = scanner.last_quad().expect("outline").ordered().corners;
        assert!((outline[0].0 - 160.0).abs() < 20.0, "{outline:?}");
        assert!((outline[0].1 - 150.0).abs() < 20.0, "{outline:?}");

        // And it goes away when the card does, so the overlay does not stick.
        let empty = GrayImage::new(640, 900);
        scanner.feed_gray(&empty.pixels, 640, 900);
        assert!(scanner.last_quad().is_none());
    }

    #[test]
    fn rgba_frames_from_a_canvas_work_the_same() {
        // What the WebView actually hands over.
        let mut scanner = Scanner::new(database(&[1, 2, 3, 4, 5]));
        let quad = Quad::new([
            (160.0, 150.0),
            (480.0, 150.0),
            (480.0, 597.0),
            (160.0, 597.0),
        ]);
        let frame = frame_with(&card_image(4), &quad, 640, 900);

        let rgba: Vec<u8> = frame
            .pixels
            .iter()
            .flat_map(|&value| [value, value, value, 255])
            .collect();

        let mut confirmed = None;
        for _ in 0..DEFAULT_NEEDED {
            if let Outcome::Confirmed(card) = scanner.feed_rgba(&rgba, 640, 900) {
                confirmed = Some(card.name.clone());
            }
        }
        assert_eq!(confirmed.as_deref(), Some("Card 4"));
    }

    #[test]
    fn a_truncated_frame_is_declined_rather_than_crashing() {
        // A dropped frame from the camera, which does happen.
        let mut scanner = Scanner::new(database(&[1, 2]));
        assert_eq!(scanner.feed_rgba(&[0u8; 64], 640, 900), Outcome::Searching);
        assert!(scanner.recognise_still(&[0u8; 64], 640, 900).is_none());
    }

    #[test]
    fn a_scanner_with_no_artwork_database_still_runs() {
        // The artwork hashes are an optional download; the app must work without them.
        let mut scanner = Scanner::new(ArtDatabase::default());
        let quad = Quad::new([
            (160.0, 150.0),
            (480.0, 150.0),
            (480.0, 597.0),
            (160.0, 597.0),
        ]);
        let frame = frame_with(&card_image(1), &quad, 640, 900);

        assert_eq!(
            scanner.feed_gray(&frame.pixels, 640, 900),
            Outcome::Searching
        );
        // The card was still found — the overlay works, only the naming does not.
        assert!(scanner.last_quad().is_some());
    }
}
