import { rmSync } from "fs";

export default function globalTeardown() {
  const dataDir = process.env.BURROW_DATA_DIR;
  if (dataDir && dataDir.startsWith("/tmp/burrow-e2e-")) {
    try {
      rmSync(dataDir, { recursive: true, force: true });
    } catch (e) {
      console.warn(`[global-teardown] failed to clean up ${dataDir}:`, e);
    }
  }
}
