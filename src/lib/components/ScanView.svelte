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

  /**
   * The same, on a phone.
   *
   * Every frame is a fresh buffer crossing the IPC bridge, and at 640 wide, ten times a second,
   * that is 3 MB a second of garbage for the WebView to collect. A phone reported the camera
   * dying after fifteen to twenty seconds, which is about what that adds up to. Detection works
   * at 320 internally, so 480 loses nothing that mattered.
   */
  const CAPTURE_WIDTH_TOUCH = 480;

  /** Between frames. */
  const FRAME_INTERVAL = 100;

  /**
   * The same, on a phone: five a second rather than ten.
   *
   * The documented budget was always 5–10 fps on a mid-range phone, and the voter needs five
   * frames to agree — one second at this rate, which is the pause a person makes anyway.
   */
  const FRAME_INTERVAL_TOUCH = 200;

  /** Coarse pointer rather than width: a narrow desktop window is still a desktop. */
  const onTouch =
    typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches;

  /** Consecutive failures before the camera is given up on. */
  const MAX_FRAME_FAILURES = 15;

  type Destination = "physical" | "digital" | "deck" | "pool";

  type Pending = ScannedCard & { quantity: number };

  let status = $state<ScanStatus | null>(null);
  let error = $state<string | null>(null);
  let cameraError = $state<string | null>(null);
  let running = $state(false);
  let result = $state<ScanResult | null>(null);
  let pending = $state<Pending[]>([]);
  let committing = $state(false);
  /** Held until the next card replaces it, so the readout keeps naming what it just added. */
  let lastConfirmed = $state<string | null>(null);
  /** The last frame failure, shown over the viewfinder while the session keeps trying. */
  let frameError = $state<string | null>(null);

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
  /**
   * Consecutive frame failures.
   *
   * One bad frame used to stop the camera outright, which on a phone looked like the camera
   * closing itself a second after opening. A frame can fail for reasons that pass — a dropped
   * capture, a size the canvas has not caught up with — so the session now survives a run of
   * them and only gives up when it is clearly not transient.
   */
  let failures = 0;

  /**
   * Whether the destination sheet is expanded, on a phone.
   *
   * Collapsed while scanning so the camera keeps the screen, and opened by tapping the summary
   * line. It has no effect above 860px, where the panel sits beside the viewfinder.
   */
  let sheetOpen = $state(false);

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

  /**
   * `automatic` is set only by the one reopen after the system took the camera. A start the user
   * asked for clears that budget — otherwise the first automatic retry would spend it forever
   * and pressing the button again would do nothing but repeat the give-up message.
   */
  async function start(automatic = false) {
    if (!automatic) reopened = false;
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

    // Watch the stream itself, not only the frames taken from it.
    //
    // Reported symptom: the camera stops after a while with no message at all. That silence was
    // the bug's own fingerprint — nothing here observed the track. `grab()` reports what it
    // catches, so a *throwing* frame was visible, but a track that simply ends throws nothing:
    // the loop keeps ticking against a dead source and every frame is silently blank.
    //
    // Android ends a camera track for reasons that are not faults: another app takes the camera,
    // the screen locks, the activity is backgrounded, the system reclaims it under memory
    // pressure. All of them look identical from here, so all of them get named and, once, retried.
    for (const track of stream.getTracks()) {
      track.addEventListener("ended", () => {
        frameError = "The camera was released — reopening.";
        void restart();
      });
      track.addEventListener("mute", () => {
        frameError = "The camera stopped sending frames. Another app may have taken it.";
      });
      track.addEventListener("unmute", () => {
        frameError = null;
      });
    }

    if (video) {
      video.srcObject = stream;
      // A WebView pauses media of its own accord — on losing focus, on some memory pressure —
      // and a paused `<video>` yields the same blank frames as a dead track.
      video.addEventListener("pause", () => {
        if (running) frameError = "The camera was paused — resuming.";
        void video?.play().catch(() => {});
      });
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
    timer = setInterval(() => void grab(), onTouch ? FRAME_INTERVAL_TOUCH : FRAME_INTERVAL);
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
    lastConfirmed = null;
    failures = 0;
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

    const width = Math.min(onTouch ? CAPTURE_WIDTH_TOUCH : CAPTURE_WIDTH, sourceWidth);
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
      failures = 0;
      frameError = null;
      drawOutline(outcome, width, height);
      if (outcome.state === "confirmed" && outcome.card) accept(outcome.card);
    } catch (e) {
      failures += 1;
      // Shown over the viewfinder rather than only in the side panel, which on a phone sits
      // below the fold: an error nobody can see is an error nobody can report.
      frameError = `${e}`;
      if (failures >= MAX_FRAME_FAILURES) {
        error = `The camera stopped after ${failures} failed frames: ${e}`;
        stop();
      }
    } finally {
      inFlight = false;
    }
  }

  /**
   * Reopen the camera once after the system took it away.
   *
   * Once, deliberately. If the camera is gone for a reason that persists — permission revoked,
   * another app holding it — retrying in a loop turns one failure into a spinning one, and the
   * message the user needs is that it did not come back.
   */
  let reopened = false;
  async function restart() {
    if (!running || reopened) {
      if (reopened) frameError = "The camera closed again. Reopen the Scan tab to try afresh.";
      return;
    }
    reopened = true;
    stop();
    await start(true);
    if (running) frameError = null;
  }

  function accept(card: ScannedCard) {
    const known = pending.some((entry) => entry.oracleId === card.oracleId);
    pending = known
      ? pending.map((entry) =>
          entry.oracleId === card.oracleId ? { ...entry, quantity: entry.quantity + 1 } : entry,
        )
      : [...pending, { ...card, quantity: 1 }];
    lastConfirmed = card.name;
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
          : "#b8b1ac";

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

  /** Writes one entry to wherever the user chose. */
  async function write(entry: Pending) {
    if (destination === "deck" && deckId !== null) {
      // This can genuinely fail per card: the deck command looks the name up in the catalog
      // and refuses an oracle id it does not know, which happens when the artwork data is
      // newer than the card data.
      await api.deckAddCard(deckId, entry.oracleId, entry.quantity, zone);
      return;
    }
    // A draft pool is a physical holding in a container named after the pool. There is no
    // separate concept to invent: that is exactly what a pool is once the draft is over.
    const box = destination === "pool" ? poolName.trim() : container.trim();
    await api.collectionAdd({
      pool: destination === "digital" ? "digital" : "physical",
      oracle_id: entry.oracleId,
      name: entry.name,
      // The printing is left blank on purpose. Scanning identifies the *artwork*, and several
      // printings share one; resolving which set a card came from needs the printings artifact,
      // which does not exist yet. Guessing a set here would be worse than leaving it open.
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

  async function commit() {
    if (!pending.length || !destinationReady) return;
    committing = true;
    error = null;

    // Each entry leaves the list the moment its write lands, rather than clearing the whole
    // list at the end. If the tenth card fails — and it can, a deck refuses an oracle id its
    // catalog does not know — the first nine are already written, and a list that still held
    // them would add them a second time the moment the user pressed the button again.
    const queue = [...pending];
    let written = 0;

    try {
      for (const entry of queue) {
        await write(entry);
        pending = pending.filter((row) => row.oracleId !== entry.oracleId);
        written += 1;
      }
    } catch (e) {
      error =
        `${e}

${written} card${written === 1 ? "" : "s"} were added. ` +
        `The ones still listed were not, and are safe to retry.`;
    } finally {
      committing = false;
      if (written > 0) onCommitted();
    }
  }

  /** What the collapsed sheet says: where cards are going, in the words the user chose. */
  const destinationLabel = $derived(
    destination === "deck"
      ? (decks.find((deck) => deck.id === deckId)?.name ?? "Choose a deck")
      : destination === "pool"
        ? poolName.trim() || "Name the pool"
        : destination === "digital"
          ? "Digital collection"
          : "Physical collection",
  );

  const progress = $derived(
    result?.state === "tracking" && result.needed > 0 ? result.votes / result.needed : 0,
  );

  /** The card to present full-frame: the one just recognised, while it is still in view. */
  const showing = $derived(
    result?.state === "confirmed" || result?.state === "holding" ? result.card : null,
  );

  /**
   * How sure the match is, in words as well as bits.
   *
   * `margin` is how much worse the nearest *different* card was, so it measures whether anything
   * else was close — which is the question that matters. The bits stay visible beside the word
   * because a single adjective from an algorithm is not something to take on trust.
   */
  const certainty = $derived.by(() => {
    const margin = showing?.margin ?? 0;
    if (margin >= 24) return { word: "Certain", margin };
    if (margin >= 12) return { word: "Probable", margin };
    return { word: "Uncertain", margin };
  });

  /**
   * Undo the last automatic add.
   *
   * Recognition is confident enough to add without asking, which keeps both hands free for the
   * cards. That is only defensible if undoing is one tap and sits beside the result rather than
   * three screens away.
   */
  function reject(card: ScannedCard) {
    pending = pending
      .map((entry) =>
        entry.oracleId === card.oracleId ? { ...entry, quantity: entry.quantity - 1 } : entry,
      )
      .filter((entry) => entry.quantity > 0);
    lastConfirmed = null;
    rejected = card.name;
  }

  let rejected = $state<string | null>(null);
  $effect(() => {
    if (showing) rejected = null;
  });
</script>

<section class="scan">
  <div class="viewfinder">
    <!-- svelte-ignore a11y_media_has_caption -->
    <video bind:this={video} playsinline muted></video>
    <canvas bind:this={overlay} class="overlay"></canvas>

    <!-- One subject, the whole frame, over its own artwork. See docs/dev/design.md: this is the
         screen the direction was chosen for. It disappears by itself when the card leaves the
         frame, which is why it needs no dismiss button. -->
    {#if running && showing}
      <div class="cinema">
        {#if showing.imageArt}
          <img class="hero" src={showing.imageArt} alt="" />
        {/if}
        <div class="scrim"></div>
        <div class="fg">
          <p class="certainty" class:weak={certainty.margin < 12}>
            {certainty.word} · {certainty.margin} bits clear
          </p>
          <h3 class="cname">{showing.name}</h3>
          <p class="dest">
            {#if rejected === showing.name}
              Removed. Move the card out of frame and back to try again.
            {:else}
              Added to {destinationLabel}
            {/if}
          </p>
          {#if rejected !== showing.name}
            <div class="acts">
              <button type="button" class="act" onclick={() => reject(showing)}>
                Not this card
              </button>
            </div>
          {/if}
        </div>
      </div>
    {/if}

    {#if !running}
      <div class="curtain">
        {#if cameraError}
          <p class="headline">Camera unavailable</p>
          <p class="reason">{cameraError}</p>
          <button onclick={() => void start()}>Try again</button>
        {:else if status && !status.loaded}
          <!-- The camera still starts. Detection, rectification and the outline all work
               without the fingerprints — only naming a card does not — and blocking the button
               here made it impossible to find out whether the camera opens at all, which is the
               one thing a fresh install needs to tell us. -->
          <p class="headline">No artwork fingerprints installed</p>
          <p class="hint">
            The camera works, and the outline will show what it finds — but nothing can be named
            without <code>arthashes.bin</code>, an optional download of about 6 MB. Build it with:
          </p>
          <pre>cargo run --release -p build-artifacts -- --art-only</pre>
          {#if status.error}<p class="reason">{status.error}</p>{/if}
          <button class="primary" onclick={() => void start()}>Start the camera anyway</button>
          <button onclick={() => void api.scanReload().then((s) => (status = s))}>
            Check again
          </button>
        {:else}
          <p class="headline">Ready to scan</p>
          <p class="hint">
            Hold one card at a time over a plain <strong>mid-grey or wooden</strong> surface —
            not a black one. Magic cards have a black border, and on a dark table there is
            nothing to tell the two apart. Recognition uses the artwork, so the card's language
            does not matter.
          </p>
          <button class="primary" onclick={() => void start()}>Start the camera</button>
        {/if}
      </div>
    {:else}
      <div class="readout" class:found={result?.state === "confirmed" || result?.state === "holding"}>
        {#if frameError}
          <span class="name muted">{frameError}</span>
        {:else if result?.state === "confirmed" || result?.state === "holding"}
          <span class="name">{result.card?.name ?? lastConfirmed ?? "Added"}</span>
          <span class="tag">added</span>
        {:else if result?.state === "tracking"}
          <span class="name">{result.card?.name}</span>
          <span class="ring" style="--fill: {progress}"></span>
        {:else if result?.quad}
          <!-- The outline found an edge but the crop did not match anything. On a dark table
               that edge is usually the card's *interior* rather than its border, so the
               background is the first thing to change. -->
          <span class="name muted">Card seen, no match — try a lighter background</span>
        {:else}
          <span class="name muted">Looking for a card…</span>
        {/if}
      </div>
      <button class="stop" onclick={stop}>Stop</button>
    {/if}
  </div>

  <div class="panel" class:open={sheetOpen}>
    <!-- Only rendered as a control on a phone; on a desktop it is hidden and the panel is
         simply always open. -->
    <button
      type="button"
      class="handle"
      aria-expanded={sheetOpen}
      onclick={() => (sheetOpen = !sheetOpen)}
    >
      <span class="grab" aria-hidden="true"></span>
      <span class="summary">
        {destinationLabel}{#if totalPending > 0} · {totalPending} scanned{/if}
      </span>
    </button>

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
            <!-- Locked while the batch is being written: the commit loop walks a snapshot of
                 this list and removes each row as its write lands, so editing underneath it
                 would be editing rows that are already on their way to the database. -->
            <div class="quantity">
              <button
                disabled={committing}
                onclick={() => adjust(entry.oracleId, -1)}
                aria-label="One fewer">−</button>
              <span>{entry.quantity}</span>
              <button
                disabled={committing}
                onclick={() => adjust(entry.oracleId, 1)}
                aria-label="One more">+</button>
            </div>
            <button
              class="remove"
              disabled={committing}
              onclick={() => remove(entry.oracleId)}
              aria-label="Remove">
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
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 18px;
    box-shadow: var(--sheen), var(--lift);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  video,
  /* ---- the full-frame result ------------------------------------------------
     Every value here comes from docs/dev/design.md. The artwork is `art_crop`, never a whole
     card: a card image carries its own border, title and text box, and a name laid over one
     collides with them. */
  .cinema {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    background: var(--ground);
    overflow: hidden;
    animation: rise 200ms ease-out;
  }

  @media (prefers-reduced-motion: reduce) {
    .cinema { animation: none; }
  }

  @keyframes rise {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .cinema .hero {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  /* Bottom-anchored and ending opaque, so the name lands on solid ground rather than on
     whatever the painting happens to have put there. */
  .cinema .scrim {
    position: absolute;
    inset: 0;
    background: var(--scrim);
  }

  .cinema .fg {
    position: relative;
    padding: 24px 22px 26px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .cinema .certainty {
    margin: 0;
    font-size: var(--t-meta);
    font-weight: 600;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--gold);
  }

  /* An uncertain match says so in the danger colour rather than in smaller type. The one thing
     this screen must never do is present a guess with the confidence of a fact. */
  .cinema .certainty.weak {
    color: var(--danger);
  }

  .cinema .cname {
    margin: 0;
    font-size: var(--t-hero);
    font-weight: 700;
    line-height: 1.04;
    letter-spacing: -0.03em;
    color: var(--ink);
    text-wrap: balance;
  }

  .cinema .dest {
    margin: 0;
    font-size: var(--t-body);
    color: var(--ink-2);
  }

  .cinema .acts {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin-top: 4px;
  }

  /* A pill, and a real one: 44px tall, because this is the control someone reaches for while
     holding a card in the other hand. */
  .cinema .act {
    min-height: 44px;
    padding: 0 22px;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.28);
    background: rgba(255, 255, 255, 0.12);
    color: var(--ink);
    font-size: var(--t-body);
    font-weight: 600;
  }

  .cinema .act:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.2);
    border-color: rgba(255, 255, 255, 0.42);
  }

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
    font-size: var(--t-meta);
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
    background: rgba(8, 8, 10, 0.9);
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
    font-size: var(--t-meta);
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

  .handle {
    display: none;
  }

  .panel {
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 18px;
    background: rgba(20, 20, 24, 0.8);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    box-shadow: var(--sheen), var(--lift);
    padding: 22px;
    overflow-y: auto;
    min-height: 0;
  }

  /* A destination is a choice, not a field caption. The global `label` rule sets captions in
     spaced uppercase, which turned these four options into shouting fragments that wrapped. */
  .destinations label {
    display: flex;
    align-items: center;
    gap: 11px;
    margin: 0;
    padding: 0 14px;
    min-height: 44px;
    border-radius: 12px;
    border: 1px solid transparent;
    background: rgba(255, 255, 255, 0.04);
    color: var(--ink-2);
    font-size: var(--t-body);
    font-weight: 500;
    letter-spacing: 0;
    text-transform: none;
    cursor: pointer;
  }

  .destinations label:hover {
    background: rgba(255, 255, 255, 0.09);
    color: var(--ink);
  }

  .destinations label.active {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.24);
    color: var(--ink);
    font-weight: 600;
  }

  .destinations input[type="radio"] {
    width: 17px;
    height: 17px;
    min-height: 0;
    flex: none;
    accent-color: var(--accent);
  }

  h3 {
    margin: 0 0 10px;
    font-size: var(--t-meta);
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
    font-size: var(--t-meta);
  }

  .destinations {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 18px;
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
    font-size: var(--t-meta);
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
    font-size: var(--t-meta);
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
    font-size: var(--t-meta);
    margin-top: 10px;
    /* The commit failure message is two paragraphs: what went wrong, then what was written. */
    white-space: pre-line;
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

  /* The side panel no longer fits beside the viewfinder. */
  @media (max-width: 1180px) and (min-width: 861px) {
    .scan {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(280px, 1fr) auto;
    }

    .panel {
      overflow-y: visible;
    }
  }

  /* Phone: the camera owns the screen and everything else floats over it.
     Splitting the height between a viewfinder and a panel gave the camera a letterbox, on the
     one screen that exists *for* the phone. See docs/dev/ui.md. */
  @media (max-width: 860px) {
    .scan {
      display: block;
      position: relative;
      height: 100%;
      gap: 0;
    }

    .viewfinder {
      position: absolute;
      inset: 0;
      border: 0;
      border-radius: 0;
    }

    /* The sheet sits on the app's gutter like every other card, not on a number of its own.
       It floats over the viewfinder, so it also keeps the safe-area inset the full-bleed
       viewfinder underneath has no use for. */
    .panel {
      position: absolute;
      left: var(--gutter);
      right: var(--gutter);
      bottom: calc(var(--gutter) + env(safe-area-inset-bottom, 0px));
      /* Collapsed to the handle: enough to say where cards are going and how many, no more. */
      max-height: 52px;
      padding: 0 12px 10px;
      background: rgba(20, 20, 24, 0.96);
      backdrop-filter: blur(8px);
      border-color: rgba(255, 255, 255, 0.12);
      overflow: hidden;
      transition: max-height 0.18s ease;
    }

    .panel.open {
      max-height: 72%;
      overflow-y: auto;
    }

    .handle {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 5px;
      width: 100%;
      /* Sticky so the summary stays reachable once the sheet is scrolled. */
      position: sticky;
      top: 0;
      background: inherit;
      /* The one control between the user and the camera; it gets a real target. */
      min-height: 44px;
      padding: 8px 0 6px;
      border: 0;
      border-radius: 0;
      color: var(--text);
      font-size: 13px;
    }

    .grab {
      width: 34px;
      height: 3px;
      border-radius: 2px;
      background: var(--border-strong);
    }

    .summary {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 100%;
    }

    /* Over a camera image, a translucent readout is unreadable. */
    .readout {
      bottom: 70px;
      background: rgba(8, 8, 10, 0.95);
    }

    .curtain {
      padding: 20px 16px;
    }

    /* Below the 44px rule, and these sit in a scrolling list where a mis-tap deletes a row. */
    .quantity button,
    .remove {
      width: 34px;
      height: 34px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .panel {
      transition: none;
    }
  }
</style>
