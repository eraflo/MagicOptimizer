//! Squaring up a card and cropping its artwork.

use crate::geometry::{
    homography_from_quads, rectified_quad, Quad, RECTIFIED_HEIGHT, RECTIFIED_WIDTH,
};
use crate::hash::GrayView;

/// Where the illustration sits on a card, as fractions of the card's size.
///
/// Modern frames put the art window in roughly this box. The values are deliberately **inset**
/// from the true edges: pHash is sensitive to how much border creeps in, and a crop that
/// sometimes includes a sliver of frame and sometimes does not produces two different hashes
/// for the same card. Losing a little art costs nothing; including a variable amount of frame
/// costs the match.
pub const ART_LEFT: f32 = 0.09;
pub const ART_RIGHT: f32 = 0.91;
pub const ART_TOP: f32 = 0.11;
pub const ART_BOTTOM: f32 = 0.56;

/// A greyscale buffer the crate owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrayImage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl GrayImage {
    pub fn new(width: u32, height: u32) -> GrayImage {
        GrayImage {
            pixels: vec![0; (width as usize) * (height as usize)],
            width,
            height,
        }
    }

    pub fn view(&self) -> GrayView<'_> {
        GrayView {
            pixels: &self.pixels,
            width: self.width,
            height: self.height,
        }
    }

    fn set(&mut self, x: u32, y: u32, value: u8) {
        if x < self.width && y < self.height {
            self.pixels[(y as usize) * (self.width as usize) + (x as usize)] = value;
        }
    }
}

/// Squares a quadrilateral up into a card-shaped image.
///
/// Samples **backwards** — for each output pixel, work out where it came from — rather than
/// forwards. Mapping input pixels onto the output would leave holes wherever the transform
/// stretches, and this way every output pixel is written exactly once.
pub fn rectify(source: &GrayView<'_>, quad: &Quad) -> Option<GrayImage> {
    let ordered = quad.ordered();
    let target = rectified_quad();

    // The inverse direction: output coordinates back to input coordinates.
    let inverse = homography_from_quads(&target, &ordered)?;

    let mut out = GrayImage::new(RECTIFIED_WIDTH, RECTIFIED_HEIGHT);
    for y in 0..RECTIFIED_HEIGHT {
        for x in 0..RECTIFIED_WIDTH {
            let (sx, sy) = inverse.apply((x as f32 + 0.5, y as f32 + 0.5));
            out.set(x, y, sample_bilinear(source, sx, sy));
        }
    }
    Some(out)
}

