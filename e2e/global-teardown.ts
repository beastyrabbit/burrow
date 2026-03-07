import { rmSync, unlinkSync } from "fs";
import { tmpdir } from "os";

export default function globalTeardown() {
  const rootDir = process.env.BURROW_E2E_ROOT_DIR;
  const markerPath = process.env.BURROW_E2E_ROOT_MARKER;
  const expectedPrefix = `${tmpdir()}/burrow-e2e-`;

  if (rootDir && rootDir.startsWith(expectedPrefix)) {
    try {
      rmSync(rootDir, { recursive: true, force: true });
    } catch (e) {
      console.warn(`[global-teardown] failed to clean up ${rootDir}:`, e);
    }
  }

  if (markerPath) {
    try {
      unlinkSync(markerPath);
    } catch {
      // Ignore missing marker file.
    }
  }
}
