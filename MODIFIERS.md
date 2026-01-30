# Modifier Key Actions

Every result category defines behavior for modifier+Enter combos.

## Action Table

| Category | Enter | Shift+Enter | Ctrl+Enter |
|----------|-------|-------------|------------|
| **onepass** | Type password via `wtype` (hide window, 1s sleep, type) | Copy password (`wl-copy`) | Copy username (`wl-copy`) |
| **file** | Open (`xdg-open`) | Open directory in terminal (`$TERMINAL`/`foot`) | Open in VS Code |
| **vector** | Open (`xdg-open`) | Open directory in terminal | Open in VS Code |
| **app** | Launch | Launch | Launch |
| **history** | Re-launch | Re-launch | Re-launch |
| **ssh** | SSH connect | SSH connect | Copy `ssh user@host` to clipboard |
| **math** | No-op | Copy result to clipboard | Copy result to clipboard |
| **action** | Run action | Run action | Run action |
| **info** | No-op | No-op | No-op |

## Reserved Modifiers

Alt, AltGr, and Shift+Ctrl are reserved for future use. They currently fall through to Enter (None) behavior.

## Security

- Passwords are passed to `wtype` via `Command::new("wtype").arg("--").arg(password)` — no shell expansion.
- Passwords are never logged or included in error messages.
- All file paths are passed as separate arguments to `Command`, never interpolated into shell strings.
