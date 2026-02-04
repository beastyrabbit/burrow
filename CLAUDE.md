# Burrow — Application Launcher

## Mandatory Testing Rules

**NEVER commit code that hasn't been tested. This is non-negotiable.**

### TESTS FIRST — No Exceptions

**ALWAYS write failing tests BEFORE writing implementation code. This is TDD and it is mandatory.**

1. Write the test that describes the expected behavior
2. Run it — confirm it fails
3. Write the minimum implementation to make it pass
4. Run it — confirm it passes
5. Only then move to the next piece of functionality

**Never write implementation code without a corresponding test already existing.** If you catch yourself writing implementation first, STOP, delete it, and write the test first.

**Tests must be thorough and informative:**
- Test the FULL pipeline, not just individual functions in isolation
- Use REAL system data (installed apps, real files) — not fake/mock inputs
- Assert on the actual output format the consumer will receive (e.g., if the frontend needs a data URI, assert on data URI — not on intermediate file paths)
- Include descriptive failure messages that show what went wrong: `assert!(result.starts_with("data:"), "expected data URI, got: {result}")`
- Set minimum thresholds (e.g., "at least 3 of 6 known apps must resolve") to catch silent regressions
- When a test passes but the feature doesn't work, the test is wrong — write a better test that would have caught the real issue

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
- Playwright e2e tests start `pnpm tauri dev` automatically via `playwright.config.ts` webServer config (or you can run it manually)

## Configuration

- **Config file:** `~/.config/burrow/config.toml` (TOML format, auto-created with defaults on first run)
- **Override priority:** env vars (`BURROW_*`) > config.toml > defaults
- **Config module:** `src-tauri/src/config.rs` — loaded once at startup via `OnceLock`
- **Key env vars:** `BURROW_OLLAMA_URL`, `BURROW_MODEL_EMBEDDING`, `BURROW_MODEL_CHAT`, `BURROW_MODEL_CHAT_LARGE`, `BURROW_MODEL_CHAT_LARGE_PROVIDER`, `BURROW_INDEX_MODE`, `BURROW_VECTOR_SEARCH_ENABLED`, `BURROW_OPENROUTER_API_KEY`, `OPENROUTER_API_KEY`

### Default Config Values

| Section | Key | Default |
|---------|-----|---------|
| `models.embedding.name` | Embedding model | `qwen3-embedding:8b` |
| `models.embedding.provider` | Embedding provider | `ollama` |
| `models.chat.name` | Small/fast chat model | `gpt-oss:20b` |
| `models.chat.provider` | Chat provider | `ollama` |
| `models.chat_large.name` | Large/powerful chat model | `gpt-oss:120b` |
| `models.chat_large.provider` | Large model provider | `ollama` |
| `ollama.url` | Ollama API URL | `http://localhost:11434` |
| `ollama.timeout_secs` | Embedding request timeout | `30` |
| `ollama.chat_timeout_secs` | Chat request timeout | `120` |
| `chat.rag_enabled` | Enable RAG for chat-docs | `true` |
| `chat.max_context_snippets` | Max context for RAG | `5` |
| `vector_search.enabled` | Enable content search | `true` |
| `vector_search.top_k` | Max results | `10` |
| `vector_search.min_score` | Min cosine similarity | `0.3` |
| `vector_search.max_file_size_bytes` | Max file size to index | `1000000` |
| `vector_search.index_mode` | Index mode: "all" or "custom" | `all` |
| `vector_search.index_dirs` | Dirs when index_mode="custom" | `~/Documents, ~/Projects, ~/Downloads` |
| `vector_search.exclude_patterns` | Glob patterns to exclude | `.cache, .git, node_modules, target, etc.` |
| `indexer.interval_hours` | Re-index interval | `24` |
| `indexer.file_extensions` | Indexed file types | `txt, md, rs, ts, tsx, js, py, toml, yaml, yml, json, sh, css, html, pdf, doc, docx, xlsx, xls, pptx, odt, ods, odp, csv, rtf` |
| `indexer.max_content_chars` | Max chars per file | `4096` |
| `history.max_results` | Frecent results shown | `6` |
| `search.max_results` | Max search results | `10` |
| `search.debounce_ms` | Input debounce | `80` |
| `openrouter.api_key` | OpenRouter API key | `""` (empty) |

