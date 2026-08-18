<script lang="ts">
  import * as api from "../api";
  import type { ImportSummary, SyncStatus } from "../types";

  let { onImported }: { onImported: () => void } = $props();

  let status = $state<SyncStatus | null>(null);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    void refresh();
  });

  async function refresh() {
    try {
      status = await api.syncStatus();
    } catch (e) {
      error = String(e);
    }
  }

  async function save() {
    busy = true;
    error = null;
    done = null;
    try {
      const contents = await api.syncExport();
      // A blob and an anchor, rather than a Rust command that writes to a path. Choosing where
      // a file goes is the platform's job, and a command that writes wherever it is told is a
      // command that can be told to write anywhere.
      const url = URL.createObjectURL(new Blob([contents], { type: "application/json" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = `magicoptimizer-${new Date().toISOString().slice(0, 10)}.json`;
      link.click();
      URL.revokeObjectURL(url);
      done = "Backup saved.";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function load(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;

    busy = true;
    error = null;
    done = null;
    try {
      const contents = await file.text();
      // Never forced from here. The command refuses when anything is already stored, and the
      // right answer to that refusal is for the user to export first — not for this button to
      // override it on their behalf.
      const summary: ImportSummary = await api.syncImport(contents, false);
      done =
        `Restored ${summary.holdings} holding(s), ${summary.decks} deck(s) ` +
        `and ${summary.games} game(s).`;
      await refresh();
      onImported();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
      input.value = "";
    }
  }
</script>

<section class="backup">
  <h3>Backup and transfer</h3>

  <p class="why">
    Your collection, decks and game log live in one place on this device and nowhere else. The
    game log especially cannot be rebuilt from anything — nobody remembers last March's games.
  </p>

  {#if status}
    <p class="holds">
      {status.holdings} holding(s) · {status.decks} deck(s) · {status.games} game(s)
    </p>
  {/if}

  <div class="actions">
    <button class="primary" disabled={busy} onclick={() => void save()}>Save a backup</button>

    <label class="restore" class:disabled={busy}>
      <input type="file" accept="application/json,.json" onchange={(e) => void load(e)} />
      Restore from a file
    </label>
  </div>

  <p class="note">
    There is no server and no account, so moving your data between a PC and a phone means saving
    this file and opening it on the other one.
  </p>

  {#if status && !status.empty}
    <p class="note warn">
      Restoring here would <strong>add to</strong> what is already stored rather than replace it,
      so the same cards would count twice. The app refuses that — save a backup of what is here
      first if you want to keep it.
    </p>
  {/if}

  {#if done}<p class="done">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</section>

<style>
  .backup {
    border-top: 1px solid var(--border);
    padding-top: 14px;
    margin-top: 18px;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .why,
  .note,
  .holds {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    margin: 0 0 10px;
  }

  .holds {
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }

  .warn {
    color: var(--danger);
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 10px;
  }

  /* A file input styled as a button: the native control cannot be, and a bare "Choose file"
     next to a styled button looks like a bug. */
  .restore {
    display: inline-flex;
    align-items: center;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--panel-raised);
    font-size: 13px;
    cursor: pointer;
  }

  .restore.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .restore input {
    display: none;
  }

  .done {
    color: var(--success);
    font-size: 12px;
    margin: 0;
  }

  .error {
    color: var(--danger);
    font-size: 12px;
    line-height: 1.5;
    margin: 0;
  }
</style>
