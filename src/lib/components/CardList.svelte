<script lang="ts">
  import ManaCost from "./ManaCost.svelte";
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
   * The app is offline-first, so a list has to look deliberate with no network at all — an empty
   * grey rectangle per row would be worse than the text list this replaces. Built from the card's
   * own colours, so even the placeholder carries real information.
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

  /** Artwork that 404s or is blocked leaves the placeholder in place rather than a broken icon. */
  function hide(event: Event) {
    (event.currentTarget as HTMLImageElement).style.visibility = "hidden";
  }
</script>

<div class="list" role="listbox" aria-label="Search results" tabindex="-1">
  {#each cards as card (card.oracle_id)}
    <button
      type="button"
      class="row"
      class:selected={card.oracle_id === selected}
      onclick={() => onselect(card.oracle_id)}
      role="option"
      aria-selected={card.oracle_id === selected}
    >
      <!-- The artwork, not the whole card: a card shrunk to 56px is illegible, while the
           painting at the same size is recognisable at a glance. See docs/dev/design.md. -->
      <span class="thumb" style="background: {swatch(card.colors)}">
        {#if card.image_art}
          <img src={card.image_art} alt="" loading="lazy" decoding="async" onerror={hide} />
        {/if}
      </span>

      <span class="what">
        <span class="name">
          {card.name}
          {#if card.game_changer}
            <span class="badge gc" title="On the official Commander Game Changers list">GC</span>
          {/if}
          {#if card.faces > 1}
            <span class="badge faces" title="{card.faces} faces">{card.faces}&#8202;faces</span>
          {/if}
        </span>
        <span class="type">{card.type_line}</span>
      </span>

      <span class="cost"><ManaCost cost={card.mana_cost} /></span>

      <span class="owned">
        {#if owned[card.oracle_id]}
          <span class="count" title="Copies in your collection">{owned[card.oracle_id]}</span>
        {/if}
      </span>
    </button>
  {:else}
    <p class="empty">No cards match these filters.</p>
  {/each}
</div>

<style>
  .list {
    flex: 1;
    overflow-y: auto;
    min-width: 0;
  }

  .row {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr) auto auto;
    gap: 12px;
    align-items: center;
    width: 100%;
    text-align: left;
    padding: 8px 14px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--line);
    border-radius: 0;
  }

  .row:hover {
    background: var(--ground-2);
  }

  /* The selected row lifts rather than tints. A colour here would be a second accent, and the
     gold is spoken for — it marks what you own and nothing else. */
  .row.selected {
    background: var(--ground-3);
    box-shadow: inset 2px 0 0 var(--ink);
  }

  .thumb {
    width: 56px;
    height: 42px;
    border-radius: 4px;
    overflow: hidden;
    position: relative;
    flex: none;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }

  .thumb img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .what {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: var(--t-body);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .type {
    color: var(--ink-2);
    font-size: var(--t-meta);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cost {
    justify-self: start;
  }

  .owned {
    justify-self: end;
    min-width: 26px;
    display: flex;
    justify-content: flex-end;
  }

  /* Gold, and gold appears nowhere else. In a shop the only question is whether you already own
     the card, so that answer gets the one colour the interface keeps for itself. */
  .count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    height: 22px;
    padding: 0 7px;
    border-radius: 999px;
    background: var(--gold);
    color: #1c1509;
    font-size: var(--t-meta);
    font-weight: 700;
  }

  .badge {
    font-size: var(--t-meta);
    font-weight: 700;
    letter-spacing: 0.03em;
    padding: 0 5px;
    border-radius: 4px;
    flex: none;
    color: var(--ink-2);
    border: 1px solid var(--line-strong);
  }

  .badge.gc {
    color: var(--danger);
    border-color: rgba(228, 87, 61, 0.45);
  }

  .empty {
    padding: 32px 16px;
    text-align: center;
    color: var(--ink-3);
  }

  /* Phones: the cost moves under the type line rather than squeezing the name. The artwork stays
     — it is the fastest way to recognise a card, and that matters more on a small screen. */
  @media (max-width: 860px) {
    .row {
      grid-template-columns: 56px minmax(0, 1fr) auto;
      gap: 10px;
      padding: 9px 14px;
    }

    .cost {
      grid-column: 2;
      grid-row: 2;
      justify-self: start;
    }

    .what {
      grid-column: 2;
      grid-row: 1;
    }

    .owned {
      grid-column: 3;
      grid-row: 1 / span 2;
      align-self: center;
    }
  }
</style>
