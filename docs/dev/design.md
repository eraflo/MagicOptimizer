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
4. **The chassis is achromatic and almost black; all the colour comes from the light.** Ground
   `#08080a`, panels `#141418`, text pure white. Every hue in the app is the ambient glow, the
   card art or one of the five mana colours — nothing else is allowed to be coloured.

   This was arrived at by elimination. An earlier pass warmed *every* surface to a sepia brown so
   that the neutrals shared the ember's hue; the result put colour everywhere and contrast
   nowhere, and since artwork is warm too it sank into the ground instead of coming off it. Four
   candidates were rendered on the same screen and this one chosen. Against `#08080a` an
   off-white reads as dirty, so the ink is pure `#ffffff` — the opposite of what the warm pass
   required, and the reason that pass has to be replaced wholesale rather than nudged.
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

## The ground is lit, and it is lit by the content

The single device that separates this direction from "a dark theme", and the one that took
longest to arrive at. `#app` carries three radial gradients whose colour is written to the root
by `App.svelte` from the **colour identity of the card in view**: a Boros card bathes the app in
amber, a Dimir one in indigo, and with nothing selected it holds a warm ember.

Two things about it are load-bearing:

* **It must be strong enough to be the room**, not a tint someone has to look for. The first
  attempt ran at `0.26` alpha and was invisible beside the mockup it came from. The tints are
  also *saturated* — on `#08080a` a muted one simply reads as grey, where on the earlier warm
  ground the surface was already doing half the work.
* **Panels float on it.** `main` has padding and a gap, and each panel is an 18px-radius card
  with a hairline all round — not a full-height column separated from its neighbour by a 1px
  divider. That grid of hairlines is what read as "database tool" through three palettes.

## The control layer is the leverage, not the tokens

Two rounds were spent swapping colour tokens while every control kept its old shape, and the app
still read as the old interface. It was a fair complaint: a token changes hues, the shared
element layer in `src/app.css` changes *shapes*, and that layer is inherited by every view.

What it now defines, and what each replaced:

| | Was | Is |
|---|---|---|
| Buttons | 6px-radius grey rectangles, 13px | Pills, 38px tall, 15px, translucent fill |
| Inputs | Light 1px box on the ground | A dark well with an almost-invisible edge, 40px, 10px radius |
| Labels | 12px grey prose | 13px uppercase, spaced, so a panel reads as captions |
| Panels | Flat `--panel` | **Glass** — translucent, blurred, hairline |
| Filter chips | 12px outlines | Pills, pale fill when active |
| Colour toggles | Hollow rings | Solid mana discs, desaturated until chosen |

**Glass is the one decorative device the direction allows**, and it earns its place: a panel laid
over artwork has to let some through, or the app is a grid of grey boxes again.

### What "premium" actually consisted of

Reported three times as missing, and it was never one thing. Held against the mockup it came down
to depth, air and light, all three of which are tokens now:

* **`--lift`** — a **ladder** of four low-opacity layers of increasing blur, never one shadow.
  Things that float read as premium; flat cards with a hairline read as diagrams. Two rules were
  learned the hard way:
  * **A single shadow bands.** `0 14px 34px rgba(0,0,0,.5)` falls off steeply enough that an
    8-bit display quantises it into visible rings, and worst over a near-black ground where each
    step is a large fraction of what is left. Four layers each band too, but at different radii,
    so the steps interleave and vanish.
  * **No layer is tighter than 2px.** Every surface here is translucent glass, so a dense shadow
    hugging the edge shows *through* the panel and pools just inside each corner.
* **`--sheen`** — a full `inset 0 0 0 1px` ring, which is how glass catches light. It began as
  `inset 0 1px 0` along the top edge, and that was wrong on every rounded card: a one-pixel
  horizontal line is drawn flat and stops dead where the radius turns, leaving a visible nick in
  both upper corners. A ring follows the curve the whole way round.
