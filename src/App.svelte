<script lang="ts">
  import * as api from "./lib/api";
  import CardDetail from "./lib/components/CardDetail.svelte";
  import CardList from "./lib/components/CardList.svelte";
  import CollectionView from "./lib/components/CollectionView.svelte";
  import SearchPanel from "./lib/components/SearchPanel.svelte";
  import type { CardDetails, CardSummary, CatalogStatus, SearchRequest } from "./lib/types";

  let tab = $state<"browse" | "collection">("browse");
  /** Only has an effect below 1180px, where the filter panel is a drawer. */
  let filtersOpen = $state(false);
  let status = $state<CatalogStatus | null>(null);
  let formatList = $state<[string, string][]>([]);
  let containers = $state<string[]>([]);
  let owned = $state<Record<string, number>>({});

  let request = $state<SearchRequest>({
    text: "",
    cardTypes: [],
    identity: "",
    format: "",
    gameChangersOnly: false,
    commandersOnly: false,
    ownedOnly: false,
  });

  let results = $state<CardSummary[]>([]);
  let total = $state(0);
  let searching = $state(false);
  let selectedId = $state<string | null>(null);
  let selected = $state<CardDetails | null>(null);
  let error = $state<string | null>(null);

  async function refreshCollectionSideData() {
    try {
      [owned, containers] = await Promise.all([
        api.collectionOwnedQuantities("all"),
        api.collectionContainers(),
      ]);
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    void (async () => {
      status = await api.catalogStatus();
      formatList = await api.formats();
      await refreshCollectionSideData();
    })();
  });

  // Debounced so typing does not fire a scan of the whole catalog per keystroke. The scan
  // itself is ~5 ms; the debounce is about not thrashing the IPC bridge.
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const snapshot = JSON.stringify(request);
    if (!status?.loaded) return;

    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      void runSearch(JSON.parse(snapshot) as SearchRequest);
    }, 180);
  });

  async function runSearch(current: SearchRequest) {
    searching = true;
    error = null;
    try {
      const response = await api.searchCards(current);
      results = response.cards;
      total = response.total;
    } catch (e) {
      error = String(e);
      results = [];
      total = 0;
    } finally {
      searching = false;
    }
  }

  // Counted so the drawer button can say how many filters are hidden behind it — otherwise a
  // narrow window can silently filter results with nothing on screen explaining why.
  const activeFilterCount = $derived(
    [
      (request.text ?? "").trim() !== "",
      (request.cardTypes ?? []).length > 0,
      (request.identity ?? "") !== "",
      (request.format ?? "") !== "",
      request.minManaValue != null,
      request.maxManaValue != null,
      request.gameChangersOnly === true,
      request.commandersOnly === true,
      request.ownedOnly === true,
    ].filter(Boolean).length,
  );

  async function select(oracleId: string) {
    selectedId = oracleId;
    try {
      selected = await api.cardDetails(oracleId);
    } catch (e) {
      error = String(e);
      selected = null;
    }
  }

  function closeDetail() {
    selectedId = null;
    selected = null;
  }

  async function addSelected(options: {
    pool: "physical" | "digital";
    quantity: number;
    finish: "nonfoil" | "foil" | "etched";
    condition:
      | "near_mint"
      | "lightly_played"
      | "moderately_played"
      | "heavily_played"
      | "damaged";
    language: string;
    container: string;
    section: string;
  }) {
    if (!selected) return;
    try {
      await api.collectionAdd({
        pool: options.pool,
        oracle_id: selected.oracle_id,
        name: selected.name,
        set_code: selected.set_code,
        collector_number: selected.collector_number,
        language: options.language,
        finish: options.finish,
        condition: options.condition,
        quantity: options.quantity,
        location: options.container.trim()
          ? {
              container: options.container.trim(),
              section: options.section.trim() || null,
              slot: null,
            }
          : null,
        notes: "",
      });
      await refreshCollectionSideData();
    } catch (e) {
      error = String(e);
    }
  }

  async function reload() {
    status = await api.reloadCatalog();
  }
</script>

