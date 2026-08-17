# Card recognition

> **Status** — design document. Shipped in phase 6.

## Approach

Contour detection → perspective correction → **perceptual hash of the artwork** → nearest neighbor
by Hamming distance.

No neural network. This approach is proven by several open-source projects, runs offline, is fast,
and has one decisive property here: **the fingerprint is computed on the artwork, so it is
language-independent.** Cards in French are recognized with no special handling, where an approach
based on OCR of the card name would have failed.

## Pipeline

On a frame downscaled to ~640 px, in a dedicated thread, with frame skipping.

1. **Grayscale → blur → Canny** (`imageproc`)
2. **Contours → polygon approximation** → keep only quadrilaterals whose aspect ratio is close to
   a card's: 63 × 88 mm, i.e. ≈ 0.716
3. **Homography** → rectify to 488 × 680 (the dimensions of Scryfall's `normal` image)
4. **Crop the artwork region** → dHash/pHash at **16 × 16, i.e. 256 bits**
5. **Nearest neighbor** by Hamming distance over `arthashes.bin`: 50,000 × 256 bits brute-forced
   with SIMD, a few hundred microseconds. No approximate index is needed — do not add complexity
   before measuring a problem.
6. **Temporal voting** over N consecutive frames plus a confidence threshold → accept

Temporal voting is what makes continuous video mode usable: a single frame can be wrong, agreement
across several consecutive frames much less so.

## Sensitive points

- **Crop framing is critical.** pHash is sensitive to segmentation: too much border or too little,
  and the distance explodes. This is the first place to look if the recognition rate disappoints.
- **Reprints sharing the same artwork.** The hash cannot distinguish them, by construction. We
  offer the possible printings, defaulting to the most recent. OCR of the collector number at the
  bottom of the card is a later avenue, not required.
- **Foils and glare.** The main expected failure mode. Measure it on the test set before reaching
  for a fix.
- **Performance budget.** Target 5–10 fps of processing on a mid-range phone. The bottleneck is
  moving the frame from the WebView into Rust, not the hash.

## Capture

Behind a `FrameSource` trait:

- **Android** — `getUserMedia` in the WebView (`CAMERA` permission), frames pulled onto a
  `<canvas>`, RGBA buffer passed to Rust via a Tauri command.
  **Fallback if the WebView misbehaves: a Kotlin CameraX plugin.**
- **Desktop** — the `nokhwa` crate.

## Four destinations

The pipeline is identical; only the sink changes. That is what makes the most expensive part of
the project pay off.

| Mode | Use |
|---|---|
| → Collection | Data entry, with a sticky storage location |
| → New deck | Digitize an existing physical deck in one pass |
| → Draft/sealed pool | Enter your pool **after** the draft, then build the 40 |
| → Trade list | Build a valued list of cards to trade |

## Golden test set

~50 annotated photos of real cards, deliberately including the hard cases:

- cards in **French** and other languages
- **foils** under several lighting angles
- **damaged** or worn cards
- cards **in sleeves**
- **full-art** treatments (borderless, showcase)
- varied backgrounds, including cluttered ones

Metrics: precision and recall, with a **regression threshold enforced in CI**. Without that guard,
an innocuous change to the Canny threshold can degrade recognition with nobody noticing.
