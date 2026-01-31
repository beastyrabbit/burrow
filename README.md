# Burrow

A fast application launcher for Linux (Wayland), built with Tauri v2, React, and Rust.

## Features

- **App search** — fuzzy-match installed desktop applications
- **File search** — find files by name across configured directories
- **Content search** — semantic search over file contents using Ollama embeddings
- **SSH hosts** — search and connect to hosts from `~/.ssh/config`
- **1Password** — search and auto-type or copy credentials
- **Calculator** — inline math evaluation
- **Launch history** — frecency-ranked recent launches
- **Settings commands** — `:reindex`, `:update`, `:config`, `:stats`, `:progress`
- **Modifier keys** — Shift+Enter and Ctrl+Enter trigger alternate actions per category (see [MODIFIERS.md](MODIFIERS.md))

## Requirements

- Linux with Wayland
- [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- `wtype` and `wl-copy` for typing and clipboard
- [Ollama](https://ollama.com/) for content search (optional)
- [1Password CLI](https://developer.1password.com/docs/cli/) for password integration (optional)

## Setup

```bash
pnpm install
```

## Development

```bash
# Frontend dev server (requires "pnpm tauri dev" for backend bridge on :3001)
pnpm dev

# Full Tauri app (real backend)
pnpm tauri dev
```

## Testing

```bash
# Rust unit tests (from src-tauri/)
cd src-tauri && cargo test

# Playwright e2e tests
npx playwright test
```

## Configuration

Config file: `~/.config/burrow/config.toml` (auto-created on first run).

Environment variables (`BURROW_*`) override config file values.

See [CLAUDE.md](CLAUDE.md) for full configuration reference.

## Query Prefixes

| Prefix | Provider |
|--------|----------|
| *(none)* | App search (or math if expression detected) |
| ` ` (space) | File search |
| ` *` (space + asterisk) | Content/vector search |
| `!` | 1Password |
| `ssh ` | SSH hosts |
| `:` | Settings commands |

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Enter | Execute default action |
| Shift+Enter | Alternate action (copy password, open dir in terminal, copy math result) |
| Ctrl+Enter | Secondary action (copy username, open in VS Code, copy ssh command) |
| Arrow Up/Down | Navigate results |
| Escape | Clear search |

See [MODIFIERS.md](MODIFIERS.md) for the full modifier key action table.

## Project Structure

```text
src/                    # React frontend
  App.tsx               # Main UI component
  types.ts              # Shared types (Modifier)
  mock-tauri.ts         # Mock backend for browser testing
src-tauri/src/          # Rust backend
  actions/              # Modifier key action dispatch
  commands/             # Search providers (apps, files, ssh, onepass, etc.)
  router.rs             # Query classification and search dispatch
  config.rs             # TOML configuration
  indexer.rs            # Background file indexer
  ollama.rs             # Ollama embedding client
e2e/                    # Playwright e2e tests
```
