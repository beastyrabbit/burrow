import assert from "node:assert/strict";
import test from "node:test";

import { buildTauriDevOverride, registerTempFileCleanup } from "./tauri-helpers.mjs";

test("buildTauriDevOverride points Tauri at the shared Portless dev command", () => {
  assert.deepEqual(
    buildTauriDevOverride({
      repoRoot: "/repo",
      frontendUrl: "https://burrow.localhost:2468",
    }),
    {
      build: {
        beforeDevCommand: 'node "/repo/scripts/portless-dev.mjs"',
        devUrl: "https://burrow.localhost:2468",
      },
    },
  );
});

test("registerTempFileCleanup removes temp files on SIGTERM", () => {
  const handlers = new Map();
  let cleanupCalls = 0;
  const processRef = {
    on(event, handler) {
      handlers.set(event, handler);
    },
  };

  registerTempFileCleanup(processRef, () => {
    cleanupCalls += 1;
  });

  handlers.get("SIGTERM")?.();

  assert.equal(cleanupCalls, 1);
});
