<script lang="ts">
  import * as api from "../api";
  import type { Choice, DeckView, Score, SearchResult } from "../types";

  let {
    deckId,
    format = "",
    onapplied,
  }: {
    deckId: number;
    /** The deck's format. Brackets only exist in Commander, so the control only shows there. */
    format?: string;
    /** The deck changed; the editor has to redraw. */
    onapplied: (view: DeckView) => void;
  } = $props();

  let archetypes = $state<Choice[]>([]);
  let pools = $state<Choice[]>([]);
  let archetype = $state("midrange");
  let pool = $state("everything");
  let onlyPlayedCards = $state(true);
  /** Empty means no bracket constraint, which is the default. */
  let maxBracket = $state("");

  const isCommander = $derived(format === "commander");

  let current = $state<Score | null>(null);
  let result = $state<SearchResult | null>(null);
  let applied = $state<Set<string>>(new Set());
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    void (async () => {
      try {
        const options = await api.optimizerOptions();
        archetypes = options.archetypes;
        pools = options.pools;
      } catch (e) {
        error = String(e);
      }
    })();
  });

  // Re-scores whenever the deck or the archetype changes. Cheap: a score is a few
  // milliseconds, while a search is seconds, which is why the two are separate buttons.
  $effect(() => {
    const id = deckId;
    const choice = archetype;
    void (async () => {
      try {
        current = await api.deckScore(id, choice);
      } catch (e) {
        error = String(e);
        current = null;
      }
    })();
  });

  async function optimize() {
    busy = true;
    error = null;
    result = null;
    applied = new Set();
    try {
      result = await api.deckOptimize(
        deckId,
        archetype,
        pool,
        undefined,
        onlyPlayedCards,
        maxBracket === "" ? undefined : Number(maxBracket),
      );
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function apply(removeOracleId: string, addOracleId: string) {
    try {
      const view = await api.deckApplySuggestion(deckId, removeOracleId, addOracleId);
      applied = new Set([...applied, `${removeOracleId}->${addOracleId}`]);
      onapplied(view);
      current = await api.deckScore(deckId, archetype);
    } catch (e) {
      error = String(e);
    }
  }

  function key(remove: string, add: string): string {
    return `${remove}->${add}`;
  }

  function grade(total: number): string {
    if (total >= 85) return "strong";
    if (total >= 70) return "fair";
    return "weak";
  }
</script>

<section class="optimize">
  <header>
    <h4>Optimizer</h4>
    {#if current}
      <span class="total {grade(current.total)}">{current.total.toFixed(0)}<small>/100</small></span>
    {/if}
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if current && !current.reliable}
    <p class="caveat">
      {current.unresolved_cards} card(s) are not in the loaded card data, so these numbers
      describe only part of the deck.
    </p>
  {/if}

  {#if current}
    <ul class="criteria">
      {#each current.criteria as criterion}
        <li>
          <div class="criterion-head">
            <span class="criterion-name">
              {criterion.name}
              {#if !criterion.derived}
                <span class="convention" title="A conventional target, not a calculation">
                  convention
                </span>
              {/if}
            </span>
            <span class="criterion-score">{(criterion.score * 100).toFixed(0)}%</span>
          </div>
          <div class="meter"><div class="fill" style="width: {criterion.score * 100}%"></div></div>
          <p class="criterion-detail">{criterion.detail}</p>
        </li>
      {/each}
    </ul>

    <p class="simulation">
      Over {current.simulation.games.toLocaleString()} simulated games:
      {(current.simulation.keepable_opening_hands * 100).toFixed(0)}% keepable openers,
      {current.simulation.average_mulligans.toFixed(2)} mulligans on average.
    </p>
  {/if}

  <div class="controls">
    <div>
      <label for="opt-archetype">Playing as</label>
      <select id="opt-archetype" bind:value={archetype}>
        {#each archetypes as option}
          <option value={option.key}>{option.label}</option>
        {/each}
      </select>
    </div>
    <div>
      <label for="opt-pool">Suggest from</label>
      <select id="opt-pool" bind:value={pool}>
        {#each pools as option}
          <option value={option.key}>{option.label}</option>
        {/each}
      </select>
    </div>
  </div>

  <label class="checkbox">
    <input type="checkbox" bind:checked={onlyPlayedCards} />
    Only cards people actually play
  </label>

  {#if isCommander}
    <div class="control">
      <label for="max-bracket">Stay within bracket</label>
      <select id="max-bracket" bind:value={maxBracket}>
        <option value="">No limit</option>
        <option value="2">2 — Core</option>
        <option value="3">3 — Upgraded</option>
      </select>
    </div>
    <p class="limitation">
      This holds the deck to the Game Changer count the bracket allows — the one rule that can be
      checked from the card data alone. It cannot see two-card combos or mass land denial, so
      check the finished deck against the bracket panel rather than trusting it.
    </p>
  {/if}

  <button type="button" class="primary run" onclick={optimize} disabled={busy}>
    {busy ? "Searching…" : "Suggest improvements"}
  </button>

  <p class="limitation">
    The score measures the mana base, the curve and the opening hand. It does not know what a
    card <em>does</em> — so treat this as a mana and curve check, not as card advice.
  </p>

  {#if result}
    {#if result.candidates_considered === 0}
      <p class="caveat">
        Nothing to suggest from: no card passed the filters. Try widening the card pool.
      </p>
    {:else if result.suggestions.length === 0}
      <p class="empty">
        No improvement found among {result.candidates_considered.toLocaleString()} candidates.
      </p>
    {:else}
      <div class="verdict">
        {result.before.total.toFixed(1)} → {result.after.total.toFixed(1)}
        with {result.suggestions.length} change(s)
      </div>

      <ul class="suggestions">
        {#each result.suggestions as suggestion}
          {@const id = key(suggestion.remove_oracle_id, suggestion.add_oracle_id)}
          <li class:done={applied.has(id)}>
            <div class="swap">
              <span class="out">−1 {suggestion.remove_name}</span>
              <span class="in">+1 {suggestion.add_name}</span>
              <span class="gain">
                +{(suggestion.score_after - suggestion.score_before).toFixed(2)}
              </span>
              <button
                type="button"
                onclick={() => apply(suggestion.remove_oracle_id, suggestion.add_oracle_id)}
                disabled={applied.has(id)}
              >
                {applied.has(id) ? "Applied" : "Apply"}
              </button>
            </div>
            {#each suggestion.reasons as reason}
              <p class="reason">{reason}</p>
            {/each}
          </li>
        {/each}
      </ul>
      <p class="limitation">
        Each change is measured against the deck as it stands, so they can be applied in any
        order — or not at all.
      </p>
    {/if}
  {/if}
</section>

<style>
  .optimize {
    border-top: 1px solid var(--border);
    padding-top: 12px;
    margin-top: 14px;
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  h4 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .total {
    font-size: 20px;
    font-weight: 700;
  }

  .total small {
    font-size: 11px;
    font-weight: 400;
    color: var(--text-dim);
  }

  .total.strong {
    color: var(--success);
  }

  .total.fair {
    color: #d9a441;
  }

  .total.weak {
    color: var(--danger);
  }

  .criteria {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
  }

  .criteria li {
    margin-bottom: 9px;
  }

  .criterion-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 12px;
  }

  .criterion-name {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .criterion-score {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  .convention {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(217, 164, 65, 0.15);
    border: 1px solid rgba(217, 164, 65, 0.35);
    color: #d9a441;
  }

  .meter {
    height: 4px;
    border-radius: 999px;
    background: var(--panel-raised);
    margin: 3px 0;
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: linear-gradient(90deg, var(--gold), #f0d19a);
  }

  .criterion-detail {
    margin: 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.4;
  }

  .simulation {
    margin: 0 0 12px;
    font-size: 11.5px;
    color: var(--text-muted);
    line-height: 1.45;
  }

  .controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin-bottom: 9px;
  }

  .run {
    width: 100%;
    margin-top: 10px;
  }

  .limitation {
    margin: 9px 0 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.45;
  }

  .verdict {
    margin: 14px 0 8px;
    font-weight: 600;
  }

  .suggestions {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .suggestions li {
    padding: 8px 0;
    border-top: 1px solid var(--line);
  }

  .suggestions li.done {
    opacity: 0.5;
  }

  .swap {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    grid-template-areas:
      "out gain button"
      "in  gain button";
    gap: 1px 8px;
    align-items: center;
    font-size: 12.5px;
  }

  .out {
    grid-area: out;
    color: var(--danger);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .in {
    grid-area: in;
    color: var(--success);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gain {
    grid-area: gain;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  .swap button {
    grid-area: button;
    font-size: 12px;
    padding: 4px 10px;
  }

  .reason {
    margin: 5px 0 0;
    font-size: 11px;
    color: var(--text-dim);
    line-height: 1.45;
  }

  .caveat {
    margin: 0 0 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: rgba(217, 164, 65, 0.1);
    border: 1px solid rgba(217, 164, 65, 0.35);
    color: #d9a441;
    font-size: 11.5px;
    line-height: 1.45;
  }

  .empty {
    margin: 12px 0 0;
    color: var(--text-dim);
    font-size: 12px;
  }

  .error {
    margin: 0 0 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: rgba(228, 87, 61, 0.12);
    border: 1px solid rgba(228, 87, 61, 0.4);
    color: var(--danger);
    font-size: 12px;
  }
</style>
