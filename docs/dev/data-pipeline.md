# Data pipeline

> **Status** — `cards.rkyv`, `combos.rkyv` and `arthashes.bin` are **built and measured**; the
> sizes below marked "measured" come from real runs. `printings.rkyv`, `meta.rkyv` and
> `embeddings.bin` are design only.

`tools/build-artifacts` is a development CLI that turns public sources into compact binary
artifacts. **It never runs inside the app.**

## Why preprocess at all

The full Scryfall bulk is several hundred megabytes of JSONL. Ingesting that on a phone is out of
the question: memory, parse time, disk. The CLI trims it once, on a PC, and produces files the
device only has to memory-map.

## Artifacts produced

| File | Source | Size | Contents |
|---|---|---|---|
| `cards.rkyv` ✅ | Scryfall bulk `oracle-cards` + tagger | **26.4 MB measured** | One entry per oracle card: name, cost, cmc, colors, identity, types, text, P/T, keywords, legalities, EDHREC rank, Game Changer flag, **functional tags** |
| `printings.rkyv` | Scryfall bulk `default-cards` | ~10 MB | Printings: set, collector number, language, artwork, prices |
| `arthashes.bin` ✅ | Scryfall `unique-artwork` + images | **6.2 MB measured** | **50,391 measured** × 256-bit pHash with the matching printing and oracle id |
| `meta.rkyv` | `json.edhrec.com` | ~5 MB | Inclusion rates, synergy scores |
| `combos.rkyv` ✅ | Commander Spellbook bulk dump | **53.8 MB measured** | Combos: required cards, produced result, colour identity, Commander legality |
| `embeddings.bin` | Public decklist corpus | 5–9 MB | 35,000 × 64 dimensions in f16 |

Published as **GitHub Releases**, never committed. The app checks a version and a checksum at
startup.

## Source constraints

### Scryfall

- **JSONL only** since 20 July 2026. The old JSON format was retired; do not write a parser for it.
- **A descriptive User-Agent is mandatory.** Generic UAs are blocked — observed directly, their own
  documentation returns 403 for a default UA. Send an `Accept` header too.
- Stay under **10 requests/second**, and under **2/s** on `/cards/collection`.
- Downloading the ~50,000 artwork images to compute hashes is the longest step. It runs once per
  build, rate-limited and resumable after an interruption.

### Measured, 2026-08-17

Building `cards.rkyv` from a cold cache: a 24.5 MB gzipped download, then **35,306 cards and
3,320 tokens and emblems skipped, in 5.6 seconds**, producing a 25.9 MB artifact.

Two findings from that first real run are worth keeping:

* **`game_changer` is a field Scryfall already provides.** The build produced exactly **53**
  flagged cards, matching the official Commander list. No separate Game Changers list has to be
  fetched or maintained anywhere.
* **The format list had already drifted** from what the docs suggested: `explorer` is gone, and
  `competitivebrawl` and `tlr` are new. This is exactly the failure the unknown-key report below
  exists to catch.

### Detecting format drift

Every legality key Scryfall sends is mapped through `Format::from_scryfall_key`. Unmapped keys
are collected and printed as a loud warning at the end of the run, because the alternative is
invisible data loss — cards would simply stop appearing in searches for that format, with
nothing anywhere saying why.

When the warning fires: add the variant to `mtg_core::Format`, update `LEGALITY_SLOTS`, bump
`FORMAT_VERSION`, and update the pinned list in the `format_list_matches_scryfall_as_observed`
test.

### EDHREC and Commander Spellbook

**Unofficial** endpoints, liable to change without notice. Only this CLI talks to them, never the
app. If one becomes unavailable, the corresponding artifact is missing and the app must degrade
gracefully to heuristics alone — not crash.

Commander Spellbook publishes its whole database as a single gzipped file at
`https://json.commanderspellbook.com/variants.json.gz`, regenerated several times a day. That is
what the build reads: one request, measured at 105,328 variants in 5.4 seconds.

Do **not** go back to paginating `/variants/?limit=100`. That was the first implementation and it
failed: about a thousand requests, rate-limited into a wall of `429`s and finally a `503` around
offset 30,800 — and because pagination accumulates in memory, one failure near the end threw away
everything before it. The bulk file is also the more considerate way to use donated infrastructure.

