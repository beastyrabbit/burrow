import test from "node:test";
import assert from "node:assert/strict";

import {
  computeAppName,
  computeFrontendUrl,
  isPortlessEnabled,
  normalizeWorktreeSlug,
  resolvePortlessConfig,
} from "./portless-resolver.mjs";

test("main worktree keeps the base burrow app name", () => {
  assert.equal(computeAppName({ baseName: "burrow", isMainWorktree: true, branchName: "main" }), "burrow");
});

test("linked worktrees prepend a normalized branch slug", () => {
  assert.equal(
    computeAppName({
      baseName: "burrow",
      isMainWorktree: false,
      branchName: "t3code/add-portless-integration",
    }),
    "add-portless-integration.burrow",
  );
});

test("branch normalization uses the last path segment", () => {
  assert.equal(normalizeWorktreeSlug("feature/foo_bar"), "foo-bar");
});

test("frontend url uses the portless localhost gateway", () => {
  assert.equal(computeFrontendUrl("add-portless-integration.burrow"), "http://add-portless-integration.burrow.localhost:1355");
});

test("PORTLESS_HTTPS does not change the http-only Portless dev URL", () => {
  assert.match(
    resolvePortlessConfig({
      cwd: process.cwd(),
      env: { ...process.env, PORTLESS_HTTPS: "1" },
    }).frontendUrl,
    /^http:\/\//,
  );
});

test("PORTLESS=false disables the Portless wrapper", () => {
  assert.equal(isPortlessEnabled({ PORTLESS: "false" }), false);
});
