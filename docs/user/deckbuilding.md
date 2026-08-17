# Building and optimizing decks

> **🚧 Not available yet.** Decks and formats arrive in phase 3, the optimizer in phase 4,
> personalized recommendations in phase 8.

## Formats

Every format is supported and freely selectable: Standard, Pioneer, Modern, Legacy, Vintage,
Pauper, Commander, Brawl, and Limited (draft and sealed).

The format determines deck size, how many copies are allowed, the sideboard, banned and restricted
cards, and color identity for Commander. Ban lists come straight from the card data, so they stay
current on their own.

## Three ways to work

- **Improve an existing deck.** You start from your list; the app proposes replacements, fixes the
  mana curve and land count, and flags anything illegal.
- **Build from scratch.** You give a constraint — a commander, an archetype, colors — and the app
  proposes a complete list.
- **Measure.** Without changing anything, get a numeric diagnosis of the deck.

## Using only what you own

One setting decides what the optimizer is allowed to suggest:

| Setting | Effect |
|---|---|
| **Owned only** | No suggestion outside your collection |
| **Owned + wishlist** | Also allows cards you are considering buying |
| **All cards** | No constraint — to see what the ideal deck would look like |
| **Pool only** | In draft and sealed: restricted to the pool you scanned |

## Reading the score

A deck's score is not an opaque number. It breaks down into named criteria, and **every suggestion
tells you which ones it improves**:

- **Mana curve** — how costs are distributed against the target profile
- **Mana base** — whether you have enough sources of each color to cast your spells on time
- **Color consistency** — respecting the deck's identity
- **Roles** — the balance between removal, card draw, ramp and threats
- **Synergy** — what works well together
- **Legality** — an illegal deck is rejected, not penalized

## Simulation

The app simulates thousands of goldfish games to estimate: the probability of keeping your opening
hand, of hitting your curve on turns one through four, of finding your Nth land.

That is what lets you objectively compare two versions of the same deck.

## Personalized recommendations

From phase 8, a personal model learns from your choices — the suggestions you accept, the ones you
reject — and tunes its proposals to your playstyle. It runs entirely on your device, and you can
reset it at any time.
