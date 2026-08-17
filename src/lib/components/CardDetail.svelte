<script lang="ts">
  import ManaCost from "./ManaCost.svelte";
  import { CONDITIONS, FINISHES, LANGUAGES } from "../types";
  import type { CardDetails, Condition, Finish, Pool, StoredDeck, Zone } from "../types";

  let {
    card,
    ownedCount,
    containers,
    decks,
    onadd,
    onaddtodeck,
    onclose,
  }: {
    card: CardDetails | null;
    ownedCount: number;
    containers: string[];
    decks: StoredDeck[];
    onaddtodeck: (deckId: number, quantity: number, zone: Zone) => Promise<void>;
    /** Closes the sheet on phones, where the panel covers the list. */
    onclose: () => void;
    onadd: (options: {
      pool: Pool;
      quantity: number;
      finish: Finish;
      condition: Condition;
      language: string;
      container: string;
      section: string;
    }) => Promise<void>;
  } = $props();

  let pool = $state<Pool>("physical");
  let quantity = $state(1);
  let finish = $state<Finish>("nonfoil");
  let condition = $state<Condition>("near_mint");
  let language = $state("en");
  let container = $state("");
  let section = $state("");
  let adding = $state(false);
  let justAdded = $state(false);

  // Deck target, also kept between cards: building a deck means adding card after card.
  let deckId = $state<number | null>(null);
  let deckZone = $state<Zone>("main");
  let deckQuantity = $state(1);
  let addedToDeck = $state(false);

  // Default to the first deck once the list arrives, so the button is never a no-op.
  $effect(() => {
    if (deckId === null && decks.length > 0) deckId = decks[0].id;
  });

  async function addToDeck() {
    if (deckId === null) return;
    adding = true;
    try {
      await onaddtodeck(deckId, deckQuantity, deckZone);
      addedToDeck = true;
      setTimeout(() => (addedToDeck = false), 1400);
    } finally {
      adding = false;
    }
  }

  // Location and printing settings are deliberately *not* reset between cards: entering a
  // binder means setting them once and then adding card after card.
  async function add() {
    if (!card) return;
    adding = true;
    try {
      await onadd({ pool, quantity, finish, condition, language, container, section });
      justAdded = true;
      setTimeout(() => (justAdded = false), 1400);
    } finally {
      adding = false;
    }
  }
</script>

