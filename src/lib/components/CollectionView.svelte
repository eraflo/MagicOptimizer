<script lang="ts">
  import * as api from "../api";
  import BackupPanel from "./BackupPanel.svelte";
  import { CONDITIONS } from "../types";
  import type { Holding, Pool, Stats } from "../types";

  let { onchanged }: { onchanged: () => void } = $props();

  let pool = $state<Pool | "all">("all");
  let holdings = $state<Holding[]>([]);
  let stats = $state<Stats | null>(null);
  let filter = $state("");
  let error = $state<string | null>(null);
  let loading = $state(true);

  const conditionLabels = Object.fromEntries(CONDITIONS.map((c) => [c.value, c.label]));

  async function load() {
    loading = true;
    error = null;
    try {
      [holdings, stats] = await Promise.all([api.collectionList(pool), api.collectionStats(pool)]);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Re-runs when `pool` changes.
    void pool;
    void load();
  });

  const visible = $derived(
    holdings
      .filter((h) => {
        const needle = filter.trim().toLowerCase();
        if (!needle) return true;
        return (
          h.name.toLowerCase().includes(needle) ||
          (h.location?.container ?? "").toLowerCase().includes(needle)
        );
      })
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  async function changeQuantity(holding: Holding, delta: number) {
    const next = holding.quantity + delta;
    try {
      await api.collectionSetQuantity(holding.id, Math.max(next, 0));
      await load();
      onchanged();
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(holding: Holding) {
    try {
      await api.collectionRemove(holding.id);
      await load();
      onchanged();
    } catch (e) {
      error = String(e);
    }
  }

  // Empty rather than a dash: on phones these become a dot-separated list, where a placeholder
  // for "nothing" is just noise. A blank cell reads fine in the desktop table too.
  function locationOf(holding: Holding): string {
    const location = holding.location;
    if (!location) return "";
    let text = location.container;
    if (location.section) text += `, ${location.section}`;
    if (location.slot != null) text += ` #${location.slot}`;
    return text;
  }
</script>

<section class="collection">
  <header>
    <div class="tabs">
      {#each [["all", "Everything"], ["physical", "Physical"], ["digital", "Digital"]] as [value, label]}
        <button
          type="button"
          class="tab"
          class:active={pool === value}
          onclick={() => (pool = value as Pool | "all")}
        >
          {label}
        </button>
      {/each}
    </div>

    {#if stats}
      <div class="stats">
        <span><strong>{stats.total_copies.toLocaleString()}</strong> cards</span>
        <span><strong>{stats.distinct_cards.toLocaleString()}</strong> different</span>
        <span><strong>{stats.holdings.toLocaleString()}</strong> stacks</span>
      </div>
    {/if}

    <input
      class="filter"
      type="search"
      placeholder="Filter by name or container"
      bind:value={filter}
    />
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p class="empty">Loading…</p>
  {:else if holdings.length === 0}
    <div class="empty">
      <p>Nothing here yet.</p>
      <p class="hint">
        Find cards in the Browse tab and add them. From phase 6 you will be able to scan them
        with the camera instead.
      </p>
    </div>
  {:else}
    <div class="table" role="table">
      <div class="head" role="row">
        <span role="columnheader">Card</span>
        <span role="columnheader">Printing</span>
        <span role="columnheader">Condition</span>
        <span role="columnheader">Location</span>
        <span role="columnheader" class="right">Copies</span>
        <span role="columnheader"></span>
      </div>

      {#each visible as holding (holding.id)}
        <div class="row" role="row">
          <span class="name" role="cell">
            {holding.name}
            {#if holding.finish !== "nonfoil"}
              <span class="tag foil">{holding.finish}</span>
            {/if}
            {#if holding.language !== "en"}
              <span class="tag">{holding.language.toUpperCase()}</span>
            {/if}
            {#if holding.pool === "digital"}
              <span class="tag digital">digital</span>
            {/if}
          </span>
          <span class="dim" role="cell">
            {holding.set_code ? `${holding.set_code.toUpperCase()} ${holding.collector_number}` : ""}
          </span>
          <span class="dim" role="cell">{conditionLabels[holding.condition] ?? holding.condition}</span>
          <span class="dim" role="cell">{locationOf(holding)}</span>
          <span class="quantity" role="cell">
            <button type="button" class="ghost step" onclick={() => changeQuantity(holding, -1)}>
              −
            </button>
            <strong>{holding.quantity}</strong>
            <button type="button" class="ghost step" onclick={() => changeQuantity(holding, 1)}>
              +
            </button>
          </span>
          <span class="actions" role="cell">
            <button type="button" class="ghost remove" onclick={() => remove(holding)}>
              Remove
            </button>
          </span>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Placed here rather than in a settings screen the app does not have: the collection is
       the largest thing a person would hate to lose, so this is where they will look for it. -->
  <BackupPanel onImported={onchanged} />
</section>

<style>
  .collection {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }

  .tabs {
    display: flex;
    gap: 4px;
  }

  .tab {
    background: transparent;
    border-color: transparent;
    color: var(--text-muted);
    padding: 5px 12px;
  }

  .tab.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--text);
  }

  .stats {
    display: flex;
    gap: 16px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .stats strong {
    color: var(--text);
  }

  .filter {
    margin-left: auto;
    max-width: 280px;
  }

  .table {
    flex: 1;
    overflow-y: auto;
  }

  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 2fr) 110px 130px minmax(0, 1.3fr) 108px 88px;
    gap: 12px;
    align-items: center;
    padding: 7px 16px;
  }

  .head {
    position: sticky;
    top: 0;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-dim);
    z-index: 1;
  }

  .row {
    border-bottom: 1px solid var(--line);
  }

  .row:hover {
    background: var(--panel);
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

  .dim {
    color: var(--text-muted);
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .right {
    text-align: right;
  }

  .quantity {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
  }

  .step {
    width: 24px;
    height: 24px;
    padding: 0;
    font-size: 15px;
    line-height: 1;
  }

  .remove {
    font-size: 12px;
    color: var(--text-dim);
  }

  .remove:hover {
    color: var(--danger);
  }

  .tag {
    font-size: 9.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--panel-raised);
    border: 1px solid var(--border-strong);
    color: var(--text-muted);
    flex: none;
  }

  /* Foil is a property of the physical card, not a state of the interface, so it reads as a
     sheen rather than as a colour. */
  .tag.foil {
    background: linear-gradient(120deg, rgba(255, 255, 255, 0.18), rgba(216, 169, 81, 0.16));
    border-color: rgba(255, 255, 255, 0.32);
    color: var(--ink);
  }

  .tag.digital {
    background: rgba(138, 122, 194, 0.16);
    border-color: rgba(138, 122, 194, 0.4);
    color: var(--mana-b);
  }

  .empty {
    padding: 56px 16px;
    text-align: center;
    color: var(--text-dim);
  }

  .empty p {
    margin: 0 0 6px;
  }

  .hint {
    font-size: 12px;
    max-width: 420px;
    margin: 0 auto;
  }

  .error {
    margin: 12px 16px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: rgba(228, 87, 61, 0.12);
    border: 1px solid rgba(228, 87, 61, 0.4);
    color: var(--danger);
  }

  @media (max-width: 1100px) {
    header {
      flex-wrap: wrap;
      gap: 10px 16px;
    }

    .filter {
      margin-left: 0;
      max-width: none;
      flex: 1 1 100%;
      order: 3;
    }
  }

  /* Phones: six columns will not fit, so each holding becomes a stacked block. A grid with
     narrower columns was tried first and just produced six unreadable slivers. */
  @media (max-width: 860px) {
    .head {
      display: none;
    }

    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 3px 10px;
      padding: 11px 14px;
    }

    /* Name, stepper and Remove share the first line; the metadata always takes the second.
       Letting the metadata share a line instead made Remove wrap on some rows and not others,
       which looked like a bug. */
    .name {
      flex: 1 1 130px;
      order: 1;
      white-space: normal;
    }

    .quantity {
      order: 2;
    }

    .actions {
      order: 3;
    }

    /* A zero-height full-width item, which forces the metadata onto its own line. Flexbox
       decides line breaks before it grows anything, so relying on the name expanding to push
       the metadata down does not work — the first metadata field gets pulled up beside it. */
    .row::after {
      content: "";
      order: 4;
      flex-basis: 100%;
      height: 0;
    }

    .dim {
      order: 5;
      font-size: 12px;
      white-space: nowrap;
    }

    .dim:empty {
      display: none;
    }

    /* Middle dots between the metadata bits, since the column headers are gone. The general
       sibling combinator with :not(:empty) is what keeps a dot from leading the line when the
       earlier fields are blank. */
    .dim:not(:empty) ~ .dim:not(:empty)::before {
      content: "·";
      margin-right: 10px;
      color: var(--text-dim);
    }
  }
</style>
