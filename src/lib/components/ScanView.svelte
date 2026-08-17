<script lang="ts">
  import * as api from "../api";
  import type { ScanResult, ScanStatus, ScannedCard, StoredDeck, Zone } from "../types";

  let {
    decks,
    containers,
    onCommitted,
  }: {
    decks: StoredDeck[];
    containers: string[];
    onCommitted: () => void;
  } = $props();

  /**
   * Width the frame is reduced to before being sent.
   *
   * Detection works at 320 internally and the artwork hash is sampled down to a 17x16 grid, so
   * more than this buys nothing — it only makes each frame heavier to hand across.
   */
  const CAPTURE_WIDTH = 640;

  /** Between frames. Ten a second is well inside the budget and feels immediate. */
  const FRAME_INTERVAL = 100;

  type Destination = "physical" | "digital" | "deck" | "pool";

  type Pending = ScannedCard & { quantity: number };

  let status = $state<ScanStatus | null>(null);
  let error = $state<string | null>(null);
  let cameraError = $state<string | null>(null);
  let running = $state(false);
  let result = $state<ScanResult | null>(null);
  let pending = $state<Pending[]>([]);
  let committing = $state(false);
  let justAdded = $state<string | null>(null);

  let destination = $state<Destination>("physical");
  let deckId = $state<number | null>(null);
  let zone = $state<Zone>("main");
  let container = $state("");
  let poolName = $state("");

  let video: HTMLVideoElement | null = $state(null);
  let overlay: HTMLCanvasElement | null = $state(null);

  // Not $state: these are plumbing, and making them reactive would re-run effects on every
  // frame for no visible change.
  let stream: MediaStream | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;
  let grabber: HTMLCanvasElement | null = null;
  let gray: Uint8Array | null = null;
  let inFlight = false;

  $effect(() => {
    void (async () => {
      try {
        status = await api.scanStatus();
      } catch (e) {
        error = String(e);
      }
    })();
    // Stop the camera when the view goes away — a light left on is both a battery drain and,
    // rightly, alarming.
    return () => stop();
  });

  async function start() {
    cameraError = null;
    error = null;
    try {
      // `environment` asks for the rear camera, which is the one pointed at the table. It is a
      // hint: a laptop with only a front camera still works.
      stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: { ideal: "environment" }, width: { ideal: 1280 } },
        audio: false,
      });
    } catch (e) {
      // The documented Android WebView failure lands here. Say what to do rather than printing
      // a DOMException at the user.
      cameraError = `The camera could not be opened: ${e}`;
      return;
    }

    if (video) {
      video.srcObject = stream;
      await video.play().catch((e) => {
        cameraError = `The camera stream would not start: ${e}`;
      });
    }

    try {
      await api.scanReset();
    } catch (e) {
      error = String(e);
    }

    running = true;
    timer = setInterval(() => void grab(), FRAME_INTERVAL);
  }

  function stop() {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
    stream?.getTracks().forEach((track) => track.stop());
    stream = null;
    if (video) video.srcObject = null;
    running = false;
    result = null;
    clearOverlay();
  }

  async function grab() {
    // Frames arrive faster than they are recognised on a slow device. Dropping the ones that
    // arrive mid-recognition keeps the queue from growing without bound; the voter only needs
    // agreement, not every frame.
    if (!video || inFlight || video.readyState < 2) return;
    const sourceWidth = video.videoWidth;
    const sourceHeight = video.videoHeight;
    if (!sourceWidth || !sourceHeight) return;

    const width = Math.min(CAPTURE_WIDTH, sourceWidth);
    const height = Math.round((sourceHeight / sourceWidth) * width);

    if (!grabber) grabber = document.createElement("canvas");
    if (grabber.width !== width || grabber.height !== height) {
      grabber.width = width;
      grabber.height = height;
    }
    const context = grabber.getContext("2d", { willReadFrequently: true });
    if (!context) return;

    context.drawImage(video, 0, 0, width, height);
    const rgba = context.getImageData(0, 0, width, height).data;

    // Converted here rather than in Rust so a quarter of the bytes cross the boundary. The
    // weights match `mtg_vision::rgba_to_gray` exactly — a different luma formula would shift
    // every hash away from the reference set.
    if (!gray || gray.length !== width * height) gray = new Uint8Array(width * height);
    for (let i = 0, p = 0; p < gray.length; i += 4, p += 1) {
      gray[p] = (rgba[i] * 77 + rgba[i + 1] * 150 + rgba[i + 2] * 29) >> 8;
    }

    inFlight = true;
    try {
      const outcome = await api.scanFrame(gray, width, height);
      result = outcome;
      drawOutline(outcome, width, height);
      if (outcome.state === "confirmed" && outcome.card) accept(outcome.card);
    } catch (e) {
      error = String(e);
      stop();
    } finally {
      inFlight = false;
    }
  }

  function accept(card: ScannedCard) {
    const known = pending.some((entry) => entry.oracleId === card.oracleId);
    pending = known
      ? pending.map((entry) =>
          entry.oracleId === card.oracleId ? { ...entry, quantity: entry.quantity + 1 } : entry,
        )
      : [...pending, { ...card, quantity: 1 }];
    justAdded = card.name;
    setTimeout(() => {
      if (justAdded === card.name) justAdded = null;
    }, 1200);
  }

  function clearOverlay() {
    const context = overlay?.getContext("2d");
    if (overlay && context) context.clearRect(0, 0, overlay.width, overlay.height);
  }

  function drawOutline(outcome: ScanResult, width: number, height: number) {
    if (!overlay) return;
    if (overlay.width !== width || overlay.height !== height) {
      overlay.width = width;
      overlay.height = height;
    }
    const context = overlay.getContext("2d");
    if (!context) return;
    context.clearRect(0, 0, width, height);
    if (!outcome.quad) return;

    // Green once the card is named, amber while it is only being tracked, grey when a card is
    // clearly there but nothing matched — which is the difference between "hold still" and
    // "this card is not in the database".
    const colour =
      outcome.state === "confirmed" || outcome.state === "holding"
        ? "#43aa6a"
        : outcome.state === "tracking"
          ? "#e8a33d"
          : "#8f99ae";

    context.strokeStyle = colour;
    context.lineWidth = 3;
    context.lineJoin = "round";
    context.beginPath();
    outcome.quad.forEach(([x, y], index) => {
      if (index === 0) context.moveTo(x, y);
      else context.lineTo(x, y);
    });
    context.closePath();
    context.stroke();
  }

  function remove(oracleId: string) {
    pending = pending.filter((entry) => entry.oracleId !== oracleId);
  }

  function adjust(oracleId: string, by: number) {
    pending = pending
      .map((entry) =>
        entry.oracleId === oracleId ? { ...entry, quantity: entry.quantity + by } : entry,
      )
      .filter((entry) => entry.quantity > 0);
  }

  const totalPending = $derived(pending.reduce((sum, entry) => sum + entry.quantity, 0));

  const destinationReady = $derived(
    destination === "deck" ? deckId !== null : destination === "pool" ? poolName.trim() !== "" : true,
  );

  async function commit() {
    if (!pending.length || !destinationReady) return;
    committing = true;
    error = null;
    try {
      for (const entry of pending) {
        if (destination === "deck" && deckId !== null) {
          await api.deckAddCard(deckId, entry.oracleId, entry.quantity, zone);
        } else {
          // A draft pool is a physical holding in a container named after the pool. There is no
          // separate concept to invent: that is exactly what a pool is once the draft is over.
          const box = destination === "pool" ? poolName.trim() : container.trim();
          await api.collectionAdd({
            pool: destination === "digital" ? "digital" : "physical",
            oracle_id: entry.oracleId,
            name: entry.name,
            set_code: "",
            collector_number: "",
            language: "en",
            finish: "nonfoil",
            condition: "near_mint",
            quantity: entry.quantity,
            location: box ? { container: box } : null,
            notes: "",
          });
        }
      }
      pending = [];
      onCommitted();
    } catch (e) {
      error = String(e);
    } finally {
      committing = false;
    }
  }

  const progress = $derived(
    result?.state === "tracking" && result.needed > 0 ? result.votes / result.needed : 0,
  );
