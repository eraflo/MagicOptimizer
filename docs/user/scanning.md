# Scanning your cards

> **Available on the desktop app now.** On Android the code is in place but has not yet been run
> on a real phone — if the camera misbehaves there, that is a known gap rather than a surprise.
>
> The advice below is **measured**, not guessed: 280 simulated photographs of real card images,
> across ten cards, two angles and seven background shades. What it has not yet met is an actual
> camera pointed at an actual shoebox of foils.

## How it works

You pass your cards one at a time in front of the camera, without pressing anything. The app
finds the card in the frame, straightens it, and identifies it continuously.

Recognition works on the **artwork**, not on the text. The direct consequence is that the card's
language does not matter: your French, Japanese or German cards are recognised exactly like
English ones.

Nothing is written to your collection while you scan. Recognised cards collect in a list beside
the viewfinder, and you confirm the whole batch at the end — a misread is a great deal easier to
fix before it reaches your collection than after.

## Before you can scan

Recognition needs one extra download, `arthashes.bin`, of about 6 MB. It holds a fingerprint of
every distinct artwork — 50,391 of them — and no images at all. If you never scan cards you never
need it, which is why it is not bundled.

If you are running from a checkout, build it yourself:

```bash
cargo run --release -p build-artifacts -- --art-only
```

That takes a couple of hours the first time, almost all of it waiting politely between image
downloads. It resumes where it stopped, so interrupting it costs nothing.

## The four destinations

Before starting, choose where the scanned cards go.

| Destination | When to use it |
|---|---|
| **Physical collection** | Entering or extending your collection. A storage box can be attached as you go. |
| **Digital collection** | Kept separate from your physical cards, so the two never get mixed up. |
| **A deck** | Digitising a physical deck you already built, to analyse or optimise it. Choose the zone as well — main, sideboard or command. |
| **A draft or sealed pool** | Entering your pool **once the draft is over**, then building your 40 cards. It is stored as physical cards in a box named after the pool, which is what a pool is by then. |

There is deliberately **no assistance during a draft**: electronic devices are banned in
tournament play, and it would be poorly received regardless. The pool is entered afterwards.

## Reading the viewfinder

An outline is drawn around whatever the app thinks is a card, and its colour is the whole status
display:

| Outline | Meaning |
|---|---|
| **Grey** | A card is there, but nothing matched. Usually the background: if it is dark, the outline has probably snapped to the card's *interior* rather than its edge, which shifts the crop just enough to break recognition. Try a lighter surface first. |
| **Amber** | A match, being confirmed. A small ring fills as successive frames agree. |
| **Green** | Confirmed and added to the list. |

No outline at all means no card was found, which is a framing or lighting problem rather than a
recognition one.

## Getting good results

**Do not use a black or very dark surface.** This is the single thing that matters most, and it
is the opposite of what seems obvious. Magic cards have a black border: on a dark table there is
nothing separating the two, and recognition collapses. Measured on real card images:

| Surface | Recognised, of 20 frames |
|---|---|
| Near-black | 2 |
| Dark grey | 14 |
| Charcoal | 17 |
| **Mid grey** | **20** |
| **Slightly lighter grey** | **20** |
| Light grey | 11 |
| White | 12 |

A plain **mid-grey or wooden** table is the sweet spot, and it is worth getting right: it is the
difference between every frame working and one in ten. White is noticeably worse than grey,
because the pale parts of the artwork start blending into it instead.

The rest:

- **One card at a time.** Several cards in frame will find the largest one and ignore the rest.
- **Diffuse lighting.** The main enemy is glare, particularly on foils. Avoid a light aimed
  straight down the camera's axis.
- **The whole card in frame**, flat, with no finger over the artwork. A slight tilt is fine —
  perspective is corrected automatically.
- **Pause about half a second per card.** Identification is confirmed across several successive
  frames, which is what prevents mistakes; it also means you cannot go very fast.

## Edge cases

- **Reprints with identical artwork** cannot be told apart, because they are the same painting.
  The app records the card correctly and picks a printing; if the exact printing matters to you,
  adjust it in your collection afterwards.
- **Both faces of a double-faced card** are recognised, so scanning a werewolf's back face finds
  the card rather than nothing.
- **Foils** are the hardest case, because of glare. Change the angle if recognition hesitates.
- **Sleeved cards** should work, glare permitting.
- **Tokens and emblems** are not in the artwork data at all. There is no such physical card to
  add to a collection.
- **A card the app declines to name** is the intended behaviour when two different cards match
  about equally well. Naming one of them would be a guess, and a wrong card added silently is a
  worse outcome than a card you have to enter by hand. Across all 280 measured photographs the
  app named a card correctly or declined — **it never named the wrong one**. That is the
  property the thresholds are set to protect, and it is why a few declined frames are not worth
  worrying about: the next one usually works.
