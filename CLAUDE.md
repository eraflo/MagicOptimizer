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
   (NDK not found, unresolved symbols at link time, runtime crashes). The catalog is 35,306
   cards: a read-only `rkyv` mmap plus `redb` for mutable state is more than enough — measured
   at ~22 ms to open and ~5 ms for a full scan including rules-text search. **Do not add any
   crate that pulls in C or C++** without first checking it cross-compiles to
   `aarch64-linux-android`.

   The single exception is `tools/build-artifacts`, which needs an HTTP client and therefore a
   TLS stack. It runs on a PC only, is never built for mobile and is never bundled into the app.
   Do not use it as precedent for anything under `crates/` or `src-tauri/`.

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

6. **Persisted data uses `oracle_id`, never `CardId`.**
   `CardId` is a position in one catalog artifact and shifts on every rebuild. A collection or
   deck keyed on it would appear to work, then silently become a different collection after the
   next set release — nothing errors and the damage surfaces much later. `CardId` stays in
   memory; anything written to disk uses Scryfall's stable `oracle_id`.

7. **No account, no telemetry, no user data leaving the device.**
   The one network host the app is allowed to reach is `cards.scryfall.io`, for card images, and
   the content security policy in `tauri.conf.json` enforces exactly that. Widening it needs a
   reason.

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
| `mtg-optimizer` | Scoring, simulated annealing, Monte Carlo simulation, hypergeometric math. Pure Rust, own PRNG — the simulation must be reproducible or the search chases noise. |
| `mtg-combo` | Combo detection, Commander bracket estimation. The combo artifact is **optional** — everything degrades to saying what it could not check. |
| `mtg-journal` | Game log, win rate statistics. Nothing here may report a rate without the uncertainty beside it — three wins of three is not 100%. |
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
- **Do not maintain a Game Changers list.** Scryfall ships a `game_changer` boolean on every
  card; the build produces exactly the 53 flagged cards on its own. Never hardcode the list.
- **The format list drifts, and silently.** Already observed: `explorer` retired, `competitivebrawl`
  and `tlr` added. `build-artifacts` prints a loud warning for any legality key it cannot map,
  because the alternative is a whole format vanishing from search with nothing to explain it. If
  that warning fires: add the variant to `mtg_core::Format`, update `LEGALITY_SLOTS`, bump
  `FORMAT_VERSION`, and fix the pinned list in `format_list_matches_scryfall_as_observed`.
- **The decklist corpus the embeddings plan assumes does not exist.** Checked 2026-08-18:
  MTGJSON's bulk deck files are 3,004 *products* (Secret Lairs, Jumpstart packs, redemption
  lists) of which maybe 700 are real decks — three orders of magnitude short. EDHREC gives real
  co-occurrence but only each card's top neighbours, Commander-only, at ~4.8 GB over 35,000
  requests. Moxfield and MTGTop8 have the data and are not ours to scrape. Do not start phase 8
  on the assumption that a corpus is a download away; see `docs/dev/ml.md`.
- **`edhrec_rank` as a quality signal is gated to Commander-shaped formats, and that gate is
  measured.** Ungated, a real Modern burn list scored 0.18 on card quality — Goblin Guide and
  Lava Spike are barely played in Commander — and the search answered by offering Commander
  staples like Shatterskull Smashing in their place. The signal made the advice actively worse.
  `Format::edhrec_rank_is_meaningful` is the check; do not remove it because the data happens to
  be available.
- **Every criterion in the score is about mana, so the score is maximised by playing more
  lands.** Mana base, land drops and opening hands all improve monotonically with lands; the
  curve criterion is the only thing pushing back, and roles only fires once a group drops below
  its minimum. Measured: removing the curve weight made the search *more* land-hungry, not less
  — it was restraining it. This is why the optimizer still offers to trade a burn spell for a
  land on a deck whose mana is already fine. **Nothing yet values a spell as a spell**, and no
  amount of tuning the existing criteria fixes that; it needs a term the score does not have.
- **`land_drops_criterion`'s horizon comes from the deck's curve**, not a flat turn four. It is
  the earliest turn at which 90% of the deck's spells are castable, clamped to the turns
  actually simulated. The flat version marked a burn deck down for missing a turn-four land it
  has no use for: 0.62 against 0.84 at its real horizon of turn three. Do not put the constant
  back.
- **The optimizer does not know what a card does — except through tags.** `mtg_core::Tag` gives
  it 35 functional roles, and `roles_criterion` is the one criterion that can see effect at all.
  It scores **shortfall only**: below the archetype's conventional minimum costs marks, above it
  is neither rewarded nor punished, because there is no defensible number for "too much
  removal". Shares are taken over cards whose role is *known* — the tagger covers 72% of the
  catalog, and scoring an untagged card as roleless would invent a weakness the search would
  then act on. Under eight identifiable spells the criterion reports weight zero rather than a
  guess.
