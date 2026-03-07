import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

export const BASE_APP_NAME = "burrow";
export const DEFAULT_PORTLESS_PORT = "1355";
export const FALLBACK_VITE_PORT = "1420";

function readCommand(command, args, cwd) {
  return execFileSync(command, args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function tryReadCommand(command, args, cwd) {
  try {
    return readCommand(command, args, cwd);
  } catch {
    return "";
  }
}

function realPathOrResolved(inputPath) {
  try {
    return fs.realpathSync.native(inputPath);
  } catch {
    return path.resolve(inputPath);
  }
}

export function isPortlessEnabled(env = process.env) {
  const value = env.PORTLESS?.toLowerCase();
  return value !== "0" && value !== "false" && value !== "skip";
}

export function normalizeWorktreeSlug(branchName) {
  const lastSegment = branchName.split("/").filter(Boolean).pop() ?? branchName;
  const normalized = lastSegment
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return normalized || "worktree";
}

export function computeAppName({ baseName = BASE_APP_NAME, isMainWorktree, branchName }) {
  if (isMainWorktree) {
    return baseName;
  }

  return `${normalizeWorktreeSlug(branchName)}.${baseName}`;
}

export function computeFrontendUrl(
  appName,
  { protocol = "http", usePortless = true, proxyPort = DEFAULT_PORTLESS_PORT } = {},
) {
  if (!usePortless) {
    return `http://localhost:${FALLBACK_VITE_PORT}`;
  }

  return `${protocol}://${appName}.localhost:${proxyPort}`;
}

export function resolveWorktreeContext(cwd = process.cwd()) {
  const repoRoot = realPathOrResolved(
    tryReadCommand("git", ["rev-parse", "--show-toplevel"], cwd) || cwd,
  );
  const branchName =
    tryReadCommand("git", ["branch", "--show-current"], repoRoot) || "main";

  const worktreeOutput = tryReadCommand("git", ["worktree", "list", "--porcelain"], repoRoot);
  const worktreeRoots = worktreeOutput
    .split(/\r?\n/)
    .filter((line) => line.startsWith("worktree "))
    .map((line) => realPathOrResolved(line.slice("worktree ".length)));

  const mainWorktreeRoot = worktreeRoots[0] ?? repoRoot;

  return {
    branchName,
    isMainWorktree: repoRoot === mainWorktreeRoot,
    repoRoot,
  };
}

export function resolvePortlessConfig({ cwd = process.cwd(), env = process.env } = {}) {
  const { branchName, isMainWorktree, repoRoot } = resolveWorktreeContext(cwd);
  const usePortless = isPortlessEnabled(env);
  const proxyPort = env.PORTLESS_PORT || DEFAULT_PORTLESS_PORT;
  const protocol = "http";
  const appName = computeAppName({
    baseName: BASE_APP_NAME,
    isMainWorktree,
    branchName,
  });

  return {
    appName,
    branchName,
    frontendUrl: computeFrontendUrl(appName, { protocol, proxyPort, usePortless }),
    isMainWorktree,
    protocol,
    proxyPort,
    repoRoot,
    usePortless,
  };
}

export function ensurePortlessAvailable() {
  const result = spawnSync("portless", ["--version"], { stdio: "ignore" });
  if (result.error || result.status !== 0) {
    console.error(
      "Portless is required for Burrow dev. Install it globally or ensure `portless` is on PATH.",
    );
    process.exit(result.status ?? 1);
  }
}