<header class="app-bar">
  <div class="brand">
    <span class="logo" aria-hidden="true">
      {#each ["--mana-w", "--mana-u", "--mana-b", "--mana-r", "--mana-g"] as color}
        <span class="dot" style="background: var({color})"></span>
      {/each}
    </span>
    <strong>MagicOptimizer</strong>
  </div>

  <nav class="tabs">
    <button type="button" class="tab" class:active={tab === "browse"} onclick={() => (tab = "browse")}>
      Browse
    </button>
    <button
      type="button"
      class="tab"
      class:active={tab === "collection"}
      onclick={() => (tab = "collection")}
    >
      Collection
    </button>
  </nav>

  <div class="status">
    {#if status?.loaded}
      <span class="ok" title={status.path}>
        {status.cards.toLocaleString()} cards · Scryfall {status.sourceUpdatedAt.slice(0, 10)}
      </span>
    {:else}
      <span class="warn">No card data</span>
      <button type="button" class="ghost" onclick={reload}>Reload</button>
    {/if}
  </div>
</header>

{#if error}
  <p class="error">{error}</p>
{/if}

{#if status && !status.loaded}
  <div class="setup">
    <h2>No card data yet</h2>
    <p>
      MagicOptimizer ships without card data and downloads it separately. Until the in-app
      downloader lands, build the catalog from a checkout:
    </p>
    <pre>cargo run --release -p build-artifacts</pre>
    <p class="dim">Expected at <code>{status.path}</code></p>
    {#if status.error}
      <p class="dim">{status.error}</p>
    {/if}
    <button type="button" class="primary" onclick={reload}>Check again</button>
  </div>
{:else if tab === "browse"}
  <main>
    <SearchPanel
      bind:request
      {formatList}
      resultCount={results.length}
      {total}
      {searching}
      open={filtersOpen}
      onclose={() => (filtersOpen = false)}
    />

    {#if filtersOpen}
      <button
        type="button"
        class="backdrop"
        aria-label="Close filters"
        onclick={() => (filtersOpen = false)}
      ></button>
    {/if}

    <div class="results">
      <div class="compact-bar">
        <button type="button" onclick={() => (filtersOpen = true)}>
          Filters{activeFilterCount > 0 ? ` (${activeFilterCount})` : ""}
        </button>
        <span class="compact-count">
          {#if searching}
            Searching…
          {:else if total === 0}
            No matches
          {:else if results.length < total}
            {results.length} of {total.toLocaleString()}
          {:else}
            {total.toLocaleString()} {total === 1 ? "card" : "cards"}
          {/if}
        </span>
      </div>
      <CardList cards={results} {owned} selected={selectedId} onselect={select} />
    </div>

    <CardDetail
      card={selected}
      ownedCount={selected ? (owned[selected.oracle_id] ?? 0) : 0}
      {containers}
      onadd={addSelected}
      onclose={closeDetail}
    />
  </main>
{:else}
  <main>
    <CollectionView onchanged={refreshCollectionSideData} />
  </main>
{/if}

<svelte:window
  onkeydown={(event) => {
    if (event.key !== "Escape") return;
    if (filtersOpen) filtersOpen = false;
    else if (selected) closeDetail();
  }}
/>

<style>
  .app-bar {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 9px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    flex: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 14px;
  }

  .logo {
    display: inline-flex;
    gap: 2px;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
  }

  .tabs {
    display: flex;
    gap: 4px;
  }

  .tab {
    background: transparent;
    border-color: transparent;
    color: var(--text-muted);
    padding: 5px 14px;
  }

  .tab.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--text);
  }

  .status {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  .ok {
    color: var(--text-muted);
  }

  .warn {
    color: var(--danger);
  }

  main {
    flex: 1;
    display: flex;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  .results {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /* Shown only once the filter panel becomes a drawer. */
  .compact-bar {
    display: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }

  .compact-count {
    font-size: 12px;
    color: var(--text-muted);
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 29;
    background: rgba(6, 8, 13, 0.6);
    border: none;
    border-radius: 0;
    padding: 0;
    cursor: default;
  }

  @media (max-width: 1180px) {
    .compact-bar {
      display: flex;
    }
  }

  /* Phones: the header has to give up something. The catalog status goes, since the tabs and
     the brand are what you navigate with. */
  @media (max-width: 640px) {
    .app-bar {
      gap: 12px;
      padding: 8px 12px;
    }

    .status .ok {
      display: none;
    }

    .brand strong {
      display: none;
    }
  }

  .error {
    margin: 0;
    padding: 8px 16px;
    background: rgba(228, 87, 61, 0.12);
    border-bottom: 1px solid rgba(228, 87, 61, 0.4);
    color: var(--danger);
    font-size: 13px;
  }

  .setup {
    margin: 64px auto;
    max-width: 520px;
    padding: 0 24px;
    text-align: center;
  }

  .setup h2 {
    margin: 0 0 12px;
    font-size: 18px;
  }

  .setup p {
    color: var(--text-muted);
    line-height: 1.6;
  }

  .setup pre {
    background: var(--panel);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    padding: 12px 14px;
    text-align: left;
    overflow-x: auto;
    font-size: 13px;
  }

  .dim {
    font-size: 12px;
    color: var(--text-dim);
  }

  code {
    font-size: 12px;
  }
</style>
