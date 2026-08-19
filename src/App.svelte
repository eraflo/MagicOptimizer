<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

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

  let tab = $state<"browse" | "decks" | "collection" | "scan" | "journal" | "data">(
    "browse",
  );
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

  // --- data artifacts -----------------------------------------------------
  let artifacts = $state<api.ArtifactStatus[]>([]);
  let downloading = $state<string | null>(null);
  let downloaded = $state(0);
  let expected = $state(0);

  $effect(() => {
    void (async () => {
      try {
        artifacts = await api.artifactsStatus();
      } catch (e) {
        error = String(e);
      }
    })();

    // Progress arrives as events, not in the reply, so a long download is visibly alive.
    const stop = listen<{ name: string; received: number; total: number }>(
      "artifact-progress",
      (event) => {
        downloaded = event.payload.received;
        expected = event.payload.total;
      },
    );
    return () => void stop.then((off) => off());
  });

  async function fetchArtifact(name: string) {
    downloading = name;
    downloaded = 0;
    expected = 0;
    error = null;
    try {
      await api.artifactsDownload(name);
      artifacts = await api.artifactsStatus();
      status = await api.catalogStatus();
      await refreshCollectionSideData();
    } catch (e) {
      error = String(e);
    } finally {
      downloading = null;
    }
  }

  async function removeArtifact(name: string) {
    error = null;
    try {
      await api.artifactsRemove(name);
      artifacts = await api.artifactsStatus();
      status = await api.catalogStatus();
    } catch (e) {
      error = String(e);
    }
  }

  const megabytes = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);

  // Quoted on the empty state so the size is known before tapping through. Read from the backend
  // list rather than written twice, because the two drifting apart is how a UI starts lying.
  const ARTIFACT_CATALOG_MB = $derived(
    artifacts.find((artifact) => artifact.required)?.megabytes ?? 26,
  );

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

  <!-- The one permanent way into the data screen, and it is deliberately the thing that already
       reports the data's state. An earlier version showed the downloader only while the catalog
       was missing, so downloading it made the panel vanish and took the two optional artifacts
       with it — they became unreachable for good. -->
  <button
    type="button"
    class="status"
    class:missing={!status?.loaded}
    aria-current={tab === "data" ? "page" : undefined}
    onclick={() => (tab = "data")}
    title={status?.loaded ? status.path : "No card data yet"}
  >
    {#if status?.loaded}
      <span class="ok">
        {status.cards.toLocaleString()} cards · Scryfall {status.sourceUpdatedAt.slice(0, 10)}
      </span>
      <span class="short">Data</span>
    {:else}
      <span class="warn">No card data</span>
    {/if}
  </button>
</header>

{#if error}
  <p class="error">{error}</p>
{/if}

<!-- Only Browse actually needs the catalog. Decks, the collection, the journal and the backup
     panel all work without it, and gating every tab on it left the whole app frozen on this
     screen — clicking a tab changed `tab` while this branch kept winning. -->
{#if tab === "data"}
  <main class="data-main">
    <div class="setup">
      <h2>Data</h2>
      <p>
        MagicOptimizer ships without card data and fetches it from the project's GitHub releases
        — static files, no account and no server. Only the first one is needed; the other two each
        unlock one feature and can wait.
      </p>
      <div class="downloads">
        {#each artifacts as artifact (artifact.name)}
          <div class="artifact">
            <div class="what">
              <strong>{artifact.label}</strong>
              <span class="dim">{artifact.about}</span>
            </div>
            {#if downloading === artifact.name}
              <span class="progress">
                {megabytes(downloaded)}{#if expected > 0} / {megabytes(expected)}{/if} MB
              </span>
            {:else if artifact.installed}
              <div class="have">
                <span class="done">Installed · {megabytes(artifact.bytes)} MB</span>
                <button
                  type="button"
                  class="ghost"
                  disabled={downloading !== null}
                  onclick={() => void removeArtifact(artifact.name)}
                >
                  Remove
                </button>
              </div>
            {:else}
              <button
                type="button"
                class:primary={artifact.required}
                disabled={downloading !== null}
                onclick={() => void fetchArtifact(artifact.name)}
              >
                Download {artifact.megabytes} MB
              </button>
            {/if}
          </div>
        {/each}
      </div>
      <p class="dim">You can also build them yourself from a checkout:</p>
      <pre>cargo run --release -p build-artifacts</pre>
      {#if status}
        <p class="dim">Stored at <code>{status.path}</code></p>
        {#if status.error}
          <p class="dim">{status.error}</p>
        {/if}
      {/if}
      <button type="button" onclick={reload}>Check again</button>
    </div>
  </main>
{:else if tab === "browse" && status && !status.loaded}
  <div class="setup">
    <h2>No card data yet</h2>
    <p>
      Browse is the one view that needs the card catalog. Everything else — decks, the collection,
      the journal and the backup panel — works without it.
    </p>
    <button type="button" class="primary" onclick={() => (tab = "data")}>
      Get card data · {ARTIFACT_CATALOG_MB} MB
    </button>
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

<!-- Bottom navigation, phone only. The top tab bar is out of thumb reach, which is the single
     biggest thing wrong with the phone build. Order is by what the device is for: the camera is
     why the app is on a phone at all, and Browse — the view a phone handles worst — goes last.
     See docs/dev/ui.md. -->
<nav class="bottom-nav" aria-label="Sections">
  {#each [["scan", "◎", "Scan"], ["collection", "▤", "Cards"], ["decks", "◈", "Decks"], ["journal", "✓", "Log"], ["browse", "⌕", "Browse"]] as [value, icon, label] (value)}
    <button
      type="button"
      class="dest"
      class:active={tab === value}
      aria-current={tab === value ? "page" : undefined}
      onclick={() => (tab = value as typeof tab)}
    >
      <span class="icon" aria-hidden="true">{icon}</span>
      {label}
    </button>
  {/each}
</nav>

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
    gap: 28px;
    padding: 12px 20px;
    /* The inset lives here, on the element that actually touches the notch. */
    padding-top: calc(12px + env(safe-area-inset-top, 0px));
    border-bottom: 1px solid rgba(244, 240, 234, 0.07);
    background: rgba(23, 21, 26, 0.82);
    backdrop-filter: blur(20px) saturate(1.2);
    -webkit-backdrop-filter: blur(20px) saturate(1.2);
    flex: none;
    z-index: 5;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 11px;
    font-size: var(--t-lede);
    font-weight: 700;
    letter-spacing: -0.015em;
  }

  .logo {
    display: inline-flex;
    gap: 2px;
  }

  .dot {
    width: 9px;
    height: 9px;
    border-radius: 999px;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.4);
  }

  .tabs {
    display: flex;
    gap: 6px;
  }

  /* Pills, as the chosen direction draws them. The active one is a pale fill with dark text —
     the interface's emphasis is light, never a hue. See docs/dev/design.md, law 6. */
  .tab {
    background: transparent;
    border-color: transparent;
    border-radius: 999px;
    color: var(--ink-2);
    font-size: var(--t-body);
    font-weight: 600;
    padding: 8px 16px;
  }

  .tab:hover:not(.active) {
    background: var(--ground-3);
    border-color: transparent;
    color: var(--ink);
  }

  .tab.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--ground);
  }

  /* A button, because it navigates. Styled as the quiet chip it used to be so the header does
     not gain a second loud control beside the tabs. */
  .status {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    font: inherit;
    font-size: 12px;
    color: inherit;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
  }

  .status:hover,
  .status[aria-current="page"] {
    background: var(--panel-raised);
    border-color: var(--border);
  }

  /* Missing data is the one state worth a colour: it is the difference between the app working
     and the app looking broken. */
  .status.missing {
    border-color: var(--danger);
  }

  /* The long form on a desktop, the word on a phone. Swapped at 640px below. */
  .short {
    display: none;
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

  /* The data screen scrolls at every width. It deliberately does not reuse `.scan-main`, which
     gives up its padding and its scrolling below 860px so the camera can own the screen. */
  .data-main {
    display: block;
    overflow-y: auto;
    padding: 16px;
  }

  /* The scan view lays itself out as a grid and needs to scroll on a phone, where the panel
     sits under the viewfinder rather than beside it. */
  .scan-main {
    display: block;
    overflow-y: auto;
    padding: 16px;
  }

  /* Hidden on a desktop, where the top tab bar is a perfectly good mouse target. */
  .bottom-nav {
    display: none;
  }

  @media (max-width: 860px) {
    .bottom-nav {
      display: flex;
      flex: none;
      background: var(--panel);
      border-top: 1px solid var(--border);
      /* On the bar itself, not only on `body`: a navigation bar under the gesture bar is a
         navigation bar that cannot be tapped. */
      padding-bottom: env(safe-area-inset-bottom, 0);
    }

    .dest {
      flex: 1;
      /* Above the 44px minimum, because these are the most-tapped controls in the app. */
      min-height: 48px;
      padding: 4px 0;
      border: 0;
      border-radius: 0;
      background: none;
      color: var(--text-dim);
      font-size: 10px;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 3px;
    }

    .dest .icon {
      font-size: 17px;
      line-height: 1;
    }

    .dest.active {
      color: var(--accent);
    }

    /* The top bar keeps the title and the data status; only the tabs move. */
    .tabs {
      display: none;
    }

    /* The camera owns the screen. */
    .scan-main {
      padding: 0;
      overflow: hidden;
    }
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
    background: rgba(12, 10, 14, 0.62);
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

    .status .short {
      display: inline;
      color: var(--text-muted);
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

  .downloads {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 4px 0 12px;
    text-align: left;
  }

  .artifact {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--panel-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }

  .artifact .what {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .artifact .what .dim {
    font-size: 12px;
  }

  .have {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .artifact button {
    min-height: 40px;
    white-space: nowrap;
  }

  .done {
    color: var(--success);
    font-size: 12px;
    white-space: nowrap;
  }

  .progress {
    color: var(--accent);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
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
