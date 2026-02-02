import { defineConfig } from "@playwright/test";

const e2eDataDir = `/tmp/burrow-e2e-${process.pid}`;

// Make available to global teardown
process.env.BURROW_DATA_DIR = e2eDataDir;

export default defineConfig({
  testDir: "./e2e",
  timeout: 15000,
  retries: 0,
  globalTeardown: "./e2e/global-teardown.ts",
  use: {
    baseURL: "http://localhost:1420",
    headless: true,
  },
  webServer: {
    command: "pnpm tauri dev",
    port: 1420,
    reuseExistingServer: true,
    timeout: 60000,
    env: { ...process.env, BURROW_DRY_RUN: "1", BURROW_DATA_DIR: e2eDataDir },
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
