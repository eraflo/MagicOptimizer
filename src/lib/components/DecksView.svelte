<script lang="ts">
  import * as api from "../api";
  import { describeViolation } from "../types";
  import type { DeckView, ExportStyle, StoredDeck, Zone } from "../types";
  import ManaCurve from "./ManaCurve.svelte";

  let {
    formatList,
    onchanged,
  }: { formatList: [string, string][]; onchanged: () => void } = $props();

  let decks = $state<StoredDeck[]>([]);
  let open = $state<DeckView | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  // Creating and importing share the panel; `mode` decides which is showing.
  let mode = $state<"none" | "create" | "import">("none");
  let draftName = $state("");
  let draftFormat = $state("commander");
  let draftText = $state("");
  let importMessages = $state<string[]>([]);
  let exported = $state<string | null>(null);

  const ZONES: { zone: Zone; label: string }[] = [
    { zone: "command", label: "Commander" },
    { zone: "main", label: "Deck" },
    { zone: "sideboard", label: "Sideboard" },
  ];

  async function refresh() {
    try {
      decks = await api.deckList();
      // Browse offers a deck picker built from the same list, so it has to hear about
      // creations and deletions.
      onchanged();
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    void refresh();
  });

  function formatName(key: string): string {
    return formatList.find(([k]) => k === key)?.[1] ?? key;
  }

  async function run<T>(action: () => Promise<T>): Promise<T | null> {
    busy = true;
    error = null;
    try {
      return await action();
    } catch (e) {
      error = String(e);
      return null;
    } finally {
      busy = false;
    }
  }

  async function openDeck(id: number) {
    exported = null;
    importMessages = [];
    const view = await run(() => api.deckGet(id));
    if (view) open = view;
  }

  async function create() {
    if (!draftName.trim()) return;
    const id = await run(() => api.deckCreate(draftName.trim(), draftFormat));
    if (id === null) return;
    mode = "none";
    draftName = "";
    await refresh();
    await openDeck(id);
  }

  async function importList() {
    if (!draftText.trim()) return;
    const outcome = await run(() =>
      api.deckImport(draftText, draftName.trim() || "Imported deck", draftFormat),
    );
    if (!outcome) return;
    mode = "none";
    draftText = "";
    draftName = "";
    open = outcome.view;
    importMessages = outcome.messages;
    await refresh();
  }

  async function remove(id: number) {
    await run(() => api.deckDelete(id));
    if (open?.id === id) open = null;
    await refresh();
  }

  async function changeQuantity(oracleId: string, zone: Zone, delta: number) {
    if (!open) return;
    const id = open.id;
    const view = await run(() =>
      delta > 0
        ? api.deckAddCard(id, oracleId, delta, zone)
        : api.deckRemoveCard(id, oracleId, -delta, zone),
    );
    if (view) open = view;
    await refresh();
  }

  async function moveTo(oracleId: string, from: Zone, to: Zone) {
    if (!open) return;
    const view = await run(() => api.deckMoveCard(open!.id, oracleId, 1, from, to));
    if (view) open = view;
  }

  async function rename(name: string, format: string) {
    if (!open) return;
    const view = await run(() => api.deckRename(open!.id, name, format));
    if (view) open = view;
    await refresh();
  }

  async function exportAs(style: ExportStyle) {
    if (!open) return;
    exported = await run(() => api.deckExport(open!.id, style));
  }

  function entriesIn(view: DeckView, zone: Zone) {
    return view.deck.entries
      .filter((e) => e.zone === zone)
      .sort((a, b) => a.name.localeCompare(b.name));
  }
</script>

