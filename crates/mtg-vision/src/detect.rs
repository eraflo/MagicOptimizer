//! Finding the card in a frame.
//!
//! # Why this is not a general contour finder
//!
//! The obvious approach is Canny plus contour tracing plus polygon approximation, which is what
//! every OpenCV tutorial does. It also means pulling in an image-processing stack for an app
//! that has to stay small on Android, and it solves a much harder problem than this one.
//!
//! The scanning flow already asks for **one card on a plain background** — that is the advice in
//! the user guide, and it is what people naturally do. Given that, the card is simply the large
//! foreground region, and finding its corners is a matter of separating it from the background
//! and taking the extreme points. No dependency, a couple of hundred lines, and every step is
//! testable.
//!
//! It gives up on cluttered scenes and on several cards at once. Those are real limitations and
//! they are the documented ones.
//!
//! # The background has to be mid-tone
//!
//! Not merely "contrasting" — **mid-tone**, and this is measured. A Magic card's border is
//! black, so against a near-black table there is nothing to separate: what gets found is the
//! card's bright interior, a few percent smaller, and that shift is enough to move the artwork
//! crop and ruin the hash. 2 photographs in 20 recognised on black, against 20 in 20 on
//! mid-grey.
//!
//! No threshold fixes this, because the border and the table genuinely are the same shade — the
//! two tests at the bottom of this file pin the pair down so nobody tries. A mid-tone background
//! wins because it is far from *both* ends of a card's tonal range at once, which is also why
//! white is measurably worse than grey.

use crate::geometry::Quad;
use crate::hash::GrayView;

/// Working width the frame is reduced to before anything else.
///
/// Detection does not need detail — it needs the outline. Everything downstream samples from
/// the *full-resolution* frame using corners scaled back up, so nothing is lost by finding
/// them cheaply.
pub const WORKING_WIDTH: u32 = 320;

/// How far from the background a pixel must be to count as foreground.
///
/// Calibrated against real card images rather than guessed. Sweeping it over 140 photographs —
/// ten cards, two poses, seven background brightnesses — 24 recognised 105 of them against 87
/// for the 34 this started at, and it won at *every* background brightness. Below about 18 the
/// mask starts catching sensor noise; above 40 it starts losing the card's darker regions.
///
/// See `tools/build-artifacts/examples/verify-scan.rs`, which produced those numbers.
pub const DEFAULT_CONTRAST: u8 = 24;

/// Smallest share of the frame the card may occupy.
///
/// Below this it is too small to hash usefully, and it is more likely to be a speck of dust
/// than a card.
pub const MIN_AREA_FRACTION: f32 = 0.06;

/// How far the shape may stray from a card's proportions.
///
/// Generous, because perspective compresses one dimension: a card tilted away from the camera
/// measures noticeably narrower than 63×88. Rejecting eagerly here means never seeing the card.
pub const ASPECT_TOLERANCE: f32 = 0.28;

/// Settings for detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectSettings {
    pub contrast: u8,
    pub min_area_fraction: f32,
    pub aspect_tolerance: f32,
}

impl Default for DetectSettings {
    fn default() -> DetectSettings {
        DetectSettings {
            contrast: DEFAULT_CONTRAST,
            min_area_fraction: MIN_AREA_FRACTION,
            aspect_tolerance: ASPECT_TOLERANCE,
        }
    }
}

/// Looks for a card, returning its corners in the frame's own coordinates.
pub fn find_card(frame: &GrayView<'_>, settings: DetectSettings) -> Option<Quad> {
    if frame.is_empty() || frame.width < 16 || frame.height < 16 {
        return None;
    }

    let (small, scale) = downscale(frame);
    let background = background_level(&small);
    let mask = foreground_mask(&small, background, settings.contrast);
    let region = largest_region(&mask, small.width, small.height)?;

    let area_fraction = region.len() as f32 / (small.width * small.height) as f32;
    if area_fraction < settings.min_area_fraction {
        return None;
    }

    let quad = corners_of(&region, small.width);
    if !quad.is_convex() || !quad.looks_like_a_card(settings.aspect_tolerance) {
        return None;
    }

    // Back to the original frame's coordinates, where the detail is.
    Some(Quad::new(quad.corners.map(|(x, y)| (x * scale, y * scale))))
}

struct Small {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl Small {
    fn at(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y as usize) * (self.width as usize) + (x as usize)]
    }
}

