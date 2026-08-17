//! Quadrilaterals, corner ordering and perspective correction.
//!
//! A card photographed at an angle is a quadrilateral, not a rectangle. Everything downstream —
//! and the hash above all — needs it squared up first, because a hash of a trapezoid does not
//! match a hash of the rectangle it came from.

use serde::{Deserialize, Serialize};

/// A Magic card is 63 × 88 mm.
pub const CARD_ASPECT_RATIO: f32 = 63.0 / 88.0;

/// Width the rectified card is rendered at. Matches Scryfall's `normal` image, so a hash taken
/// from a camera and one taken from a downloaded image are computed on the same geometry.
pub const RECTIFIED_WIDTH: u32 = 488;

/// Height to match.
pub const RECTIFIED_HEIGHT: u32 = 680;

/// A point in image space.
pub type Point = (f32, f32);

/// Four corners, in clockwise order from the top left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quad {
    pub corners: [Point; 4],
}

impl Quad {
    pub fn new(corners: [Point; 4]) -> Quad {
        Quad { corners }
    }

    /// Puts the corners in a known order: top-left, top-right, bottom-right, bottom-left.
    ///
    /// Contour finders return corners in whatever order they walked the outline, and rectifying
    /// with them unordered gives a card that is rotated or mirrored — which then hashes to
    /// nothing at all. Ordering by the sum and difference of the coordinates is the standard
    /// trick: the top-left has the smallest x+y, the bottom-right the largest, and the other
    /// two are separated by y−x.
    pub fn ordered(self) -> Quad {
        let mut corners = self.corners;
        corners.sort_by(|a, b| (a.0 + a.1).total_cmp(&(b.0 + b.1)));
        let top_left = corners[0];
        let bottom_right = corners[3];

        let mut middle = [corners[1], corners[2]];
        middle.sort_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)));
        let top_right = middle[0];
        let bottom_left = middle[1];

        Quad {
            corners: [top_left, top_right, bottom_right, bottom_left],
        }
    }

    /// Area, by the shoelace formula.
    pub fn area(&self) -> f32 {
        let c = &self.corners;
        let mut total = 0.0;
        for i in 0..4 {
            let (x1, y1) = c[i];
            let (x2, y2) = c[(i + 1) % 4];
            total += x1 * y2 - x2 * y1;
        }
        total.abs() / 2.0
    }

    /// Mean of the two vertical edges, and of the two horizontal ones.
    fn side_lengths(&self) -> (f32, f32) {
        let c = self.ordered().corners;
        let distance = |a: Point, b: Point| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        let width = (distance(c[0], c[1]) + distance(c[3], c[2])) / 2.0;
        let height = (distance(c[0], c[3]) + distance(c[1], c[2])) / 2.0;
        (width, height)
    }

    /// True when the shape could plausibly be a card seen at an angle.
    ///
    /// `tolerance` is generous on purpose: perspective compresses one dimension, so a card
    /// tilted away from the camera measures noticeably narrower than 63×88. Rejecting too
    /// eagerly here means the card is never seen at all, while a few false quads cost only a
    /// hash lookup that finds nothing.
    pub fn looks_like_a_card(&self, tolerance: f32) -> bool {
        let (width, height) = self.side_lengths();
        if width <= 1.0 || height <= 1.0 {
            return false;
        }
        // Either orientation: a card lying sideways is still a card.
        let ratio = width / height;
        let upright = (ratio - CARD_ASPECT_RATIO).abs() <= tolerance;
        let sideways = (ratio - 1.0 / CARD_ASPECT_RATIO).abs() <= tolerance / CARD_ASPECT_RATIO;
        upright || sideways
    }

    /// True when the quadrilateral is convex, which a real card outline always is.
    ///
    /// Filters out the self-intersecting shapes a contour approximation produces from noise.
    pub fn is_convex(&self) -> bool {
        let c = &self.corners;
        let mut positive = false;
        let mut negative = false;
        for i in 0..4 {
            let (ax, ay) = c[i];
            let (bx, by) = c[(i + 1) % 4];
            let (cx, cy) = c[(i + 2) % 4];
            let cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
            if cross > 0.0 {
                positive = true;
            }
            if cross < 0.0 {
                negative = true;
            }
        }
        !(positive && negative)
    }
}

