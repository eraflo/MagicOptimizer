# MagicOptimizer

Deck building and optimization for **Magic: The Gathering**, with physical collection management
through **camera recognition**.

A desktop and mobile app (Windows + Android), written in **Rust** with **Tauri 2**, that runs
**fully offline** and **without any server**. Your data never leaves your device.

> **⚠️ Work in progress.** Nothing is usable yet. See [Status](#status) for actual progress.

---

## What it does

- **Optimize a deck** in any format (Standard, Pioneer, Modern, Legacy, Vintage, Pauper,
  Commander, Brawl, Limited) — improve an existing list, build one from scratch, or simply
  measure how coherent it is.
- **Only suggest cards you own**, if you want it to: a single toggle decides whether the
  optimizer may reach outside your collection.
- **Scan your physical cards** by passing them in front of your phone's camera. Recognition
  works on the artwork, so **cards in any language are recognized with no configuration**.
- **Know where your cards are**: a storage location (binder, page) is attached to each copy
  while you scan.
- **Estimate a deck's Commander bracket** and detect its infinite combos — and even optimize a
  deck *while staying* within a given bracket.
- **Keep a game log** after the fact, to track your win rate per deck and per matchup.
- **Learn from your choices**: a small personal model, trained continuously on your device,
  tunes recommendations to your style.

### What it deliberately does not do

- **Nothing during a game.** No life counter, no rules lookup, no live pick assistant. The app is
  for preparing beforehand and recording afterwards, never for playing. Pulling out your phone at
  the table is frowned upon, and at competitive REL electronic devices are banned outright.
- **No MTG Arena collection import.** Wizards provides no API and no authentication, and it is
  impossible from Android regardless. The split between physical and digital collections still
  exists in the data model in case that ever changes.
- **No account, no telemetry, no data ever sent anywhere.**

---

## Status

The project ships in phases. It stays usable at the end of each one.

| Phase | Scope | State |
|:---:|---|:---:|
| 0 | Public repository, documentation, CI | ✅ done |
| 1 | Data foundation, card catalog, search | ✅ done |
| 2 | Desktop app, collection, storage locations | ✅ done |
| 3 | Decks, format rules, legality, import/export | ✅ done |
| 4 | Optimizer, simulation, collection constraint | ✅ done |
| 5 | Combo detection, Commander brackets | ⬜ |
| 6 | Android and video scanning | ⬜ |
| 7 | Game log | ⬜ |
| 8 | Continuously trained personal model | ⬜ |
| 9 | Sync, pricing, translations | ⬜ |

---

## Installation

No stable release yet. Every push to `main` publishes installers for Windows, macOS and Linux
to the **[nightly prerelease](https://github.com/eraflo/MagicOptimizer/releases/tag/nightly)**.

Those builds are unsigned, so Windows SmartScreen and macOS Gatekeeper will both object, and
they ship with no card data — see the release notes. To build from source instead, see
[CONTRIBUTING.md](CONTRIBUTING.md).

What works today, from a checkout. First build the card data — the app ships with none, and this
downloads Scryfall's oracle bulk file and writes `artifacts/cards.rkyv` (35,306 cards, 25.9 MB,
about 6 seconds):

```bash
cargo run --release -p build-artifacts
```

Then run the desktop app — browse every card, filter it, and record what you own:

```bash
npm install && npm run tauri dev
```

The catalog is also queryable straight from the command line:

```bash
cargo run --release -p mtg-data --example search -- --identity WU --format commander --type Legendary --type Creature
```

Import a decklist and check it — paste from Arena, Moxfield, MTGO or plain text:

```bash
cargo run --release -p mtg-deck --example deck -- --file mydeck.txt --format commander
```

Score a deck and look for improvements:

```bash
cargo run --release -p mtg-optimizer --example optimize -- --file mydeck.txt --format modern --archetype aggro
```

---

## Documentation

**Using the app** — [`docs/user/`](docs/user/)

| Page | Topic |
|---|---|
| [Getting started](docs/user/getting-started.md) | Install, first launch, downloading card data |
| [Scanning cards](docs/user/scanning.md) | The four scan modes and how to get good results |
| [Collection](docs/user/collection.md) | Storage locations, conditions, foils, duplicates |
| [Building decks](docs/user/deckbuilding.md) | Formats, the optimizer, reading the score |
| [Brackets and combos](docs/user/brackets-combos.md) | Commander brackets, Game Changers, combos |
| [Game log](docs/user/journal.md) | Recording games and reading the stats |
| [FAQ](docs/user/faq.md) | Offline use, Arena, privacy |

**Working on the code** — [`docs/dev/`](docs/dev/)

| Page | Topic |
|---|---|
| [Architecture](docs/dev/architecture.md) | Overview, what each crate is responsible for |
| [Data pipeline](docs/dev/data-pipeline.md) | Sources, artifacts, update procedure |
| [ML subsystem](docs/dev/ml.md) | Embeddings, personal ranker, retraining |
| [Vision](docs/dev/vision.md) | Card recognition, thresholds, golden test set |
| [Android](docs/dev/android.md) | Mobile build, NDK, permissions, pitfalls |

[`CLAUDE.md`](CLAUDE.md) holds the instructions for AI agents working on this repository,
including the **invariants that must never be broken**.

---

## Data sources

The app ships with no card data. On first launch it downloads artifacts built from public
sources:

- **[Scryfall](https://scryfall.com)** — card catalog, legalities, prices, artwork.
- **[EDHREC](https://edhrec.com)** — Commander inclusion and synergy statistics.
- **[Commander Spellbook](https://commanderspellbook.com)** — combo database.

None of these projects are affiliated with MagicOptimizer or involved in its development. No
artwork is redistributed: only perceptual fingerprints are published, with images fetched on
demand from Scryfall's CDN.

---

## Legal

**The [MIT license](LICENSE) covers the source code in this repository only.** It does not extend
to card data, names, text or artwork.

This project is free and non-commercial, in line with the
[Wizards of the Coast Fan Content Policy](https://company.wizards.com/en/legal/fancontentpolicy).

> MagicOptimizer is unofficial Fan Content permitted under the Fan Content Policy. Not
> approved/endorsed by Wizards. Portions of the materials used are property of Wizards of the
> Coast. ©Wizards of the Coast LLC.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
