// Tauri API bridge — always aliased from @tauri-apps/api/* by Vite.
// If running inside the Tauri webview, delegates to the real IPC runtime.
// If running in a plain browser (Playwright / dev), forwards calls to the axum
// HTTP bridge on port 3001 (either Tauri dev-server or standalone test-server).

const DEV_API = "http://127.0.0.1:3001/api";

// Tauri registers _cmd-suffixed handlers to avoid collisions with core functions.
// Commands without a _cmd suffix (hide_window, launch_app) pass through via ?? fallback.
// SYNC: keep in sync with generate_handler![] in src-tauri/src/lib.rs
type TauriMappedCmd = "search" | "health_check" | "chat_ask" | "record_launch" | "execute_action" | "get_output";
const TAURI_CMD: Record<TauriMappedCmd, `${TauriMappedCmd}_cmd`> = {
  search: "search_cmd",
  health_check: "health_check_cmd",
  chat_ask: "chat_ask_cmd",
  record_launch: "record_launch_cmd",
  execute_action: "execute_action_cmd",
  get_output: "get_output_cmd",
};

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // Tauri injects __TAURI_INTERNALS__ into its webview at startup.
  // Check at call time (not module load) in case of late injection.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const tauriInternals = (window as any).__TAURI_INTERNALS__;
  if (tauriInternals) {
    const mapped = TAURI_CMD[cmd as TauriMappedCmd];
    if (!mapped && import.meta.env.DEV) {
      console.warn(
        `[mock-tauri] No TAURI_CMD mapping for "${cmd}" — sending raw name to Tauri IPC.`,
      );
    }
    return tauriInternals.invoke(mapped ?? cmd, args);
  }

  let res: Response;
  try {
    res = await fetch(`${DEV_API}/${cmd}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(args ?? {}),
    });
  } catch (err) {
    throw new Error(
      `[mock-tauri] Cannot reach dev server at ${DEV_API}/${cmd}. ` +
        `Is "pnpm tauri dev" running? (${err})`,
    );
  }
  if (!res.ok) {
    const body = await res.text().catch(() => "(no response body)");
    throw new Error(`[mock-tauri] ${cmd} failed (${res.status}): ${body}`);
  }
  const text = await res.text();
  try {
    return JSON.parse(text);
  } catch {
    throw new Error(`[mock-tauri] ${cmd} returned invalid JSON: ${text.slice(0, 200)}`);
  }
}

/**
 * Listen for Tauri events.
 * Always a no-op — the HTTP bridge doesn't support push events.
 * Output windows use polling via invoke("get_output") instead.
 */
export async function listen<T>(
  _event: string,
  _handler: (event: { payload: T }) => void,
): Promise<() => void> {
  return () => {};
}
