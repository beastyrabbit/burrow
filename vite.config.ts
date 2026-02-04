import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// @ts-expect-error process.env available in Vite config (Node.js) but not in DOM types
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  resolve: {
    // Always alias @tauri-apps/api to mock-tauri.ts
    // mock-tauri.ts checks __TAURI_INTERNALS__ at runtime:
    // - In Tauri webview: delegates to real Tauri API
    // - In browser: uses HTTP bridge on port 3001
    alias: {
      "@tauri-apps/api/core": path.resolve(__dirname, "src/mock-tauri.ts"),
      "@tauri-apps/api/event": path.resolve(__dirname, "src/mock-tauri.ts"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
