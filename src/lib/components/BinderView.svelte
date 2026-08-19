<script lang="ts">
  import type { BinderCard } from "../types";

  let {
    cards,
    onopen,
  }: {
    cards: BinderCard[];
    /** Opening a pocket selects the holding in the table, which is where editing happens. */
    onopen: (id: number) => void;
  } = $props();

  /** Nine to a page, because that is what a real binder page holds. */
  const PER_PAGE = 9;

  /**
   * Cards grouped the way they are actually stored.
   *
   * `mtg-collection` has modelled storage locations since it was written and nothing in the
   * interface ever showed them. Grouping by container is the whole point of drawing a binder:
   * the screen should answer "which box is it in", not just "do I own it".
   */
  const groups = $derived.by(() => {
    const by = new Map<string, BinderCard[]>();
    for (const card of cards) {
      const key = card.container || "Unfiled";
      const list = by.get(key);
      if (list) list.push(card);
      else by.set(key, [card]);
    }
    // Sorted by slot where the holding gives one, so a page matches the physical page.
    for (const list of by.values()) {
      list.sort((a, b) => (a.slot ?? 9999) - (b.slot ?? 9999) || a.name.localeCompare(b.name));
    }
    return [...by.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  let page = $state<Record<string, number>>({});

  const pageOf = (container: string) => page[container] ?? 0;
  const pageCount = (list: BinderCard[]) => Math.max(1, Math.ceil(list.length / PER_PAGE));

  function turn(container: string, list: BinderCard[], by: number) {
    const next = Math.min(Math.max(pageOf(container) + by, 0), pageCount(list) - 1);
    page = { ...page, [container]: next };
  }

  /**
   * Exactly nine slots, padding with empties.
   *
   * The empty pockets are deliberate. A collection is read as much by what is missing as by what
   * is there, and a gap in a page says that faster than a row reading "0 copies" ever could.
   */
  function pocketsFor(list: BinderCard[], index: number): (BinderCard | null)[] {
    const slice = list.slice(index * PER_PAGE, index * PER_PAGE + PER_PAGE);
    return [...slice, ...Array(PER_PAGE - slice.length).fill(null)];
  }

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

{#each groups as [container, list] (container)}
  <section class="binder">
    <header>
      <div class="what">
        <h4>{container}</h4>
        <span class="tally">
          {list.reduce((sum, c) => sum + c.quantity, 0)} cards · {list.length} entries
        </span>
      </div>
      {#if pageCount(list) > 1}
        <div class="turn">
          <button
            type="button"
            class="ghost"
            disabled={pageOf(container) === 0}
            onclick={() => turn(container, list, -1)}
            aria-label="Previous page">‹</button
          >
          <span class="folio">{pageOf(container) + 1} / {pageCount(list)}</span>
          <button
            type="button"
            class="ghost"
            disabled={pageOf(container) >= pageCount(list) - 1}
            onclick={() => turn(container, list, 1)}
            aria-label="Next page">›</button
          >
        </div>
      {/if}
    </header>

    <div class="page">
      {#each pocketsFor(list, pageOf(container)) as card, index (index)}
        {#if card}
          <button
            type="button"
            class="pocket"
            onclick={() => onopen(card.id)}
            title="{card.name} — {card.set_code.toUpperCase()} {card.collector_number}{card.section
              ? `, ${card.section}`
              : ''}"
          >
            <span class="art" style="background: {swatch(card.colors)}">
              {#if card.image_art}
                <img src={card.image_art} alt="" loading="lazy" decoding="async" onerror={hide} />
              {/if}
            </span>
            <!-- The sleeve's sheen. The one decorative flourish in the view, and it is a single
                 gradient — it does most of the work of making these read as objects. -->
            <span class="sleeve"></span>
            {#if card.quantity > 1}<span class="qty">{card.quantity}</span>{/if}
            <span class="caption">{card.name}</span>
          </button>
        {:else}
          <span class="pocket empty" aria-hidden="true"></span>
        {/if}
      {/each}
    </div>
  </section>
{:else}
  <p class="nothing">Nothing stored yet. Add cards with a container and they appear as pages.</p>
{/each}

<style>
  .binder {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 20px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 18px;
    background: rgba(20, 20, 24, 0.8);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-shadow: var(--sheen), var(--lift);
  }

  .binder + .binder {
    margin-top: 14px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }

  h4 {
    margin: 0;
    font-size: var(--t-title);
    font-weight: 700;
    letter-spacing: -0.015em;
  }

  .tally {
    font-size: var(--t-meta);
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }

  .turn {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .turn button {
    width: 32px;
    min-height: 32px;
    padding: 0;
    font-size: var(--t-lede);
    line-height: 1;
  }

  .folio {
    font-size: var(--t-meta);
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
    min-width: 46px;
    text-align: center;
  }

  /* Three by three, because that is a binder page. Not a responsive grid that happens to fit
     nine — the shape is the point. */
  .page {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
    max-width: 520px;
  }

  .pocket {
    position: relative;
    aspect-ratio: 63 / 88;
    padding: 4px;
    border: none;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.05);
    /* A ring, not a top line: a one-pixel horizontal highlight stops dead where the radius
       turns and leaves a nick in both upper corners. */
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.08),
      inset 0 -2px 6px rgba(0, 0, 0, 0.45);
    overflow: hidden;
  }

  .pocket:hover:not(.empty) {
    background: rgba(255, 255, 255, 0.09);
  }

  .art {
    position: absolute;
    inset: 4px;
    border-radius: 5px;
    overflow: hidden;
  }

  .art img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    /* A card's painting lives in its upper half; a centred crop of an art_crop cuts the subject. */
    object-position: top;
    display: block;
  }

  .sleeve {
    position: absolute;
    inset: 4px;
    border-radius: 5px;
    background: linear-gradient(118deg, rgba(255, 255, 255, 0.22) 0%, rgba(255, 255, 255, 0) 40%);
    pointer-events: none;
  }

  .qty {
    position: absolute;
    right: 8px;
    bottom: 8px;
    min-width: 21px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--gold);
    color: #1c1509;
    font-size: var(--t-meta);
    font-weight: 700;
    line-height: 19px;
    text-align: center;
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.26),
      0 3px 8px rgba(0, 0, 0, 0.24);
  }

  /* Named on hover only. Nine captions at once would bury the artwork the page exists to show. */
  .caption {
    position: absolute;
    left: 4px;
    right: 4px;
    bottom: 4px;
    padding: 16px 8px 7px;
    border-radius: 0 0 5px 5px;
    background: linear-gradient(180deg, transparent, rgba(0, 0, 0, 0.85));
    color: var(--ink);
    font-size: var(--t-meta);
    line-height: 1.25;
    text-align: left;
    opacity: 0;
    transition: opacity 140ms ease;
  }

  .pocket:hover .caption,
  .pocket:focus-visible .caption {
    opacity: 1;
  }

  /* An empty pocket is information: a collection is read as much by its gaps as by its cards. */
  .pocket.empty {
    background: rgba(0, 0, 0, 0.3);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05);
  }

  .nothing {
    padding: 64px 24px;
    text-align: center;
    color: var(--ink-2);
    font-size: var(--t-lede);
    max-width: 34ch;
    margin-inline: auto;
    text-wrap: balance;
  }

  @media (pointer: coarse) {
    .caption {
      opacity: 1;
    }
  }
</style>
