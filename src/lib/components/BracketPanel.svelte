<script lang="ts">
  import * as api from "../api";
  import { BRACKET_LABELS } from "../types";
  import type { BracketAssessment, ComboStatus, Marker } from "../types";

  let { deckId }: { deckId: number } = $props();

  let assessment = $state<BracketAssessment | null>(null);
  let combos = $state<ComboStatus | null>(null);
  let error = $state<string | null>(null);
  let expanded = $state(false);

  $effect(() => {
    const id = deckId;
    void (async () => {
      try {
        assessment = await api.deckBracket(id);
      } catch (e) {
        error = String(e);
        assessment = null;
      }
    })();
  });

  $effect(() => {
    void (async () => {
      try {
        combos = await api.comboStatus();
      } catch (e) {
        error = String(e);
      }
    })();
  });

  // Bracket 1 and 5 are shown greyed out: the estimate cannot reach them, and hiding them
  // would suggest the scale only has three steps.
  const STEPS = [1, 2, 3, 4, 5];

  function markerList(markers: Marker[]): string {
    return markers.map((m) => m.name).join(", ");
  }
</script>

<section class="bracket">
  <header>
    <h4>Commander bracket</h4>
    {#if assessment}
      <span class="verdict">
        {assessment.bracket} · {BRACKET_LABELS[assessment.bracket]}
      </span>
    {/if}
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if assessment}
    <div class="scale" role="img" aria-label="Bracket {assessment.bracket} of 5">
      {#each STEPS as step}
        <span
          class="step"
          class:active={step === assessment.bracket}
          class:unreachable={step === 1 || step === 5}
          title={step === 1 || step === 5
            ? `${BRACKET_LABELS[step]} — depends on how the deck is played, not on its cards`
            : BRACKET_LABELS[step]}
        >
          {step}
        </span>
      {/each}
    </div>

    <ul class="reasons">
      {#each assessment.reasons as reason}
        <li>{reason}</li>
      {/each}
    </ul>

    {#if assessment.game_changers.length || assessment.two_card_combos.length || assessment.mass_land_denial.length || assessment.extra_turns.length || assessment.tutors.length}
      <button type="button" class="ghost toggle" onclick={() => (expanded = !expanded)}>
        {expanded ? "Hide what was found" : "Show what was found"}
      </button>
    {/if}

    {#if expanded}
      <dl class="findings">
        {#if assessment.game_changers.length}
          <dt>Game Changers ({assessment.game_changers.length})</dt>
          <dd>{markerList(assessment.game_changers)}</dd>
        {/if}
        {#if assessment.two_card_combos.length}
          <dt>Two-card combos ({assessment.two_card_combos.length})</dt>
          <dd>
            {#each assessment.two_card_combos as combo}
              <p class="combo">
                {combo.card_names.join(" + ")}
                <span class="produces">{combo.produces.join(", ")}</span>
              </p>
            {/each}
          </dd>
        {/if}
        {#if assessment.longer_combos.length}
          <dt>Longer combos ({assessment.longer_combos.length})</dt>
          <dd>
            <p class="note">These do not move the bracket; the rules single out two-card ones.</p>
            {#each assessment.longer_combos as combo}
              <p class="combo">
                {combo.card_names.join(" + ")}
                <span class="produces">{combo.produces.join(", ")}</span>
              </p>
            {/each}
          </dd>
        {/if}
        {#if assessment.mass_land_denial.length}
          <dt>Mass land denial ({assessment.mass_land_denial.length})</dt>
          <dd>{markerList(assessment.mass_land_denial)}</dd>
        {/if}
        {#if assessment.extra_turns.length}
          <dt>Extra turns ({assessment.extra_turns.length})</dt>
          <dd>{markerList(assessment.extra_turns)}</dd>
        {/if}
        {#if assessment.tutors.length}
          <dt>Tutors ({assessment.tutors.length})</dt>
          <dd>{markerList(assessment.tutors)}</dd>
        {/if}
      </dl>
    {/if}

    {#each assessment.caveats as caveat}
      <p class="caveat">{caveat}</p>
    {/each}

    {#if combos && !combos.loaded}
      <p class="caveat">
        No combo data. Build it with
        <code>cargo run --release -p build-artifacts -- --combos-only</code>.
      </p>
    {:else if combos}
      <p class="source">
        {combos.combos.toLocaleString()} combos from Commander Spellbook, {combos.fetchedAt}.
      </p>
    {/if}
  {/if}
</section>

<style>
  .bracket {
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

  .verdict {
    font-size: 13px;
    font-weight: 600;
  }

  .scale {
    display: flex;
    gap: 4px;
    margin-bottom: 10px;
  }

  .step {
    flex: 1;
    text-align: center;
    padding: 5px 0;
    border-radius: var(--radius-sm);
    background: var(--panel-raised);
    border: 1px solid var(--border-strong);
    color: var(--text-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .step.active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--text);
    font-weight: 700;
  }

  /* Shown but dimmed: the estimate cannot reach these, and hiding them would suggest the
     scale has three steps rather than five. */
  .step.unreachable {
    opacity: 0.35;
    border-style: dashed;
  }

  .reasons {
    margin: 0 0 8px;
    padding-left: 18px;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .toggle {
    font-size: 12px;
    padding: 3px 0;
  }

  .findings {
    margin: 6px 0 0;
    font-size: 12px;
  }

  .findings dt {
    color: var(--text-muted);
    font-weight: 600;
    margin-top: 8px;
  }

  .findings dd {
    margin: 3px 0 0;
    color: var(--text-dim);
    line-height: 1.5;
  }

  .combo {
    margin: 3px 0;
    color: var(--text-muted);
  }

  .produces {
    display: block;
    color: var(--text-dim);
    font-size: 11px;
  }

  .note {
    margin: 0 0 4px;
    font-size: 11px;
    font-style: italic;
  }

  .caveat {
    margin: 8px 0 0;
    font-size: 11.5px;
    color: #d9a441;
    line-height: 1.45;
  }

  .source {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--text-dim);
  }

  code {
    font-size: 10.5px;
    background: var(--bg);
    padding: 1px 4px;
    border-radius: 3px;
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