* **Air.** Panel padding 26px, gaps 28px, list rows 10/16. The old panel was correct and cramped.
* **Scrollbars absent at rest, and inset at both ends.** A permanent bar down the side of a
  floating card is furniture. The thumb fades in on hover of the thing that scrolls and is
  removed entirely on touch, where the platform draws its own. Two insets matter and only one is
  obvious: the thumb sits inside a 4px transparent border so it floats clear of the side, and
  `::-webkit-scrollbar-track` takes `margin-block: 18px` so the **lane** stops short of both
  ends. Without the second, the bar runs into the card's 18px corner and reads as stuck to the
  edge however thin it is.
* **No full-bleed banners.** The error was a red strip painted across the window under the app
  bar — the single least premium thing an interface can do. It is a floating card like everything
  else.

## Tokens

Defined in `src/app.css`. Do not write a literal colour or size in a component when one of these
fits; that drift is what makes a direction dissolve.

| Token | Value | Role |
|---|---|---|
| `--ground` | `#08080a` | The app's ground. Near-black, achromatic. |
| `--ground-2` | `#141418` | Panels, sheets, raised surfaces. |
| `--ground-3` | `#1e1e24` | Controls, chips, hover. |
| `--line` | `#2a2a31` | Hairline borders. |
| `--ink` | `#ffffff` | Primary text. Pure — an off-white on this ground reads as dirty. |
| `--ink-2` | `#b9b9c0` | Secondary text, still fully readable. |
| `--ink-3` | `#75757e` | Metadata and disabled. Never body text. |
| `--lift` / `--lift-lg` | shadows | Depth. Things that float read as premium. |
| `--sheen` | `inset 0 1px 0` | How glass catches light. One line per surface. |
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
| **Catalogue** | Contact sheet by default, list on a switch. Built. | Density *through* images. A player recognises artwork before a name. |
| **Deck editor** | Board by default — columns by mana value, cards at 63:88 — with the list on a switch. Built. | The curve is the arrangement, not a chart beside it. |
| **Collection** | Binder pages 3×3 on a switch, table by default. Built. | `mtg-collection` already models storage locations and nothing showed them. |
| **Journal** | Two floating cards, form beside results. Built. | Recording is the mobile task; the numbers are the desktop one. |

Every view designed here is now built: the contact sheet for the catalogue, the board for the
deck editor, binder pages for the collection.

### The binder

Nine pockets to a page, grouped by container, cards at 63:88. It exists because
`mtg-collection` has modelled storage locations since it was written and no screen ever showed
one — the question in front of a shelf is *which box*, not *do I own it*.

* **Empty pockets are drawn.** A collection is read as much by its gaps as by its cards, and a
  hole in a page says that faster than a row reading "0 copies".
* **Pockets sort by slot** where a holding gives one, so the screen matches the physical page.
* **Names appear on hover only.** Nine captions at once would bury the artwork the page exists
  to show.
* **The sleeve sheen is one gradient**, and it is the only decorative flourish in the view. It
  does most of the work of making these read as objects.

The table stays the default and holds the editing; a pocket opens it. Both the deck board and the
binder join to the catalog through a command of their own (`deck_board`, `collection_binder`)
rather than fattening the stored types — what a card looks like belongs to the catalog and would
go stale inside a saved deck or collection.

### The deck board

Columns by mana value, cards at their real 63:88 proportion, which is what anyone does physically
when building — spread them out, step back, look at the shape. Four decisions carry it:

* **The curve is the arrangement.** A column running too tall is visible without reading a
  number, so the histogram is hidden in board mode: drawing it beside the board would make the
  same claim twice in two shapes.
* **Lands get their own column.** Their mana value says nothing about when they are played, and
  leaving them at zero would put a third of the deck in one bar and flatten everything else.
* **0–1 and 6+ share their columns.** Few decks have enough of either end to fill one alone.
* **A card the catalog does not know goes to "Unplaced"** rather than being dropped. Silently
  losing it would make the board disagree with the list beside it.

The join happens in `deck_board`, which scans the catalog once (~5 ms) keeping only the deck's
oracle ids. Not by name — Scryfall has several cards sharing one — and not by `CardId`, which
shifts on every rebuild, which is why decks persist `oracle_id` at all.

**The catalogue offers both layouts and neither is a compromise.** The sheet shows four times the
artwork and is faster to scan, because a card is recognised by its painting before its name; the
list wins when the type line, the cost and the owned count have to be read together. The choice
persists in `localStorage`, because a density preference is a standing one.