- **The optimizer still does not know what a card does beyond its role.** It scores a mana base, a curve and an
  opening hand — nothing reads rules text, so it cannot tell Lightning Bolt from a Mountain and
  will happily suggest trading one for the other. Two separate guards exist and they do
  different jobs: **colour identity** keeps candidates castable, derived from the deck itself
  outside Commander, and **EDHREC rank** drops cards nobody plays. An earlier version of this
  note credited the rank gate with the colour job; measured against the real catalog it caught
  none of it — popularity says nothing about colour. Say so in any UI that shows the output.
- **`sources_for_confidence` is not Frank Karsten's number.** His tables condition on hands you
  keep; ours is unconditional and asks for a couple more sources. Scoring uses the probability
  directly rather than a threshold, so this only matters if someone quotes the function.
- **Optimizer suggestions are a diff, never the search's path.** Annealing takes downhill steps
  it later undoes; reporting that path and letting someone apply a subset produced six copies
  of a four-of. Any change to how suggestions are built has to preserve "applicable in any
  order, any subset".
- **The optimizer's bracket constraint enforces one criterion of three.** `max_bracket` limits
  the **Game Changer count**, which Scryfall flags per card and is therefore exact. It does not
  check two-card combos or mass land denial: both live in `mtg-combo`, the combo artifact is an
  optional download, and building its index is 17 ms inside a loop that runs thousands of times.
  A constrained deck can still sit above its target for a reason the search cannot see, so the
  UI says to check the finished deck against the bracket panel. The allowances are duplicated
  from `mtg_combo::assess` and a test pins them to it — if the two ever drift, the optimizer
  builds decks the panel then calls out.
- **The bracket estimate can only reach 2 to 4.** Brackets 1 and 5 are about how a deck is
  played, not what is in it — two identical lists can sit in 2 and 1. Never present the number
  as covering the full scale, and never drop the caveats from a UI that shows it.
- **Fetch combos from the bulk dump, never by paginating.** `json.commanderspellbook.com/
  variants.json.gz` is the whole database in one request — 105,328 variants in five seconds,
  measured. Paginating `/variants/?limit=100` does not work: even at 600 ms between requests
  with exponential backoff it collapsed into a wall of `429`s and a final `503` around offset
  30,800, losing the entire run. It is still an unofficial endpoint with no contract, so
  unexpected variant statuses are reported loudly, like the legality-key warning.
- **That dump is named `.gz` *and* served with `Content-Encoding: gzip`.** Whether the bytes
  arriving are still compressed depends on the HTTP client — `ureq` unwraps it by default.
  `spellbook::gunzip_if_needed` sniffs the magic number rather than assuming either way.
- **Do not interpret Spellbook's `bracketTag`.** The values are single letters whose meaning is
  undocumented. It is stored verbatim; the bracket comes from Wizards' published criteria.
- **Do not add a search index without a measurement first.** The linear scan is ~5 ms over the
  whole catalog. An inverted index would be cost with no benefit today; see
  `docs/dev/architecture.md` for the numbers to beat.
- **Tauri does not give you a TLS backend, and nothing warns you.** `reqwest` is in the tree
  because Tauri depends on it, but Tauri enables only `json`. The downloader shipped with
  `default-features = false` and no TLS feature, on the assumption that feature unification would
  supply one; it does not, so the binary could not open an `https` URL at all and every download
  failed with "could not reach" — on the phone and on the desktop alike, with every unit test
  green. `src-tauri` now names the stack explicitly: `rustls-no-provider` with **`ring`**, because
  reqwest's own `rustls` feature pulls `aws-lc-rs`, which is C, wants cmake, and the resolver
  refuses it for `aarch64-linux-android` outright. Roots are **bundled** (`webpki-roots`) rather
  than the platform's, because reqwest 0.13 verifies through `rustls-platform-verifier` and on
  Android that must be handed a JVM over JNI first. `ring` is C, and is the deliberate second
  exception to invariant 1 — checked against `aarch64-linux-android` before being added, and it
  needs the NDK's clang, which `cargo tauri android` puts on the path. Run
  `cargo test -p magicoptimizer -- --ignored` to prove the artifacts really arrive; that test
  exists because no offline test can catch a missing TLS backend.
- **Data artifacts live in the `data` release, never `nightly`.** The nightly workflow deletes
  and recreates its release on every push to `main` — that already destroyed an APK someone had
  attached by hand. Anything meant to outlive a push goes in `data`, which CI does not touch.
