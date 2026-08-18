# Android build

> **Status, 2026-08-18** — **an APK exists.** `app-arm64-debug.apk`, 154 MB, package
> `dev.eraflo.magicoptimizer`, with the `CAMERA` permission confirmed present by `aapt2 dump
> permissions` rather than assumed from the manifest. The whole Rust workspace builds into a real
> `aarch64-linux-android` shared library, so invariant 1 holds against a real NDK rather than a
> `cargo check`.
>
> **Nothing has run on a device yet.** Whether `getUserMedia` works in the Android WebView is
> still open, and no amount of building answers it.

## Prerequisites

- Android SDK, **NDK**, and **SDK Command-line Tools** — the last is easy to miss, and Tauri
  refuses without it with a message that only says it "skipped" installing them.
- A **JDK 17 or 21**. Not 25: the generated project uses Android Gradle Plugin 8.11, which does
  not support it, and Android Studio's bundled JBR *is* 25. Not 11 or 8 either.
- `ANDROID_HOME`, `NDK_HOME` and `JAVA_HOME` set.
- All four Rust targets.

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi
rustup target add i686-linux-android x86_64-linux-android
npm run tauri -- android init     # already done; the project is committed
npm run tauri -- android build --debug --target aarch64
```

`npm run tauri` rather than `cargo tauri`: the CLI is an npm devDependency here, so there is
nothing extra to install.

## Building on Windows

Two failures met on a real machine, neither about this project's code, both worth an hour if
rediscovered.

**Symlinks.** Tauri links the built `.so` into `app/src/main/jniLibs/<abi>/`, and Windows refuses
without Developer Mode or an elevated shell: *Creation symbolic link is not allowed for this
system*. Enable Developer Mode, or use the recipe below.

**The JDK.** Android Studio ships JBR 25 and Gradle answers `Unsupported class file major version
69`. Install a JDK 21 and point `JAVA_HOME` at it — Android Studio can fetch one from
*Settings -> Build Tools -> Gradle -> Gradle JDK -> Download JDK*, or `winget install
Microsoft.OpenJDK.21`.

### The recipe that works without Developer Mode

Three steps, and the third carries the subtlety. Tauri's Gradle plugin adds a
`rustBuild<Abi><Variant>` task that re-invokes the Tauri CLI through `npm`, which fails inside
Gradle — and would only walk back into the symlink anyway. The library is already in place by
then, so that task is skipped.

```powershell
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:NDK_HOME     = "$env:ANDROID_HOME\ndk\<version>"
$env:JAVA_HOME    = "C:\Program Files\Microsoft\jdk-21.0.12.8-hotspot"

# 1. Build the Rust library. This succeeds; only the symlink afterwards fails.
npm run tauri -- android build --debug --target aarch64

# 2. Put it where Gradle expects it.
Copy-Item target\aarch64-linux-android\debug\libmagicoptimizer_lib.so `
          src-tauri\gen\android\app\src\main\jniLibs\arm64-v8a\

# 3. Assemble, skipping the task that would rebuild it.
cd src-tauri\gen\android
.\gradlew.bat assembleArm64Debug -x :app:rustBuildArm64Debug
```

The APK lands in `app/build/outputs/apk/arm64/debug/`, signed with the debug key, so it installs
straight onto a phone.

**It is 154 MB**, because a debug build carries its symbols. Fine for trying it, wrong for
shipping: a release build needs signing keys this project does not have yet, and sorting that out
is what stands between here and an APK in a GitHub release.

## The nightly release deletes and recreates itself

Every push to `main` deletes the `nightly` release and makes a new one, so **anything uploaded by
hand disappears at the next push**. That happened once: an APK was attached manually and was gone
within the hour.

Whatever belongs in a release has to be built by the release. `nightly.yml` therefore has an
`android` job that builds the APK on Ubuntu — where the jniLibs symlink causes no trouble — with
a Temurin 21 toolchain, and attaches it. Do not go back to uploading by hand.

## The version cannot be 0.0.0

Android refuses it outright. That is why the workspace moved to `0.0.1`; `Cargo.toml`,
`package.json` and `tauri.conf.json` have to agree.

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

1. **The `CAMERA` permission.** `android init` does not add it, so it was added by hand and is
   committed. **Re-running `android init` regenerates the manifest and drops it again** — if
   scanning suddenly cannot open the camera, look there first.

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