/// A 3×3 projective transform, row-major.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography(pub [f32; 9]);

impl Homography {
    /// Maps a point through the transform.
    pub fn apply(&self, point: Point) -> Point {
        let h = &self.0;
        let (x, y) = point;
        let w = h[6] * x + h[7] * y + h[8];
        if w.abs() < f32::EPSILON {
            return (0.0, 0.0);
        }
        (
            (h[0] * x + h[1] * y + h[2]) / w,
            (h[3] * x + h[4] * y + h[5]) / w,
        )
    }
}

/// Solves for the transform taking `from` to `to`.
///
/// Four point correspondences give eight equations in the eight unknowns of a homography (the
/// ninth element is fixed at 1 by scale). Returns `None` when the points are degenerate —
/// three of them collinear, say — which a noisy contour can easily produce.
pub fn homography_from_quads(from: &Quad, to: &Quad) -> Option<Homography> {
    let mut matrix = [[0.0f32; 9]; 8];

    for i in 0..4 {
        let (x, y) = from.corners[i];
        let (u, v) = to.corners[i];

        matrix[i * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        matrix[i * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }

    let solution = solve(matrix)?;
    Some(Homography([
        solution[0],
        solution[1],
        solution[2],
        solution[3],
        solution[4],
        solution[5],
        solution[6],
        solution[7],
        1.0,
    ]))
}

/// Gaussian elimination with partial pivoting on an 8×9 augmented matrix.
fn solve(mut matrix: [[f32; 9]; 8]) -> Option<[f32; 8]> {
    for column in 0..8 {
        // Pivot on the largest remaining magnitude, or the elimination loses precision fast on
        // the near-singular systems a wobbly contour produces.
        let pivot = (column..8)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))?;
        if matrix[pivot][column].abs() < 1e-9 {
            // Degenerate: the four points do not define a transform.
            return None;
        }
        matrix.swap(column, pivot);

        let divisor = matrix[column][column];
        for value in matrix[column].iter_mut() {
            *value /= divisor;
        }

        let pivot_row = matrix[column];
        for (row, values) in matrix.iter_mut().enumerate() {
            if row == column {
                continue;
            }
            let factor = values[column];
            if factor == 0.0 {
                continue;
            }
            for (value, pivot) in values.iter_mut().zip(pivot_row.iter()).skip(column) {
                *value -= factor * pivot;
            }
        }
    }

    let mut solution = [0.0f32; 8];
    for (i, value) in solution.iter_mut().enumerate() {
        *value = matrix[i][8];
        if !value.is_finite() {
            return None;
        }
    }
    Some(solution)
}

