# Card recognition

> **Status** — built and measured against real card images. What has *not* happened is a real
> camera pointed at real cardboard; every number below comes from real Scryfall images put
> through synthetic distortion. Sections that describe intentions rather than code say so.

## Approach

Find the card → correct the perspective → **perceptual hash of the artwork** → nearest neighbour
by Hamming distance → agree across several frames.

No neural network. This runs offline, is fast, and has one decisive property here: **the
fingerprint is computed on the artwork, so it is language-independent.** French cards are
recognised with no special handling, where OCR of the card name would have failed.

## Pipeline

`mtg_vision::Scanner` is the whole thing behind one method. Each stage is a module tested on its
own; the crate has **no image codec dependency at all**, because frames arrive from a WebView
canvas already decoded.

1. **Detect** (`detect.rs`) — downscale to 320 wide, estimate the background from the median of
   the frame's border, mask everything that differs from it, flood-fill for the largest region,
   take its extreme points as the corners.
2. **Rectify** (`rectify.rs`) — an 8×8 Gaussian elimination for the homography, then backward
   sampling with bilinear interpolation into 488×680, which is the size of Scryfall's `normal`
   image.
3. **Crop** (`rectify.rs`) — the artwork box, at fixed fractions of the card.
4. **Hash** (`hash.rs`) — box-sample to a 17×16 grid, compare horizontally: **256 bits**.
5. **Match** (`matcher.rs`) — brute force over `arthashes.bin`. 50,391 entries at 32 bytes is
   1.6 MB, small enough to sit in cache; an approximate index would be more code and another
   failure mode for microseconds nobody would notice.
6. **Vote** (`vote.rs`) — 5 agreeing frames out of a window of 12, then one confirmation per
   card presented.

### Why detection is not a contour finder

Every tutorial reaches for Canny plus contour tracing plus polygon approximation. That solves a
much harder problem than this one, and costs an image-processing stack in an app that has to
stay small on Android. The scanning flow already asks for one card on a plain background — so
the card *is* the large foreground region, and its corners are that region's extreme points.
A couple of hundred lines, no dependency, every step testable.

The trade is real and it is the documented one: cluttered scenes and several cards at once are
out of scope.

## Measured

`tools/build-artifacts/examples/verify-scan.rs` takes real reference hashes from the artwork
build, downloads the real images again, and distorts them the way a camera would — perspective,
blur, sensor noise, an unevenly lit table. Ten cards × two poses × seven background shades.

Background brightness turned out to dominate everything else. With the shipped settings, ten
cards per cell:

| Table brightness | Square on | Tilted and blurred |
|---|---|---|
| 18 (near black) | 2 | 0 |
| 50 | 7 | 7 |
| 80 | 9 | 8 |
| 110 | **10** | **10** |
| 140 | **10** | **10** |
| 170 | 5 | 6 |
| 210 (near white) | 10 | 2 |

Run it again and the numbers move by a card or two: the sample is drawn from whatever
`.cache/arthashes.jsonl` holds, which grows as the artwork build progresses. The shape is
stable, and the shape is the point.

**Magic cards have a black border.** Against a table as dark as that border there is nothing to
separate: what gets found is the card's bright interior, a few percent smaller, and that shift is
enough to move the artwork crop and ruin the hash. No threshold fixes it — the two really are the
same shade. `detect.rs` carries a pair of tests pinning this down so nobody chases it.

The user guide therefore says to use a **mid-tone** surface, which is the opposite of the
intuitive advice. Two tests, `a_black_bordered_card_on_a_dark_table_is_not_found_in_full` and
`the_same_card_on_a_mid_tone_table_is_found_in_full`, keep the pair honest.

The contrast threshold was calibrated the same way rather than guessed: 24 recognised 105 of 140
photographs against 87 for the 34 it started at, and won at every background brightness.

**Across all 280 attempts the scanner named a card correctly or declined — it never named the
wrong one.** That is the property `DEFAULT_MAX_DISTANCE` and `DEFAULT_MARGIN` exist to protect,
and it is why declined frames are not worth optimising away: the voter simply waits for the next
one.

## Sensitive points

- **Crop framing is critical**, and the measurements above are that principle showing up in
  practice. A perceptual hash compares layout; a few percent of shift moves it much further than
  camera noise ever does. This is the first place to look if recognition disappoints.
- **Reference and query must be framed identically.** `arthashes.bin` hashes the `normal` card
  image through the *same* `crop_artwork` and `hash_gray` the scanner uses — not Scryfall's
  `art_crop`, which is framed differently. Changing `hash_gray`, the `ART_*` constants or
  `RECTIFIED_*` invalidates every published hash; bump `ARCHIVE_VERSION` so old files are
  refused rather than silently matching nothing.
- **The greyscale weights must match on both sides.** 77/150/29, in `mtg_vision::rgba_to_gray`
  and in `ScanView.svelte`. The `image` crate uses different ones, which is why the artwork build
  does not use its conversion.
- **Reprints sharing artwork** cannot be distinguished, by construction. `printings_of` exists so
  the UI can offer the choice. Only a *different oracle id* counts as a rival in the ambiguity
  check, or every reprinted card would be unrecognisable.
- **Foils and glare** remain the untested failure mode. There is no substitute for real cards.
- **Performance budget.** 5–10 fps on a mid-range phone. The bottleneck is moving the frame from
  the WebView into Rust, which is why frames are captured at 640 wide, converted to greyscale in
  JavaScript, sent as a raw IPC body, and dropped when one is already in flight.

## Capture

There is **no `FrameSource` trait**. Both desktop and Android use `getUserMedia` in the WebView,
and the frame source is the `grab()` function in `ScanView.svelte`. If the Android WebView proves
unreliable, the fallback is a Kotlin CameraX plugin — introducing the abstraction at that point
rather than before there is a second implementation to justify it.

## Four destinations

The pipeline is identical; only the sink changes. That is what makes the most expensive part of
the project pay off.

| Destination | Written as |
|---|---|
| Physical collection | A holding in the physical pool, with an optional storage box |
| Digital collection | A holding in the digital pool, kept separate |
| A deck | Deck entries in the chosen zone |
| Draft or sealed pool | Physical holdings in a box named after the pool — which is what a pool is, once the draft is over |

Nothing is written while scanning. Recognised cards collect in a list the user reviews and then
confirms: a misread is far easier to fix before it reaches a collection than after.

## Still to do

- **A golden set of real photographs**, with precision and recall enforced in CI. The synthetic
  distortion above is a good proxy for perspective and noise; it says nothing about foils,
  sleeves, wear, or a real camera's optics. Until that set exists, the recognition rate here is
  an estimate.
- **Run it on an Android device.** None of the Android path has been executed — see
  [`android.md`](android.md).
