import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri convention: https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [react()],

  // prevent vite from obscuring rust errors
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  // env prefix for tauri variables
  envPrefix: ["VITE_", "TAURI_"],
});