<aside class="panel" class:has-card={card !== null}>
  {#if !card}
    <p class="placeholder">Select a card to see its details.</p>
  {:else}
    <div class="sheet-head">
      <button type="button" class="ghost" onclick={onclose}>&#8592; Back to results</button>
    </div>

    {#if card.image_normal}
      <img class="art" src={card.image_normal} alt={card.name} loading="lazy" />
    {/if}

    <header>
      <h2>{card.name}</h2>
      <ManaCost cost={card.mana_cost} />
    </header>

    <p class="type">{card.type_line}</p>

    <div class="badges">
      <span class="badge">{card.rarity}</span>
      <span class="badge">{card.set_code.toUpperCase()} {card.collector_number}</span>
      {#if card.game_changer}<span class="badge warn">Game Changer</span>{/if}
      {#if card.reserved}<span class="badge warn">Reserved List</span>{/if}
      {#if card.edhrec_rank}<span class="badge">EDHREC #{card.edhrec_rank}</span>{/if}
    </div>

    {#if card.face_views.length > 1}
      {#each card.face_views as face}
        <section class="face">
          <div class="face-head">
            <strong>{face.name}</strong>
            <ManaCost cost={face.mana_cost} />
          </div>
          <p class="face-type">{face.type_line}</p>
          <p class="text">{face.oracle_text}</p>
          {#if face.power}<p class="pt">{face.power}/{face.toughness}</p>{/if}
        </section>
      {/each}
    {:else}
      <p class="text">{card.oracle_text}</p>
      {#if card.power}<p class="pt">{card.power}/{card.toughness}</p>{/if}
      {#if card.loyalty}<p class="pt">Loyalty {card.loyalty}</p>{/if}
    {/if}

    <details class="legality">
      <summary>
        Legal in {card.legal_formats.length}
        {card.legal_formats.length === 1 ? "format" : "formats"}
      </summary>
      <p class="legal-list">{card.legal_formats.join(", ") || "None"}</p>
      {#if card.restricted_formats.length}
        <p class="legal-list restricted">
          Restricted: {card.restricted_formats.join(", ")}
        </p>
      {/if}
      {#if card.banned_formats.length}
        <p class="legal-list banned">Banned: {card.banned_formats.join(", ")}</p>
      {/if}
    </details>

    {#if decks.length > 0}
      <section class="add">
        <h3>Add to deck</h3>
        <div class="deck-row">
          <select bind:value={deckId} aria-label="Deck">
            {#each decks as deck}
              <option value={deck.id}>{deck.name}</option>
            {/each}
          </select>
          <select bind:value={deckZone} aria-label="Zone">
            <option value="main">Deck</option>
            <option value="sideboard">Sideboard</option>
            <option value="command">Commander</option>
          </select>
          <input
            type="number"
            min="1"
            max="99"
            bind:value={deckQuantity}
            aria-label="Copies"
            class="deck-qty"
          />
          <button type="button" onclick={addToDeck} disabled={adding}>
            {addedToDeck ? "Added" : "Add"}
          </button>
        </div>
      </section>
    {/if}

    <section class="add">
      <div class="add-head">
        <h3>Add to collection</h3>
        {#if ownedCount > 0}
          <span class="owned">You own {ownedCount}</span>
        {/if}
      </div>

      <div class="grid">
        <div>
          <label for="add-pool">Collection</label>
          <select id="add-pool" bind:value={pool}>
            <option value="physical">Physical</option>
            <option value="digital">Digital</option>
          </select>
        </div>
        <div>
          <label for="add-qty">Copies</label>
          <input id="add-qty" type="number" min="1" max="999" bind:value={quantity} />
        </div>
        <div>
          <label for="add-finish">Finish</label>
          <select id="add-finish" bind:value={finish}>
            {#each FINISHES as option}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>
        <div>
          <label for="add-condition">Condition</label>
          <select id="add-condition" bind:value={condition}>
            {#each CONDITIONS as option}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>
        <div>
          <label for="add-language">Language</label>
          <select id="add-language" bind:value={language}>
            {#each LANGUAGES as option}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>
        <div>
          <label for="add-container">Stored in</label>
          <input
            id="add-container"
            list="containers"
            placeholder="Binder 3"
            bind:value={container}
          />
          <datalist id="containers">
            {#each containers as name}<option value={name}></option>{/each}
          </datalist>
        </div>
        <div class="span-2">
          <label for="add-section">Section</label>
          <input id="add-section" placeholder="page 12" bind:value={section} />
        </div>
      </div>

      <button type="button" class="primary add-button" onclick={add} disabled={adding}>
        {#if justAdded}
          Added
        {:else}
          Add {quantity} {quantity === 1 ? "copy" : "copies"}
        {/if}
      </button>
      <p class="hint">
        Printing and location are kept between cards, so entering a binder means setting them
        once.
      </p>
    </section>
  {/if}
</aside>

<style>
  .panel {
    width: 352px;
    flex: none;
    padding: 16px;
    overflow-y: auto;
    border-left: 1px solid var(--border);
    background: var(--panel);
  }

  .placeholder {
    color: var(--text-dim);
    text-align: center;
    margin-top: 48px;
  }

  .art {
    width: 100%;
    border-radius: 12px;
    display: block;
    margin-bottom: 14px;
    background: var(--panel-raised);
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: 16px;
    line-height: 1.3;
  }

  h3 {
    margin: 0;
    font-size: 13px;
    color: var(--text-muted);
    font-weight: 600;
  }

  .type {
    margin: 4px 0 10px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-bottom: 12px;
  }

  .badge {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--panel-raised);
    border: 1px solid var(--border-strong);
    color: var(--text-muted);
  }

  .badge.warn {
    background: rgba(228, 87, 61, 0.14);
    border-color: rgba(228, 87, 61, 0.4);
    color: var(--danger);
  }

  .text {
    white-space: pre-wrap;
    font-size: 13px;
    line-height: 1.55;
    margin: 0 0 10px;
  }

  .pt {
    margin: 0 0 10px;
    font-weight: 700;
  }

  .face {
    border-left: 2px solid var(--border-strong);
    padding-left: 10px;
    margin-bottom: 12px;
  }

  .face-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .face-type {
    margin: 2px 0 6px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .legality {
    margin-bottom: 16px;
    font-size: 12px;
  }

  .legality summary {
    cursor: pointer;
    color: var(--text-muted);
  }

  .legal-list {
    margin: 6px 0 0;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .legal-list.banned {
    color: var(--danger);
  }

  .legal-list.restricted {
    color: #d9a441;
  }

  .add {
    border-top: 1px solid var(--border);
    padding-top: 14px;
  }

  .add-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .owned {
    font-size: 12px;
    color: var(--success);
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 9px;
    margin-bottom: 12px;
  }

  .span-2 {
    grid-column: span 2;
  }

  .add-button {
    width: 100%;
  }

  .deck-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 56px auto;
    gap: 6px;
    align-items: center;
  }

  .deck-row select {
    min-width: 0;
  }

  .deck-qty {
    text-align: center;
  }

  @media (max-width: 420px) {
    .deck-row {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    }
  }

  .hint {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.4;
  }

  /* The back button only exists when the panel covers the list. */
  .sheet-head {
    display: none;
    margin: -4px 0 8px -6px;
  }

  /* Narrow desktop and tablet: give the list the space the panel was taking. */
  @media (max-width: 1180px) {
    .panel {
      width: 312px;
    }
  }

  /* Phones: there is no room for two columns, so the detail becomes a sheet over the list.
     It is only in the layout at all once a card is selected — the "select a card" placeholder
     would otherwise cover the list it is asking you to pick from. */
  @media (max-width: 860px) {
    .panel {
      display: none;
    }

    .panel.has-card {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 40;
      width: 100%;
      border-left: none;
      padding-bottom: 32px;
    }

    .sheet-head {
      display: block;
      position: sticky;
      top: -16px;
      margin: -16px -16px 8px;
      padding: 8px 10px;
      background: var(--panel);
      border-bottom: 1px solid var(--border);
      z-index: 1;
    }

    .art {
      max-width: 320px;
      margin-inline: auto;
    }
  }

  @media (max-width: 420px) {
    .grid {
      grid-template-columns: 1fr;
    }

    .span-2 {
      grid-column: span 1;
    }
  }
</style>
