<script lang="ts">
  import { parseManaCost, splitCostHalves } from "../mana";

  let { cost }: { cost: string } = $props();

  const halves = $derived(splitCostHalves(cost));

  // White and green pips are light enough that dark text reads better on them; the rest need
  // light text. Deciding here rather than in CSS keeps the pip markup to one class.
  function textClass(colorVar: string): string {
    return colorVar === "--mana-u" || colorVar === "--mana-b" || colorVar === "--mana-r"
      ? "light-text"
      : "dark-text";
  }
</script>

<span class="mana-cost">
  {#each halves as half, index}
    {#if index > 0}<span class="separator">//</span>{/if}
    {#each parseManaCost(half) as symbol}
      <span
        class="pip {symbol.secondColorVar ? 'hybrid' : ''} {textClass(symbol.colorVar)}"
        style="--pip-color: var({symbol.colorVar}); --pip-color-2: var({symbol.secondColorVar ??
          symbol.colorVar})"
        title={symbol.label}
      >
        {symbol.label}
      </span>
    {/each}
  {/each}
</span>

<style>
  .separator {
    color: var(--text-dim);
    font-size: 11px;
    margin: 0 2px;
  }
</style>