## Architecture

- **Stack:** Tauri v2 + React + TypeScript frontend, Rust backend
- **HTTP bridge:** In debug builds, Tauri spawns an axum HTTP server on `127.0.0.1:3001` (`src-tauri/src/dev_server.rs`) exposing all commands as `POST /api/{cmd}` endpoints
- **Frontend bridge client:** `src/mock-tauri.ts` — Vite aliases `@tauri-apps/api/core` to this file outside Tauri. It forwards `invoke()` calls to the HTTP bridge via `fetch()`. Playwright tests run against the real backend through this bridge.
- **Routing:** Prefix-based input dispatch in `src-tauri/src/router.rs`
- **Vector search:** SQLite brute-force cosine similarity (no HNSW needed at <100k vectors). Embeddings via Ollama HTTP API, stored as BLOBs in `~/.local/share/burrow/vectors.db`

## Commands

| Command | Purpose |
|---------|---------|
| `cd src-tauri && cargo test` | Run all Rust unit tests (375+ tests) |
| `npx playwright test` | Run all e2e tests (starts `pnpm tauri dev` if needed) |
| `pnpm dev` | Start Vite dev server on :1420 (needs HTTP bridge on :3001) |
| `pnpm tauri dev` | Start full Tauri app (real backend) |
| `pnpm build` | Build frontend for production |

## CLI Commands

| Command | Purpose |
|---------|---------|
| `burrow` | Launch the GUI |
| `burrow toggle` | Toggle window visibility |
| `burrow reindex` | Full reindex of all configured directories |
| `burrow update` | Incremental index update |
| `burrow index <file>` | Index a single file |
| `burrow health` | Check system health (Ollama, Vector DB) |
| `burrow stats` | Show statistics (indexed files, launches) |
| `burrow config` | Open config file in editor |
| `burrow progress` | Show current indexer progress |
| `burrow daemon` | Start/manage the background daemon |
| `burrow chat "query"` | Chat with AI (no document context) |
| `burrow chat-docs "query"` | Chat with AI using document context (RAG) |
| `burrow models list` | Show current model configuration |
| `burrow models set` | Interactive model configuration |

### Chat Commands

```bash
# RAG chat with document context (uses large model by default)
burrow chat-docs "What's in my project notes?"

# Direct chat without context
burrow chat "Explain Rust ownership"

# Use small/fast model instead of large
burrow chat --small "Hello"
burrow chat-docs --small "Summarize"
```

### Model Configuration

```bash
# List current models
burrow models list

# Interactive model selector
burrow models set           # Full interactive flow
burrow models set chat_large  # Configure specific model type
```

Model types: `embedding`, `chat`, `chat_large`
Providers: `ollama`, `openrouter`

## Project Structure

