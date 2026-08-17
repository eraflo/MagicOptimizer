# Data pipeline

> **Status** — the oracle catalog half is **implemented** (phase 1). The remaining artifacts
> are design only.

`tools/build-artifacts` is a development CLI that turns public sources into compact binary
artifacts. **It never runs inside the app.**

## Why preprocess at all

The full Scryfall bulk is several hundred megabytes of JSONL. Ingesting that on a phone is out of
the question: memory, parse time, disk. The CLI trims it once, on a PC, and produces files the
device only has to memory-map.

## Artifacts produced

| File | Source | Size | Contents |
|---|---|---|---|
| `cards.rkyv` ✅ | Scryfall bulk `oracle-cards` | **25.9 MB measured** | One entry per oracle card: name, cost, cmc, colors, identity, types, text, P/T, keywords, legalities, EDHREC rank, Game Changer flag |
| `printings.rkyv` | Scryfall bulk `default-cards` | ~10 MB | Printings: set, collector number, language, artwork, prices |
| `arthashes.bin` | Scryfall `unique-artwork` + images | ~2 MB | 50,000 × 256-bit pHash with the matching printing id |
| `meta.rkyv` | `json.edhrec.com` | ~5 MB | Inclusion rates, synergy scores |
| `combos.rkyv` | Commander Spellbook `variants` | ~3 MB | Combos: required cards, result, prerequisites |
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

Commander Spellbook's full database has not been downloadable in one piece since 2024, so the
snapshot is built by paginating `variants`.

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
