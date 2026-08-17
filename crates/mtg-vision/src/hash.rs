//! Perceptual hashing of card artwork.
//!
//! # Why the artwork and not the card
//!
//! Hashing the illustration rather than the whole card is what makes recognition
//! **language-independent**. A French *Forêt* and an English *Forest* share the same painting,
//! so they hash identically — where an approach based on reading the name would need OCR and a
//! per-language dictionary. It also survives the frame changes between borderless, showcase and
//! retro treatments, which alter everything around the art but not the art itself.
//!
//! The hash is a 16×16 difference hash: 256 bits, each one recording whether a pixel is
//! brighter than the one to its right. Comparing *neighbours* rather than absolute values is
//! what makes it survive a phone camera — exposure, white balance and lighting shift every
//! pixel together and leave the differences between them largely intact.

use serde::{Deserialize, Serialize};

/// Width of the grid the hash is computed on. One extra column is sampled to give each cell a
/// right-hand neighbour to compare against.
pub const HASH_GRID: usize = 16;

/// Bits in a hash: one per cell of the grid.
pub const HASH_BITS: usize = HASH_GRID * HASH_GRID;

/// Bytes a hash occupies.
pub const HASH_BYTES: usize = HASH_BITS / 8;

/// A 256-bit perceptual hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtHash(pub [u8; HASH_BYTES]);

impl ArtHash {
    pub const ZERO: ArtHash = ArtHash([0; HASH_BYTES]);

    /// Number of differing bits.
    ///
    /// This is the whole matching operation: two images of the same painting land within a few
    /// bits of each other, two different paintings land near 128 — half the bits, which is what
    /// random chance gives.
    pub fn distance(&self, other: &ArtHash) -> u32 {
        self.0
            .iter()
            .zip(other.0.iter())
            .map(|(a, b)| (a ^ b).count_ones())
            .sum()
    }

    pub fn as_bytes(&self) -> &[u8; HASH_BYTES] {
        &self.0
    }

    /// Renders as hex, for logs and test fixtures.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parses the form produced by [`ArtHash::to_hex`].
    pub fn from_hex(hex: &str) -> Option<ArtHash> {
        if hex.len() != HASH_BYTES * 2 {
            return None;
        }
        let mut bytes = [0u8; HASH_BYTES];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()?;
        }
        Some(ArtHash(bytes))
    }
}

