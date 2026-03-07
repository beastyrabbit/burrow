import assert from "node:assert/strict";
import test from "node:test";

import { buildFrontendWebServer, buildPlaywrightUse } from "./playwright-config.mjs";

test("playwright browser settings stay protocol-agnostic", () => {
  assert.deepEqual(buildPlaywrightUse("https://burrow.localhost:1355"), {
    baseURL: "https://burrow.localhost:1355",
    headless: true,
  });
});

test("playwright frontend web server uses the shared dev url without HTTPS-only flags", () => {
  assert.deepEqual(buildFrontendWebServer("https://burrow.localhost:1355"), {
    command: "pnpm dev",
    url: "https://burrow.localhost:1355",
    reuseExistingServer: true,
    timeout: 10000,
  });
});