</script>

<section class="scan">
  <div class="viewfinder">
    <!-- svelte-ignore a11y_media_has_caption -->
    <video bind:this={video} playsinline muted></video>
    <canvas bind:this={overlay} class="overlay"></canvas>

    {#if !running}
      <div class="curtain">
        {#if status && !status.loaded}
          <p class="headline">No artwork fingerprints installed</p>
          <p class="hint">
            Recognition needs <code>arthashes.bin</code>, an optional download of about 6 MB.
            Build it yourself with:
          </p>
          <pre>cargo run --release -p build-artifacts -- --art-only</pre>
          {#if status.error}<p class="reason">{status.error}</p>{/if}
          <button onclick={() => void api.scanReload().then((s) => (status = s))}>
            Check again
          </button>
        {:else if cameraError}
          <p class="headline">Camera unavailable</p>
          <p class="reason">{cameraError}</p>
          <button onclick={() => void start()}>Try again</button>
        {:else}
          <p class="headline">Ready to scan</p>
          <p class="hint">
            Hold one card at a time against a plain, contrasting background. Recognition uses the
            artwork, so the card's language does not matter.
          </p>
          <button class="primary" onclick={() => void start()}>Start the camera</button>
        {/if}
      </div>
    {:else}
      <div class="readout" class:found={result?.state === "confirmed" || result?.state === "holding"}>
        {#if result?.state === "confirmed" || result?.state === "holding"}
          <span class="name">{justAdded ?? result.card?.name ?? "Added"}</span>
          <span class="tag">added</span>
        {:else if result?.state === "tracking"}
          <span class="name">{result.card?.name}</span>
          <span class="ring" style="--fill: {progress}"></span>
        {:else if result?.quad}
          <span class="name muted">Card seen, no match</span>
        {:else}
          <span class="name muted">Looking for a card…</span>
        {/if}
      </div>
      <button class="stop" onclick={stop}>Stop</button>
    {/if}
  </div>

  <div class="panel">
    <h3>Where these go</h3>
    <div class="destinations">
      <label class:active={destination === "physical"}>
        <input type="radio" bind:group={destination} value="physical" />
        Physical collection
      </label>
      <label class:active={destination === "digital"}>
        <input type="radio" bind:group={destination} value="digital" />
        Digital collection
      </label>
      <label class:active={destination === "deck"}>
        <input type="radio" bind:group={destination} value="deck" />
        A deck
      </label>
      <label class:active={destination === "pool"}>
        <input type="radio" bind:group={destination} value="pool" />
        A draft or sealed pool
      </label>
    </div>

    {#if destination === "physical"}
      <label class="field">
        Storage box <span class="optional">optional</span>
        <input list="scan-containers" bind:value={container} placeholder="e.g. Blue binder" />
      </label>
      <datalist id="scan-containers">
        {#each containers as name (name)}<option value={name}></option>{/each}
      </datalist>
    {:else if destination === "deck"}
      <label class="field">
        Deck
        <select bind:value={deckId}>
          <option value={null}>Choose a deck…</option>
          {#each decks as deck (deck.id)}<option value={deck.id}>{deck.name}</option>{/each}
        </select>
      </label>
      <label class="field">
        Zone
        <select bind:value={zone}>
          <option value="main">Main deck</option>
          <option value="side">Sideboard</option>
          <option value="command">Command zone</option>
        </select>
      </label>
    {:else if destination === "pool"}
      <label class="field">
        Pool name
        <input bind:value={poolName} placeholder="e.g. Prerelease 2026-08-17" />
      </label>
      <p class="note">
        Kept as physical cards in a box of that name — which is what a pool is, once the draft is
        over.
      </p>
    {/if}

    <h3 class="scanned-head">
      Scanned
      {#if totalPending > 0}<span class="count">{totalPending}</span>{/if}
    </h3>

    {#if pending.length === 0}
      <p class="empty">
        Nothing yet. Cards are collected here as they are recognised, and nothing is written until
        you confirm — a misread is a great deal easier to fix before it reaches your collection.
      </p>
    {:else}
      <ul class="pending">
        {#each pending as entry (entry.oracleId)}
          <li>
            <span class="card-name">{entry.name}</span>
            <div class="quantity">
              <button onclick={() => adjust(entry.oracleId, -1)} aria-label="One fewer">−</button>
              <span>{entry.quantity}</span>
              <button onclick={() => adjust(entry.oracleId, 1)} aria-label="One more">+</button>
            </div>
            <button class="remove" onclick={() => remove(entry.oracleId)} aria-label="Remove">
              ×
            </button>
          </li>
        {/each}
      </ul>

      <button class="primary commit" disabled={committing || !destinationReady} onclick={() => void commit()}>
        {committing ? "Adding…" : `Add ${totalPending} card${totalPending === 1 ? "" : "s"}`}
      </button>
      {#if !destinationReady}
        <p class="note">Choose a destination first.</p>
      {/if}
    {/if}

    {#if status?.loaded}
      <p class="footnote">{status.artworks.toLocaleString()} artworks known.</p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>
</section>

<style>
  .scan {
    display: grid;
    grid-template-columns: 1fr 320px;
    gap: 16px;
    height: 100%;
    min-height: 0;
  }

  .viewfinder {
    position: relative;
    background: #000;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  video,
  .overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .curtain {
    position: relative;
    max-width: 420px;
    padding: 24px;
    text-align: center;
    color: var(--text);
  }

  .headline {
    font-size: 15px;
    font-weight: 600;
    margin: 0 0 8px;
  }

  .hint,
  .reason {
    font-size: 13px;
    color: var(--text-muted);
    margin: 0 0 12px;
    line-height: 1.5;
  }

  .reason {
    color: var(--danger);
  }

  pre {
    background: var(--panel-raised);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: 8px 10px;
    font-size: 12px;
    overflow-x: auto;
    text-align: left;
    margin: 0 0 12px;
  }

  .readout {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 12px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: rgba(13, 15, 22, 0.82);
    border: 1px solid var(--border-strong);
    backdrop-filter: blur(6px);
  }

  .readout.found {
    border-color: var(--success);
  }

  .name {
    font-size: 14px;
    font-weight: 600;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.muted {
    font-weight: 400;
    color: var(--text-muted);
  }

  .tag {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--success);
  }

  /* A ring that fills as frames agree, so waiting looks like progress rather than a stall. */
  .ring {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: conic-gradient(var(--accent) calc(var(--fill) * 360deg), var(--border-strong) 0);
    flex: none;
  }

  .stop {
    position: absolute;
    top: 12px;
    right: 12px;
  }

  .panel {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px;
    overflow-y: auto;
    min-height: 0;
  }

  h3 {
    margin: 0 0 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .scanned-head {
    margin-top: 20px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .count {
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 999px;
    padding: 1px 8px;
    font-size: 11px;
  }

  .destinations {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 12px;
  }

  .destinations label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    font-size: 13px;
    cursor: pointer;
  }

  .destinations label.active {
    background: var(--accent-soft);
    border-color: var(--accent);
  }

  .field {
    display: block;
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 10px;
  }

  .field input,
  .field select {
    width: 100%;
    margin-top: 4px;
  }

  .optional {
    color: var(--text-dim);
  }

  .note,
  .footnote,
  .empty {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    margin: 8px 0 0;
  }

  .footnote {
    color: var(--text-dim);
    margin-top: 16px;
  }

  .error {
    color: var(--danger);
    font-size: 12px;
    margin-top: 10px;
  }

  .pending {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pending li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border-radius: var(--radius-sm);
    font-size: 13px;
  }

  .pending li:nth-child(odd) {
    background: var(--panel-raised);
  }

  .card-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .quantity {
    display: flex;
    align-items: center;
    gap: 4px;
    font-variant-numeric: tabular-nums;
  }

  .quantity button,
  .remove {
    padding: 0;
    width: 22px;
    height: 22px;
    line-height: 1;
  }

  .commit {
    width: 100%;
    margin-top: 12px;
  }

  /* The filter-drawer breakpoint: the side panel no longer fits beside the viewfinder. */
  @media (max-width: 1180px) {
    .scan {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(280px, 1fr) auto;
    }

    .panel {
      overflow-y: visible;
    }
  }

  /* Phone: the viewfinder takes the screen, since that is the whole task. */
  @media (max-width: 860px) {
    .scan {
      grid-template-rows: minmax(0, 60vh) auto;
      gap: 12px;
    }
  }
</style>
