# Android build

> **Status** — design document. Shipped in phase 6. Exact procedures will be completed and
> verified then.

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
cross-compiles cleanly to `aarch64-linux-android`. The current stack (`rkyv`, `redb`,
`fixedbitset`, `imageproc`) is pure Rust. Keep it that way.

## Camera

`CAMERA` permission in the manifest. The default approach is `getUserMedia` in the WebView, whose
behavior varies across Android System WebView versions.

**Planned fallback: a Kotlin CameraX plugin**, behind the `FrameSource` trait — the boundary is
already in place so the switch touches nothing else.

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
(68 kB of JavaScript, 25 kB gzipped).

For the vision pipeline, the dominant cost is transferring the frame from the WebView into Rust,
not computing the hash. Measure before optimizing, but hold the 5–10 fps processing target on a
mid-range phone.

The optimizer's Monte Carlo runs **single-threaded and bounded** on Android, against `rayon` on
desktop: do not saturate a phone's cores or drain its battery.

## Storage

`redb` and the mmap'd artifacts go in the app's private data directory. The heavy artifacts —
`arthashes.bin`, `embeddings.bin` — are optional downloads: someone who never scans cards should
not have to fetch them.
