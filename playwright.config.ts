import { existsSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { defineConfig } from "@playwright/test";

const e2eRootMarker = process.env.BURROW_E2E_ROOT_MARKER?.trim()
  || join(tmpdir(), `burrow-e2e-root-${process.pid}.txt`);
const existingRootDir = process.env.BURROW_E2E_ROOT_DIR?.trim()
  || (existsSync(e2eRootMarker) ? readFileSync(e2eRootMarker, "utf8").trim() : "");
const e2eRootDir = existingRootDir || realpathSync(mkdtempSync(join(tmpdir(), "burrow-e2e-")));

writeFileSync(e2eRootMarker, `${e2eRootDir}\n`);

const e2eDataDir = join(e2eRootDir, "data");
const e2eXdgHome = join(e2eRootDir, "xdg-home");
const e2eXdgShared = join(e2eRootDir, "xdg-shared");
const e2eXdgLate = join(e2eRootDir, "xdg-late");
const e2eApplicationsDir = join(e2eXdgHome, "applications");
const e2eLateApplicationsDir = join(e2eXdgLate, "applications");
const e2eXdgDataDirs = [e2eXdgShared, e2eXdgLate].join(":");

// Make available to global teardown
process.env.BURROW_E2E_ROOT_DIR = e2eRootDir;
process.env.BURROW_E2E_ROOT_MARKER = e2eRootMarker;
process.env.BURROW_DATA_DIR = e2eDataDir;
process.env.BURROW_E2E_APP_DIR = e2eApplicationsDir;
process.env.BURROW_E2E_LATE_APP_DIR = e2eLateApplicationsDir;
process.env.XDG_DATA_HOME = e2eXdgHome;
process.env.XDG_DATA_DIRS = e2eXdgDataDirs;

export default defineConfig({
  testDir: "./e2e",
  timeout: 15000,
  retries: 0,
  workers: 1,
  globalTeardown: "./e2e/global-teardown.ts",
  use: {
    baseURL: "http://localhost:1420",
    headless: true,
  },
  webServer: [
    {
      command: "cargo run --bin test-server",
      cwd: "./src-tauri",
      port: 3001,
      reuseExistingServer: false,
      timeout: 120000, // First cold build may be slow; subsequent runs are instant
      env: {
        ...process.env,
        BURROW_E2E_ROOT_DIR: e2eRootDir,
        BURROW_DRY_RUN: "1",
        BURROW_DATA_DIR: e2eDataDir,
        BURROW_E2E_APP_DIR: e2eApplicationsDir,
        BURROW_E2E_LATE_APP_DIR: e2eLateApplicationsDir,
        XDG_DATA_HOME: e2eXdgHome,
        XDG_DATA_DIRS: e2eXdgDataDirs,
      },
    },
    {
      command: "pnpm dev",
      port: 1420,
      reuseExistingServer: true,
      timeout: 10000,
    },
  ],
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