/// Reads a pixel at fractional coordinates, blending the four around it.
///
/// Nearest-neighbour sampling would alias badly under rotation, and aliasing is exactly the
/// kind of high-frequency noise that flips hash bits.
fn sample_bilinear(source: &GrayView<'_>, x: f32, y: f32) -> u8 {
    if source.is_empty() || !x.is_finite() || !y.is_finite() {
        return 0;
    }

    let x = x.clamp(0.0, (source.width - 1) as f32);
    let y = y.clamp(0.0, (source.height - 1) as f32);

    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(source.width - 1);
    let y1 = (y0 + 1).min(source.height - 1);

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let top = f32::from(source.at(x0, y0)) * (1.0 - fx) + f32::from(source.at(x1, y0)) * fx;
    let bottom = f32::from(source.at(x0, y1)) * (1.0 - fx) + f32::from(source.at(x1, y1)) * fx;
    (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
}

/// Crops the artwork out of a rectified card.
pub fn crop_artwork(card: &GrayView<'_>) -> GrayImage {
    let left = (card.width as f32 * ART_LEFT) as u32;
    let right = (card.width as f32 * ART_RIGHT) as u32;
    let top = (card.height as f32 * ART_TOP) as u32;
    let bottom = (card.height as f32 * ART_BOTTOM) as u32;

    let width = right.saturating_sub(left).max(1);
    let height = bottom.saturating_sub(top).max(1);

    let mut out = GrayImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            out.set(x, y, card.at(left + x, top + y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::hash_gray;

    /// Smooth, low-frequency content — which is what card artwork actually is.
    ///
    /// An earlier version of these tests used 8-pixel diagonal stripes, and a modest tilt
    /// moved the hash 113 bits out of 256. That was the *fixture* being pathological rather
    /// than the code being wrong: shifting a high-frequency periodic pattern by a few pixels
    /// inverts its phase completely. Paintings have no such structure.
    fn painting(width: u32, height: u32) -> Vec<u8> {
        (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let fx = x as f32 / width as f32;
                    let fy = y as f32 / height as f32;
                    // A couple of broad blobs and a gradient: distinctive, but nothing that
                    // repeats every few pixels.
                    let blob = ((fx - 0.3).powi(2) + (fy - 0.25).powi(2)).sqrt();
                    let other = ((fx - 0.7).powi(2) + (fy - 0.7).powi(2)).sqrt();
                    let value = 120.0 + 110.0 * (1.0 - blob * 2.0).max(0.0)
                        - 70.0 * (1.0 - other * 2.0).max(0.0)
                        + 30.0 * fy;
                    value.clamp(0.0, 255.0) as u8
                })
            })
            .collect()
    }

    /// A dark field with one bright square, for checking where things land.
    fn marked_corner(width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![20u8; (width * height) as usize];
        for y in 0..height / 6 {
            for x in 0..width / 6 {
                pixels[(y * width + x) as usize] = 240;
            }
        }
        pixels
    }

    #[test]
    fn a_rectangle_rectifies_to_the_standard_size() {
        let pixels = painting(400, 560);
        let source = GrayView::new(&pixels, 400, 560).expect("source");
        let quad = Quad::new([(0.0, 0.0), (400.0, 0.0), (400.0, 560.0), (0.0, 560.0)]);

        let card = rectify(&source, &quad).expect("rectified");
        assert_eq!(card.width, RECTIFIED_WIDTH);
        assert_eq!(card.height, RECTIFIED_HEIGHT);
    }

    #[test]
    fn the_top_left_of_the_quad_lands_in_the_top_left_of_the_output() {
        // The direct correctness check, independent of any hashing: a mark in one corner of
        // the source region has to come out in the matching corner. Gets it wrong and the
        // card comes out rotated or mirrored, which hashes to nothing at all.
        let pixels = marked_corner(400, 560);
        let source = GrayView::new(&pixels, 400, 560).expect("source");
        let card = rectify(
            &source,
            &Quad::new([(0.0, 0.0), (400.0, 0.0), (400.0, 560.0), (0.0, 560.0)]),
        )
        .expect("rectified");

        let quarter_w = RECTIFIED_WIDTH / 4;
        let quarter_h = RECTIFIED_HEIGHT / 4;
        assert!(
            card.view().at(quarter_w / 2, quarter_h / 2) > 200,
            "top left should be bright"
        );
        assert!(
            card.view()
                .at(RECTIFIED_WIDTH - quarter_w, RECTIFIED_HEIGHT - quarter_h)
                < 60,
            "bottom right should be dark"
        );
        assert!(
            card.view().at(RECTIFIED_WIDTH - quarter_w, quarter_h / 2) < 60,
            "top right should be dark"
        );
    }

    #[test]
    fn a_tilted_card_rectifies_to_nearly_the_same_hash_as_a_straight_one() {
        // The property the whole pipeline rests on: a card photographed at an angle has to
        // produce the hash of the card seen flat.
        let pixels = painting(600, 840);
        let source = GrayView::new(&pixels, 600, 840).expect("source");

        let straight = Quad::new([(50.0, 50.0), (450.0, 50.0), (450.0, 610.0), (50.0, 610.0)]);
        // The same region, seen from an angle.
        let tilted = Quad::new([(50.0, 60.0), (450.0, 40.0), (455.0, 600.0), (45.0, 620.0)]);

        let a = hash_gray(&rectify(&source, &straight).expect("a").view());
        let b = hash_gray(&rectify(&source, &tilted).expect("b").view());

        let distance = a.distance(&b);
        assert!(
            distance < 40,
            "a modest tilt moved the hash {distance} bits"
        );
    }

    #[test]
    fn a_scaled_view_of_the_same_card_hashes_close() {
        // A camera held nearer or further away, which is every frame.
        let small = painting(300, 420);
        let large = painting(900, 1260);

        let a = hash_gray(
            &rectify(
                &GrayView::new(&small, 300, 420).expect("small"),
                &Quad::new([(0.0, 0.0), (300.0, 0.0), (300.0, 420.0), (0.0, 420.0)]),
            )
            .expect("a")
            .view(),
        );
        let b = hash_gray(
            &rectify(
                &GrayView::new(&large, 900, 1260).expect("large"),
                &Quad::new([(0.0, 0.0), (900.0, 0.0), (900.0, 1260.0), (0.0, 1260.0)]),
            )
            .expect("b")
            .view(),
        );

        let distance = a.distance(&b);
        assert!(
            distance < 20,
            "the same art at two scales was {distance} bits apart"
        );
    }

    #[test]
    fn corner_order_does_not_change_the_result() {
        // Contour finders return corners in arbitrary order; rectify sorts them itself.
        let pixels = painting(400, 560);
        let source = GrayView::new(&pixels, 400, 560).expect("source");

        let corners = [(20.0, 20.0), (380.0, 20.0), (380.0, 540.0), (20.0, 540.0)];
        let reference = rectify(&source, &Quad::new(corners)).expect("reference");

        for rotation in 1..4 {
            let mut shuffled = corners;
            shuffled.rotate_left(rotation);
            assert_eq!(
                rectify(&source, &Quad::new(shuffled)).expect("rotated"),
                reference,
                "rotation {rotation}"
            );
        }
    }

    #[test]
    fn a_degenerate_quad_returns_nothing_rather_than_garbage() {
        // A collapsed contour must not rectify into noise that then matches a random card.
        let pixels = painting(400, 560);
        let source = GrayView::new(&pixels, 400, 560).expect("source");
        let line = Quad::new([(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)]);
        assert!(rectify(&source, &line).is_none());
    }

    #[test]
    fn sampling_outside_the_image_clamps_rather_than_panicking() {
        // A quad can extend past the frame when a card is half out of shot.
        let pixels = painting(100, 140);
        let source = GrayView::new(&pixels, 100, 140).expect("source");
        let overhanging = Quad::new([
            (-50.0, -50.0),
            (150.0, -50.0),
            (150.0, 200.0),
            (-50.0, 200.0),
        ]);
        assert!(rectify(&source, &overhanging).is_some());
    }

    #[test]
    fn the_artwork_crop_is_inset_from_the_card_edges() {
        // Deliberately: a crop that sometimes catches a sliver of frame hashes differently
        // from one that does not.
        let card = GrayImage::new(RECTIFIED_WIDTH, RECTIFIED_HEIGHT);
        let art = crop_artwork(&card.view());

        assert!(art.width < RECTIFIED_WIDTH);
        assert!(
            art.height < RECTIFIED_HEIGHT / 2,
            "art occupies the upper half"
        );
        assert!(art.width > RECTIFIED_WIDTH / 2, "but is not a tiny sliver");
    }

    #[test]
    fn the_artwork_crop_takes_the_region_it_claims() {
        // Marked pixels inside and outside the art window, to check the box is where the
        // constants say it is.
        let mut card = GrayImage::new(RECTIFIED_WIDTH, RECTIFIED_HEIGHT);
        let inside_y = (RECTIFIED_HEIGHT as f32 * 0.3) as u32;
        let outside_y = (RECTIFIED_HEIGHT as f32 * 0.8) as u32;
        for x in 0..RECTIFIED_WIDTH {
            card.set(x, inside_y, 200);
            card.set(x, outside_y, 200);
        }

        let art = crop_artwork(&card.view());
        let bright: u32 = art.pixels.iter().filter(|p| **p == 200).count() as u32;
        // Only the row inside the window survives.
        assert!(
            bright > 0 && bright < art.width * 2,
            "{bright} bright pixels"
        );
    }

    #[test]
    fn cropping_a_zero_sized_image_does_not_panic() {
        let empty = GrayImage::new(0, 0);
        let art = crop_artwork(&empty.view());
        assert_eq!(art.width, 1);
        assert_eq!(art.height, 1);
    }
}
