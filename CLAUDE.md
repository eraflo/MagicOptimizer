# CLAUDE.md — instructions for AI agents

This file is for Claude Code and any other agent working on this repository.
Read it in full before changing anything.

---

## The project in three lines

A Tauri 2 + Rust app for building and optimizing Magic: The Gathering decks,
with physical collection management through camera recognition.
Targets Windows and Android, runs **fully offline with no server**.

---

## Non-negotiable invariants

These are architectural decisions, not preferences. **Never break one without explicitly asking
the user.** Each cost real time to establish.

1. **No native dependencies, never SQLite.**
   `rusqlite` / `libsqlite3-sys` cross-compiled to Android is a documented, recurring pain point
   (NDK not found, unresolved symbols at link time, runtime crashes). The catalog is 35,000
   entries: a read-only `rkyv` mmap plus `redb` for mutable state is more than enough, and both
   compile everywhere without a C toolchain. **Do not add any crate that pulls in C or C++**
   without first checking it cross-compiles to `aarch64-linux-android`.

2. **The app is never used during a game.**
   A product constraint set by the user: pulling out your phone at the table is socially frowned
   upon, and at competitive REL electronic devices are banned. No life counter, no rules lookup at
   the table, no live pick assistant. Everything happens **before** (preparation, deck building)
   or **after** (data entry, game log). If a feature idea requires consulting the app mid-game, it
   is out of scope.

3. **Offline first.**
   Once the artifacts are downloaded, absolutely everything must work in airplane mode. The
   network does exactly three things: download artifacts, fetch card images for the cache, refresh
   prices. None of the three may sit on a critical path.

4. **No data artifact is ever committed.**
   Not Scryfall bulk, not artwork, not `*.rkyv`, not `arthashes.bin`, not `embeddings.bin`. They
   are produced by `tools/build-artifacts` and published as GitHub Releases. This is a repository
   size concern, but also an intellectual property one (see Legal below).

5. **The card model is multi-face from day one.**
   Double-faced, split, adventure, aftermath. A single-face model would have to be redone later,
   and that refactor would touch every part of the codebase.

6. **No account, no telemetry, no user data leaving the device.**

---

## Crate map

The domain logic lives in crates decoupled from Tauri: they test with `cargo test` without ever
running a mobile build. **Preserve that separation** — do not put domain logic in `src-tauri/`,
which should hold only thin commands delegating to the crates.

| Crate | Responsibility |
|---|---|
| `mtg-core` | Shared types: `CardId`, `Color`, `ManaCost`, `Format`. Depends on no other crate. |
| `mtg-data` | Card catalog: `rkyv` mmap loading, in-memory indexes, search and filtering. |
| `mtg-collection` | Physical and digital collections, storage locations, draft pools. `redb`. |
| `mtg-deck` | Deck model, format rules, legality checking, import/export. |
| `mtg-optimizer` | Scoring, simulated annealing, Monte Carlo simulation, hypergeometric math. |
| `mtg-combo` | Combo detection, Commander bracket estimation. |
| `mtg-journal` | Game log, win rate statistics. |
| `mtg-vision` | Card detection, homography, pHash, matching. |
| `mtg-ml` | Card embeddings and the personal re-ranker. |
| `tools/build-artifacts` | Dev CLI: public sources → binary artifacts. **Never runs inside the app.** |

---

## Commands

```bash
cargo test --workspace          # tests
cargo fmt --all                 # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo tauri dev                 # desktop app
cargo tauri android dev         # Android app (device or emulator)
cargo run -p build-artifacts    # generate data artifacts
```

CI runs `fmt --check`, `clippy -D warnings` and `test --workspace`. A Clippy warning fails CI:
fix it, do not silence it with `#[allow]` unless you write the justification in a comment.

---

## Conventions

- **All repository content is in English** — documentation, comments, identifiers, commit
  messages, and user-facing strings.
- **Errors**: `thiserror` in crates, `anyhow` only in binaries (`src-tauri`, `tools`).
- **No `unwrap()` or `expect()` in domain crates.** Fine in tests, and in binary startup code
  where failing really should stop the program.
- **Never `panic!` on external data.** The catalog and artifacts come from the network: treat them
  as untrusted, validate, and return an error.
- **Conventional commits**: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- **One branch per phase**, `main` protected.

---

## Known pitfalls

- **Scryfall bulk data is JSONL-only** since 20 July 2026. Do not write a parser for the old JSON
  format: it no longer exists.
- **Scryfall blocks generic User-Agents.** Observed directly: their own documentation returns 403
  for a default UA. Always send a descriptive User-Agent and an `Accept` header. Stay under 10
  requests/second, and under 2/s on `/cards/collection`.
- **The Game Changers list changes over time** (53 cards as of 2026-02-09). It must come from the
  artifact, never be hardcoded.
- **`getUserMedia` in the Android WebView** can misbehave depending on the version. The planned
  fallback is a Kotlin CameraX plugin behind the `FrameSource` trait — do not invent a different
  approach without discussing it.
- **EDHREC and Commander Spellbook are unofficial endpoints.** They can break without notice. Only
  `build-artifacts` talks to them, never the app, and missing data must degrade gracefully to
  heuristics alone.
- **Android performance budget.** The vision pipeline must hold 5–10 fps on a mid-range phone.
  Measure before optimizing, but do not let a regression through.

---

## Legal

Free, non-commercial project under the
[Wizards Fan Content Policy](https://company.wizards.com/en/legal/fancontentpolicy).
Concrete consequences for the code:

- **Add no paid features** and no monetization links.
- **Never commit or redistribute card artwork.** The design publishes only perceptual
  fingerprints; images are fetched on demand from Scryfall's CDN. This is a defensive property
  worth preserving.
- The Fan Content notice must stay in the README and in the app's About screen.
- Attribution to Scryfall, EDHREC and Commander Spellbook must stay visible.

---

## Where to find what

- Overall architecture and data flow → [`docs/dev/architecture.md`](docs/dev/architecture.md)
- Sources, artifact formats, update procedure → [`docs/dev/data-pipeline.md`](docs/dev/data-pipeline.md)
- Embeddings, ranker features, retraining → [`docs/dev/ml.md`](docs/dev/ml.md)
- Recognition pipeline and golden test set → [`docs/dev/vision.md`](docs/dev/vision.md)
- Android build, NDK, permissions → [`docs/dev/android.md`](docs/dev/android.md)
