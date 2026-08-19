<script lang="ts">
  import type { ColorPips, CurveBucket } from "../types";

  let {
    curve,
    colorPips,
    averageManaValue,
  }: {
    curve: CurveBucket[];
    colorPips: ColorPips[];
    averageManaValue: number;
  } = $props();

  // Scaled against the tallest column rather than the deck size: the point is the shape, and
  // against 100 cards every bar would be a sliver.
  const tallest = $derived(Math.max(1, ...curve.map((b) => b.count)));

  const COLOR_VARS: Record<string, string> = {
    W: "--mana-w",
    U: "--mana-u",
    B: "--mana-b",
    R: "--mana-r",
    G: "--mana-g",
  };
</script>

<section class="curve">
  <header>
    <h4>Mana curve</h4>
    <span class="average">avg {averageManaValue.toFixed(2)}</span>
  </header>

  <div class="bars">
    {#each curve as bucket}
      <div class="column" title="{bucket.count} cards at mana value {bucket.mana_value}{bucket.is_overflow ? ' or more' : ''}">
        <span class="count" class:zero={bucket.count === 0}>{bucket.count}</span>
        <div class="bar" style="height: {(bucket.count / tallest) * 100}%"></div>
        <span class="label">{bucket.mana_value}{bucket.is_overflow ? "+" : ""}</span>
      </div>
    {/each}
  </div>
  <p class="note">Lands are excluded — they would all pile into the zero column.</p>

  {#if colorPips.some((p) => p.pips > 0)}
    <h4 class="pips-title">Coloured symbols</h4>
    <div class="pips">
      {#each colorPips.filter((p) => p.pips > 0) as pip}
        <span class="pip-count" style="--color: var({COLOR_VARS[pip.color]})">
          <span class="dot"></span>
          <strong>{pip.pips}</strong>
          <span class="cards">in {pip.cards}</span>
        </span>
      {/each}
    </div>
  {/if}
</section>

<style>
  .curve {
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  h4 {
    margin: 0 0 8px;
    font-size: var(--t-meta);
    font-weight: 600;
    color: var(--text-muted);
  }

  .average {
    font-size: var(--t-meta);
    color: var(--text-dim);
  }

  .bars {
    display: flex;
    align-items: flex-end;
    gap: 4px;
    height: 104px;
  }

  .column {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    height: 100%;
    min-width: 0;
  }

  .count {
    font-size: var(--t-meta);
    color: var(--text-muted);
    margin-bottom: 2px;
  }

  .count.zero {
    color: var(--text-dim);
  }

  .bar {
    width: 100%;
    min-height: 2px;
    border-radius: 3px 3px 0 0;
    /* Ink, not a hue. A curve is a measurement of the deck, and the five colours are reserved
       for mana — a blue bar next to a blue mana pip would read as a claim about the deck. */
    background: linear-gradient(180deg, var(--ink-2), rgba(184, 177, 172, 0.35));
  }

  .label {
    font-size: var(--t-meta);
    color: var(--text-dim);
    margin-top: 4px;
  }

  .note {
    margin: 8px 0 0;
    font-size: var(--t-meta);
    color: var(--text-dim);
  }

  .pips-title {
    margin-top: 14px;
  }

  .pips {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .pip-count {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--t-meta);
    color: var(--text-muted);
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 999px;
    background: var(--color);
  }

  .cards {
    color: var(--text-dim);
    font-size: var(--t-meta);
  }
</style>
