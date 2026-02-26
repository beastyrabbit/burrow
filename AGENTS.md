# Burrow

## Project Overview
Desktop app and local command launcher using a Rust backend and Vite frontend.

## Mandatory Rules
- `NEVER` commit code that has not been tested.
- Write/fail tests before implementation (TDD) for all behavior changes.

## Tooling
- Rust unit tests: `cd src-tauri && cargo test`
- UI tests: `npx playwright test`
- `pnpm dev`, `pnpm tauri dev`, `pnpm build`

## Ports
- Vite dev server: `1420`
- Vite HMR: `1421`
- HTTP bridge/test server: `3001`
- Registered in `/home/beasty/projects/.ports`
