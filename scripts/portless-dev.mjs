import { spawn } from "node:child_process";

import {
  ensurePortlessAvailable,
  FALLBACK_VITE_PORT,
  resolvePortlessConfig,
} from "./portless-resolver.mjs";

const config = resolvePortlessConfig();
const viteArgs = process.argv.slice(2);

if (config.usePortless) {
  ensurePortlessAvailable();
}

const command = config.usePortless ? "portless" : "vite";
const args = config.usePortless ? [config.appName, "vite", ...viteArgs] : viteArgs;

if (!config.usePortless) {
  console.error(`Portless disabled; using http://localhost:${FALLBACK_VITE_PORT}`);
}

const child = spawn(command, args, {
  cwd: config.repoRoot,
  env: process.env,
  stdio: "inherit",
});

child.on("error", (error) => {
  console.error(`Failed to start ${command}: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