<section class="decks">
  <aside class="list">
    <header>
      <h3>Decks</h3>
      <div class="actions">
        <button type="button" onclick={() => (mode = mode === "create" ? "none" : "create")}>
          New
        </button>
        <button type="button" onclick={() => (mode = mode === "import" ? "none" : "import")}>
          Import
        </button>
      </div>
    </header>

    {#if mode !== "none"}
      <div class="draft">
        <label for="draft-name">Name</label>
        <input
          id="draft-name"
          bind:value={draftName}
          placeholder={mode === "import" ? "Imported deck" : "Krenko goblins"}
        />

        <label for="draft-format">Format</label>
        <select id="draft-format" bind:value={draftFormat}>
          {#each formatList as [key, name]}
            <option value={key}>{name}</option>
          {/each}
        </select>

        {#if mode === "import"}
          <label for="draft-text">Decklist</label>
          <textarea
            id="draft-text"
            rows="9"
            bind:value={draftText}
            placeholder={"Commander\n1 Krenko, Mob Boss\n\nDeck\n99 Mountain"}
          ></textarea>
          <p class="hint">
            Paste from Arena, Moxfield, MTGO or plain text — all four are understood. Lines that
            cannot be read are reported rather than dropped.
          </p>
          <button type="button" class="primary" onclick={importList} disabled={busy}>
            Import
          </button>
        {:else}
          <button type="button" class="primary" onclick={create} disabled={busy}>Create</button>
        {/if}
      </div>
    {/if}

    <div class="rows">
      {#each decks as deck (deck.id)}
        <button
          type="button"
          class="row"
          class:active={open?.id === deck.id}
          onclick={() => openDeck(deck.id)}
        >
          <span class="deck-name">{deck.name}</span>
          <span class="deck-format">{formatName(deck.format)}</span>
        </button>
      {:else}
        <p class="empty">No decks yet.</p>
      {/each}
    </div>
  </aside>

  <div class="editor">
    {#if error}
      <p class="error">{error}</p>
    {/if}

    {#if !open}
      <p class="placeholder">Select a deck, or create one.</p>
    {:else}
      <header class="deck-head">
        <input
          class="title"
          value={open.deck.name}
          onchange={(event) => rename(event.currentTarget.value, open!.deck.format)}
        />
        <select
          value={open.deck.format}
          onchange={(event) => rename(open!.deck.name, event.currentTarget.value)}
        >
          {#each formatList as [key, name]}
            <option value={key}>{name}</option>
          {/each}
        </select>
        <button type="button" class="ghost danger" onclick={() => remove(open!.id)}>Delete</button>
      </header>

      {#if importMessages.length}
        <div class="problems">
          <strong>{importMessages.length} line(s) could not be imported</strong>
          <ul>
            {#each importMessages as message}<li>{message}</li>{/each}
          </ul>
        </div>
      {/if}

      <div class="columns">
        <div class="zones">
          {#each ZONES as { zone, label }}
            {@const entries = entriesIn(open, zone)}
            {#if entries.length || zone === "main"}
              <section class="zone">
                <h4>
                  {label}
                  <span class="zone-count">
                    {entries.reduce((sum, e) => sum + e.quantity, 0)}
                  </span>
                </h4>
                {#each entries as entry (entry.oracle_id)}
                  <div class="entry">
                    <span class="qty">{entry.quantity}</span>
                    <span class="entry-name">{entry.name}</span>
                    <span class="entry-actions">
                      <button
                        type="button"
                        class="ghost step"
                        onclick={() => changeQuantity(entry.oracle_id, zone, -1)}
                        aria-label="Remove one"
                      >
                        −
                      </button>
                      <button
                        type="button"
                        class="ghost step"
                        onclick={() => changeQuantity(entry.oracle_id, zone, 1)}
                        aria-label="Add one"
                      >
                        +
                      </button>
                      {#if zone === "main"}
                        <button
                          type="button"
                          class="ghost move"
                          title="Move to the command zone"
                          onclick={() => moveTo(entry.oracle_id, "main", "command")}
                        >
                          ⌂
                        </button>
                      {:else if zone === "command"}
                        <button
                          type="button"
                          class="ghost move"
                          title="Move back to the deck"
                          onclick={() => moveTo(entry.oracle_id, "command", "main")}
                        >
                          ↓
                        </button>
                      {/if}
                    </span>
                  </div>
                {:else}
                  <p class="empty-zone">
                    Nothing here yet. Add cards from the Browse tab.
                  </p>
                {/each}
              </section>
            {/if}
          {/each}
        </div>

        <aside class="side">
          <section class="legality" class:ok={open.legality.violations.length === 0}>
            <h4>
              {#if open.legality.violations.length === 0}
                Legal in {formatName(open.legality.format)}
              {:else}
                {open.legality.violations.length} problem(s)
              {/if}
            </h4>

            <p class="counts">
              {open.legality.main_count} deck
              {#if open.legality.command_count}· {open.legality.command_count} command{/if}
              {#if open.legality.sideboard_count}· {open.legality.sideboard_count} side{/if}
              {#if open.legality.commander_identity}
                · identity {open.legality.commander_identity}
              {/if}
            </p>

            {#if open.legality.approximate_rules}
              <p class="approximate">
                This format's construction rules are inferred rather than confirmed, so treat
                this verdict as a hint.
              </p>
            {/if}

            {#if open.legality.violations.length}
              <ul>
                {#each open.legality.violations as violation}
                  <li>{describeViolation(violation)}</li>
                {/each}
              </ul>
            {/if}
          </section>

          <section class="numbers">
            <span><strong>{open.stats.total_cards}</strong> cards</span>
            <span><strong>{open.stats.lands}</strong> lands</span>
            <span><strong>{open.stats.creatures}</strong> creatures</span>
          </section>

          {#if open.stats.unresolved_cards > 0}
            <p class="approximate">
              {open.stats.unresolved_cards} card(s) are not in the loaded card data, so these
              numbers are incomplete.
            </p>
          {/if}

          <ManaCurve
            curve={open.stats.curve}
            colorPips={open.stats.color_pips}
            averageManaValue={open.stats.average_mana_value}
          />

          <section class="export">
            <h4>Export</h4>
            <div class="export-buttons">
              <button type="button" onclick={() => exportAs("plain")}>Plain</button>
              <button type="button" onclick={() => exportAs("arena")}>Arena</button>
              <button type="button" onclick={() => exportAs("mtgo")}>MTGO</button>
            </div>
            {#if exported !== null}
              <textarea class="exported" readonly rows="8" value={exported}></textarea>
            {/if}
          </section>
        </aside>
      </div>
    {/if}
  </div>
</section>

<style>
  .decks {
    flex: 1;
    display: flex;
    min-width: 0;
    overflow: hidden;
  }

  .list {
    width: 264px;
    flex: none;
    border-right: 1px solid var(--border);
    background: var(--panel);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .list header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }

  h3 {
    margin: 0;
    font-size: 13px;
  }

  h4 {
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    gap: 5px;
  }

  .actions button {
    font-size: 12px;
    padding: 4px 9px;
  }

  .draft {
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }

  .draft button {
    width: 100%;
    margin-top: 10px;
  }

  .draft textarea {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12px;
    resize: vertical;
  }

  .rows {
    flex: 1;
  }

  .row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    width: 100%;
    text-align: left;
    padding: 9px 14px;
    background: transparent;
    border: none;
    border-bottom: 1px solid rgba(38, 44, 59, 0.6);
    border-radius: 0;
  }

  .row:hover {
    background: var(--panel-raised);
  }

  .row.active {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .deck-format {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .editor {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 16px;
  }

  .deck-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }

  .title {
    font-size: 16px;
    font-weight: 600;
    flex: 1;
  }

  .deck-head select {
    width: auto;
    min-width: 160px;
  }

  .columns {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 20px;
    align-items: start;
  }

  .zone {
    margin-bottom: 18px;
  }

  .zone-count {
    color: var(--text-dim);
    font-weight: 400;
    margin-left: 4px;
  }

  .entry {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
    padding: 4px 0;
    border-bottom: 1px solid rgba(38, 44, 59, 0.5);
  }

  .qty {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .entry-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry-actions {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 100ms;
  }

  .entry:hover .entry-actions,
  .entry:focus-within .entry-actions {
    opacity: 1;
  }

  .step,
  .move {
    width: 24px;
    height: 24px;
    padding: 0;
    font-size: 14px;
    line-height: 1;
  }

  .side {
    position: sticky;
    top: 0;
  }

  .legality {
    padding: 10px 12px;
    border-radius: var(--radius);
    background: rgba(228, 87, 61, 0.1);
    border: 1px solid rgba(228, 87, 61, 0.35);
    margin-bottom: 14px;
  }

  .legality.ok {
    background: rgba(67, 170, 106, 0.1);
    border-color: rgba(67, 170, 106, 0.35);
  }

  .legality h4 {
    color: var(--danger);
    margin-bottom: 4px;
  }

  .legality.ok h4 {
    color: var(--success);
  }

  .legality ul,
  .problems ul {
    margin: 8px 0 0;
    padding-left: 18px;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .counts {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  .approximate {
    margin: 8px 0 0;
    font-size: 11.5px;
    color: #d9a441;
    line-height: 1.45;
  }

  .numbers {
    display: flex;
    gap: 14px;
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 14px;
  }

  .numbers strong {
    color: var(--text);
  }

  .problems {
    padding: 10px 12px;
    border-radius: var(--radius);
    background: rgba(217, 164, 65, 0.1);
    border: 1px solid rgba(217, 164, 65, 0.35);
    margin-bottom: 14px;
    font-size: 13px;
  }

  .export {
    border-top: 1px solid var(--border);
    padding-top: 12px;
    margin-top: 14px;
  }

  .export-buttons {
    display: flex;
    gap: 6px;
  }

  .exported {
    margin-top: 10px;
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 11.5px;
    resize: vertical;
  }

  .danger:hover {
    color: var(--danger);
  }

  .placeholder,
  .empty {
    color: var(--text-dim);
    text-align: center;
    padding: 40px 16px;
  }

  .empty-zone {
    color: var(--text-dim);
    font-size: 12px;
    margin: 4px 0;
  }

  .hint {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.45;
  }

  .error {
    margin: 0 0 12px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: rgba(228, 87, 61, 0.12);
    border: 1px solid rgba(228, 87, 61, 0.4);
    color: var(--danger);
  }

  @media (max-width: 1180px) {
    .columns {
      grid-template-columns: minmax(0, 1fr);
    }

    .side {
      position: static;
    }
  }

  @media (max-width: 860px) {
    .decks {
      flex-direction: column;
    }

    .list {
      width: 100%;
      border-right: none;
      border-bottom: 1px solid var(--border);
      max-height: 45%;
    }

    .entry-actions {
      opacity: 1;
    }
  }
</style>
