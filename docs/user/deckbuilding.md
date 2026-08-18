# Building and optimizing decks

> **Available now**: decks, formats, legality checking and the optimizer. Personalised
> recommendations are still to come; that section is marked below.

## Formats

Every format is supported and freely selectable: Standard, Pioneer, Modern, Legacy, Vintage,
Pauper, Commander, Brawl, and Limited (draft and sealed).

The format determines deck size, how many copies are allowed, the sideboard, banned and restricted
cards, and color identity for Commander. Ban lists come straight from the card data, so they stay
current on their own.

## Importing a list

Paste a decklist from anywhere — Arena, Moxfield, MTGO and plain text are all understood, and
you do not have to say which one it is. Sections like `Deck`, `Sideboard` and `Commander` are
followed if present, `SB:` prefixes work, and `4x Lightning Bolt` is as acceptable as
`4 Lightning Bolt (M21) 137`.

You also do not have to spell cards exactly. Accents, apostrophes and capitals are ignored, so
`lim-duls vault` finds *Lim-Dûl's Vault*. Double-faced and adventure cards are found by their
front face, which is how every site writes them: `Bonecrusher Giant` resolves to
*Bonecrusher Giant // Stomp*.

**Lines that cannot be read are listed, never dropped.** A list that quietly imports 58 of your
60 cards is worse than one that tells you which two it could not place. The same goes for
genuinely ambiguous names: `Fire` is half of both *Fire // Ice* and *Start // Fire*, so the app
asks rather than guessing.

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
| **Pool only** | In draft and sealed: restricted to the pool you entered, by hand or by [scanning it](scanning.md) |

## What the app tells you about a deck

Every edit re-checks the deck immediately. Instead of a verdict, you get a list of specific
problems you can act on:

- the deck is the wrong size, and by how much
- too many copies of a card — with basic lands and cards like *Relentless Rats* correctly exempt
- a card that is banned, restricted, or simply not in the format
- a card outside your commander's colour identity, naming both identities
- a command zone that is empty, too full, or holds something that cannot be a commander

Alongside it: the mana curve, how many coloured symbols each colour is asked for, and the land
and creature counts.

For two formats — Competitive Brawl and TLR — the construction rules are **inferred** rather
than confirmed, because they are not publicly documented. The app says so rather than
presenting a guess as a verdict.

## Reading the optimizer's score

A deck gets a score out of 100, but the number on its own is not the point. It breaks into
named criteria, each with a sentence saying what was measured:

- **Mana base** — how often your coloured costs can actually be paid on the turn they come up.
  Computed from the deck itself, counting *symbols* rather than cards: `{B}{B}` asks far more of
  a mana base than `{1}{B}`. One card you cannot cast at all drags this down even when the rest
  of the deck is fine.
- **Land drops** — how often you make every land drop through turn four, measured over ten
  thousand simulated games. Scored on the drops themselves rather than on a land count, because
  a deck can run the conventional twenty-four lands and still stumble.
- **Opening hands** — how often your first seven is worth keeping, and how many mulligans you
  take on average.
- **Curve** — how closely your costs match the shape the archetype you picked usually wants.

That last one is marked **convention** in the app, and the distinction matters. The first three
are calculated from your deck. The curve target is a shape deckbuilders generally aim for, not a
fact — a deck that deviates is not thereby wrong, which is why it counts for less.

## Asking for suggestions

The optimizer proposes single-card swaps, each with what it improved and by how much. They are
independent: apply any of them, in any order, or none.

**What it is good at:** telling you your mana base cannot support a card, that you are short on
lands, or that your curve is nothing like what you said you were playing.

**What it cannot do:** judge a card. Nothing in the scoring reads rules text, so it has no idea
that a counterspell is better than a vanilla 2/2 — it only sees mana values, colours and card
types. There is a filter that keeps it to cards people actually play, but treat its output as a
mana and curve check rather than as card advice. Recommendations that understand what a card is
*for* are not built yet.

## Simulation

The app simulates thousands of goldfish games to estimate: the probability of keeping your opening
hand, of hitting your curve on turns one through four, of finding your Nth land.

That is what lets you objectively compare two versions of the same deck.

## Personalized recommendations

Later, a personal model will learn from your choices — the suggestions you accept, the ones you
reject — and tunes its proposals to your playstyle. It runs entirely on your device, and you can
reset it at any time.
