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
      <span class="name">
        {card.name}
        {#if card.game_changer}
          <span class="badge gc" title="On the official Commander Game Changers list">GC</span>
        {/if}
        {#if card.faces > 1}
          <span class="badge faces" title="{card.faces} faces">{card.faces}&#8202;faces</span>
        {/if}
      </span>

      <span class="cost"><ManaCost cost={card.mana_cost} /></span>

      <span class="type">{card.type_line}</span>

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
    grid-template-columns: minmax(0, 1.5fr) auto minmax(0, 1.4fr) 44px;
    gap: 12px;
    align-items: center;
    width: 100%;
    text-align: left;
    padding: 7px 14px;
    background: transparent;
    border: none;
    border-bottom: 1px solid rgba(38, 44, 59, 0.65);
    border-radius: 0;
  }

  .row:hover {
    background: var(--panel);
  }

  .row.selected {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .type {
    color: var(--text-muted);
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cost {
    justify-self: start;
  }

  .owned {
    justify-self: end;
  }

  .count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 20px;
    padding: 0 6px;
    border-radius: 999px;
    background: rgba(67, 170, 106, 0.16);
    color: var(--success);
    border: 1px solid rgba(67, 170, 106, 0.35);
    font-size: 11px;
    font-weight: 700;
  }

  .badge {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.03em;
    padding: 1px 5px;
    border-radius: 4px;
    flex: none;
  }

  .badge.gc {
    background: rgba(228, 87, 61, 0.16);
    color: var(--danger);
    border: 1px solid rgba(228, 87, 61, 0.35);
  }

  .badge.faces {
    background: rgba(138, 122, 194, 0.16);
    color: var(--mana-b);
    border: 1px solid rgba(138, 122, 194, 0.35);
  }

  .empty {
    padding: 32px 16px;
    text-align: center;
    color: var(--text-dim);
  }

  /* Phones: the type line moves under the name instead of competing with it for width. */
  @media (max-width: 860px) {
    .row {
      grid-template-columns: minmax(0, 1fr) auto 40px;
      grid-template-areas:
        "name cost owned"
        "type type owned";
      gap: 2px 10px;
      padding: 9px 14px;
    }

    .name {
      grid-area: name;
    }

    .cost {
      grid-area: cost;
      justify-self: end;
    }

    .type {
      grid-area: type;
      font-size: 12px;
    }

    .owned {
      grid-area: owned;
      align-self: center;
    }
  }
</style>
