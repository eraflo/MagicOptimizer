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

## Performance

The WebView is the bottleneck, which is why the frontend is Svelte with a light bundle.

For the vision pipeline, the dominant cost is transferring the frame from the WebView into Rust,
not computing the hash. Measure before optimizing, but hold the 5–10 fps processing target on a
mid-range phone.

The optimizer's Monte Carlo runs **single-threaded and bounded** on Android, against `rayon` on
desktop: do not saturate a phone's cores or drain its battery.

## Storage

`redb` and the mmap'd artifacts go in the app's private data directory. The heavy artifacts —
`arthashes.bin`, `embeddings.bin` — are optional downloads: someone who never scans cards should
not have to fetch them.