/// The rectangle a rectified card occupies.
pub fn rectified_quad() -> Quad {
    let w = RECTIFIED_WIDTH as f32;
    let h = RECTIFIED_HEIGHT as f32;
    Quad::new([(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point, tolerance: f32) -> bool {
        (a.0 - b.0).abs() <= tolerance && (a.1 - b.1).abs() <= tolerance
    }

    #[test]
    fn corners_are_ordered_clockwise_from_the_top_left() {
        // Whatever order the contour finder walked them in.
        let expected = [(10.0, 10.0), (110.0, 12.0), (108.0, 150.0), (12.0, 148.0)];
        for rotation in 0..4 {
            let mut shuffled = expected;
            shuffled.rotate_left(rotation);
            assert_eq!(Quad::new(shuffled).ordered().corners, expected);
        }
    }

    #[test]
    fn ordering_survives_a_reversed_winding() {
        let expected = [(10.0, 10.0), (110.0, 12.0), (108.0, 150.0), (12.0, 148.0)];
        let mut reversed = expected;
        reversed.reverse();
        assert_eq!(Quad::new(reversed).ordered().corners, expected);
    }

    #[test]
    fn a_card_shaped_rectangle_is_recognised() {
        // 63 x 88 at some arbitrary scale.
        let card = Quad::new([(0.0, 0.0), (126.0, 0.0), (126.0, 176.0), (0.0, 176.0)]);
        assert!(card.looks_like_a_card(0.15));
    }

    #[test]
    fn a_square_is_not_a_card() {
        let square = Quad::new([(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]);
        assert!(!square.looks_like_a_card(0.15));
    }

    #[test]
    fn a_card_lying_sideways_is_still_a_card() {
        let sideways = Quad::new([(0.0, 0.0), (176.0, 0.0), (176.0, 126.0), (0.0, 126.0)]);
        assert!(sideways.looks_like_a_card(0.15));
    }

    #[test]
    fn a_degenerate_quad_is_rejected_rather_than_dividing_by_zero() {
        let flat = Quad::new([(0.0, 0.0), (100.0, 0.0), (100.0, 0.0), (0.0, 0.0)]);
        assert!(!flat.looks_like_a_card(0.15));
        assert_eq!(flat.area(), 0.0);
    }

    #[test]
    fn convexity_rejects_a_self_intersecting_shape() {
        let card = Quad::new([(0.0, 0.0), (126.0, 0.0), (126.0, 176.0), (0.0, 176.0)]);
        assert!(card.is_convex());

        // A bowtie, which a noisy contour approximation readily produces.
        let bowtie = Quad::new([(0.0, 0.0), (126.0, 176.0), (126.0, 0.0), (0.0, 176.0)]);
        assert!(!bowtie.is_convex());
    }

    #[test]
    fn area_is_computed_regardless_of_winding() {
        let clockwise = Quad::new([(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)]);
        let mut corners = clockwise.corners;
        corners.reverse();
        assert_eq!(clockwise.area(), 200.0);
        assert_eq!(Quad::new(corners).area(), 200.0);
    }

    #[test]
    fn an_identity_transform_leaves_points_alone() {
        let square = rectified_quad();
        let homography = homography_from_quads(&square, &square).expect("solvable");
        for corner in square.corners {
            assert!(close(homography.apply(corner), corner, 0.01), "{corner:?}");
        }
    }

    #[test]
    fn a_transform_maps_the_corners_it_was_built_from() {
        // The property the whole rectification rests on.
        let tilted = Quad::new([(52.0, 31.0), (301.0, 74.0), (288.0, 402.0), (33.0, 358.0)]);
        let target = rectified_quad();

        let homography = homography_from_quads(&tilted, &target).expect("solvable");
        for (source, expected) in tilted.corners.iter().zip(target.corners.iter()) {
            assert!(
                close(homography.apply(*source), *expected, 0.5),
                "{source:?} should map to {expected:?}, got {:?}",
                homography.apply(*source)
            );
        }
    }

    #[test]
    fn a_transform_and_its_inverse_return_a_point_to_where_it_started() {
        let tilted = Quad::new([(52.0, 31.0), (301.0, 74.0), (288.0, 402.0), (33.0, 358.0)]);
        let target = rectified_quad();

        let forward = homography_from_quads(&tilted, &target).expect("forward");
        let back = homography_from_quads(&target, &tilted).expect("back");

        let point = (170.0, 210.0);
        let round_tripped = back.apply(forward.apply(point));
        assert!(close(round_tripped, point, 1.0), "{round_tripped:?}");
    }

    #[test]
    fn collinear_points_do_not_define_a_transform() {
        // A contour that collapsed to a line, which noise produces. Returning None beats
        // returning nonsense that rectifies into garbage and matches a random card.
        let line = Quad::new([(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)]);
        assert!(homography_from_quads(&line, &rectified_quad()).is_none());
    }

    #[test]
    fn repeated_points_do_not_define_a_transform() {
        let degenerate = Quad::new([(5.0, 5.0), (5.0, 5.0), (5.0, 5.0), (5.0, 5.0)]);
        assert!(homography_from_quads(&degenerate, &rectified_quad()).is_none());
    }

    #[test]
    fn the_rectified_size_matches_scryfalls_normal_image() {
        // So a hash from the camera and a hash from a downloaded image are computed on the
        // same geometry.
        assert_eq!(RECTIFIED_WIDTH, 488);
        assert_eq!(RECTIFIED_HEIGHT, 680);
        let ratio = RECTIFIED_WIDTH as f32 / RECTIFIED_HEIGHT as f32;
        assert!((ratio - CARD_ASPECT_RATIO).abs() < 0.02, "{ratio}");
    }
}