**The card panel follows the selection; it has no control of its own.** It appears when a card is
opened and goes away when the same card is clicked again or the panel is closed — which is the
only thing that ever decided whether it had anything to say. It used to sit there permanently
holding "Select a card to see its details", costing 372px to say nothing.

**The filter panel does have a toggle**, because nothing else can tell whether you are still
filtering. Measured on a 1250px window: the results are 914px with the filters open and **1220px
with them away**, and the sheet goes from seven columns to ten. With a card open as well it is
528px and four. The state persists; the toggle exists only above 1180px, where the panel is a
permanent column rather than a drawer.

Its button is a 30px icon with no label — the standard sidebar mark, a panel with one edge
filled, and the fill is what distinguishes showing from hidden. A labelled pill beside the layout
switch made the bar read as three competing controls when one of them is a utility.

Cell width is measured, not picked: the results column sits between a 292px filter panel and a
372px detail panel, so at a 1250px window it is about 528px wide. `minmax(118px, 1fr)` gave three
columns there — a list with bigger pictures. At 104px it gives four, and ten on a full-width
desktop.

**Nothing renders below 13px.** Eighty-one declarations under the floor were swept out of ten
components in one pass — that alone was most of what still read as unfinished, and it is the law
easiest to break by writing `font-size: 11px` for a caption. The one exception is `ManaCost`,
whose numeral sits inside a 17px disc and cannot grow with the scale.

One rule the placeholders enforce: a list row must look deliberate **with no network at all**.
Artwork comes from Scryfall's CDN, so an offline install gets the card's own colours as a swatch
rather than an empty grey rectangle — which would be worse than the text list it replaced.

## One scroller per view, not one per card

A view that stacks cards vertically scrolls **as a column**; the cards do not each scroll inside
themselves. The Collection tried the other arrangement and its backup card fell off the bottom of
the window — measured at 553px of children in a 467px box, with nothing to scroll because each
card owned its own overflow and the container was `hidden`.

The rule that follows: `overflow: hidden` on a column of cards is a promise that the content
fits, and content never fits at every window height. Side-by-side panels (Browse, Decks, Journal)
are the opposite case and do scroll individually, because each is a full-height column of its own.

## Guardrails

- **No second accent.** If something needs to stand out and gold is taken, use weight, size or
  position. Adding a hue is how an interface stops meaning anything.
- **No number without its margin** where the calculation knows one. `71% ±9`, never `71.3%`.
  This is already the rule in `mtg-journal`; the interface must stop contradicting it.
- **Never show a bare percentage for a match confidence without the way to refuse it.**
- **No text below 13px.** None. Including captions.
- **Do not put `image_normal` in a background.** See law 1.
- **A `<label>` that wraps a choice is not a caption.** The global rule sets labels in spaced
  uppercase, which is right for "Deck" above a field and wrong for Won / Lost / Draw, for a radio
  row, or for a file input dressed as a button. Those three all shipped shouting and wrapping.
  Any label containing an `input[type=radio]`, a `.checkbox`, or acting as a button resets
  `text-transform` and `letter-spacing`.
- **The shell is pinned, never sized in viewport units.** `#app` is `position: fixed; inset: 0`.
  `100vh` and `100dvh` are interpretations — of browser chrome, of the keyboard, of whatever the
  Android WebView believes its window to be — and on a phone the app was reported filling half
  the screen with the rest left as bare ground. Four insets are not an interpretation.
- **`min-height` on a button does nothing for its label unless the button is flex.** A button is
  `inline-block` by default: the text sits on its baseline at the top of the content box and the
  minimum height is made up underneath it. Every pill in the app had its label riding high — the
  boxes were centred and what was inside them was not, which is invisible in a measurement of the
  boxes and obvious to anyone looking.
- **A card supplies the frame, not the room.** Giving a panel the floating-card treatment does
  not give its contents padding — the deck list shipped with its header, its form and its rows
  flush against the rounded edge. Check the inside after moving the outside.
- **Never `overflow: hidden` on a vertical stack of cards.** Something always falls off the
  bottom at some window height, and there is nothing to scroll to reach it.
