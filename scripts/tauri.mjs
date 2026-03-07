import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { resolvePortlessConfig } from "./portless-resolver.mjs";

const args = process.argv.slice(2);
const command = args[0];
const commandArgs = args.slice(1);
const config = resolvePortlessConfig();

function spawnTauri(extraArgs = []) {
  const child = spawn("tauri", extraArgs, {
    cwd: config.repoRoot,
    env: process.env,
    stdio: "inherit",
  });

  child.on("error", (error) => {
    console.error(`Failed to start tauri: ${error.message}`);
    process.exit(1);
  });

  child.on("exit", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 1);
  });
}

if (command !== "dev") {
  spawnTauri(args);
} else {
  const overridePath = path.join(
    os.tmpdir(),
    `burrow-tauri-dev-${process.pid}.json`,
  );
  const overrideConfig = {
    build: {
      beforeDevCommand: `node "${path.join(config.repoRoot, "scripts/portless-dev.mjs")}"`,
      devUrl: config.frontendUrl,
    },
  };

  fs.writeFileSync(overridePath, `${JSON.stringify(overrideConfig, null, 2)}\n`);
  process.on("exit", () => fs.rmSync(overridePath, { force: true }));

  spawnTauri(["dev", "--config", overridePath, ...commandArgs]);
}
