import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { parseModifier } from "./types";
import { CategoryIcon } from "./category-icons";
import "./styles.css";

interface InputSpec {
  placeholder: string;
  template: string;
}

interface SearchResult {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  exec: string;
  input_spec?: InputSpec;
}

const CATEGORY_LABELS: Record<string, string> = {
  app: "App",
  history: "Recent",
  file: "File",
  ssh: "SSH",
  onepass: "1Pass",
  math: "Calc",
  vector: "Content",
  chat: "Chat",
  info: "Info",
  action: "Action",
  special: "Special",
};

function ResultIcon({ icon, category }: { icon: string; category: string }) {
  const [broken, setBroken] = useState(false);
  useEffect(() => {
    setBroken(false);
  }, [icon]);
  if (!icon || broken) {
    return (
      <div className="result-icon-placeholder">
        <CategoryIcon category={category} />
      </div>
    );
  }
  return (
    <img
      className="result-icon"
      src={icon}
      alt=""
      onError={() => { console.warn(`[ResultIcon] failed to load icon for category="${category}"`); setBroken(true); }}
    />
  );
}

function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [notification, setNotification] = useState("");
  const [chatAnswer, setChatAnswer] = useState("");
  const [chatLoading, setChatLoading] = useState(false);
  const [healthOk, setHealthOk] = useState(true);
  // Secondary input mode state
  const [secondaryMode, setSecondaryMode] = useState<{
    active: boolean;
    result: SearchResult | null;
    previousQuery: string;
  }>({ active: false, result: null, previousQuery: "" });
  const [secondaryInput, setSecondaryInput] = useState("");
  const notificationTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const visibilityEpoch = useRef(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const queryRef = useRef(query);

  // Mouse hover state machine to prevent accidental selection when window appears under cursor.
  // Phases: initial (waiting) -> tracking (measuring movement) -> enabled (hover active).
  // Resets to "initial" when window is hidden.
  type MouseHoverState =
    | { phase: "initial" }
    | { phase: "tracking"; start: { x: number; y: number } }
    | { phase: "enabled" };
  const mouseStateRef = useRef<MouseHoverState>({ phase: "initial" });

  const doSearch = useCallback(async (q: string) => {
    try {
      const res = await invoke<SearchResult[]>("search", { query: q });
      setResults(res);
      setSelectedIndex(0);
    } catch (e) {
      console.error("Search failed:", e);
      setResults([]);
    }
  }, []);

  useEffect(() => {
    setChatAnswer("");
    const timer = setTimeout(() => doSearch(query), query ? 80 : 0);
    return () => clearTimeout(timer);
  }, [query, doSearch]);

  // Load frecent on mount
  useEffect(() => {
    doSearch("");
  }, [doSearch]);

  // Health check on mount + poll every 30s
  useEffect(() => {
    const checkHealth = async () => {
      try {
        const status = await invoke<{ ollama: boolean; vector_db: boolean; api_key: boolean }>("health_check");
        setHealthOk(status.ollama && status.vector_db);
      } catch (e) {
        console.error("Health check failed:", e);
        setHealthOk(false);
      }
    };
    checkHealth();
    const interval = setInterval(checkHealth, 30000);
    return () => clearInterval(interval);
  }, []);

  // Keep queryRef in sync with query state
  useEffect(() => {
    queryRef.current = query;
  }, [query]);

  // Listen for vault-load-result events from 1Password load action
  useEffect(() => {
    const unlisten = listen<{ ok: boolean; message: string }>("vault-load-result", (event) => {
      const { ok, message } = event.payload;
      const prefix = ok ? "✓ " : "✗ ";
      setNotification(prefix + message);
      if (notificationTimer.current) clearTimeout(notificationTimer.current);
      notificationTimer.current = setTimeout(() => setNotification(""), ok ? 4000 : 8000);
      // Refresh search to show loaded items (use ref to get current query without re-subscribing)
      if (ok) {
        doSearch(queryRef.current);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [doSearch]);

  // Clear input when window is hidden
  useEffect(() => {
    const onVisibilityChange = () => {
      if (document.hidden) {
        visibilityEpoch.current += 1;
        setQuery("");
        setSelectedIndex(0);
        setChatAnswer("");
        setChatLoading(false);
        setSecondaryMode({ active: false, result: null, previousQuery: "" });
        setSecondaryInput("");
        mouseStateRef.current = { phase: "initial" };
      }
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => document.removeEventListener("visibilitychange", onVisibilityChange);
  }, []);

  // Enable mouse hover selection only after intentional movement (>10px)
  // Prevents accidental selection from jitter when window appears under cursor
  useEffect(() => {
    const THRESHOLD = 10;
    const onMouseMove = (e: MouseEvent) => {
      const state = mouseStateRef.current;
      if (state.phase === "enabled") return;
      if (state.phase === "initial") {
        mouseStateRef.current = { phase: "tracking", start: { x: e.clientX, y: e.clientY } };
        return;
      }
      const dx = e.clientX - state.start.x;
      const dy = e.clientY - state.start.y;
      if (dx * dx + dy * dy > THRESHOLD * THRESHOLD) {
        mouseStateRef.current = { phase: "enabled" };
      }
    };
    window.addEventListener("mousemove", onMouseMove);
    return () => window.removeEventListener("mousemove", onMouseMove);
  }, []);

  // Hide window when it loses focus (standard launcher behavior).
  // Debounce guards against focus churn during show/reposition transitions.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const onBlur = () => {
      if (timer) { clearTimeout(timer); }
      timer = setTimeout(() => {
        timer = null;
        invoke("hide_window").catch((e) => console.error("hide_window failed:", e));
      }, 150);
    };
    const onFocus = () => {
      if (timer) { clearTimeout(timer); timer = null; }
    };
    window.addEventListener("blur", onBlur);
    window.addEventListener("focus", onFocus);
    return () => {
      if (timer) clearTimeout(timer);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("focus", onFocus);
    };
  }, []);

  // Auto-scroll selected item into view on keyboard navigation
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const child = list.children[selectedIndex] as HTMLElement | undefined;
    child?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const executeAction = useCallback(async (e: React.KeyboardEvent | null, itemOverride?: SearchResult) => {
    // In secondary mode, use the stored result
    const item = secondaryMode.active ? secondaryMode.result : (itemOverride ?? results[selectedIndex]);
    if (!item) return;

    // Check if we should enter secondary mode (result has input_spec and not already in secondary mode)
    if (item.input_spec && !secondaryMode.active) {
      setSecondaryMode({ active: true, result: item, previousQuery: query });
      setQuery("");
      setSecondaryInput("");
      return;
    }

    // Note: In secondary mode, modifiers are captured but have no effect on Special category
    // commands (which use input_spec). This is intentional - secondary mode is for collecting
    // text input, not selecting action variants. Modifiers are still passed to allow future
    // categories to use them if needed.
    const modifier = e
      ? parseModifier({
          shift: e.shiftKey,
          ctrl: e.ctrlKey,
          alt: e.altKey,
          altgr: e.getModifierState("AltGraph"),
        })
      : "none";

    // Get the secondary input value if we're in secondary mode
    const currentSecondaryInput = secondaryMode.active ? secondaryInput : undefined;

    // Reset secondary mode after capturing input
    if (secondaryMode.active) {
      setSecondaryMode({ active: false, result: null, previousQuery: "" });
      setSecondaryInput("");
    }

    if (item.category === "chat") {
      const epoch = visibilityEpoch.current;
      setChatLoading(true);
      setChatAnswer("");
      try {
        const answer = await invoke<string>("chat_ask", { query });
        if (visibilityEpoch.current !== epoch || document.hidden) return;
        setChatAnswer(answer);
      } catch (e) {
        if (visibilityEpoch.current !== epoch || document.hidden) return;
        const errMsg = e instanceof Error ? e.message : String(e);
        setChatAnswer(`Error: ${errMsg}`);
      } finally {
        if (visibilityEpoch.current === epoch && !document.hidden) {
          setChatLoading(false);
        }
      }
      return;
    }

    // Actions are handled separately via run_setting
    if (item.category === "action") {
      try {
        setNotification("Running...");
        const msg = await invoke<string>("run_setting", { action: item.id });
        setNotification(msg);
      } catch (err) {
        const errMsg = err instanceof Error ? err.message : String(err);
        setNotification(`Error: ${errMsg}`);
      } finally {
        if (notificationTimer.current) clearTimeout(notificationTimer.current);
        notificationTimer.current = setTimeout(() => setNotification(""), 4000);
      }
      return;
    }

    // Record launch for non-ephemeral categories
    if (!["math", "info"].includes(item.category)) {
      try {
        await invoke("record_launch", {
          id: item.id,
          name: item.name,
          exec: item.exec,
          icon: item.icon,
          description: item.description,
        });
      } catch (err) {
        console.error("Record launch failed:", err);
      }
    }

    // Dispatch to backend execute_action with optional secondary input
    try {
      await invoke("execute_action", {
        result: item,
        modifier,
        secondaryInput: currentSecondaryInput,
      });
    } catch (err) {
      console.error("Execute action failed:", err);
      const errMsg = err instanceof Error ? err.message : String(err);
      setNotification(`✗ Action failed: ${errMsg}`);
      if (notificationTimer.current) clearTimeout(notificationTimer.current);
      notificationTimer.current = setTimeout(() => setNotification(""), 6000);
    }
  }, [results, selectedIndex, query, secondaryMode, secondaryInput]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          if (!secondaryMode.active) {
            setSelectedIndex((i) => Math.min(i + 1, results.length - 1));
          }
          break;
        case "ArrowUp":
          e.preventDefault();
          if (!secondaryMode.active) {
            setSelectedIndex((i) => Math.max(i - 1, 0));
          }
          break;
        case "Enter":
          e.preventDefault();
          executeAction(e);
          break;
        case "Escape":
          e.preventDefault();
          if (secondaryMode.active) {
            const restoredQuery = secondaryMode.previousQuery;
            setSecondaryMode({ active: false, result: null, previousQuery: "" });
            setSecondaryInput("");
            setQuery(restoredQuery);
            doSearch(restoredQuery);
          } else {
            invoke("hide_window").catch((e) => console.error("hide_window failed:", e));
          }
          break;
      }
    },
    [results.length, executeAction, secondaryMode.active, doSearch]
  );

  const categoryLabel = (cat: string): string =>
    CATEGORY_LABELS[cat] ?? cat;

  return (
    <div className="launcher" onKeyDown={handleKeyDown}>
      <div className="search-container">
        <input
          ref={inputRef}
          className={`search-input ${secondaryMode.active ? "secondary" : ""}`}
          type="text"
          value={secondaryMode.active ? secondaryInput : query}
          onChange={(e) =>
            secondaryMode.active
              ? setSecondaryInput(e.target.value)
              : setQuery(e.target.value)
          }
          placeholder={
            secondaryMode.active
              ? secondaryMode.result?.input_spec?.placeholder
              : "Search apps, files, SSH hosts..."
          }
          autoFocus
          spellCheck={false}
        />
        {!healthOk && !secondaryMode.active && (
          <span
            className="health-indicator"
            title="System health issue — click to check"
            onClick={() => setQuery(":health")}
          >!</span>
        )}
      </div>
      {notification && (
        <div className="notification">{notification}</div>
      )}
      {chatLoading && (
        <div className="chat-answer chat-loading">Thinking...</div>
      )}
      {chatAnswer && !chatLoading && (
        <div className="chat-answer">{chatAnswer}</div>
      )}
      {secondaryMode.active ? (
        <div className="secondary-indicator">
          <span className="secondary-name">{secondaryMode.result?.name}</span>
          <span className="secondary-hint">Press Enter to confirm, Escape to cancel</span>
        </div>
      ) : (
        <ul ref={listRef} className="results-list">
          {results.map((item, i) => (
            <li
              key={item.id}
              className={`result-item ${i === selectedIndex ? "selected" : ""}`}
              onMouseEnter={() => { if (mouseStateRef.current.phase === "enabled") setSelectedIndex(i); }}
              onClick={() => executeAction(null, item)}
            >
              <ResultIcon icon={item.icon} category={item.category} />
              <div className="result-content">
                <span className="result-name">{item.name}</span>
                {item.description && (
                  <span className="result-desc">{item.description}</span>
                )}
              </div>
              <span className="result-badge">{categoryLabel(item.category)}</span>
            </li>
          ))}
          {results.length === 0 && query && (
            <li className="result-item empty">No results</li>
          )}
        </ul>
      )}
    </div>
  );
}

export default App;
