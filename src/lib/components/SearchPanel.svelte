<script lang="ts">
  import type { SearchRequest } from "../types";

  let {
    request = $bindable(),
    formatList,
    resultCount,
    total,
    searching,
  }: {
    request: SearchRequest;
    formatList: [string, string][];
    resultCount: number;
    total: number;
    searching: boolean;
  } = $props();

  const COLORS = [
    { symbol: "W", var: "--mana-w", label: "White" },
    { symbol: "U", var: "--mana-u", label: "Blue" },
    { symbol: "B", var: "--mana-b", label: "Black" },
    { symbol: "R", var: "--mana-r", label: "Red" },
    { symbol: "G", var: "--mana-g", label: "Green" },
  ];

  const TYPES = [
    "Creature",
    "Instant",
    "Sorcery",
    "Artifact",
    "Enchantment",
    "Planeswalker",
    "Land",
    "Battle",
    "Legendary",
  ];

  function toggleColor(symbol: string) {
    const current = request.identity ?? "";
    request.identity = current.includes(symbol)
      ? current.replace(symbol, "")
      : current + symbol;
  }

  function toggleType(kind: string) {
    const current = request.cardTypes ?? [];
    request.cardTypes = current.includes(kind)
      ? current.filter((t) => t !== kind)
      : [...current, kind];
  }

  function reset() {
    request.text = "";
    request.cardTypes = [];
    request.identity = "";
    request.format = "";
    request.gameChangersOnly = false;
    request.commandersOnly = false;
    request.ownedOnly = false;
  }
</script>

<aside class="panel">
  <div class="field">
    <label for="search-text">Name or rules text</label>
    <input
      id="search-text"
      type="search"
      placeholder="draw a card"
      bind:value={request.text}
      autocomplete="off"
    />
  </div>

  <div class="field">
    <span class="field-label">Colour identity</span>
    <div class="colors">
      {#each COLORS as color}
        <button
          type="button"
          class="color-toggle"
          class:active={(request.identity ?? "").includes(color.symbol)}
          style="--color: var({color.var})"
          onclick={() => toggleColor(color.symbol)}
          title={color.label}
          aria-pressed={(request.identity ?? "").includes(color.symbol)}
        >
          {color.symbol}
        </button>
      {/each}
    </div>
    <p class="hint">
      Cards playable under a commander of these colours. Colourless cards always qualify.
    </p>
  </div>

  <div class="field">
    <span class="field-label">Card type</span>
    <div class="chips">
      {#each TYPES as kind}
        <button
          type="button"
          class="chip"
          class:active={(request.cardTypes ?? []).includes(kind)}
          onclick={() => toggleType(kind)}
          aria-pressed={(request.cardTypes ?? []).includes(kind)}
        >
          {kind}
        </button>
      {/each}
    </div>
  </div>

  <div class="field">
    <label for="search-format">Legal in</label>
    <select id="search-format" bind:value={request.format}>
      <option value="">Any format</option>
      {#each formatList as [key, name]}
        <option value={key}>{name}</option>
      {/each}
    </select>
  </div>

  <div class="field row">
    <div>
      <label for="mv-min">Mana value from</label>
      <input id="mv-min" type="number" min="0" step="1" bind:value={request.minManaValue} />
    </div>
    <div>
      <label for="mv-max">to</label>
      <input id="mv-max" type="number" min="0" step="1" bind:value={request.maxManaValue} />
    </div>
  </div>

  <div class="field toggles">
    <label class="checkbox">
      <input type="checkbox" bind:checked={request.ownedOnly} />
      Only cards I own
    </label>
    <label class="checkbox">
      <input type="checkbox" bind:checked={request.commandersOnly} />
      Can be a commander
    </label>
    <label class="checkbox">
      <input type="checkbox" bind:checked={request.gameChangersOnly} />
      Game Changers only
    </label>
  </div>

  <div class="footer">
    <span class="count">
      {#if searching}
        Searching…
      {:else if total === 0}
        No matches
      {:else if resultCount < total}
        Showing {resultCount} of {total.toLocaleString()}
      {:else}
        {total.toLocaleString()} {total === 1 ? "card" : "cards"}
      {/if}
    </span>
    <button type="button" class="ghost" onclick={reset}>Reset</button>
  </div>
</aside>

<style>
  .panel {
    width: 268px;
    flex: none;
    padding: 16px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--panel);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .field-label {
    display: block;
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 6px;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .colors {
    display: flex;
    gap: 6px;
  }

  .color-toggle {
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 999px;
    font-weight: 700;
    font-size: 12px;
    background: transparent;
    border: 2px solid var(--color);
    color: var(--color);
    opacity: 0.55;
  }

  .color-toggle:hover {
    opacity: 0.85;
    background: transparent;
  }

  .color-toggle.active {
    background: var(--color);
    color: #10131b;
    opacity: 1;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .chip {
    padding: 3px 9px;
    font-size: 12px;
    border-radius: 999px;
    background: transparent;
    border: 1px solid var(--border-strong);
    color: var(--text-muted);
  }

  .chip.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--text);
  }

  .toggles {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .hint {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.4;
  }

  .footer {
    margin-top: auto;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .count {
    font-size: 12px;
    color: var(--text-muted);
  }
</style>
