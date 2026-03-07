# Burrow

## Project Overview
Desktop app and local command launcher using a Rust backend and Vite frontend.

## Mandatory Rules
- `NEVER` commit code that has not been tested.
- Write/fail tests before implementation (TDD) for all behavior changes.

## Tooling
- Rust unit tests: `cd src-tauri && cargo test`
- UI tests: `npx playwright test`
- `pnpm dev`, `pnpm dev:url`, `pnpm tauri dev`, `pnpm build`

## Ports
- User-facing dev URL: `http://<name>.localhost:1355` via Portless
- Raw Vite fallback: `http://localhost:1420` with `PORTLESS=0 pnpm dev`
- Vite HMR: `1421` in Tauri dev
- HTTP bridge/test server: `127.0.0.1:3001`
- Registered in `/home/beasty/projects/.ports`
