# Burrow — Application Launcher

## Mandatory Testing Rules

**NEVER commit code that hasn't been tested. This is non-negotiable.**

### Before Every Commit

1. **Run Rust unit tests:**
   ```bash
   cd src-tauri && cargo test
   ```
   All tests must pass. Zero failures allowed.

2. **Run Playwright e2e tests:**
   ```bash
   npx playwright test
   ```
   All tests must pass. If the Vite dev server isn't running, Playwright will start it automatically via `playwright.config.ts` webServer config.

3. **Visual verification with Playwright MCP:**
   - Navigate to `http://localhost:1420`
   - Test the actual feature you changed (type in search, verify results, test keyboard nav)
   - Page loads alone are NOT sufficient — interact with the UI

### When Adding New Features

- Add Rust unit tests for any new backend logic (commands, parsing, search)
- Add Playwright e2e tests for any new UI behavior
- If a new provider is added, add tests for: empty query, matching query, no-match query, edge cases
- Mock new Tauri commands in `src/mock-tauri.ts` so Playwright tests work without Tauri runtime

## Architecture

- **Stack:** Tauri v2 + React + TypeScript frontend, Rust backend
- **Frontend mock:** `src/mock-tauri.ts` — aliases `@tauri-apps/api/core` when running outside Tauri (Vite alias in `vite.config.ts`). All Playwright tests run against this mock.
- **Routing:** Prefix-based input dispatch in `src-tauri/src/router.rs`

## Commands

| Command | Purpose |
|---------|---------|
| `cd src-tauri && cargo test` | Run all Rust unit tests (91 tests) |
| `npx playwright test` | Run all e2e tests (25 tests) |
| `pnpm dev` | Start Vite dev server on :1420 (mock backend) |
| `pnpm tauri dev` | Start full Tauri app (real backend) |
| `pnpm build` | Build frontend for production |

## Project Structure

- `src-tauri/src/commands/` — Backend providers (apps, history, math, ssh, onepass, files)
- `src-tauri/src/router.rs` — Input classification and dispatch
- `src/App.tsx` — Main UI component
- `src/mock-tauri.ts` — Mock backend for browser-only testing
- `e2e/` — Playwright e2e tests
- `playwright.config.ts` — Playwright configuration

## Patterns

- Extract pure functions from system-dependent code for testability (e.g. `parse_ssh_config_content` takes a string, `filter_hosts` takes a vec)
- Use `#[cfg(test)]` modules in each Rust file
- Use `tempfile` crate for filesystem tests
- Use in-memory SQLite (`Connection::open_in_memory()`) for history tests
