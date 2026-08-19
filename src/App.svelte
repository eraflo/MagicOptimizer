<script lang="ts">
  import { listen } from "@tauri-apps/api/event";

  import * as api from "./lib/api";
  import CardDetail from "./lib/components/CardDetail.svelte";
  import CardList from "./lib/components/CardList.svelte";
  import CardSheet from "./lib/components/CardSheet.svelte";
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

  /**
   * How the catalogue is drawn.
   *
   * A real switch rather than one layout trying to be both. The contact sheet shows sixteen
   * artworks where the list shows six rows and is faster to scan, because a card is recognised by
   * its painting before its name; the list wins when you need the type line, the cost and the
   * owned count side by side. Neither is a compromise on the other.
   */
  let layout = $state<"sheet" | "list">(
    (localStorage.getItem("catalogue-layout") as "sheet" | "list" | null) ?? "sheet",
  );

  $effect(() => localStorage.setItem("catalogue-layout", layout));

  /**
   * Whether the filter panel is showing.
   *
   * Three columns share the width and you rarely need all of them: filtering and reading a card
   * are separate moments. Collapsing this one hands its 292px to the results, which is four more
   * columns of artwork on the contact sheet. It persists, because it is a working preference
   * rather than a per-visit one.
   *
   * The card panel has no equivalent control on purpose — it follows the selection, which is the
   * only thing that ever decided whether it had anything to say.
   */
  let filtersShown = $state(localStorage.getItem("panel-filters") !== "off");

  $effect(() => localStorage.setItem("panel-filters", filtersShown ? "on" : "off"));
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
    // Clicking the card that is already open closes it, which is the whole control: the panel
    // appears when a card has something to say and goes away when it does not.
    if (selectedId === oracleId) {
      closeDetail();
      return;
    }
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

  /**
   * The warm glow behind everything, taking its hue from the card in view.
   *
   * This is the device that separates the chosen direction from a dark theme: the ground is not
   * flat, it is lit, and it is lit by the content. A Boros card bathes the app in amber, a Dimir
   * one in indigo. With nothing selected it stays a warm ember, because a black rectangle is
   * what made every previous attempt look like a database tool.
   */
  const ambient = $derived.by(() => {
    // Saturated, because they now sit on near-black rather than on a warm surface that was
    // already doing half the work. On #08080a a muted tint simply reads as grey.
    const tints: Record<string, [string, string]> = {
      W: ["255, 232, 160", "228, 178, 88"],
      U: ["70, 160, 255", "48, 92, 214"],
      B: ["168, 124, 232", "96, 60, 170"],
      R: ["255, 146, 58", "190, 60, 30"],
      G: ["104, 210, 102", "44, 140, 78"],
    };
    const chosen = [...(selected?.color_identity ?? "")]
      .map((c) => tints[c])
      .filter(Boolean);
    const [a, b] = chosen.length
      ? [chosen[0][0], (chosen[1] ?? chosen[0])[1]]
      : ["255, 146, 58", "190, 60, 30"];
    // Strong enough to be the room the app sits in, not a tint someone has to look for. The
    // first attempt at 0.26 was invisible beside the mockup it was copied from.
    return { one: `rgba(${a}, 0.42)`, two: `rgba(${b}, 0.3)` };
  });

  // Written to the root rather than to a wrapper: `#app` is created by `main.ts`, and the glow
  // has to sit behind every screen, not inside one of them.
  $effect(() => {
    const root = document.documentElement;
    root.style.setProperty("--amb-1", ambient.one);
    root.style.setProperty("--amb-2", ambient.two);
  });

  /** The bottom bar, in the order the device is for: the camera first, Browse last. */
  const DESTINATIONS = [
    // A viewfinder: four corner brackets round a lens.
    {
      value: "scan",
      label: "Scan",
      path: "M3 8V5.5A2.5 2.5 0 0 1 5.5 3H8M16 3h2.5A2.5 2.5 0 0 1 21 5.5V8M21 16v2.5a2.5 2.5 0 0 1-2.5 2.5H16M8 21H5.5A2.5 2.5 0 0 1 3 18.5V16M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
    },
    // A stack of cards, offset.
    {
      value: "collection",
      label: "Cards",
      path: "M8 4h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM4 8v10a2 2 0 0 0 2 2h10",
    },
    // Two cards fanned, which is what a deck looks like held.
    {
      value: "decks",
      label: "Decks",
      path: "M10 3.5h8a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2h-8a2 2 0 0 1-2-2v-11a2 2 0 0 1 2-2zM5 7l-1.4 10.3a2 2 0 0 0 1.7 2.2l7 1",
    },
    // A ruled log with a mark against one line.
    {
      value: "journal",
      label: "Log",
      path: "M6 3.5h12a1.5 1.5 0 0 1 1.5 1.5v14a1.5 1.5 0 0 1-1.5 1.5H6A1.5 1.5 0 0 1 4.5 19V5A1.5 1.5 0 0 1 6 3.5zM8 9h8M8 13h8M8 17h4",
    },
    // A magnifier.
    {
      value: "browse",
      label: "Browse",
      path: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM16.2 16.2 21 21",
    },
  ] as const;

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
      shown={filtersShown}
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
        <button
          type="button"
          class="side-toggle"
          aria-pressed={filtersShown}
          aria-label={filtersShown ? "Hide the filters" : "Show the filters"}
          title={filtersShown ? "Hide the filters" : "Show the filters"}
          onclick={() => (filtersShown = !filtersShown)}
        >
          <!-- The standard sidebar mark: a panel with one edge filled. It says which edge moves,
               which no glyph in the character set manages. -->
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <rect x="1.5" y="2.5" width="13" height="11" rx="2.5" />
            <path class="fill" d="M2 5a2.5 2.5 0 0 1 2.5-2.5H6v11H4.5A2.5 2.5 0 0 1 2 11z" />
          </svg>
        </button>

        <button type="button" class="drawer" onclick={() => (filtersOpen = true)}>
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

        <div class="layout" role="group" aria-label="Result layout">
          <button
            type="button"
            class="mode"
            class:on={layout === "sheet"}
            aria-pressed={layout === "sheet"}
            onclick={() => (layout = "sheet")}
          >
            <span aria-hidden="true">▦</span> Sheet
          </button>
          <button
            type="button"
            class="mode"
            class:on={layout === "list"}
            aria-pressed={layout === "list"}
            onclick={() => (layout = "list")}
          >
            <span aria-hidden="true">▤</span> List
          </button>
        </div>

      </div>
      {#if layout === "sheet"}
        <CardSheet cards={results} {owned} selected={selectedId} onselect={select} />
      {:else}
        <CardList cards={results} {owned} selected={selectedId} onselect={select} />
      {/if}
    </div>

    <CardDetail
      card={selected}
      ownedCount={selected ? (owned[selected.oracle_id] ?? 0) : 0}
      {containers}
      {decks}
      onadd={addSelected}
      onaddtodeck={addSelectedToDeck}
      shown={selected !== null}
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
  <!-- Drawn rather than typed. The five destinations used ◎ ▤ ◈ ✓ ⌕, which come from five
       different Unicode blocks: mismatched weights, mismatched optical sizes, and a tick mark
       standing in for a game log. One 24px grid and one stroke width fixes all of it. -->
  {#each DESTINATIONS as { value, label, path } (value)}
    <button
      type="button"
      class="dest"
      class:active={tab === value}
      aria-current={tab === value ? "page" : undefined}
      onclick={() => (tab = value as typeof tab)}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d={path} /></svg>
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
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    background: rgba(8, 8, 10, 0.86);
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
    font-size: var(--t-meta);
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

  /* Panels float on the lit ground with gaps between them, instead of butting together with a
     1px divider. That grid of hairlines is what read as "database tool" no matter the palette. */
  main {
    flex: 1;
    display: flex;
    gap: 14px;
    padding: 14px;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  /* The scan and data screens own their whole surface. */
  main.scan-main,
  main.data-main {
    gap: 0;
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
      min-height: 52px;
      padding: 7px 0 5px;
      border: 0;
      border-radius: 0;
      background: none;
      color: var(--ink-3);
      font-size: 11.5px;
      font-weight: 600;
      letter-spacing: 0.01em;
      box-shadow: none;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 5px;
    }

    .dest svg {
      width: 22px;
      height: 22px;
      fill: none;
      stroke: currentColor;
      stroke-width: 1.6;
      stroke-linecap: round;
      stroke-linejoin: round;
      transition: transform 160ms ease;
    }

    .dest:hover:not(.active) {
      background: none;
      color: var(--ink-2);
    }

    /* Ink, not a hue — the chassis carries no colour. The mark lifts a little and thickens,
       which reads as "you are here" without borrowing a second accent. */
    .dest.active {
      color: var(--ink);
    }

    .dest.active svg {
      stroke-width: 2;
      transform: translateY(-1px);
    }

    @media (prefers-reduced-motion: reduce) {
      .dest svg {
        transition: none;
      }
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

  /* A card on the stage like the other two, not the leftover space between them. */
  .results {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 18px;
    background: rgba(20, 20, 24, 0.74);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-shadow: var(--sheen), var(--lift);
    overflow: hidden;
    min-width: 0;
  }

  /* Shown only once the filter panel becomes a drawer. */
  /* Permanent now: it carries the layout switch, which has to be reachable at every width. Only
     the Filters button inside it is narrow-only. */
  .compact-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
    flex: none;
  }

  /* The drawer button only exists where the filter panel is a drawer. */
  .compact-bar > button.drawer {
    display: none;
  }

  /* The collapse toggles only exist where there is a panel to collapse — below their
     breakpoints the panels are already a drawer and a full-screen sheet. */
  /* Square, quiet, icon only. A labelled pill beside the layout switch made the bar read as
     three competing controls; this one is a utility and should sit back. */
  .side-toggle {
    display: none;
    align-items: center;
    justify-content: center;
    width: 30px;
    min-height: 30px;
    padding: 0;
    border-radius: 9px;
    border-color: transparent;
    background: transparent;
    color: var(--ink-3);
    box-shadow: none;
  }

  .side-toggle:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    border-color: transparent;
    color: var(--ink);
  }

  .side-toggle[aria-pressed="true"] {
    color: var(--ink);
  }

  .side-toggle svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }

  /* The filled edge is what distinguishes "panel showing" from "panel hidden". */
  .side-toggle svg .fill {
    stroke: none;
    fill: currentColor;
    opacity: 0;
    transition: opacity 140ms ease;
  }

  .side-toggle[aria-pressed="true"] svg .fill {
    opacity: 1;
  }

  .compact-count {
    margin-right: auto;
    min-width: 0;
    /* Never wrap. On a phone "No matches" broke across two lines and took the whole bar with
       it, so a row of controls became a two-storey block. */
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .layout {
    display: flex;
    gap: 4px;
    padding: 3px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.05);
  }

  .mode {
    min-height: 30px;
    padding: 0 13px;
    border: none;
    background: transparent;
    border-radius: 999px;
    color: var(--ink-3);
    font-size: var(--t-meta);
    font-weight: 600;
    box-shadow: none;
  }

  .mode:hover:not(.on) {
    background: transparent;
    color: var(--ink-2);
  }

  .mode.on {
    background: var(--accent);
    color: var(--ground);
  }

  .compact-count {
    font-size: var(--t-meta);
    color: var(--text-muted);
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 29;
    background: rgba(0, 0, 0, 0.7);
    border: none;
    border-radius: 0;
    padding: 0;
    cursor: default;
  }

  @media (min-width: 1181px) {
    .side-toggle {
      display: inline-flex;
    }
  }

  /* Phones: the layout switch keeps its marks and drops its words. Three labelled controls do
     not fit across 375px, and the two marks are already the clearer half. */
  @media (max-width: 640px) {
    .mode {
      font-size: 0;
      padding: 0 14px;
    }

    .mode span {
      font-size: var(--t-lede);
    }
  }

  /* Below 1180px the filter panel becomes a drawer, so the way back into it appears here. */
  @media (max-width: 1180px) {
    .compact-bar > button.drawer {
      display: inline-flex;
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

    /* The name stays. Hiding it left the bar holding five dots and a status pill with a gulf
       between them — the app's own header stopped saying which app it was. What gives way at
       this width is the gap, not the identity. */
    .brand strong {
      font-size: var(--t-body);
    }
  }

  /* A card that floats, not a band painted across the window. A full-bleed red strip under
     the app bar is the single least premium thing an interface can do. */
  .error {
    margin: 14px 14px -4px;
    padding: 12px 16px;
    border-radius: 14px;
    background: rgba(228, 87, 61, 0.14);
    border: 1px solid rgba(228, 87, 61, 0.34);
    box-shadow: var(--sheen), var(--lift);
    color: #f0a08c;
    font-size: var(--t-body);
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
    font-size: var(--t-meta);
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
    font-size: var(--t-meta);
    white-space: nowrap;
  }

  .progress {
    color: var(--accent);
    font-size: var(--t-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  /* A card like every other screen, and left-aligned. Centred prose on a bare ground was the
     one place left that still looked like a holding page rather than part of the app. */
  .setup {
    margin: 8px auto;
    max-width: 620px;
    padding: 30px 32px 32px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 18px;
    background: rgba(20, 20, 24, 0.8);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-shadow: var(--sheen), var(--lift);
    text-align: left;
  }

  .setup h2 {
    margin: 0 0 12px;
    font-size: var(--t-head);
    font-weight: 700;
    letter-spacing: -0.018em;
  }

  .setup p {
    margin: 0 0 14px;
    color: var(--ink-2);
    max-width: 62ch;
  }

  .setup pre {
    margin: 6px 0 18px;
    padding: 12px 14px;
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.45);
    border: 1px solid rgba(255, 255, 255, 0.07);
    color: var(--ink-2);
    font-size: var(--t-meta);
    overflow-x: auto;
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
    font-size: var(--t-meta);
    color: var(--text-dim);
  }

  code {
    font-size: var(--t-meta);
  }
</style>
