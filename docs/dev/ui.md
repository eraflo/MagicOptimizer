# Interface: one app, two shapes

> **Status — superseded for visual direction; still current for layout.**
>
> This was written when the mobile interface had been tried on a phone and reported as bad, and
> "make it responsive" is not an actionable instruction. Its **layout** reasoning still holds and
> is still followed: what each device is for, bottom navigation on mobile, the per-view treatment
> table, the touch and type rules.
>
> Its **visual** proposals do not. Three directions were rejected after this was written, and the
> one that landed is in [`design.md`](design.md) — near-black chassis, artwork as the source of
> colour, floating glass cards on a lit ground. Where the two disagree, `design.md` wins.
>
> Everything the "order to do this in" section lists is now built. The line further down saying
> there is no artifact downloader is also out of date; there is one, and a `Data` screen for it.

## The mistake to stop making

The current interface is a desktop layout with three media queries bolted on. That is not a
responsive design, it is a desktop design made narrow, and it shows: the tab bar sits at the top
where no thumb reaches, tables become stacks of cramped rows, and the scan view — the one screen
that exists *for* the phone — is laid out as a grid with a side panel.

The fix is not more breakpoints. It is deciding what each device is **for**, and letting the two
layouts differ where the tasks differ.

## What each device is actually for

This is not a guess. Invariant 2 says the app is **never used during a game**, which decides
where each task happens.

| Task | Where it really happens | Why |
|---|---|---|
| Scanning a collection | **Phone** | The camera is there. This is the phone's headline feature. |
| Checking whether you own a card | **Phone** | In a shop, at a table, away from a desk. |
| Logging a game | **Phone** | Straight after playing, standing up, one-handed. |
| Browsing and filtering the catalog | **Desktop** | Multi-column comparison, a real keyboard for search. |
| Building and editing a deck | **Desktop** | Long session, list beside detail, drag and drop later. |
| Optimising, combos, brackets | **Desktop** | Dense output, several panels read together. |

So the two are **the same app with different front doors**. Not a subset — everything stays
reachable on both — but what greets you differs, and so does what gets the screen.

## Navigation

**Desktop keeps the top tab bar.** It is beside the app title, it is a mouse target, and five
items fit comfortably.

**Mobile moves to a bottom bar**, in thumb reach, in task order:

```
   ┌─────────────────────────┐        ┌─────────────────────────┐
   │ ▣ MagicOptimizer  35,306│        │  Collection          ⚙ │
   ├─────────────────────────┤        ├─────────────────────────┤
   │ Browse Decks Coll Scan  │        │                         │
   ├──────┬───────────┬──────┤        │                         │
   │      │           │      │        │      (one column,       │
   │filter│  results  │detail│        │       full width)       │
   │      │           │      │        │                         │
   │      │           │      │        │                         │
   └──────┴───────────┴──────┘        ├─────────────────────────┤
                                      │  ⌾    ▤    ◈    ☰    ⊞  │
        desktop: 3 columns            │ Scan Coll Decks Log More│
        tabs on top                   └─────────────────────────┘
                                        mobile: bottom bar,
                                        scan first
```

Two deliberate differences beyond position:

* **The order changes.** Scan comes first on a phone and does not exist as a first-class desktop
  concern; Browse leads on desktop and lives behind *More* on a phone. Ordering navigation by
  what the device is for is the whole point.
* **Five items is the ceiling.** With Journal that is already five, so anything added later goes
  behind *More* rather than shrinking the bar. The **Data** screen is the first thing to hit this
  and it does not go in the bar: it is reached from the status chip in the header, which is
  already the thing that reports whether there is any data. A destination you visit twice does
  not deserve a permanent slot, but it does deserve to exist.

## Per-view treatment

| View | Desktop | Mobile |
|---|---|---|
| **Browse** | Three columns: filters, results, detail. | One column. Filters in a sheet from the bottom, not a side drawer. Detail as a full-screen sheet. |
| **Decks** | List beside editor. | List, then editor as a pushed page with a back arrow. |
| **Collection** | Table. | Cards, not rows: name, quantity and location stacked, with the count large enough to read at arm's length. |
| **Scan** | Viewfinder with the destination panel beside it. | **Full-bleed viewfinder.** Destination and the scanned list as a sheet pulled up from the bottom, collapsed to a single line while scanning. |
| **Journal** | Form beside results. | Form first — recording is the mobile task — results below it. |

Scan is the one that most needs rethinking. Today it is a grid with a 320px side panel, which on
a phone squeezes the camera into a letterbox. The camera should own the screen, and everything
else should be a sheet over it.

## Rules that apply to both

These are not preferences; each has a reason.

* **Touch targets are 44px minimum**, keyed on `pointer: coarse` rather than width — a narrow
  desktop window still has a mouse, a large tablet still has fingers. This rule already exists
  and is not honoured everywhere; the `×` buttons in the scan and journal lists are 22px.
* **Inputs are 16px on touch.** Anything smaller makes some WebViews zoom on focus, which then
  strands the user zoomed in.
* **Nothing scrolls horizontally.** Any table, code block or wide row gets its own
  `overflow-x: auto` container rather than pushing the page.
* **`env(safe-area-inset-*)` on the bottom bar**, not just on `body` — a navigation bar under the
  gesture bar is a navigation bar that cannot be tapped.
* **One accent colour, and it means "this is the primary action".** It currently also marks
  active tabs, selected destinations and the adjusted win rate. That is three meanings, so it
  carries none.
* **Type scale**: 12 / 13 / 15 / 22. The current interface uses 11px in several places, which is
  below what most people read comfortably on a phone at arm's length.

## What the phone reveals that the desktop hides

Worth stating, because both are already reported and neither is a styling problem:

* **The artifact downloader now exists**, and building it exposed a navigation problem the
  desktop never showed. It was written as a first-run interstitial, gated on the catalog being
  missing — so downloading the catalog satisfied the gate, the panel disappeared, and the two
  optional artifacts became permanently unreachable. Anything that manages state cannot live in
  a screen whose condition its own success destroys. It is a destination now, reached from the
  status chip in the header at every width.
* **The app is 142 MB** because it is a debug build. A release build with a signing key is what
  makes it installable in a way anyone would accept.

## Order to do this in

Judged by how much each fixes relative to what it costs:

1. ~~**Bottom navigation on mobile**~~ — done.
2. ~~**Full-bleed scan** with a bottom sheet~~ — done.
3. **Touch target and type audit** against the rules above. Mechanical, and it is the difference
   between "cramped" and "fine".
4. **Collection as cards on mobile** instead of a stacked table.
5. **Accent colour discipline.** Cheap, and it makes every screen easier to read.

Everything above is a proposal. The person who has actually used it on a phone should say which
of these matches what annoyed them, because a design document written by someone who has only
seen the desktop is a hypothesis.
