import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// https://vite.dev/config/
const isTauriBuild = !!process.env.TAURI_ENV_PLATFORM;

export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    alias: isTauriBuild
      ? {}
      : {
          "@tauri-apps/api/core": path.resolve(
            __dirname,
            "src/mock-tauri.ts"
          ),
          "@tauri-apps/api/event": path.resolve(
            __dirname,
            "src/mock-tauri.ts"
          ),
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
}));
