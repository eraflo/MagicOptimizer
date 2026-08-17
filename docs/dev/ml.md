# ML subsystem

> **Status** — design document. Shipped in phase 8. The hyperparameters below are starting points,
> to be revised against real measurements.

## The problem

Recommend cards for a deck. Heuristics (mana curve, color consistency, Karsten's formulas) answer
"is this deck structurally sound?" but not "do these two cards belong together?". Synergy is
empirical knowledge, not a rule: hand-coding it does not scale to 35,000 cards.

Hence two stages, with distinct roles.

---

## Stage A — card embeddings

Trained **offline** on a PC, shipped with the app, never modified on the device.

### Principle

Item2vec, i.e. word2vec applied to something other than text: **each decklist is a "sentence" and
each card a "word"**. Two cards that often appear in the same decks end up close in the vector
space. The model learns synergy statistically, without a single rule being written.

It is the same principle behind EDHREC's recommendations, except we own it and it runs offline.

### Corpus

Public decklists (Moxfield, Archidekt, EDHREC). A deck is an unordered set of cards, so there is
no sequential context: the window spans the whole deck.

Precautions:
- Deduplicate near-identical lists, or popular archetypes drown out everything else.
- Subsample very frequent cards (basic lands, staples). Standard word2vec subsampling applies
  as-is.
- Keep separate corpora per major format family: synergy in Commander is not synergy in Modern.

### Starting parameters

| Parameter | Value | Note |
|---|---|---|
| Dimensions | 64 | 128 if quality justifies it, at twice the memory |
| Negative samples | 5 | |
| Epochs | 5–10 | |
| Subsampling threshold | 1e-3 | |
| Minimum frequency | 5 decks | Filters out noise from barely-played cards |

### Shipping

35,000 × 64 in **f16** ≈ 4.5 MB. Flat file indexed by `CardId`, mmap'd like everything else.
An **optional** artifact: without it, the app falls back to heuristics.

### Evaluation

A validation set of decks held out of the corpus. Metric: remove *k* cards from a deck and measure
how many come back in the top-N recommendations (recall@N). This doubles as the optimizer's
integration test.

---

## Stage B — personal re-ranker

Trained **continuously on the device**, unique to each user. This is the part that learns from
your choices.

### Model

Logistic regression, or a 2-layer MLP if regression plateaus. Deliberately tiny: a few kilobytes
of weights, training in microseconds. It runs effortlessly on a phone, which is exactly the point.

### Features

All normalized to `[0, 1]` or standardized using statistics frozen into the artifact — definitely
not recomputed on the device, or the weights stop meaning the same thing between sessions.

| Feature | Source |
|---|---|
| Embedding similarity to the deck centroid | `mtg-ml`, stage A |
| Fit against the target mana curve | `mtg-optimizer` |
| Color / identity consistency | `mtg-optimizer` |
| Card is owned | `mtg-collection` |
| Price | `printings.rkyv` |
| Meta rank / inclusion rate | `meta.rkyv` |
| Aggregate heuristic score | `mtg-optimizer` |
| Win rate prior | `mtg-journal` |

### Training signal

| Event | Label |
|---|---|
| Suggestion accepted | positive |
| Suggestion rejected | negative |
| Card present in a saved deck | weak positive |
| Card removed from a deck | weak negative |

Updated by **online SGD**, one step per event. Starting learning rate 0.01 with decay. L2
regularization so a handful of clicks cannot swing the model.

### Score blending

```
final_score = w_h · heuristics + w_e · embeddings + w_p · personal_model
```

`w_p` starts at zero and grows with the number of examples seen (confidence ramp, saturating
around a few hundred examples). Without it, the first ten clicks would dictate everything.

The UI shows how many examples have been learned and offers a **model reset**. A personal model
you can neither inspect nor undo is a model you cannot trust.

---

## The game log signal

`mtg-journal` provides a performance prior per deck and per archetype.

**This signal is weak and must be treated as such.** A few dozen games is very little data, and it
is confounded with opponent skill, luck, and piloting quality. It cannot drive the model directly.

Integration: a Beta-Binomial aggregate with **Bayesian shrinkage toward the meta prior**. A deck at
3 wins out of 4 rises only slightly above its baseline; real volume is needed before the estimate
moves meaningfully.

The immediate value of the log is **descriptive** — knowing your win rate per matchup is useful in
itself. The contribution to the model is a bonus that builds slowly, over months of play. Say so
plainly in the UI, so nobody expects an AI that learns fast.

---

## Explicitly ruled out

- **Training a large network on a phone.** Unrealistic in memory and battery terms.
- **Local LLM.** Off-topic and beyond the hardware budget.
- **Retraining embeddings on-device.** The corpus is not there and never will be.

## Retraining the embeddings

```bash
cargo run -p build-artifacts -- --embeddings --corpus ./data/decklists --out ./artifacts
```

Redo this on every major format rotation, or when the corpus has grown significantly. Check
recall@N on the validation set **before** publishing: an embedding quality regression is invisible
to the naked eye but degrades every recommendation.
