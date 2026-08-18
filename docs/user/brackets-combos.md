# Commander brackets and combos

> **Available now**, once the combo data is downloaded. See [what it cannot tell you](#what-this-cannot-tell-you).

## Brackets, briefly

Wizards introduced an official five-level power scale for Commander, meant to let a table agree on
expectations before playing.

| Bracket | Name | Spirit |
|:---:|---|---|
| 1 | Exhibition | Theme decks; winning is not the point |
| 2 | Core | The level of a preconstructed deck out of the box |
| 3 | Upgraded | A clearly improved precon, with deliberate card choices |
| 4 | Optimized | Built to win, no restrictions |
| 5 | cEDH | Competitive in the strict sense |

**Game Changers** are an official list of cards considered the most format-warping. It is **not a
ban list**: it is a power signal. Playing more of them simply pushes a deck into a higher bracket.

- Brackets 1 and 2: no Game Changers
- Bracket 3: up to three
- Brackets 4 and 5: no limit

The list changes over time; the app keeps it current along with its card data.

## What the app does with it

**Estimate your deck's bracket** by counting the Game Changers present and spotting the things
that weigh on power: two-card infinite combos, chained extra turns, mass land destruction.

The result is justified rather than asserted: "bracket 3, because of these two Game Changers and
this combo" — so you know what to cut if you are aiming lower.

**Optimize while staying in a bracket.** This is the most useful part day to day: "improve this
deck, but keep it bracket 2". If your playgroup has agreed on a level, the optimizer respects it
instead of mechanically pushing you upward.

## What this cannot tell you

**The estimate only ever says 2, 3 or 4.** Brackets 1 and 5 are not about what is in a deck but
about how it is played: bracket 1 is a theme deck where winning is beside the point, bracket 5 is
a deck built to win a tournament. Two decks with identical cards can sit in 1 and 2, or in 4 and
5. Nothing the app can see distinguishes them, so it does not pretend to.

Tutors are reported but not counted. The published rules say they should be "sparse" below
bracket 3 without saying how many is too many, and inventing a number would be presenting a
guess as a rule.

Mass land denial and extra turns are found by reading rules text, which is a heuristic. It looks
for effects that destroy *all* lands, so stax pieces that merely slow lands down — Winter Orb and
the like — are deliberately not counted.

## Combo detection

The app flags the infinite combos present in your list, using a database of known combos. Useful
in both directions: to find them, but also to discover you have one without realizing — which
changes the deck's bracket.

The combo database comes from [Commander Spellbook](https://commanderspellbook.com), a community
project unaffiliated with MagicOptimizer.

## Building to a bracket

When optimising a Commander deck you can ask the search to **stay within a bracket**. It then
refuses any suggestion that would push the deck past the Game Changer count that bracket allows:
none for bracket 2, three for bracket 3.

One honest limit. That is the only bracket rule the optimizer can check on its own, because
Scryfall flags Game Changers on the card itself. Two-card combos and mass land denial need the
combo database and the card's rules text, which the search cannot afford to consult thousands of
times per run. So a deck built under this constraint can still land above its target for a reason
the search never saw — **check the finished deck against the bracket panel**, which does look at
all three.
