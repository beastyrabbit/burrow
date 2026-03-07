import { defineConfig } from "@playwright/test";
import {
  buildFrontendWebServer,
  buildPlaywrightUse,
} from "./scripts/playwright-config.mjs";
import { resolvePortlessConfig } from "./scripts/portless-resolver.mjs";

const e2eDataDir = `/tmp/burrow-e2e-${process.pid}`;
const { frontendUrl } = resolvePortlessConfig();

// Make available to global teardown
process.env.BURROW_DATA_DIR = e2eDataDir;

export default defineConfig({
  testDir: "./e2e",
  timeout: 15000,
  retries: 0,
  globalTeardown: "./e2e/global-teardown.ts",
  use: buildPlaywrightUse(frontendUrl),
  webServer: [
    {
      command: "cargo run --bin test-server",
      cwd: "./src-tauri",
      port: 3001,
      reuseExistingServer: true,
      timeout: 120000, // First cold build may be slow; subsequent runs are instant
      env: { ...process.env, BURROW_DRY_RUN: "1", BURROW_DATA_DIR: e2eDataDir },
    },
    buildFrontendWebServer(frontendUrl),
  ],
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
