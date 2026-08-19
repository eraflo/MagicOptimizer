<script lang="ts">
  import type { SearchRequest } from "../types";

  let {
    request = $bindable(),
    formatList,
    resultCount,
    total,
    searching,
    open = false,
    onclose,
  }: {
    request: SearchRequest;
    formatList: [string, string][];
    resultCount: number;
    total: number;
    searching: boolean;
    /** Only meaningful below 1180px, where the panel is a drawer. */
    open?: boolean;
    onclose: () => void;
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

<aside class="panel" class:open>
  <div class="drawer-head">
    <strong>Filters</strong>
    <button type="button" class="ghost" onclick={onclose} aria-label="Close filters">Done</button>
  </div>

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
    width: 292px;
    flex: none;
    padding: 22px 20px 28px;
    overflow-y: auto;
    border: 1px solid rgba(244, 240, 234, 0.07);
    border-radius: 18px;
    background: rgba(32, 29, 36, 0.55);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* Uppercase and spaced. A filter panel is a stack of captions; setting them as prose is what
     made the old sidebar read like a settings dialog. */
  .field-label {
    display: block;
    font-size: var(--t-meta);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-3);
    margin-bottom: 9px;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .colors {
    display: flex;
    gap: 8px;
  }

  /* The five colours are the one place the interface is allowed to be colourful, so they get
     to be solid discs rather than outlines — this is the mana identity, not a form control. */
  .color-toggle {
    width: 40px;
    height: 40px;
    min-height: 0;
    padding: 0;
    border-radius: 999px;
    font-weight: 700;
    font-size: var(--t-meta);
    background: var(--color);
    border: 1px solid rgba(0, 0, 0, 0.4);
    color: var(--ground);
    opacity: 0.34;
    filter: saturate(0.5);
  }

  .color-toggle:hover {
    opacity: 0.68;
    filter: saturate(0.8);
    background: var(--color);
  }

  .color-toggle.active {
    opacity: 1;
    filter: none;
    box-shadow: 0 0 0 2px rgba(244, 240, 234, 0.75);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }

  .chip {
    min-height: 34px;
    padding: 0 14px;
    font-size: var(--t-meta);
    font-weight: 600;
    border-radius: 999px;
    background: rgba(244, 240, 234, 0.06);
    border: 1px solid rgba(244, 240, 234, 0.14);
    color: var(--ink-2);
  }

  .chip:hover:not(.active) {
    background: rgba(244, 240, 234, 0.13);
    color: var(--ink);
  }

  .chip.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--ground);
  }

  .toggles {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .hint {
    margin: 8px 0 0;
    font-size: var(--t-meta);
    color: var(--ink-3);
    line-height: 1.5;
  }

  .footer {
    margin-top: auto;
    padding-top: 18px;
    border-top: 1px solid rgba(244, 240, 234, 0.08);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .count {
    font-size: var(--t-meta);
    color: var(--ink-2);
    font-variant-numeric: tabular-nums;
  }

  /* The drawer header only exists when the panel is a drawer. */
  .drawer-head {
    display: none;
    align-items: center;
    justify-content: space-between;
    margin: -4px 0 -4px;
  }

  /* Below 1180px there is no room for a permanent sidebar: it slides in over the results.
     Kept mounted rather than conditionally rendered so filter state survives closing it. */
  @media (max-width: 1180px) {
    .panel {
      position: fixed;
      top: 0;
      bottom: 0;
      left: 0;
      z-index: 30;
      width: min(320px, 86vw);
      transform: translateX(-100%);
      transition: transform 180ms ease;
      box-shadow: 12px 0 32px rgba(0, 0, 0, 0.45);
      /* Hidden from assistive tech and from tab order while closed. */
      visibility: hidden;
    }

    .panel.open {
      transform: none;
      visibility: visible;
    }

    .drawer-head {
      display: flex;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .panel {
      transition: none;
    }
  }
</style>
