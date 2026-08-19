<script lang="ts">
  import type { CardSummary } from "../types";

  let {
    cards,
    owned,
    selected,
    onselect,
  }: {
    cards: CardSummary[];
    owned: Record<string, number>;
    selected: string | null;
    onselect: (oracleId: string) => void;
  } = $props();

  /**
   * A stand-in for artwork that has not arrived, or cannot.
   *
   * More important here than in the list: a sheet is *made* of images, so an install with no
   * network would otherwise be a grid of empty rectangles. Built from the card's own colours, so
   * even the placeholder says something true.
   */
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

<div class="sheet" role="listbox" aria-label="Search results" tabindex="-1">
  {#each cards as card (card.oracle_id)}
    <button
      type="button"
      class="cell"
      class:selected={card.oracle_id === selected}
      onclick={() => onselect(card.oracle_id)}
      role="option"
      aria-selected={card.oracle_id === selected}
      title={card.name}
    >
      <span class="frame" style="background: {swatch(card.colors)}">
        {#if card.image_art}
          <img src={card.image_art} alt="" loading="lazy" decoding="async" onerror={hide} />
        {/if}
        {#if owned[card.oracle_id]}
          <span class="own">{owned[card.oracle_id]}</span>
        {/if}
      </span>
      <span class="cap">{card.name}</span>
    </button>
  {:else}
    <p class="empty">No cards match these filters.</p>
  {/each}
</div>

<style>
  /* Density *through* the images rather than despite them. Sixteen artworks where a list shows
     six rows, and faster to scan than a column of names: anyone with a season of play recognises
     a card by its painting well before reading its title. See docs/dev/design.md. */
  .sheet {
    flex: 1;
    overflow-y: auto;
    min-width: 0;
    display: grid;
    /* 104px, measured rather than picked: the results column sits between a 292px filter panel
       and a 372px detail panel, so at a 1250px window it is about 500px wide. At 118px that gave
       three columns, which is a list with bigger pictures rather than a sheet. */
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    align-content: start;
    gap: 13px 9px;
    padding: 14px;
  }

  .cell {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 0;
    min-height: 0;
    background: transparent;
    border: none;
    border-radius: 0;
    text-align: left;
    box-shadow: none;
  }

  .cell:hover {
    background: transparent;
  }

  .frame {
    position: relative;
    aspect-ratio: 4 / 3;
    border-radius: 6px;
    overflow: hidden;
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.2),
      0 4px 10px rgba(0, 0, 0, 0.24);
    transition: transform 140ms ease, box-shadow 140ms ease;
  }

  .cell:hover .frame {
    transform: translateY(-2px);
    box-shadow:
      0 2px 6px rgba(0, 0, 0, 0.2),
      0 8px 20px rgba(0, 0, 0, 0.26);
  }

  .frame img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  /* Over the artwork rather than beside it: the sheet has no room for a column, and how many
     you own is the one thing worth reading without opening a card. */
  .own {
    position: absolute;
    top: 5px;
    right: 5px;
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
      0 1px 3px rgba(0, 0, 0, 0.24),
      0 3px 8px rgba(0, 0, 0, 0.22);
  }

  .cap {
    font-size: var(--t-meta);
    line-height: 1.3;
    color: var(--ink-2);
    /* Two lines, then ellipsis. One line cuts half the multiword names in the game; three would
       make the rows ragged and cost a whole row of artwork per screen. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .cell:hover .cap {
    color: var(--ink);
  }

  /* The selected cell is marked on the artwork, which is where the eye already is. Gold is the
     one colour the interface keeps, and here it means the same thing it always does — this is
     the card in hand. */
  .cell.selected .frame {
    outline: 2px solid var(--gold);
    outline-offset: 2px;
  }

  .cell.selected .cap {
    color: var(--ink);
    font-weight: 600;
  }

  .empty {
    grid-column: 1 / -1;
    padding: 72px 24px;
    text-align: center;
    color: var(--ink-2);
    font-size: var(--t-lede);
    max-width: 34ch;
    margin-inline: auto;
    text-wrap: balance;
  }

  /* Phones: smaller cells, so a sheet still shows enough of the catalogue to be worth the mode. */
  @media (max-width: 860px) {
    .sheet {
      grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
      gap: 12px 8px;
      padding: 12px;
    }
  }
</style>
