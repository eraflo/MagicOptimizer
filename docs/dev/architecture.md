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
│              ├── mtg-collection                          │
│              ├── mtg-journal                             │
│              ├── mtg-vision                              │
│              └── mtg-ml                                  │
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
| `mtg-journal` | Recorded games, win rate aggregates, Bayesian priors. | `mtg-core`, `mtg-deck` |
| `mtg-vision` | Card detection, homography, pHash, matching. | `mtg-core` |
| `mtg-ml` | Card embeddings, personal re-ranker. | `mtg-core`, `mtg-journal` |

## Storage — why not SQLite

`rusqlite` / `libsqlite3-sys` cross-compiled to Android causes documented, recurring problems:
NDK not found, unresolved symbols at link time, runtime crashes on the emulator. The debugging
cost is out of proportion to the actual need.

The catalog is ~35,000 cards. Rust iteration over precomputed bitsets answers in a few
milliseconds, which is plenty for a UI. So:

- **Read-only** → an `rkyv` artifact, mmap'd, zero-copy deserialization, indexes rebuilt at
  startup (`HashMap<name, CardId>`, inverted index over oracle text, `fixedbitset` per color,
  type and format legality).
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
work matter more than a rich component ecosystem.

The frontend holds **no domain logic**. It renders, and it sends commands.
