<script lang="ts">
  import * as api from "../api";
  import type { DeckHistory, GameResult, StoredDeck, WinRate } from "../types";

  let { decks }: { decks: StoredDeck[] } = $props();

  let deckId = $state<number | null>(null);
  let history = $state<DeckHistory | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);

  // Recording a game has to take seconds, so the form opens pre-filled with today's date and
  // the only three fields that are required.
  const today = new Date().toISOString().slice(0, 10);
  let playedAt = $state(today);
  let result = $state<GameResult>("win");
  let opponents = $state("");
  let mulligans = $state<string>("");
  let notes = $state("");
  let detailsOpen = $state(false);

  /** The date a deck change was made, for the before-and-after comparison. */
  let since = $state("");

  $effect(() => {
    if (deckId === null && decks.length > 0) deckId = decks[0].id;
  });

  $effect(() => {
    const id = deckId;
    const date = since;
    if (id === null) return;
    void (async () => {
      try {
        history = await api.journalDeckHistory(id, date || undefined);
        error = null;
      } catch (e) {
        error = String(e);
      }
    })();
  });

  async function record() {
    if (deckId === null) return;
    saving = true;
    error = null;
    try {
      await api.journalAdd({
        deckId,
        playedAt,
        result,
        opponents: opponents
          .split(",")
          .map((name) => name.trim())
          .filter(Boolean),
        mulligans: mulligans === "" ? null : Number(mulligans),
        notes,
      });
      // Only the fields that change between games are cleared. The date stays, because an
      // evening is entered as several games at once.
      opponents = "";
      mulligans = "";
      notes = "";
      history = await api.journalDeckHistory(deckId, since || undefined);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function remove(id: number) {
    if (deckId === null) return;
    try {
      await api.journalRemove(id);
      history = await api.journalDeckHistory(deckId, since || undefined);
    } catch (e) {
      error = String(e);
    }
  }

  const percent = (value: number) => `${Math.round(value * 100)}%`;

  /**
   * How much a rate can be trusted, in words.
   *
   * A number alone invites the reader to believe it. This is the same information the interval
   * carries, said in a way nobody has to interpret.
   */
  function confidence(rate: WinRate): string {
    const decided = rate.wins + rate.losses;
    if (decided === 0) return "no decided games yet";
    const width = rate.high - rate.low;
    if (width > 0.5) return `${decided} games — far too few to tell`;
    if (width > 0.3) return `${decided} games — still very uncertain`;
    if (width > 0.15) return `${decided} games — a rough idea`;
    return `${decided} games — a reasonable estimate`;
  }

  const deckName = $derived(decks.find((deck) => deck.id === deckId)?.name ?? "");
</script>

<section class="journal">
  <div class="entry">
    <h3>Record a game</h3>
    <p class="after">
      Filled in after the game, never during — see the
      <a href="https://github.com/eraflo/MagicOptimizer#status">project constraints</a>.
    </p>

    <label class="field">
      Deck
      <select bind:value={deckId}>
        {#each decks as deck (deck.id)}<option value={deck.id}>{deck.name}</option>{/each}
      </select>
    </label>

    <label class="field">
      Date
      <input type="date" bind:value={playedAt} />
    </label>

    <div class="results">
      {#each [["win", "Won"], ["loss", "Lost"], ["draw", "Draw"]] as [value, label] (value)}
        <label class:active={result === value}>
          <input type="radio" bind:group={result} {value} />
          {label}
        </label>
      {/each}
    </div>

    <label class="field">
      Opponents <span class="optional">optional, comma separated</span>
      <input bind:value={opponents} placeholder="Atraxa, Krenko, Edgar" />
    </label>

    <details bind:open={detailsOpen}>
      <summary>More</summary>
      <label class="field">
        Mulligans <span class="optional">the single best predictor of a loss</span>
        <input type="number" min="0" max="7" bind:value={mulligans} />
      </label>
      <label class="field">
        Notes
        <input bind:value={notes} placeholder="Kept a one-lander" />
      </label>
    </details>

    <button class="primary" disabled={saving || deckId === null} onclick={() => void record()}>
      {saving ? "Saving…" : "Record"}
    </button>
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  <div class="results-panel">
    {#if decks.length === 0}
      <p class="empty">Build a deck first — a game log needs something to log games against.</p>
    {:else if history}
      <h3>{deckName}</h3>

      {#if history.overall.games === 0}
        <p class="empty">No games recorded yet.</p>
      {:else}
        <!-- Both numbers, always. Showing the observed rate alone is the thing this whole
             feature is built to avoid. -->
        <div class="rates">
          <div class="rate">
            <span class="value">{percent(history.overall.observed)}</span>
            <span class="rate-label">observed</span>
          </div>
          <div class="rate primary-rate">
            <span class="value">{percent(history.overall.adjusted)}</span>
            <span class="rate-label">adjusted</span>
          </div>
          <div class="rate">
            <span class="value small">
              {percent(history.overall.low)}–{percent(history.overall.high)}
            </span>
            <span class="rate-label">likely range</span>
          </div>
        </div>
        <p class="confidence">{confidence(history.overall)}</p>
        <p class="tally">
          {history.overall.wins}W · {history.overall.losses}L
          {#if history.overall.draws > 0}· {history.overall.draws}D{/if}
        </p>

        {#if history.matchups.length > 0}
          <h4>Hardest matchups</h4>
          <ul class="matchups">
            {#each history.matchups.slice(0, 8) as matchup (matchup.archetype)}
              <li>
                <span class="archetype">{matchup.archetype}</span>
                <span class="record">{matchup.rate.wins}–{matchup.rate.losses}</span>
                <span class="adjusted">{percent(matchup.rate.adjusted)}</span>
              </li>
            {/each}
          </ul>
        {/if}

        <h4>Did a change help?</h4>
        <label class="field">
          Games since
          <input type="date" bind:value={since} />
        </label>
        {#if history.change}
          <p class="change">
            Before: {percent(history.change.before.adjusted)} ({history.change.before.wins}–{history
              .change.before.losses}) · After: {percent(history.change.after.adjusted)}
            ({history.change.after.wins}–{history.change.after.losses})
          </p>
          <p class="verdict" class:conclusive={history.change.conclusive}>
            {#if history.change.conclusive}
              The difference is large enough to be more than luck.
            {:else}
              Not enough games to tell the difference from luck — which is the usual answer, and
              an honest one.
            {/if}
          </p>
        {/if}

        <h4>Games</h4>
        <ul class="games">
          {#each history.games as game (game.id)}
            <li>
              <span class="date">{game.played_at}</span>
              <span class="outcome {game.result}">{game.result}</span>
              <span class="against">
                {game.opponents.map((o) => o.archetype).join(", ") || "—"}
              </span>
              <button class="remove" onclick={() => void remove(game.id)} aria-label="Delete">
                ×
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
</section>

<style>
  .journal {
    display: grid;
    grid-template-columns: 330px 1fr;
    gap: 14px;
    height: 100%;
    min-height: 0;
  }

  .entry,
  .results-panel {
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 18px;
    background: rgba(20, 20, 24, 0.8);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-shadow: var(--sheen), var(--lift);
    padding: 24px;
    overflow-y: auto;
    min-height: 0;
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--t-meta);
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  h4 {
    margin: 20px 0 8px;
    font-size: var(--t-meta);
    font-weight: 600;
    color: var(--text-muted);
  }

  .after,
  .optional,
  .confidence,
  .empty,
  .tally {
    font-size: var(--t-meta);
    color: var(--text-muted);
    line-height: 1.5;
  }

  .after {
    margin: 0 0 12px;
  }

  .field {
    display: block;
    font-size: var(--t-meta);
    color: var(--text-muted);
    margin-bottom: 10px;
  }

  .field input,
  .field select {
    width: 100%;
    margin-top: 4px;
  }

  .results {
    display: flex;
    gap: 4px;
    margin-bottom: 10px;
  }

  /* Won / Lost / Draw is a choice, not a field caption. The global `label` rule sets captions in
     spaced uppercase, which is right for "Deck" and wrong for a control you press. */
  .results label {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 40px;
    margin: 0;
    padding: 0;
    border-radius: 999px;
    border: 1px solid transparent;
    background: rgba(255, 255, 255, 0.05);
    color: var(--ink-2);
    font-size: var(--t-body);
    font-weight: 600;
    letter-spacing: 0;
    text-transform: none;
    cursor: pointer;
  }

  .results label:hover:not(.active) {
    background: rgba(255, 255, 255, 0.1);
    color: var(--ink);
  }

  .results label.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--ground);
  }

  .results input {
    display: none;
  }

  .rates {
    display: flex;
    gap: 10px;
    margin: 4px 0 6px;
  }

  .rate {
    flex: 1;
    background: var(--panel-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 10px;
    text-align: center;
  }

  /* The adjusted figure is the one to read, so it is the one that looks primary. */
  .primary-rate {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .value {
    display: block;
    font-size: 22px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .value.small {
    font-size: 15px;
  }

  .rate-label {
    display: block;
    font-size: var(--t-meta);
    color: var(--text-muted);
    margin-top: 2px;
  }

  .matchups,
  .games {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .matchups li,
  .games li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 6px;
    border-radius: var(--radius-sm);
    font-size: 13px;
  }

  .matchups li:nth-child(odd),
  .games li:nth-child(odd) {
    background: var(--panel-raised);
  }

  .archetype,
  .against {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .record,
  .adjusted,
  .date {
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }

  .outcome {
    text-transform: capitalize;
    font-size: var(--t-meta);
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--panel-raised);
  }

  .outcome.win {
    color: var(--success);
  }

  .outcome.loss {
    color: var(--danger);
  }

  .change {
    font-size: 13px;
    margin: 4px 0;
  }

  .verdict {
    font-size: var(--t-meta);
    color: var(--text-muted);
    line-height: 1.5;
    margin: 0;
  }

  .verdict.conclusive {
    color: var(--success);
  }

  .remove {
    padding: 0;
    width: 22px;
    height: 22px;
    line-height: 1;
  }

  .error {
    color: var(--danger);
    font-size: var(--t-meta);
    margin-top: 10px;
  }

  /* The entry form no longer fits beside the results, so the view becomes one scrolling column
     rather than two cards each scrolling inside half the height. Splitting a phone screen in two
     gave the form a keyhole to be filled in through and the results a keyhole to be read in;
     one scroll gives each of them the whole screen in turn.

     The clip box is pushed out and the cards put back, so their shadows finish rather than being
     sliced flat against the scroller's edge. */
  @media (max-width: 1180px) {
    .journal {
      display: flex;
      flex-direction: column;
      gap: 14px;
      overflow-y: auto;
      padding: 4px 20px 8px;
      margin: -4px -20px -8px;
    }

    .entry,
    .results-panel {
      flex: none;
      overflow-y: visible;
    }
  }
</style>