/// Reduces the frame, returning it and the factor to scale coordinates back up by.
fn downscale(frame: &GrayView<'_>) -> (Small, f32) {
    if frame.width <= WORKING_WIDTH {
        return (
            Small {
                pixels: frame.pixels[..(frame.width * frame.height) as usize].to_vec(),
                width: frame.width,
                height: frame.height,
            },
            1.0,
        );
    }

    let scale = frame.width as f32 / WORKING_WIDTH as f32;
    let width = WORKING_WIDTH;
    let height = ((frame.height as f32 / scale) as u32).max(1);

    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            // Box average over the source cell, which suppresses the sensor noise that would
            // otherwise punch holes in the foreground mask.
            let x0 = (x as f32 * scale) as u32;
            let x1 = (((x + 1) as f32 * scale) as u32)
                .min(frame.width)
                .max(x0 + 1);
            let y0 = (y as f32 * scale) as u32;
            let y1 = (((y + 1) as f32 * scale) as u32)
                .min(frame.height)
                .max(y0 + 1);

            let mut total = 0u32;
            let mut count = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    total += u32::from(frame.at(sx, sy));
                    count += 1;
                }
            }
            pixels.push(total.checked_div(count).unwrap_or(0) as u8);
        }
    }

    (
        Small {
            pixels,
            width,
            height,
        },
        scale,
    )
}

