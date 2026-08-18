<script lang="ts">
  import * as api from "./lib/api";
  import CardDetail from "./lib/components/CardDetail.svelte";
  import CardList from "./lib/components/CardList.svelte";
  import CollectionView from "./lib/components/CollectionView.svelte";
  import DecksView from "./lib/components/DecksView.svelte";
  import JournalView from "./lib/components/JournalView.svelte";
  import ScanView from "./lib/components/ScanView.svelte";
  import SearchPanel from "./lib/components/SearchPanel.svelte";
  import type {
    CardDetails,
    CardSummary,
    CatalogStatus,
    SearchRequest,
    StoredDeck,
    Zone,
  } from "./lib/types";

  let tab = $state<"browse" | "decks" | "collection" | "scan" | "journal">("browse");
  /** Only has an effect below 1180px, where the filter panel is a drawer. */
  let filtersOpen = $state(false);
  let status = $state<CatalogStatus | null>(null);
  let formatList = $state<[string, string][]>([]);
  let containers = $state<string[]>([]);
  let owned = $state<Record<string, number>>({});
  let decks = $state<StoredDeck[]>([]);

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
    // allSettled rather than all: one failing lookup should not blank the other two.
    const [ownedResult, containersResult, decksResult] = await Promise.allSettled([
      api.collectionOwnedQuantities("all"),
      api.collectionContainers(),
      api.deckList(),
    ]);
    if (ownedResult.status === "fulfilled") owned = ownedResult.value;
    if (containersResult.status === "fulfilled") containers = containersResult.value;
    if (decksResult.status === "fulfilled") decks = decksResult.value;

    const failure = [ownedResult, containersResult, decksResult].find(
      (r) => r.status === "rejected",
    );
    if (failure && failure.status === "rejected") error = String(failure.reason);
  }

  // Each of these is fetched independently. Chaining them behind one try block meant a
  // failing catalog status also swallowed the format list and the collection, with nothing on
  // screen to say why — the app just came up subtly empty.
  $effect(() => {
    void (async () => {
      try {
        status = await api.catalogStatus();
      } catch (e) {
        error = String(e);
      }
      try {
        formatList = await api.formats();
      } catch (e) {
        error = String(e);
      }
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

  async function addSelectedToDeck(deckId: number, quantity: number, zone: Zone) {
    if (!selected) return;
    try {
      await api.deckAddCard(deckId, selected.oracle_id, quantity, zone);
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
    <button type="button" class="tab" class:active={tab === "decks"} onclick={() => (tab = "decks")}>
      Decks
    </button>
    <button
      type="button"
      class="tab"
      class:active={tab === "collection"}
      onclick={() => (tab = "collection")}
    >
      Collection
    </button>
    <button type="button" class="tab" class:active={tab === "scan"} onclick={() => (tab = "scan")}>
      Scan
    </button>
    <button
      type="button"
      class="tab"
      class:active={tab === "journal"}
      onclick={() => (tab = "journal")}
    >
      Journal
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
      {decks}
      onadd={addSelected}
      onaddtodeck={addSelectedToDeck}
      onclose={closeDetail}
    />
  </main>
{:else if tab === "decks"}
  <main>
    <DecksView {formatList} onchanged={refreshCollectionSideData} />
  </main>
{:else if tab === "collection"}
  <main>
    <CollectionView onchanged={refreshCollectionSideData} />
  </main>
{:else if tab === "scan"}
  <main class="scan-main">
    <ScanView {decks} {containers} onCommitted={refreshCollectionSideData} />
  </main>
{:else}
  <main class="scan-main">
    <JournalView {decks} />
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

  /* The scan view lays itself out as a grid and needs to scroll on a phone, where the panel
     sits under the viewfinder rather than beside it. */
  .scan-main {
    display: block;
    overflow-y: auto;
    padding: 16px;
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