- **The Android build has host requirements that cost an hour each to rediscover**, and none of
  them is about this project's code: Tauri needs the SDK **Command-line Tools** as well as the
  NDK; Android refuses a version of `0.0.0`; Windows refuses the symlink Tauri makes into
  `jniLibs` without Developer Mode; and Android Studio's bundled JBR is Java 25 while the
  generated Android Gradle Plugin 8.11 stops at 21. All four are in `docs/dev/android.md` with
  their exact error text.
- **The `CAMERA` permission is hand-added to the generated manifest**, and re-running
  `cargo tauri android init` regenerates that file and drops it. If scanning stops being able to
  open the camera, look there before anywhere else.
- **`getUserMedia` in the Android WebView** can misbehave depending on the version, and nothing
  has yet run on a device — the Rust library builds for `aarch64-linux-android`, but no APK has
  been produced. The planned fallback
  is a Kotlin CameraX plugin; that boundary does **not** exist yet, the frame source is `grab()`
  in `ScanView.svelte`. Do not describe it as prepared.
- **Tauri's raw IPC is not available on the device, and the frame path must not assume it is.**
  `scan_frame` rejected anything that was not `InvokeBody::Raw`, and on Android the IPC falls back
  to `postMessage`, which carries JSON only. So **every frame was refused**: the scanner never saw
  a single pixel, and the camera "stopping" was the 15-failure guard doing its job. No amount of
  work on detection, hashing or the artwork archive could have made recognition function. The
  frontend now tries raw once, remembers the answer, and falls back to base64 — a third more
  bytes for a path that works everywhere. `docs/dev/frame-transport.md` has the real fix, which is
  to stop sending frames at all and send the 32-byte hash.
- **Camera frames go over raw IPC where it exists, never as a JSON array of numbers.** A 640×480 greyscale frame is
  300 KB; as a `Vec<u8>` argument Tauri would serialize three hundred thousand JSON numbers ten
  times a second. `scan_frame` takes a `tauri::ipc::Request` and reads dimensions from headers.
- **The greyscale weights in `ScanView.svelte` must match `mtg_vision::rgba_to_gray`.** Both use
  77/150/29. The reference hashes in `arthashes.bin` were computed with those weights, so a
  different luma formula on the query side would shift every hash away from the whole database —
  silently, and in a way no test on either side alone would catch.
- **Scanning needs a mid-tone background, and this is measured, not guessed.** Magic cards have
  a black border: on a near-black table there is nothing to separate, detection snaps to the
  card's bright interior instead, and the shifted crop ruins the hash. 2 of 20 photographs
  recognised on black against 20 of 20 on mid-grey. No threshold fixes it — the border and the
  table really are the same shade. Two tests in `detect.rs` pin the pair down; do not "fix" the
  contrast threshold to chase it. Re-measure with
  `cargo run --release -p build-artifacts --example verify-scan`.
- **`arthashes.bin` hashes the `normal` card image, not Scryfall's `art_crop`.** Reference and
  query must be framed identically, and the query is cropped from a rectified card at fixed
  fractions. Changing `hash_gray`, the `ART_*` constants or `RECTIFIED_*` invalidates every
  published hash: bump `ARCHIVE_VERSION` so old files are refused rather than silently matching
  nothing.
- **A test that asserts fresh-install behaviour must use `AppState::without_artifacts`.**
  `AppState::new` falls back to `artifacts/` in the checkout so `tauri dev` works after a build,
  which means a test using it passes or fails depending on what the developer has generated.
  That already happened once, to the combo test.
- **EDHREC and Commander Spellbook are unofficial endpoints.** They can break without notice. Only
  `build-artifacts` talks to them, never the app, and missing data must degrade gracefully to
  heuristics alone.
- **Android performance budget.** The vision pipeline must hold 5–10 fps on a mid-range phone.
  Measure before optimizing, but do not let a regression through.
- **The interface has a written direction now, and it was expensive to arrive at.** Three
  proposals were rejected before one landed; the reasons are in `docs/dev/design.md` and they are
  not matters of taste. The one that matters most: **the artwork is the interface.** Magic has
  thirty years of illustration and the app displayed none of it, which is why it read as ugly —
  not the palette. Full-frame views use `image_art` (Scryfall's `art_crop`), **never**
  `image_normal`: a whole card carries its own border, title and text box, and anything laid over
  one collides with them. A test in `dto.rs` pins the two apart. Nothing in the interface goes
  below 13px, and the ground is a *warm* near-black — the old `#0d0f16` is a developer-tool blue
  that cooled every illustration on it.
- **The UI is already responsive; keep it that way.** Three CSS breakpoints — 1180px collapses
  the filter panel into a drawer, 860px turns the card detail into a full-screen sheet and
  stacks the collection table. Any new view needs the same treatment, and touch sizing keys on
  `pointer: coarse` rather than on width. See `docs/dev/android.md`.

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
- Visual direction, tokens, per-screen treatment → [`docs/dev/design.md`](docs/dev/design.md)
