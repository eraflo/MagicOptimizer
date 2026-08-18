# Getting started

> **🚧 No release published yet**, but the desktop app runs from a checkout. Installers and the
> in-app data download arrive later; this page grows as they do.

## Running it today

You need [Rust](https://rustup.rs), [Node.js](https://nodejs.org) and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
```

The app ships with no card data at all, so build the catalog first. This downloads Scryfall's
oracle bulk file — about 25 MB — and converts it:

```bash
cargo run --release -p build-artifacts
```

Then start the app:

```bash
npm run tauri dev
```

If you skip the first step the app opens on a screen telling you so, rather than looking broken.

## What you can do so far

- **Browse** every card, filtering by name and rules text, colour identity, type, format
  legality and mana value.
- **Add cards to your collection**, recording the printing, finish, condition, language and
  where the card is stored.
- **Keep physical and digital collections apart**, and see how many copies you own of anything
  while browsing.
- **Build and import decks**, check their legality in any format, and see the mana curve.
- **Optimise a deck**, restricted to the cards you own or drawing on everything.
- **Scan physical cards with a camera** into your collection, a deck or a draft pool — see
  [Scanning your cards](scanning.md).
- **Spot combos and estimate a Commander bracket** for a deck.

The game journal and price tracking are still to come — see
[the project status](../../README.md#status).

## Still to come on this page

- **Installing on Windows** — download and launch.
- **Installing on Android** — installing the APK, which permissions are requested and why.
- **First launch** — downloading the card data. The app ships with no data at all: it fetches a
  catalog of around thirty megabytes, once. The data needed for scanning and recommendations is
  larger and stays **optional**: you only download it if you use those features.
- **Checking that offline works** — after that first download, the app should run in airplane mode.
- **Updating the data** — on every new set release.

## In the meantime

- [FAQ](faq.md) — what the project does, what it does not, and why.
- [Project status](../../README.md#status) — actual progress, phase by phase.
