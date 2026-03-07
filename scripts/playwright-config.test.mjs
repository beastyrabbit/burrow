import assert from "node:assert/strict";
import test from "node:test";

import { buildFrontendWebServer, buildPlaywrightUse } from "./playwright-config.mjs";

test("https playwright browser settings ignore certificate errors", () => {
  assert.deepEqual(buildPlaywrightUse("https://burrow.localhost:1355"), {
    baseURL: "https://burrow.localhost:1355",
    headless: true,
    ignoreHTTPSErrors: true,
  });
});

test("https playwright web server polling ignores certificate errors", () => {
  assert.deepEqual(buildFrontendWebServer("https://burrow.localhost:1355"), {
    command: "pnpm dev",
    url: "https://burrow.localhost:1355",
    reuseExistingServer: true,
    timeout: 10000,
    ignoreHTTPSErrors: true,
  });
});