- `src-tauri/src/config.rs` — TOML config loading, env overrides, defaults, ModelsConfig
- `src-tauri/src/ollama.rs` — Ollama HTTP client, cosine similarity, embedding serialization, model fetching
- `src-tauri/src/commands/` — Backend providers (apps, history, math, ssh, onepass, files, vectors, chat, health, settings)
- `src-tauri/src/chat.rs` — Provider-agnostic AI chat (Ollama/OpenRouter), RAG prompt building
- `src-tauri/src/text_extract.rs` — Document text extraction (PDF, DOC via external LibreOffice, DOCX, XLSX, ODS, etc.)
- `src-tauri/src/router.rs` — Input classification and dispatch
- `src-tauri/src/dev_server.rs` — Axum HTTP bridge for browser/Playwright testing (debug builds only, `#[cfg(debug_assertions)]`)
- `src/App.tsx` — Main UI component
- `src-tauri/src/icons.rs` — Freedesktop icon name → base64 data URI resolution
- `src/category-icons.tsx` — Lucide SVG icons for non-app result categories
- `src/mock-tauri.ts` — HTTP bridge client (forwards `invoke()` to axum server on :3001)
- `e2e/` — Playwright e2e tests
- `e2e/icons.spec.ts` — Icon rendering e2e tests
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
- When adding a new settings action: add `SettingDef` in `commands/settings.rs`, add match arm in `run_setting()` in `lib.rs`, update settings count in tests
- When adding a new Tauri command: add route in `dev_server.rs` with request body struct, add to `generate_handler![]` in `lib.rs`
- `dev_server.rs` endpoints mirror Tauri command signatures — each gets a Deserialize body struct and calls the same function
- Playwright tests now run against real backend via HTTP bridge — no more mock data to maintain in `mock-tauri.ts`
- Pre-commit hooks run rustfmt — always run `cargo fmt` before staging, or stage after the first failed commit attempt
- Health indicator checks only core services (Ollama, vector DB), not optional features (API key) to avoid false alarms
- `e2e/launcher.spec.ts`, `e2e/edge-cases.spec.ts`, and `e2e/icons.spec.ts` have hardcoded settings count — update when adding new settings
- Ollama server defaults to `localhost:11434` — existing user configs override all defaults
- When new config keys are added, regenerate config (`rm ~/.config/burrow/config.toml`) or manually add new keys
- When defaults change (values only), existing configs continue working with their current values
- External tool extraction (e.g. LibreOffice for `.doc`) uses `spawn` + `wait-timeout` crate for timeout enforcement — never use blocking `Command::output()` for external processes that may hang
- Icons use base64 data URIs (`data:image/png;base64,...`) — NOT Tauri asset protocol. Asset protocol scope/CSP is unreliable; data URIs work everywhere.
- Desktop entry dedup: `load_desktop_entries()` uses `HashSet<id>` to prevent duplicates from overlapping dirs (XDG_DATA_DIRS, flatpak, snap)
- `cargo build` may silently skip recompilation after Edit tool changes — use `touch <file> && cargo build` to force recompile
- `pnpm tauri dev` requires port 1420 free — kill stale processes with `lsof -ti:1420 | xargs kill -9` before restart
- `tauri.conf.json` changes require full `tauri dev` restart (not just HMR)
- Known pre-existing bug: `pdf-extract` crate panics on some PDFs (`unwrap()` on `None` at lib.rs:1383`) — can crash the Tauri process
- Flatpak/Snap app dirs are already in `XDG_DATA_DIRS` on this system — adding them again in `desktop_dirs()` causes duplicates without dedup
- GitHub repo owner is `beastyrabbit` (not `beasty`)
- CodeRabbit is incremental — it won't re-review already-reviewed commits. Post `@coderabbitai review` comment on PR to trigger review of latest push.
- `Zeroizing<String>` doesn't implement `PartialEq<&str>` — use `&*val` in test assertions: `assert_eq!(&*get_password("id").unwrap(), "expected")`
- All `op` CLI calls must use `spawn` + `wait-timeout` (not blocking `Command::output()`) — signin gets 120s (user interaction), other calls get 30s
- `env_override_openrouter_api_key` test is flaky when a real `OPENROUTER_API_KEY` env var is set in the shell — known issue, retry on failure
- Model configuration now uses unified `[models.*]` sections instead of scattered `ollama.embedding_model` and `openrouter.model`
- `index_mode = "all"` indexes home directory; `index_mode = "custom"` uses `index_dirs` list
- `exclude_patterns` in config supports glob patterns (e.g., `*.pyc`, `node_modules`) and absolute paths (e.g., `/proc`)
- File name search (`commands/files.rs`) now uses same directories as content indexer via `indexer::get_search_directories()`
- CLI chat commands (`burrow chat`, `burrow chat-docs`) support `--small` flag to use smaller/faster model
- `burrow models set` interactive selector fetches available models from Ollama (`/api/tags`) or OpenRouter (`/api/v1/models`)

## Reference Repos

When a dependency's behavior is unclear or docs are insufficient, clone the repo into `examples/` for local analysis:

```bash
git clone --depth 1 https://github.com/org/repo.git examples/repo
```

`examples/` is gitignored. Clone what you need, delete when done.
