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

## Configuration

- **Config file:** `~/.config/burrow/config.toml` (TOML format, auto-created with defaults on first run)
- **Override priority:** env vars (`BURROW_*`) > config.toml > defaults
- **Config module:** `src-tauri/src/config.rs` — loaded once at startup via `OnceLock`
- **Key env vars:** `BURROW_OLLAMA_URL`, `BURROW_OLLAMA_EMBEDDING_MODEL`, `BURROW_VECTOR_SEARCH_ENABLED`

### Default Config Values

| Section | Key | Default |
|---------|-----|---------|
| `ollama.url` | Ollama API URL | `http://localhost:11434` |
| `ollama.embedding_model` | Embedding model | `qwen3-embedding:8b` |
| `ollama.timeout_secs` | Request timeout | `30` |
| `vector_search.enabled` | Enable content search | `true` |
| `vector_search.top_k` | Max results | `10` |
| `vector_search.min_score` | Min cosine similarity | `0.3` |
| `history.max_results` | Frecent results shown | `10` |
| `search.max_results` | Max search results | `10` |

## Architecture

- **Stack:** Tauri v2 + React + TypeScript frontend, Rust backend
- **Frontend mock:** `src/mock-tauri.ts` — aliases `@tauri-apps/api/core` when running outside Tauri (Vite alias in `vite.config.ts`). All Playwright tests run against this mock.
- **Routing:** Prefix-based input dispatch in `src-tauri/src/router.rs`
- **Vector search:** SQLite brute-force cosine similarity (no HNSW needed at <100k vectors). Embeddings via Ollama HTTP API, stored as BLOBs in `~/.local/share/burrow/vectors.db`

## Commands

| Command | Purpose |
|---------|---------|
| `cd src-tauri && cargo test` | Run all Rust unit tests |
| `npx playwright test` | Run all e2e tests |
| `pnpm dev` | Start Vite dev server on :1420 (mock backend) |
| `pnpm tauri dev` | Start full Tauri app (real backend) |
| `pnpm build` | Build frontend for production |

## Project Structure

- `src-tauri/src/config.rs` — TOML config loading, env overrides, defaults
- `src-tauri/src/ollama.rs` — Ollama HTTP client, cosine similarity, embedding serialization
- `src-tauri/src/commands/` — Backend providers (apps, history, math, ssh, onepass, files, vectors)
- `src-tauri/src/router.rs` — Input classification and dispatch
- `src/App.tsx` — Main UI component
- `src/mock-tauri.ts` — Mock backend for browser-only testing
- `e2e/` — Playwright e2e tests
- `playwright.config.ts` — Playwright configuration

## Data Storage

| File | Purpose |
|------|---------|
| `~/.config/burrow/config.toml` | User configuration |
| `~/.local/share/burrow/history.db` | Launch history (SQLite) |
| `~/.local/share/burrow/vectors.db` | File content embeddings (SQLite) |

## Patterns

- Extract pure functions from system-dependent code for testability (e.g. `parse_ssh_config_content` takes a string, `filter_hosts` takes a vec)
- Use `#[cfg(test)]` modules in each Rust file
- Use `tempfile` crate for filesystem tests
- Use in-memory SQLite (`Connection::open_in_memory()`) for DB tests
- Config uses `OnceLock` for thread-safe singleton; tests use `parse_config()` directly
- Configure your Ollama instance URL and embedding model in `~/.config/burrow/config.toml`
