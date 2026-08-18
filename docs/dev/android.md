# Android build

> **Status** — the code is in place and the domain crates are **verified** to cross-compile to
> `aarch64-linux-android`, which CI now enforces. The steps below that need an actual SDK, NDK
> or device — `android init`, the manifest edit, and whether `getUserMedia` behaves in the
> WebView — have **not** been run. They are marked where they appear. Treat them as the plan,
> not as a report.

## Prerequisites

- Android SDK and **NDK**
- `ANDROID_HOME` and `NDK_HOME` set
- Rust targets: `aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`,
  `x86_64-linux-android`

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi
cargo tauri android init
cargo tauri android dev
```

## The main pitfall: native dependencies

**This is why this project does not use SQLite.**

`rusqlite` / `libsqlite3-sys` cross-compiled to Android causes documented, recurring problems:
`aarch64-linux-android-clang` not found, unresolved symbols when linking against the bundled
`sqlite3.c`, runtime crashes on the emulator at the first call. The time spent debugging a C
toolchain far exceeds the benefit, for a need that 35,000 in-memory entries cover comfortably.

**Rule: before adding a dependency, check it pulls in neither C nor C++**, or that it
cross-compiles cleanly to `aarch64-linux-android`. The domain crates currently pull in `rkyv`,
`redb`, `memmap2`, `unicode-normalization`, `serde` and `thiserror` — all pure Rust —
and nothing else. `mtg-vision`
deliberately has no image codec at all: frames arrive from the WebView canvas already decoded,
so there was never a reason to carry one.

CI checks this rather than trusting it:

```bash
cargo check --target aarch64-linux-android \
  -p mtg-core -p mtg-data -p mtg-collection -p mtg-deck \
  -p mtg-optimizer -p mtg-combo -p mtg-journal -p mtg-vision
```

`cargo check` does not link, so this needs the Rust target but neither the SDK nor the NDK — and
a C dependency fails it at build-script time, which is the violation worth catching. Verified
passing on 2026-08-17.

## Camera

The frame path is `getUserMedia` → `<canvas>` → greyscale in JavaScript → raw IPC → `mtg-vision`.
The greyscale conversion happens on the JavaScript side using the same 77/150/29 weights as
`mtg_vision::rgba_to_gray`, which cuts the bytes crossing the boundary to a quarter. Frames go
over as a **raw IPC body**, not as a command argument: a 640×480 frame passed as a `number[]`
would be three hundred thousand JSON numbers, ten times a second.

**Not yet verified on a device.** Two things need checking the first time this runs on hardware:

1. **The `CAMERA` permission**, which `cargo tauri android init` does not add. After running it
   once, add to `src-tauri/gen/android/app/src/main/AndroidManifest.xml`:

   ```xml
   <uses-permission android:name="android.permission.CAMERA" />
   <uses-feature android:name="android.hardware.camera" android:required="false" />
   ```

   `required="false"` so the app still installs on a device without a rear camera; scanning is
   one feature, not the application.

2. **Whether the WebView grants the request.** `getUserMedia` needs both the Android permission
   and the WebView's own `onPermissionRequest` to be granted. The app is served from
   `http://tauri.localhost`, which Chromium treats as a potentially trustworthy origin, so the
   secure-context requirement should be satisfied — but "should" is doing real work in that
   sentence and it has not been observed.

`ScanView` surfaces whatever `getUserMedia` throws instead of swallowing it, so the first device
run will say what actually went wrong.

**Fallback if the WebView proves unreliable: a Kotlin CameraX plugin.** This is a plan, not a
prepared boundary — the frame source is currently the `grab()` function in `ScanView.svelte`, and
swapping it would mean introducing the abstraction at that point.

## Layout

The interface is already built for a phone, so phase 6 is about the camera rather than about
redoing the UI. Three steps, all driven by CSS media queries with no JavaScript involved:

| Width | Layout |
|---|---|
| above 1180px | Three columns: filters, results, card detail |
| 860–1180px | Filters collapse into a drawer over the results |
| below 860px | One column; selecting a card opens the detail as a full-screen sheet |

Two details worth keeping:

* **Touch sizing keys on `pointer: coarse`, not on width.** A narrow window on a desktop still
  has a mouse, and a large tablet still has fingers. Inputs are 16px there because anything
  smaller makes some WebViews zoom on focus.
* `body` carries `env(safe-area-inset-*)` padding, paired with `viewport-fit=cover`, so content
  clears notches and gesture bars.

## Performance

The WebView is the bottleneck, which is why the frontend is Svelte with a light bundle
(106 kB of JavaScript, 38 kB gzipped).

For the vision pipeline, the dominant cost is transferring the frame from the WebView into Rust,
not computing the hash. Measure before optimizing, but hold the 5–10 fps processing target on a
mid-range phone. Three things are already in place for it:

* Frames are captured at **640 wide**, not at the sensor's resolution. Detection works at 320
  internally and the artwork hash is box-sampled to a 17×16 grid, so more pixels buy nothing.
* The scanner **drops frames that arrive mid-recognition** rather than queueing them. The voter
  needs agreement, not every frame, and an unbounded queue on a slow device is a stall that
  looks like a crash.
* The greyscale buffer is **reused across frames**, so a video stream does not allocate three
  hundred kilobytes ten times a second.

The optimizer's Monte Carlo is **single-threaded everywhere**, with its own PRNG so a search is
reproducible. An earlier version of this document claimed it used `rayon` on desktop; it never
has, and the workspace carries no thread pool at all. If parallelism is ever added, it must stay
bounded on Android — do not saturate a phone's cores or drain its battery.

## Storage

`redb` and the mmap'd artifacts go in the app's private data directory. The heavy artifacts —
`arthashes.bin`, `embeddings.bin` — are optional downloads: someone who never scans cards should
not have to fetch them.
