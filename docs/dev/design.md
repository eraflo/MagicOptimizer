# Design: cinema

> **Status** — the direction was chosen on 2026-08-19 after three rejected proposals. The tokens
> below are in `src/app.css`. Built so far: the scan result, the card list and the card detail.
> The rest still looks like the old interface, and that is expected — screens move one at a time.
>
> The first attempt moved only the scan result, which on a desktop is the one screen nobody can
> reach without a camera and a successful match. The report back was "nothing changed", and it
> was accurate. **Move a screen the user actually looks at, or the work is invisible.**

## How this was arrived at, so it is not undone by accident

Three earlier directions were rejected, and the reasons are worth keeping because each was a real
mistake rather than a matter of taste.

1. **Three pastiches** — a card imitation, a copy of Linear, a generic touch list. None came from
   what the app does. Rejected wholesale.
2. **An austere measurement system** — achromatic, hairlines, 12px type. Rejected as *ugly and
   unpleasant to read*, which was accurate: it was a tax form.
3. The diagnosis came from Spotify's design system, which states that album art is the primary
   source of colour and the UI is achromatic **by design**. Proposal 2 applied exactly that rule
   **without any images**. An achromatic interface whose content is also grey is a form.

Magic has thirty years of illustration and the app displayed **none of it**. That, not the
palette, is why it looked bad.

## The one idea

**One subject, given the whole frame, over its own artwork.**

Focus rather than density. Where a screen has a single subject — a scanned card, a card opened
from anywhere, a deck — that subject fills the frame and everything else floats on top of it.

This does not suit every screen, and pretending otherwise is how a direction dies. Cinema is a
**destination** treatment. Lists and catalogues keep their own structure; they inherit the
tokens, not the layout.

## Laws

1. **The artwork is the background, never a thumbnail.** Use `image_art` (Scryfall's `art_crop`),
   never `image_normal` — a whole card carries its own border, title and text box, and anything
   laid over it collides with them. Images come from `cards.scryfall.io`, which the content
   security policy already allows, and nothing is stored or redistributed.
2. **A scrim, always, and from the bottom.** Text over artwork is unreadable without one. The
   gradient runs to opaque at the bottom edge so the type sits on solid ground rather than on a
   guess about the image.
3. **Type is large and confident.** A subject's name is 34–40px. Nothing in the interface is
   below 13px, and list rows are 15px. Proposal 2 ran at 12px and that is precisely what
   "unpleasant to read" meant.
4. **The ground is a warm near-black.** `#17151a`, not the old `#0d0f16`. A blue-black is a
   developer-tool ground and it cools every illustration laid on it; a warm one lets them radiate.
5. **Actions are pills, and the honest one is as visible as the confident one.** Where the app
   proposes something it inferred — a recognised card, a suggested swap — the way to reject it
   sits beside the way to accept it, at the same size. An algorithm offering a name must not
   style disagreement as the quiet option.
6. **Colour belongs to mana.** The five colours always mean mana and never decorate. The gold
   `#d8a951` marks one thing only: **what you own** — the question the app exists to answer in a
   shop. And the interface's own emphasis is **light, not a hue**: a primary action is a pale pill
   with dark text, an active tab likewise. This is the law that was written and then not applied —
   the first pass left `--accent` a developer-tool blue on every button, tab and selected row, and
   the result read as the old app with photographs glued on.

## Tokens

Defined in `src/app.css`. Do not write a literal colour or size in a component when one of these
fits; that drift is what makes a direction dissolve.

| Token | Value | Role |
|---|---|---|
| `--ground` | `#17151a` | The app's ground. Warm near-black. |
| `--ground-2` | `#201d24` | Panels, sheets, raised surfaces. |
| `--ground-3` | `#2a262f` | Controls, chips, hover. |
| `--line` | `#34303a` | Hairline borders. |
| `--ink` | `#f0ece6` | Primary text. Warm off-white, not pure. |
| `--ink-2` | `#b8b1ac` | Secondary text, still fully readable. |
| `--ink-3` | `#837c79` | Metadata and disabled. Never body text. |
| `--gold` | `#d8a951` | Owned quantity. Nothing else. |
| `--accent` | `#f4f0ea` | Emphasis, and it is light rather than coloured. Primary fills take `--ground` as their text. |
| `--scrim` | gradient | Bottom-anchored, ends opaque at `--ground`. |

Type scale — `13 / 15 / 17 / 21 / 26 / 34 / 40`, and nothing between. Body text is 15px with a
line height of 1.55; a card name in a full-frame view is 34px or 40px with `-0.03em` tracking.

Spacing is a multiple of 4. Radii: `6px` on small controls, `10px` on panels, `999px` on pills.

## Per-screen treatment

| Screen | Treatment | Why |
|---|---|---|
| **Scan result** | Cinema, full-bleed | One card, just photographed. The screen the phone exists for. Built. |
| **Card detail** | A cinema hero over the art crop, rules below. Built. | Same subject, different room. |
| **Catalogue** | Rows now carry the art crop; the contact-sheet grid is designed, not built | Density *through* images. A player recognises artwork before a name. |
| **Deck editor** | Columns by mana value, cards at 63:88 | The curve is the arrangement, not a chart beside it. |
| **Collection** | Binder pages, 3×3 | `mtg-collection` already models storage locations and nothing shows them. |

The deck editor and the collection are designed but **not built**, and the catalogue has the
tokens without the contact-sheet layout. Do not describe any of the three as done.

One rule the placeholders enforce: a list row must look deliberate **with no network at all**.
Artwork comes from Scryfall's CDN, so an offline install gets the card's own colours as a swatch
rather than an empty grey rectangle — which would be worse than the text list it replaced.

## Guardrails

- **No second accent.** If something needs to stand out and gold is taken, use weight, size or
  position. Adding a hue is how an interface stops meaning anything.
- **No number without its margin** where the calculation knows one. `71% ±9`, never `71.3%`.
  This is already the rule in `mtg-journal`; the interface must stop contradicting it.
- **Never show a bare percentage for a match confidence without the way to refuse it.**
- **No text below 13px.** None. Including captions.
- **Do not put `image_normal` in a background.** See law 1.
