# New Tauri Command

description: Checklist for adding a new Tauri command to Burrow. Auto-invoked when implementing a new backend command. Also available as /new-command.

---

Follow this checklist when adding a new Tauri command. Every step is required.

## 1. Primary Function

Create in `src-tauri/src/commands/<module>.rs`:

```rust
pub async fn my_command(arg: &str, ctx: &AppContext) -> Result<MyResponse, String> {
    // implementation
}
```

- Takes `&AppContext` (not `AppHandle`)
- Returns `Result<T, String>`
- If new module, add `pub mod <name>;` in `src-tauri/src/commands/mod.rs`

## 2. Tauri Wrapper

Add `_cmd` suffix wrapper in the same module as the primary function:

```rust
#[tauri::command]
pub async fn my_command_cmd(
    arg: String,
    app: tauri::AppHandle,
) -> Result<MyResponse, String> {
    let ctx = app.state::<AppContext>();
    my_command(&arg, ctx.inner()).await
}
```

## 3. Register Handler

Add to `generate_handler![]` in `src-tauri/src/lib.rs`:

```rust
generate_handler![
    // ... existing commands ...
    my_command_cmd,
]
```

## 4. HTTP Route

Add in `src-tauri/src/dev_server.rs`:

```rust
#[derive(Deserialize)]
struct MyCommandBody {
    arg: String,
}

// Add route in build_router():
.route("/api/my_command", post(my_command_handler))

// Add handler:
async fn my_command_handler(
    State(ctx): State<Arc<AppContext>>,
    Json(body): Json<MyCommandBody>,
) -> Result<Json<MyResponse>, (StatusCode, String)> {
    my_command(&body.arg, &ctx)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}
```

## 5. Frontend (if needed)

Call from TypeScript via `invoke()`:

```typescript
const result = await invoke("my_command", { arg: "value" });
```

This works in both Tauri (native IPC) and browser (HTTP bridge via `mock-tauri.ts`).

## 6. Tests (TDD — write FIRST)

- Rust unit tests in `#[cfg(test)]` module using `AppContext::from_disk()` or in-memory DB
- E2E test in `e2e/` if there's UI involvement
- Use `/tdd-cycle` skill for the full workflow

## Key Notes

- `test-server` binary does NOT need changes — it calls `build_router()` which picks up new routes automatically
- The Tauri wrapper and HTTP handler both call the same primary function
- Use `AppContext` for Tauri state (`app.state::<AppContext>()`) and `Arc<AppContext>` for axum state (`State(ctx): State<Arc<AppContext>>`)