/// Median brightness around the frame's border.
///
/// The border is background by definition when the card is in the middle of the shot, and a
/// median rather than a mean so a bright corner or a shadow does not drag the estimate.
fn background_level(small: &Small) -> u8 {
    let mut samples = Vec::new();
    let border = (small.width.min(small.height) / 20).max(1);

    for y in 0..small.height {
        for x in 0..small.width {
            let on_border =
                x < border || y < border || x >= small.width - border || y >= small.height - border;
            if on_border {
                samples.push(small.at(x, y));
            }
        }
    }

    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Marks pixels that differ from the background.
fn foreground_mask(small: &Small, background: u8, contrast: u8) -> Vec<bool> {
    small
        .pixels
        .iter()
        .map(|pixel| pixel.abs_diff(background) >= contrast)
        .collect()
}

/// The largest connected blob of foreground pixels.
///
/// Flood filled iteratively rather than recursively: a region can cover most of a 320×240
/// frame, and recursion at that depth overflows the stack.
fn largest_region(mask: &[bool], width: u32, height: u32) -> Option<Vec<(u32, u32)>> {
    let mut visited = vec![false; mask.len()];
    let mut best: Option<Vec<(u32, u32)>> = None;

    for start in 0..mask.len() {
        if !mask[start] || visited[start] {
            continue;
        }

        let mut region = Vec::new();
        let mut stack = vec![start];
        visited[start] = true;

        while let Some(index) = stack.pop() {
            let x = (index as u32) % width;
            let y = (index as u32) / width;
            region.push((x, y));

            let push = |nx: u32, ny: u32, stack: &mut Vec<usize>, visited: &mut Vec<bool>| {
                let neighbour = (ny as usize) * (width as usize) + (nx as usize);
                if mask[neighbour] && !visited[neighbour] {
                    visited[neighbour] = true;
                    stack.push(neighbour);
                }
            };

            if x > 0 {
                push(x - 1, y, &mut stack, &mut visited);
            }
            if x + 1 < width {
                push(x + 1, y, &mut stack, &mut visited);
            }
            if y > 0 {
                push(x, y - 1, &mut stack, &mut visited);
            }
            if y + 1 < height {
                push(x, y + 1, &mut stack, &mut visited);
            }
        }

        if best
            .as_ref()
            .is_none_or(|current| region.len() > current.len())
        {
            best = Some(region);
        }
    }

    best
}

/// The four corners of a region.
///
/// The extreme points of `x+y` and `y−x` are the corners of a rotated rectangle: the smallest
/// sum is the top-left, the largest the bottom-right, and the difference separates the other
/// two. The same trick orders the corners, so what comes out is already in the order
/// rectification wants.
fn corners_of(region: &[(u32, u32)], _width: u32) -> Quad {
    let mut top_left = region[0];
    let mut bottom_right = region[0];
    let mut top_right = region[0];
    let mut bottom_left = region[0];

    for &(x, y) in region {
        let sum = x as i64 + y as i64;
        let difference = y as i64 - x as i64;

        if sum < top_left.0 as i64 + top_left.1 as i64 {
            top_left = (x, y);
        }
        if sum > bottom_right.0 as i64 + bottom_right.1 as i64 {
            bottom_right = (x, y);
        }
        if difference < top_right.1 as i64 - top_right.0 as i64 {
            top_right = (x, y);
        }
        if difference > bottom_left.1 as i64 - bottom_left.0 as i64 {
            bottom_left = (x, y);
        }
    }

    Quad::new([
        (top_left.0 as f32, top_left.1 as f32),
        (top_right.0 as f32, top_right.1 as f32),
        (bottom_right.0 as f32, bottom_right.1 as f32),
        (bottom_left.0 as f32, bottom_left.1 as f32),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Frame {
        pixels: Vec<u8>,
        width: u32,
        height: u32,
    }

    impl Frame {
        fn new(width: u32, height: u32, background: u8) -> Frame {
            Frame {
                pixels: vec![background; (width * height) as usize],
                width,
                height,
            }
        }

        fn view(&self) -> GrayView<'_> {
            GrayView::new(&self.pixels, self.width, self.height).expect("view")
        }

        /// Fills a quadrilateral, so a card can be drawn at an angle.
        fn fill_quad(&mut self, quad: &Quad, value: u8) {
            for y in 0..self.height {
                for x in 0..self.width {
                    if inside(quad, x as f32 + 0.5, y as f32 + 0.5) {
                        self.pixels[(y * self.width + x) as usize] = value;
                    }
                }
            }
        }

        fn fill_rect(&mut self, x0: u32, y0: u32, w: u32, h: u32, value: u8) {
            for y in y0..(y0 + h).min(self.height) {
                for x in x0..(x0 + w).min(self.width) {
                    self.pixels[(y * self.width + x) as usize] = value;
                }
            }
        }
    }

    fn inside(quad: &Quad, x: f32, y: f32) -> bool {
        let c = quad.ordered().corners;
        let mut sign = 0i32;
        for i in 0..4 {
            let (ax, ay) = c[i];
            let (bx, by) = c[(i + 1) % 4];
            let cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
            let current = if cross > 0.0 {
                1
            } else if cross < 0.0 {
                -1
            } else {
                0
            };
            if current != 0 {
                if sign == 0 {
                    sign = current;
                } else if sign != current {
                    return false;
                }
            }
        }
        true
    }

    /// A card-shaped rectangle centred in a dark frame.
    fn frame_with_card(width: u32, height: u32) -> (Frame, (u32, u32, u32, u32)) {
        let mut frame = Frame::new(width, height, 30);
        let card_w = width / 2;
        let card_h = (card_w as f32 / crate::geometry::CARD_ASPECT_RATIO) as u32;
        let x0 = (width - card_w) / 2;
        let y0 = (height.saturating_sub(card_h)) / 2;
        frame.fill_rect(x0, y0, card_w, card_h, 210);
        (frame, (x0, y0, card_w, card_h))
    }

    #[test]
    fn a_card_on_a_plain_background_is_found() {
        let (frame, (x0, y0, w, h)) = frame_with_card(640, 900);
        let quad = find_card(&frame.view(), DetectSettings::default()).expect("found");

        let corners = quad.ordered().corners;
        let tolerance = 12.0;
        assert!((corners[0].0 - x0 as f32).abs() < tolerance, "{corners:?}");
        assert!((corners[0].1 - y0 as f32).abs() < tolerance, "{corners:?}");
        assert!(
            (corners[2].0 - (x0 + w) as f32).abs() < tolerance,
            "{corners:?}"
        );
        assert!(
            (corners[2].1 - (y0 + h) as f32).abs() < tolerance,
            "{corners:?}"
        );
    }

    #[test]
    fn corners_come_back_in_the_frames_own_coordinates() {
        // Detection works on a reduced copy; the corners have to be scaled back up or every
        // rectification would crop the wrong part of the frame.
        let (frame, _) = frame_with_card(1280, 1800);
        let quad = find_card(&frame.view(), DetectSettings::default()).expect("found");
        let corners = quad.ordered().corners;
        assert!(
            corners[2].0 > 640.0,
            "corners look like working-resolution coordinates: {corners:?}"
        );
    }

    #[test]
    fn a_tilted_card_is_found() {
        let mut frame = Frame::new(640, 900, 30);
        let tilted = Quad::new([
            (140.0, 190.0),
            (470.0, 240.0),
            (430.0, 700.0),
            (100.0, 650.0),
        ]);
        frame.fill_quad(&tilted, 210);

        let found = find_card(&frame.view(), DetectSettings::default()).expect("found");
        let expected = tilted.ordered().corners;
        let actual = found.ordered().corners;
        for (a, b) in actual.iter().zip(expected.iter()) {
            assert!(
                (a.0 - b.0).abs() < 20.0 && (a.1 - b.1).abs() < 20.0,
                "{actual:?} against {expected:?}"
            );
        }
    }

    #[test]
    fn a_dark_card_on_a_light_background_is_found_too() {
        // The mask is a difference from the background, not a brightness threshold, so which
        // way round the contrast runs does not matter.
        let mut frame = Frame::new(640, 900, 230);
        frame.fill_rect(160, 190, 320, 447, 25);
        assert!(find_card(&frame.view(), DetectSettings::default()).is_some());
    }

    #[test]
    fn an_empty_frame_finds_nothing() {
        let frame = Frame::new(640, 900, 128);
        assert!(find_card(&frame.view(), DetectSettings::default()).is_none());
    }

    #[test]
    fn something_too_small_is_ignored() {
        // A speck of dust, or a card too far away to hash usefully.
        let mut frame = Frame::new(640, 900, 30);
        frame.fill_rect(300, 400, 30, 42, 210);
        assert!(find_card(&frame.view(), DetectSettings::default()).is_none());
    }

    #[test]
    fn something_that_is_not_card_shaped_is_rejected() {
        // A sheet of paper, a phone, a coaster.
        let mut frame = Frame::new(640, 900, 30);
        frame.fill_rect(120, 300, 400, 400, 210);
        assert!(find_card(&frame.view(), DetectSettings::default()).is_none());
    }

    #[test]
    fn the_largest_object_wins_when_something_else_is_in_shot() {
        // A card and a smaller distractor. The distractor must not drag the corners.
        let (mut frame, (x0, y0, w, h)) = frame_with_card(640, 900);
        frame.fill_rect(10, 10, 60, 60, 200);

        let quad = find_card(&frame.view(), DetectSettings::default()).expect("found");
        let corners = quad.ordered().corners;
        assert!(
            (corners[0].0 - x0 as f32).abs() < 15.0 && (corners[0].1 - y0 as f32).abs() < 15.0,
            "the distractor pulled the corners: {corners:?}"
        );
        assert!((corners[2].0 - (x0 + w) as f32).abs() < 15.0, "{corners:?}");
        assert!((corners[2].1 - (y0 + h) as f32).abs() < 15.0, "{corners:?}");
    }

    #[test]
    fn a_tiny_frame_is_declined_rather_than_crashing() {
        let frame = Frame::new(8, 8, 30);
        assert!(find_card(&frame.view(), DetectSettings::default()).is_none());
    }

    #[test]
    fn a_black_bordered_card_on_a_dark_table_is_not_found_in_full() {
        // A known, physical limitation rather than a bug, and worth pinning down so nobody
        // "fixes" the contrast threshold to chase it.
        //
        // Magic cards have a black border. Against a table as dark as that border there is
        // nothing to separate — the two are the same shade, and no threshold on brightness can
        // tell them apart. What gets found is the card's bright *interior*, a few percent
        // smaller, which is enough to shift the artwork crop and ruin the hash.
        //
        // Measured on real card images: on a near-black background 1 photograph in 20 was
        // recognised; on a mid-grey one, 19 or 20 of 20. The user guide says to use a mid-tone
        // surface for this reason, and it is not a nicety.
        let mut frame = Frame::new(640, 900, 22);
        let border = Quad::new([
            (160.0, 150.0),
            (480.0, 150.0),
            (480.0, 597.0),
            (160.0, 597.0),
        ]);
        frame.fill_quad(&border, 24);
        // The interior, inset by roughly the real border width.
        frame.fill_rect(175, 165, 290, 417, 190);

        let found = find_card(&frame.view(), DetectSettings::default());
        if let Some(quad) = found {
            let corners = quad.ordered().corners;
            assert!(
                corners[0].0 > 168.0,
                "the border was somehow separated from the table: {corners:?}"
            );
        }
    }

    #[test]
    fn the_same_card_on_a_mid_tone_table_is_found_in_full() {
        // The other half of the pair above: the border only becomes visible once the background
        // is far enough from it, which is exactly the advice the user guide gives.
        let mut frame = Frame::new(640, 900, 125);
        let border = Quad::new([
            (160.0, 150.0),
            (480.0, 150.0),
            (480.0, 597.0),
            (160.0, 597.0),
        ]);
        frame.fill_quad(&border, 24);
        frame.fill_rect(175, 165, 290, 417, 190);

        let quad = find_card(&frame.view(), DetectSettings::default()).expect("found");
        let corners = quad.ordered().corners;
        assert!(
            (corners[0].0 - 160.0).abs() < 12.0 && (corners[0].1 - 150.0).abs() < 12.0,
            "the whole card should be found, not its interior: {corners:?}"
        );
    }

    #[test]
    fn a_frame_of_noise_does_not_produce_a_card() {
        // The failure that would matter most: inventing a card from nothing, then adding
        // whatever it happened to match to the collection.
        let mut state = 12345u64;
        let pixels: Vec<u8> = (0..640 * 900)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                (state >> 33) as u8
            })
            .collect();
        let view = GrayView::new(&pixels, 640, 900).expect("view");
        assert!(find_card(&view, DetectSettings::default()).is_none());
    }

    #[test]
    fn detection_survives_a_noisy_card() {
        // A real camera frame is never flat. Downscaling averages most of it away, which is
        // half the reason it happens before anything else.
        let (mut frame, _) = frame_with_card(640, 900);
        let mut state = 99u64;
        for pixel in frame.pixels.iter_mut() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let noise = ((state >> 40) % 24) as u8;
            *pixel = pixel.saturating_add(noise).saturating_sub(12);
        }
        assert!(find_card(&frame.view(), DetectSettings::default()).is_some());
    }
}
