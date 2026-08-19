<script lang="ts">
  import type { BoardCard } from "../types";

  let {
    cards,
    onchange,
  }: {
    cards: BoardCard[];
    /** `delta` is +1 or −1 on one card. The board edits by adding and removing copies. */
    onchange: (oracleId: string, delta: number) => void;
  } = $props();

  /**
   * The columns, and why these buckets.
   *
   * Zero and one share a column because a deck rarely has enough of either to fill one alone, and
   * six upward share the last for the same reason at the other end. Lands are pulled out
   * entirely: their mana value says nothing about when they are played, so leaving them in the
   * zero column would put a third of the deck in one bar and flatten everything the board is for.
   */
  const BUCKETS = [
    { key: "0-1", label: "0–1", test: (v: number) => v <= 1 },
    { key: "2", label: "2", test: (v: number) => v === 2 },
    { key: "3", label: "3", test: (v: number) => v === 3 },
    { key: "4", label: "4", test: (v: number) => v === 4 },
    { key: "5", label: "5", test: (v: number) => v === 5 },
    { key: "6+", label: "6+", test: (v: number) => v >= 6 },
  ];

  const main = $derived(cards.filter((c) => c.zone === "main"));

  const columns = $derived.by(() => {
    const spells = main.filter((c) => !c.is_land && c.mana_value !== null);
    const cols = BUCKETS.map((bucket) => ({
      key: bucket.key,
      label: bucket.label,
      cards: spells.filter((c) => bucket.test(Math.round(c.mana_value ?? 0))),
    }));

    cols.push({
      key: "lands",
      label: "Lands",
      cards: main.filter((c) => c.is_land),
    });

    // A card the catalog does not know cannot be placed, and dropping it silently would make the
    // board disagree with the deck list beside it.
    const unknown = main.filter((c) => !c.is_land && c.mana_value === null);
    if (unknown.length) {
      cols.push({ key: "unknown", label: "Unplaced", cards: unknown });
    }
    return cols;
  });

  const counted = (list: BoardCard[]) => list.reduce((sum, c) => sum + c.quantity, 0);

  /** The tallest column, so the others can be drawn to scale against it. */
  const tallest = $derived(Math.max(1, ...columns.map((col) => counted(col.cards))));

  function swatch(colors: string): string {
    const hue: Record<string, string> = {
      W: "var(--mana-w)",
      U: "var(--mana-u)",
      B: "var(--mana-b)",
      R: "var(--mana-r)",
      G: "var(--mana-g)",
    };
    const parts = [...colors].map((c) => hue[c]).filter(Boolean);
    if (parts.length === 0) return "var(--mana-generic)";
    if (parts.length === 1) return parts[0];
    return `linear-gradient(135deg, ${parts.join(", ")})`;
  }

  function hide(event: Event) {
    (event.currentTarget as HTMLImageElement).style.visibility = "hidden";
  }
</script>

<div class="board">
  {#each columns as column (column.key)}
    <section class="column" class:lands={column.key === "lands"}>
      <header>
        <span class="label">{column.label}</span>
        <span class="count">{counted(column.cards)}</span>
      </header>

      <!-- The height of the stack *is* the mana curve. Nothing here is a chart drawn beside the
           deck: a column that runs too tall is visible without reading a number, which is what
           laying cards out on a table does and what a histogram in a side panel does not. -->
      <div class="stack" style="--share: {counted(column.cards) / tallest}">
        {#each column.cards as card (card.oracle_id)}
          <div class="card" title="{card.quantity}× {card.name} — {card.type_line}">
            <span class="frame" style="background: {swatch(card.colors)}">
              {#if card.image_art}
                <img src={card.image_art} alt="" loading="lazy" decoding="async" onerror={hide} />
              {/if}
              {#if card.quantity > 1}
                <span class="qty">{card.quantity}</span>
              {/if}
            </span>
            <span class="nm">{card.name}</span>
            <span class="steps">
              <button
                type="button"
                class="ghost"
                aria-label="Remove one {card.name}"
                onclick={() => onchange(card.oracle_id, -1)}>−</button
              >
              <button
                type="button"
                class="ghost"
                aria-label="Add one {card.name}"
                onclick={() => onchange(card.oracle_id, 1)}>+</button
              >
            </span>
          </div>
        {:else}
          <p class="empty">—</p>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .board {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    overflow-x: auto;
    padding-bottom: 8px;
  }

  .column {
    flex: 1 1 0;
    min-width: 108px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  /* Lands sit apart because they are not a point on the curve. */
  .column.lands {
    border-left: 1px solid var(--line);
    padding-left: 12px;
  }

  .column header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding-bottom: 7px;
    border-bottom: 1px solid var(--line);
  }

  .label {
    font-size: var(--t-meta);
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  /* The column's own count, tabular so the row of them reads as a curve. A column carrying more
     than its share of the deck is tinted, which is the only warning the board gives — there is no
     defensible number for "too many threes". */
  .count {
    font-size: var(--t-meta);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: color-mix(in srgb, var(--ink-3), var(--gold) calc(var(--share, 0) * 100%));
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    position: relative;
  }

  /* Real card proportions, 63×88, not a cropped thumbnail. The point of the board is that it
     looks like cards laid out on a table. */
  .frame {
    position: relative;
    aspect-ratio: 63 / 88;
    border-radius: 6px;
    overflow: hidden;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.5);
  }

  .frame img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    /* `top` rather than centre: a card's artwork is in its upper half, so a 63:88 crop of an
       art_crop should keep the top of the painting rather than its middle. */
    object-fit: cover;
    object-position: top;
    display: block;
  }

  .qty {
    position: absolute;
    top: 4px;
    right: 4px;
    min-width: 20px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--gold);
    color: #1c1509;
    font-size: var(--t-meta);
    font-weight: 700;
    line-height: 18px;
    text-align: center;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.55);
  }

  .nm {
    font-size: var(--t-meta);
    line-height: 1.28;
    color: var(--ink-2);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  /* Revealed on hover: two controls under every card would make the board a form. */
  .steps {
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 140ms ease;
  }

  .card:hover .steps,
  .card:focus-within .steps {
    opacity: 1;
  }

  .steps button {
    flex: 1;
    min-height: 26px;
    padding: 0;
    font-size: var(--t-body);
    line-height: 1;
    background: rgba(255, 255, 255, 0.07);
  }

  .empty {
    margin: 0;
    padding: 10px 0;
    text-align: center;
    color: var(--ink-3);
  }

  /* Touch: the steps cannot be revealed by hovering, so they are always there. */
  @media (pointer: coarse) {
    .steps {
      opacity: 1;
    }
  }

  @media (max-width: 860px) {
    .column {
      min-width: 92px;
    }
  }
</style>
