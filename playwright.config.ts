import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 15000,
  retries: 0,
  use: {
    baseURL: "http://localhost:1420",
    headless: true,
  },
  webServer: {
    command: "pnpm dev",
    port: 1420,
    reuseExistingServer: true,
    timeout: 10000,
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
