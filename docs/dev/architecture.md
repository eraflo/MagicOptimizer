# Architecture

> **Status** — design document. It describes the target; implementation follows the phases listed
> in the [README](../../README.md#status).

## Guiding principle

All domain logic lives in Rust crates that are **independent of Tauri**. They build and test with
a plain `cargo test`, with no Android toolchain and no WebView. `src-tauri/` holds only thin
commands that delegate.

This separation is not cosmetic: the mobile build is slow and finicky, and being able to iterate
on the optimizer or the vision pipeline without touching it changes the pace of work entirely.

```
┌─────────────────────────────────────────────────────────┐
│  src/            Svelte 5 + TypeScript                  │
└──────────────────────────┬──────────────────────────────┘
                           │  Tauri commands (IPC)
┌──────────────────────────┴──────────────────────────────┐
│  src-tauri/      thin commands, state, permissions      │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────┴──────────────────────────────┐
│  crates/         all domain logic, testable alone       │
│                                                          │
│   mtg-core ──┬── mtg-data ──┬── mtg-optimizer            │
│              │              ├── mtg-combo                │
│              ├── mtg-deck ──┘                            │
│              └── mtg-collection                          │
│                                                          │
│   mtg-vision  (depends on nothing — raw pixels in,       │
│                a card name out)                          │
│   mtg-journal, mtg-ml   not built yet                    │
└─────────────────────────────────────────────────────────┘
```

## What each crate does

| Crate | Responsibility | Depends on |
|---|---|---|
| `mtg-core` | Fundamental types: `CardId`, `Color`, `ManaCost`, `Format`, card faces. | — |
| `mtg-data` | Catalog loading (`rkyv` mmap), in-memory indexes, search and filtering. | `mtg-core` |
| `mtg-collection` | Physical and digital collections, storage locations, pools, per-copy metadata. | `mtg-core` |
| `mtg-deck` | Deck model, `FormatRules`, legality checking, `.txt` and `.dec` import/export. | `mtg-core`, `mtg-data` |
| `mtg-optimizer` | Scoring, simulated annealing, Monte Carlo, hypergeometric math. | `mtg-deck`, `mtg-data`, `mtg-collection` |
| `mtg-combo` | Combo detection, Commander bracket estimation. | `mtg-deck`, `mtg-data` |
| `mtg-vision` | Card detection, homography, pHash, matching, temporal voting. | — |
| `mtg-journal` *(not built)* | Recorded games, win rate aggregates, Bayesian priors. | `mtg-core`, `mtg-deck` |
| `mtg-ml` *(not built)* | Card embeddings, personal re-ranker. | `mtg-core`, `mtg-journal` |

## Storage — why not SQLite

`rusqlite` / `libsqlite3-sys` cross-compiled to Android causes documented, recurring problems:
NDK not found, unresolved symbols at link time, runtime crashes on the emulator. The debugging
cost is out of proportion to the actual need.

The catalog is ~35,000 cards. A plain Rust scan over it answers in a few milliseconds, which is
plenty for a UI. So:

- **Read-only** → an `rkyv` artifact, mmap'd, zero-copy deserialization, with a
  `HashMap<name, CardId>` rebuilt at startup. That name index is the *only* one: an earlier
  version of this document also promised an inverted index over oracle text and a `fixedbitset`
  per colour, type and legality. The measurements below are why none of them was built.
- **Mutable** → [`redb`](https://docs.rs/redb): pure Rust, ACID, embedded. Collections, decks,
  storage locations, game log, ranker weights, price cache.
- **Images** → files on disk, fetched on demand from Scryfall's CDN, LRU eviction.

The consequence is that no C toolchain appears anywhere in the build. **Every new dependency must
be checked against that criterion** before being added.

The one documented exception is `tools/build-artifacts`, which uses an HTTP client and therefore
a TLS stack. It is a development tool that runs on a PC, is never built for Android and is never
bundled into the app, so the constraint does not apply to it.

### Measured, on the real 35,306-card catalog

| Operation | Cost |
|---|---|
| Opening the artifact | ~22 ms, nearly all of it building the name index |
| Full scan with color, type and format filters | ~2 ms |
| Full scan including a rules-text substring search | ~5 ms |

This is why `mtg-data` ships **no inverted index and no bitsets**. A linear scan is already an
order of magnitude faster than a UI needs, and an index would be memory and code spent against a
problem that measurement says does not exist. The moment that stops being true — probably when
search runs per keystroke on a low-end phone — the numbers above are the baseline to beat.

### Measured, on the real 105,328-combo snapshot

| Operation | Cost |
|---|---|
| Opening `combos.rkyv` | ~30 ms |
| Building the card → combo index | ~17 ms |
| Finding combos in a normal deck | under a millisecond |
| Finding combos in a 100-card deck made entirely of combo pieces | ~4 ms |

`ComboIndex` is rebuilt on each `deck_combos` call rather than cached. At 17 ms for a
user-initiated action that is not worth the invalidation logic a cache would bring; it would be
worth revisiting if combo detection ever moved into the optimizer's inner loop, where it would
run thousands of times.

## Data flow

```
Public sources            tools/build-artifacts        GitHub Releases        Device
──────────────            ─────────────────────        ───────────────        ──────
Scryfall (JSONL)    ──┐
EDHREC              ──┼──►  trimming, hashing,     ──►  cards.rkyv       ──►  mmap + in-memory
Commander Spellbook ──┤     embedding training          arthashes.bin          indexes
public decklists    ──┘                                 combos.rkyv
                                                        embeddings.bin
```

The CLI **never** runs inside the app: the full Scryfall bulk is several hundred megabytes,
unthinkable to ingest on a phone. Details in [data-pipeline.md](data-pipeline.md).

## Frontend choice

Svelte 5 + TypeScript. On Android the WebView is the bottleneck: a light bundle and little runtime
work matter more than a rich component ecosystem. The production bundle is currently **106 kB of
JavaScript, 38 kB gzipped**, which is the number to watch as features land.

The frontend holds **no domain logic**. It renders, and it sends commands.

## Identifiers: `CardId` versus `oracle_id`

Two identifiers exist and confusing them corrupts data silently, so the rule is worth stating
plainly:

* **`CardId` is a position in one catalog artifact.** It is fast, it is what search returns, and
  it is meaningless the moment the catalog is rebuilt.
* **`oracle_id` is Scryfall's stable identifier.** It survives rebuilds and printings.

Anything written to disk — collections, decks, the game log — uses `oracle_id`. `CardId` never
leaves memory. A collection keyed on `CardId` would appear to work and then quietly become a
different collection after the next set release, which is the worst failure mode available:
nothing errors, and the damage is only noticed much later.

## Persisted data

The collection lives in a `redb` database in the platform application data directory, holdings
encoded as JSON. JSON rather than a compact binary format on purpose: it is the user's own data,
it is small, and being able to read it with any tool is worth more than the bytes saved.

A *holding* is a stack of interchangeable copies — same card, printing, language, finish,
condition and location. Adding a card that matches an existing holding raises its quantity rather
than creating a near-duplicate row, which is what stops a scanned binder from becoming thousands
of rows of one. One index exists, mapping that merge key to a holding id, because scanning calls
it once per card and a linear search there would be quadratic. Everything else scans.

## Development checks that need real artifacts

Two examples under `tools/build-artifacts` run the shipped code against the real data rather than
against fixtures. Neither is a `cargo test`: both need artifacts that are never committed, and one
needs the network. They exist because fixtures proved twice to be the thing that was wrong.

```bash
cargo run --release -p build-artifacts --example verify-scan     # needs .cache/arthashes.jsonl
cargo run --release -p build-artifacts --example verify-bracket  # needs artifacts/cards.rkyv + combos.rkyv
```

```bash
cargo run --release -p build-artifacts --example verify-optimize  # needs artifacts/cards.rkyv
cargo run --release -p build-artifacts --example verify-tags      # needs artifacts/cards.rkyv
```

Each of the three has already earned its place:

* `verify-scan` found the pipeline naming 4 photographs in 50, which led to the black-border
  finding in [`vision.md`](vision.md).
* `verify-bracket` confirms the estimate over all 105,328 combo variants — including that a deck
  reads as *less* certain, not clean, when the combo artifact is missing.
* `verify-tags` reports how much of the catalog carries a functional role, and checks ten cards
  whose role is not a matter of opinion — plus Grizzly Bears, which must come back with *no*
  role, so that an empty tag set keeps meaning "nothing known" rather than "does nothing".
* `verify-optimize` found the search offering a mono-red deck Horizon Canopy, Yavimaya Coast and
  Nomad Outpost: eight of twelve suggestions outside the deck's colours. The colour-identity
  filter existed but only applied in Commander, where it is a rule; nothing constrained the other
  formats. It now derives the identity from the deck, and the same run comes back clean.
