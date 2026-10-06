import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port; the frontend lives in ui/ so Vite doesn't watch target/ or the data files.
export default defineConfig({
  root: "ui",
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "../dist", emptyOutDir: true },
});
