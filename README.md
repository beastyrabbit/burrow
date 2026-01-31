# Burrow

A fast, keyboard-driven application launcher for Linux (Wayland), built with Tauri v2, React, and Rust. Burrow provides an extensible provider system that unifies app launching, file search, semantic content search, SSH connections, password management, and more — all from a single input bar.

## Features

- **App search** — fuzzy-match installed desktop applications, ranked by frecency (frequency + recency)
- **File search** — find files by name across configured directories
- **Content search** — semantic search over file contents using Ollama embeddings and cosine similarity
- **SSH hosts** — search and connect to hosts from `~/.ssh/config`
- **1Password** — search and auto-type or copy credentials via 1Password CLI
- **Calculator** — inline math evaluation with copy support
- **Launch history** — frecency-ranked recent launches for instant access to frequently used items
- **AI chat** — conversational AI with RAG context from indexed files (via OpenRouter)
- **Settings commands** — `:reindex`, `:update`, `:config`, `:stats`, `:progress`
- **Modifier keys** — Shift+Enter and Ctrl+Enter trigger alternate actions per category (see [MODIFIERS.md](MODIFIERS.md))

## Requirements

- Linux with Wayland
- [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- `wtype` and `wl-copy` for typing and clipboard
- [Ollama](https://ollama.com/) for content search (optional)
- [1Password CLI](https://developer.1password.com/docs/cli/) for password integration (optional)

## Install / Build

```bash
git clone https://github.com/beastyrabbit/burrow.git
cd burrow
pnpm install

# Development (hot-reload)
pnpm tauri dev

# Production build
pnpm tauri build
```

The production binary is output to `src-tauri/target/release/burrow`.

## Usage

### Query Prefixes

Type a prefix to route your query to a specific provider:

| Prefix | Provider | Example |
|--------|----------|---------|
| *(none)* | App search (or math if expression detected) | `firefox`, `2+2` |
| ` ` (space) | File search | ` notes.md` |
| ` *` (space + asterisk) | Content/vector search | ` *rust lifetime` |
| `!` | 1Password | `!github` |
| `ssh ` | SSH hosts | `ssh prod` |
| `:` | Settings commands | `:reindex` |
| `>` | AI chat | `>explain this error` |

An empty query shows your most frequently launched apps.

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Enter | Execute default action |
| Shift+Enter | Alternate action (copy password, open dir in terminal, copy math result) |
| Ctrl+Enter | Secondary action (copy username, open in VS Code, copy SSH command) |
| Arrow Up/Down | Navigate results |
| Escape | Clear search / close |

See [MODIFIERS.md](MODIFIERS.md) for the full modifier key action table per category.

## Configuration

Config file: `~/.config/burrow/config.toml` — auto-created with defaults on first run.

Environment variables (`BURROW_*`) override config file values. For example, `BURROW_OLLAMA_URL` overrides `ollama.url`.

### Default Values

| Section | Key | Default |
|---------|-----|---------|
| `ollama.url` | Ollama API URL | `http://localhost:11434` |
| `ollama.embedding_model` | Embedding model | `qwen3-embedding:8b` |
| `ollama.timeout_secs` | Request timeout | `30` |
| `vector_search.enabled` | Enable content search | `true` |
| `vector_search.top_k` | Max results | `10` |
| `vector_search.min_score` | Min cosine similarity | `0.3` |
| `vector_search.max_file_size_bytes` | Max file size to index | `1000000` |
| `vector_search.index_dirs` | Directories to index | `~/Documents, ~/Projects, ~/Downloads` |
| `indexer.interval_hours` | Re-index interval | `24` |
| `indexer.file_extensions` | Indexed file types | `txt, md, rs, ts, tsx, js, py, toml, yaml, yml, json, sh, css, html, pdf, doc, docx, xlsx, xls, pptx, odt, ods, odp, csv, rtf` |
| `indexer.max_content_chars` | Max chars per file | `4096` |
| `history.max_results` | Frecent results shown | `10` |
| `search.max_results` | Max search results | `10` |
| `search.debounce_ms` | Input debounce | `80` |
| `openrouter.api_key` | OpenRouter API key | `""` (empty) |
| `openrouter.model` | Chat model | `google/gemini-2.5-flash-preview` |

## Architecture

Burrow is a Tauri v2 app with a React + TypeScript frontend and a Rust backend. The frontend communicates with the backend through Tauri's IPC bridge (or an axum HTTP bridge on `localhost:3001` during development, enabling browser-based testing with Playwright). Launch history is stored in SQLite (`~/.local/share/burrow/history.db`). Semantic content search uses Ollama to generate embeddings, stored as BLOBs in a separate SQLite database (`~/.local/share/burrow/vectors.db`) with brute-force cosine similarity — no HNSW index needed at the expected scale. Input routing is prefix-based, dispatching queries to the appropriate provider.

## Development

```bash
# Full Tauri app with hot-reload (backend + frontend)
pnpm tauri dev

# Frontend dev server only (needs the Tauri backend running for the HTTP bridge on :3001)
pnpm dev
```

## Testing

```bash
# Rust unit tests
cd src-tauri && cargo test

# Playwright e2e tests (starts pnpm tauri dev automatically if needed)
npx playwright test
```

## Project Structure

```text
src/                    # React frontend
  App.tsx               # Main UI component
  types.ts              # Shared types (Modifier)
  mock-tauri.ts         # HTTP bridge client for browser/Playwright testing
src-tauri/src/          # Rust backend
  actions/              # Modifier key action dispatch
  commands/             # Search providers (apps, files, ssh, onepass, vectors, chat, etc.)
  router.rs             # Query classification and search dispatch
  config.rs             # TOML configuration with env var overrides
  indexer.rs            # Background file indexer
  ollama.rs             # Ollama embedding client + cosine similarity
  chat.rs               # OpenRouter AI chat with RAG context
  text_extract.rs       # Document text extraction (PDF, DOCX, XLSX, etc.)
  dev_server.rs         # Axum HTTP bridge for dev/testing (debug builds only)
  icons.rs              # Freedesktop icon → base64 data URI resolution
e2e/                    # Playwright e2e tests
```
