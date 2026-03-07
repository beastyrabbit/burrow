import path from "node:path";

export function buildTauriDevOverride(config) {
  return {
    build: {
      beforeDevCommand: `node "${path.join(config.repoRoot, "scripts/portless-dev.mjs")}"`,
      devUrl: config.frontendUrl,
    },
  };
}

export function registerTempFileCleanup(processRef, cleanup) {
  let cleanedUp = false;

  const runCleanup = () => {
    if (cleanedUp) {
      return;
    }

    cleanedUp = true;
    cleanup();
  };

  processRef.on("exit", runCleanup);

  for (const signal of ["SIGINT", "SIGTERM"]) {
    const handler = () => {
      runCleanup();
      processRef.removeListener?.(signal, handler);
      processRef.kill?.(processRef.pid, signal);
    };

    processRef.on(signal, handler);
  }
}