/// A greyscale image, as the hasher wants it.
///
/// Deliberately not an `image::GrayImage`: frames arrive from a WebView canvas as a raw byte
/// buffer, and going through a codec-carrying image type to hold bytes we already have would
/// be work and weight for nothing.
#[derive(Debug, Clone)]
pub struct GrayView<'a> {
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl<'a> GrayView<'a> {
    /// Wraps a buffer, checking it is the size it claims.
    pub fn new(pixels: &'a [u8], width: u32, height: u32) -> Option<GrayView<'a>> {
        (pixels.len() as u64 >= u64::from(width) * u64::from(height)).then_some(GrayView {
            pixels,
            width,
            height,
        })
    }

    pub fn at(&self, x: u32, y: u32) -> u8 {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        self.pixels
            .get((y as usize) * (self.width as usize) + (x as usize))
            .copied()
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Computes the difference hash of an image.
///
/// The image is box-sampled down to a 17×16 grid first. Sampling by averaging a whole cell
/// rather than picking one pixel is what keeps camera noise and JPEG artefacts from flipping
/// bits: a single speckle moves a cell's average by very little.
pub fn hash_gray(image: &GrayView<'_>) -> ArtHash {
    if image.is_empty() {
        return ArtHash::ZERO;
    }

    // One extra column so every cell has a neighbour to its right.
    let mut cells = [[0u32; HASH_GRID + 1]; HASH_GRID];
    for (row, cells_row) in cells.iter_mut().enumerate() {
        for (column, cell) in cells_row.iter_mut().enumerate() {
            *cell = average_cell(image, column, row);
        }
    }

    let mut bytes = [0u8; HASH_BYTES];
    let mut bit = 0usize;
    for row in cells.iter() {
        for column in 0..HASH_GRID {
            if row[column] > row[column + 1] {
                bytes[bit / 8] |= 1 << (bit % 8);
            }
            bit += 1;
        }
    }
    ArtHash(bytes)
}

/// Mean brightness of one cell of the sampling grid.
fn average_cell(image: &GrayView<'_>, column: usize, row: usize) -> u32 {
    let columns = (HASH_GRID + 1) as u32;
    let rows = HASH_GRID as u32;

    let x0 = (column as u32 * image.width) / columns;
    let x1 = (((column as u32) + 1) * image.width / columns).max(x0 + 1);
    let y0 = (row as u32 * image.height) / rows;
    let y1 = (((row as u32) + 1) * image.height / rows).max(y0 + 1);

    let mut total = 0u32;
    let mut count = 0u32;
    for y in y0..y1.min(image.height.max(1)) {
        for x in x0..x1.min(image.width.max(1)) {
            total += u32::from(image.at(x, y));
            count += 1;
        }
    }
    if count == 0 {
        return 0;
    }
    total / count
}

/// Converts an RGBA buffer to greyscale in place into `out`.
///
/// This is the shape a WebView canvas hands over. Weighted for perceived brightness rather
/// than a flat average, so a red card and a blue one of the same luminance do not come out at
/// different levels.
pub fn rgba_to_gray(rgba: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(rgba.len() / 4);
    for pixel in rgba.chunks_exact(4) {
        let r = u32::from(pixel[0]);
        let g = u32::from(pixel[1]);
        let b = u32::from(pixel[2]);
        // Integer approximation of the usual 0.299/0.587/0.114 weights.
        out.push(((r * 77 + g * 150 + b * 29) >> 8) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gradient, which has a predictable difference pattern.
    fn gradient(width: u32, height: u32, reversed: bool) -> Vec<u8> {
        (0..height)
            .flat_map(|_| {
                (0..width).map(move |x| {
                    let value = (x * 255 / width.max(1)) as u8;
                    if reversed {
                        255 - value
                    } else {
                        value
                    }
                })
            })
            .collect()
    }

    fn noise(width: u32, height: u32, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..width * height)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                (state >> 33) as u8
            })
            .collect()
    }

    #[test]
    fn a_hash_is_two_hundred_and_fifty_six_bits() {
        assert_eq!(HASH_BITS, 256);
        assert_eq!(HASH_BYTES, 32);
    }

    #[test]
    fn an_image_matches_itself_exactly() {
        let pixels = noise(200, 280, 7);
        let view = GrayView::new(&pixels, 200, 280).expect("view");
        let hash = hash_gray(&view);
        assert_eq!(hash.distance(&hash), 0);
        assert_eq!(hash, hash_gray(&view));
    }

    #[test]
    fn two_different_images_are_far_apart() {
        // Around 128 bits is what chance gives; anything near it means the two share nothing.
        let a = noise(200, 280, 1);
        let b = noise(200, 280, 2);
        let distance = hash_gray(&GrayView::new(&a, 200, 280).expect("a"))
            .distance(&hash_gray(&GrayView::new(&b, 200, 280).expect("b")));
        assert!(distance > 80, "unrelated images were {distance} bits apart");
    }

    #[test]
    fn the_same_image_at_a_different_size_hashes_close() {
        // A camera never frames a card at the same scale twice, so this is the property that
        // makes recognition work at all.
        let small = gradient(120, 168, false);
        let large = gradient(480, 672, false);
        let distance = hash_gray(&GrayView::new(&small, 120, 168).expect("small"))
            .distance(&hash_gray(&GrayView::new(&large, 480, 672).expect("large")));
        assert!(
            distance <= 8,
            "same image at two scales was {distance} bits apart"
        );
    }

    #[test]
    fn brightness_changes_barely_move_the_hash() {
        // The reason for comparing neighbours rather than absolute values: a phone camera
        // changes exposure between frames, which shifts every pixel together.
        let base = noise(200, 280, 5);
        let brighter: Vec<u8> = base.iter().map(|p| p.saturating_add(40)).collect();

        let distance = hash_gray(&GrayView::new(&base, 200, 280).expect("base")).distance(
            &hash_gray(&GrayView::new(&brighter, 200, 280).expect("brighter")),
        );
        assert!(distance < 25, "a brightness shift moved {distance} bits");
    }

    #[test]
    fn a_mirrored_image_hashes_differently() {
        // Sanity: the hash has to actually depend on the content's arrangement.
        let forward = gradient(200, 280, false);
        let reversed = gradient(200, 280, true);
        let distance = hash_gray(&GrayView::new(&forward, 200, 280).expect("a"))
            .distance(&hash_gray(&GrayView::new(&reversed, 200, 280).expect("b")));
        assert!(
            distance > 100,
            "a mirrored gradient was only {distance} bits away"
        );
    }

    #[test]
    fn a_flat_image_hashes_to_zero() {
        // Nothing is brighter than its neighbour, so no bit is set. Worth pinning: it means a
        // blank frame produces a hash that will not accidentally match a real card closely.
        let flat = vec![128u8; 200 * 280];
        assert_eq!(
            hash_gray(&GrayView::new(&flat, 200, 280).expect("view")),
            ArtHash::ZERO
        );
    }

    #[test]
    fn an_empty_or_undersized_buffer_is_handled_rather_than_panicking() {
        // Frames come from a WebView; a truncated one must not take the app down.
        assert!(GrayView::new(&[], 100, 100).is_none());
        assert!(GrayView::new(&[0u8; 10], 100, 100).is_none());

        let empty = GrayView::new(&[], 0, 0).expect("zero-sized view is valid");
        assert_eq!(hash_gray(&empty), ArtHash::ZERO);
    }

    #[test]
    fn hex_round_trips() {
        let pixels = noise(64, 64, 11);
        let hash = hash_gray(&GrayView::new(&pixels, 64, 64).expect("view"));

        let hex = hash.to_hex();
        assert_eq!(hex.len(), HASH_BYTES * 2);
        assert_eq!(ArtHash::from_hex(&hex), Some(hash));
    }

    #[test]
    fn malformed_hex_is_rejected() {
        assert_eq!(ArtHash::from_hex(""), None);
        assert_eq!(ArtHash::from_hex("00"), None);
        assert_eq!(ArtHash::from_hex(&"z".repeat(HASH_BYTES * 2)), None);
    }

    #[test]
    fn distance_is_symmetric_and_zero_only_for_equals() {
        let a = hash_gray(&GrayView::new(&noise(64, 64, 1), 64, 64).expect("a"));
        let b = hash_gray(&GrayView::new(&noise(64, 64, 2), 64, 64).expect("b"));

        assert_eq!(a.distance(&b), b.distance(&a));
        assert_eq!(a.distance(&a), 0);
        assert!(a.distance(&b) > 0);
        assert!(a.distance(&b) <= HASH_BITS as u32);
    }

    #[test]
    fn rgba_conversion_weights_for_perceived_brightness() {
        let mut gray = Vec::new();
        // Pure green reads brighter than pure red, which reads brighter than pure blue.
        rgba_to_gray(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255], &mut gray);

        assert_eq!(gray.len(), 3);
        assert!(gray[1] > gray[0], "green should read brightest");
        assert!(gray[0] > gray[2], "red should read brighter than blue");
    }

    #[test]
    fn rgba_conversion_handles_a_truncated_buffer() {
        let mut gray = Vec::new();
        // Three bytes left over: a partial pixel, which chunks_exact drops rather than
        // reading past the end.
        rgba_to_gray(&[255, 255, 255, 255, 1, 2, 3], &mut gray);
        assert_eq!(gray.len(), 1);
    }
}
