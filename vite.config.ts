import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri runs the dev server itself and expects a fixed port, so failures are loud rather than
// silently moving to another port and leaving the window blank.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Rust sources are watched by cargo, not by Vite.
      ignored: ["**/src-tauri/**", "**/target/**", "**/artifacts/**", "**/.cache/**"],
    },
  },
  build: {
    // The Android WebView is the bottleneck: keep the bundle small and skip sourcemaps in
    // release builds. See docs/dev/android.md.
    target: "es2021",
    sourcemap: false,
  },
});
