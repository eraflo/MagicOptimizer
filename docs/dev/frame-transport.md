# Getting camera frames into Rust

> **Status** — research and a design decision, prompted by a real symptom: on a phone the camera
> died after fifteen to twenty seconds. The first response to that was a guess (smaller frames,
> fewer per second) rather than a diagnosis, and this document exists because a guess is not a
> design.

## What the app does today, and why it is the wrong shape

`ScanView.grab()` runs on a `setInterval`, and each tick:

1. draws the `<video>` into a `<canvas>`,
2. calls `getImageData()` — **which allocates a fresh `ImageData` every frame**,
3. converts RGBA to greyscale in a JavaScript loop on the main thread,
4. sends ~300 KB over the IPC bridge.

Three things are wrong with that, independent of any measurement:

* **It allocates per frame.** `getImageData` returns a new buffer each call; at ten frames a
  second that is megabytes a second for the WebView to collect. This is the leading suspect for
  the phone dying after twenty seconds, and it is a suspicion, not a finding.
* **It works on the main thread**, the same thread painting the viewfinder.
* **It is timer-driven, not frame-driven.** A `setInterval` has no idea when the camera actually
  produced a frame, so it processes duplicates and misses others.

## What the web platform offers instead

Searching the current guidance, there is an established answer and it is not "use a smaller
canvas".

**`MediaStreamTrackProcessor`** turns a track into a `ReadableStream` of `VideoFrame` objects, and
`VideoFrame.copyTo(buffer)` writes into **a buffer you own and reuse**. That removes the
per-frame allocation entirely — the thing most likely to be killing the session. Chrome's own
WebCodecs guidance adds that because frame callbacks fire many times a second, handling them
**belongs in a worker** rather than on the main thread.

**`requestVideoFrameCallback`** replaces the interval: it fires once per actual decoded frame, so
nothing is processed twice and nothing is missed.

Together those three changes address every item in the list above, and none of them is a
constant to be tuned.

**One thing the search did not settle: whether `MediaStreamTrackProcessor` exists in the Android
System WebView**, whose version varies by device and by Play Store updates. So it cannot simply
be adopted — it has to be feature-detected, with the canvas path kept as the fallback. Writing
that down rather than assuming is the point.

## The idea the research made obvious

Every option above makes the *frame* cheaper. None of them questions why a frame crosses the
boundary at all.

It does not have to. The pipeline is:

```
detect → rectify → crop the artwork → hash → match
         └──────────── in Rust today ────────────┘
```

Only the last step needs the artwork database. Everything before it is arithmetic on pixels, and
the payload shrinks at every stage:

| Cut the boundary here | Bytes per frame | Factor |
|---|---:|---:|
| Whole greyscale frame — today | 307,200 | 1× |
| After rectification (488×680) | 331,840 | worse |
| **After cropping the artwork** | ~150,000 | 2× |
| **After hashing (17×16 samples)** | **272** | **1,100×** |
| The hash itself | 32 | 9,600× |

So the real fix is not a faster transport. It is **moving the cut**: do detection, rectification
and the box-sampling down to a 17×16 grid on the WebView side, and send Rust either those 272
bytes or the 32-byte hash. A 300 KB per-frame problem becomes a 272-byte one, and every
allocation and GC concern goes with it.

The cost is that `mtg-vision`'s detection and hashing would need a second implementation on the
JavaScript side — and **the greyscale weights and the crop fractions must match exactly**, which
is already a documented trap: the published hashes were computed with 77/150/29 and the `ART_*`
fractions, and a divergence would silently unmatch the whole database.

That argues for compiling `mtg-vision` to **WebAssembly** rather than rewriting it. Same code,
same constants, no second implementation to keep honest — and the crate is already pure Rust
with no image codec, which is exactly what makes it portable. Only the matcher, which needs the
6 MB artwork archive, stays native.

## The other end of the spectrum

For Android specifically there is a cleaner answer still: **a Kotlin CameraX plugin** feeding
frames straight into Rust, so nothing crosses the JavaScript boundary in the first place. This
has been named as the fallback since phase 6. It is the right architecture and the largest
amount of work, and it only solves Android.

## Ranked, by what each buys against what it costs

1. **`requestVideoFrameCallback` instead of the interval.** Small, safe, and it stops processing
   duplicate frames. Do this regardless of what follows.
2. **`MediaStreamTrackProcessor` with a reused buffer, feature-detected**, canvas kept as
   fallback. Removes the per-frame allocation, which is the leading suspect.
3. **Move the work into a worker.** Chrome's guidance, and it stops the pipeline competing with
   the viewfinder for the main thread.
4. **`mtg-vision` to WebAssembly, sending 272 bytes.** The structural fix. Makes 1–3 mostly
   unnecessary and removes the whole class of problem.
5. **CameraX plugin.** Best on Android, Android only, most work.

## What is not yet known

The camera stopping has **not been diagnosed**. Everything above is reasoning from the shape of
the code, and the shape is genuinely wrong — but the error text now shown over the viewfinder is
what would confirm which of these actually matters. Fixing 1–3 blind may well work and still
leave nobody any wiser.