One subtlety: the file is named `.gz` *and* served with `Content-Encoding: gzip`, so an HTTP
client that handles content encoding — `ureq` does, by default — hands over plain JSON. The build
sniffs the gzip magic number instead of assuming either way.

### Functional tags

The card catalog carries a `tags` field: a 64-bit set of *functional roles* — removal, ramp,
card advantage, counterspell, and thirty-one more. It exists because the optimizer measures a
mana base, a curve and an opening hand, and none of those can tell Lightning Bolt from a
Mountain. Tags are the smallest thing that fixes that: not an understanding of a card, but a
claim about what it is for.

They come from **Scryfall's tagger**, a community-maintained taxonomy queryable as `otag:x`. The
vocabulary lives in `mtg_core::Tag` and every name in it was checked against the tagger before
being added — `wincon`, `stax`, `token`, `pump` and `reanimation` all sound right and do not
exist, so they are absent.

There is **no bulk export**: tags are not in the bulk files and the API offers no way to ask a
card what it carries. It offers the reverse, so the build walks the vocabulary instead of the
catalog — one paginated search per tag. Measured on 2026-08-18: **327 requests, 17.5 minutes**.
That is far slower than the request count suggests, because each page carries 175 complete card
objects of which one field is kept.

Three things about this source are worth stating plainly.

* **It is unofficial and crowdsourced.** Coverage is uneven, but better than the vocabulary
  size suggests: measured, **25,391 of 35,306 cards carry at least one role (72%)**, and 73% of
  non-land cards. No tag in the vocabulary came back empty, so no name has drifted yet.
* **An empty tag set is ambiguous** between "this card has no role" and "nobody has tagged it".
  Grizzly Bears genuinely belongs to the first group. Nothing downstream may read an empty set
  as "does nothing" — it means *nothing is known*.
* **Tag names drift**, exactly like the legality keys. A tag matching no cards is reported
  loudly at the end of the build, because the alternative is a whole role disappearing from
  every deck's analysis with nothing anywhere saying why.

A failure to fetch tags does not fail the build. The catalog without them is the catalog this
project shipped for its first five phases; losing an entire rebuild to a community endpoint
being down would be the wrong trade. `--no-tags` skips them deliberately.

### Artwork hashes

`arthashes.bin` is what the camera scanner matches against. Building it means downloading one
image per distinct artwork — 50,391 of them as of 2026-08-17. A full run took **1 h 52 min** and
produced a 6.2 MB artifact, with **no image failing to download**.

Two decisions in there are load-bearing.

**It hashes the `normal` image, not `art_crop`.** Scryfall's art crop looks like the obvious
choice, and it is the wrong one. At scan time the app photographs a whole card, straightens it,
and cuts the artwork out at fixed fractions of the card. If the reference were framed by Scryfall
and the query framed by us, the two crops would not line up — and a perceptual hash compares
layout, so a few percent of shift moves it much further than camera noise ever does. The `normal`
image is a whole card at 488×680, which is exactly what the rectifier produces, so reference and
query go through the identical `crop_artwork` and `hash_gray`. Greyscale conversion goes through
`mtg_vision::rgba_to_gray` for the same reason: the `image` crate uses different luma weights, and
mixing the two would be a subtle, unfixable mismatch.

**It is resumable, and has to be.** Every hash is appended to `.cache/arthashes.jsonl` as soon as
it is computed, so an interrupted run loses at most one image. Run the command again and it picks
up where it stopped; images that failed are simply retried.

```bash
cargo run --release -p build-artifacts -- --art-only --out ./artifacts
```

Nothing about the images is redistributed. They are downloaded, hashed and discarded; what ships
is a 32-byte fingerprint per artwork, from which no image can be reconstructed. That is a
deliberate property — see the Legal section of `CLAUDE.md`.

## Updating

On every set release, or whenever the Game Changers list changes:

```bash
cargo run -p build-artifacts -- --all --out ./artifacts
```

Then publish a Release with an incremented version. The app detects the new version at startup and
offers the download.

The heavy artifacts — `arthashes.bin` and `embeddings.bin` — are **optional downloads**: someone
who never scans cards should not have to fetch them.

## Fallback

Because the CLI lives in the repository, anyone can generate their own artifacts without depending
on the Releases. That is what keeps the project usable even if the hosting disappears.
