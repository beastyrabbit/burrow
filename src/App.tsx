import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { parseModifier } from "./types";
import "./styles.css";

interface SearchResult {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  exec: string;
}

function App() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [notification, setNotification] = useState("");
  const notificationTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

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
    const timer = setTimeout(() => doSearch(query), query ? 80 : 0);
    return () => clearTimeout(timer);
  }, [query, doSearch]);

  // Load frecent on mount
  useEffect(() => {
    doSearch("");
  }, [doSearch]);

  const executeAction = useCallback(async (e: React.KeyboardEvent | null, itemOverride?: SearchResult) => {
    const item = itemOverride ?? results[selectedIndex];
    if (!item) return;

    const modifier = e
      ? parseModifier({
          shift: e.shiftKey,
          ctrl: e.ctrlKey,
          alt: e.altKey,
          altgr: e.getModifierState("AltGraph"),
        })
      : "none";

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

    // Dispatch to backend execute_action
    try {
      await invoke("execute_action", { result: item, modifier });
    } catch (err) {
      console.error("Execute action failed:", err);
    }
  }, [results, selectedIndex]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((i) => Math.min(i + 1, results.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          executeAction(e);
          break;
        case "Escape":
          e.preventDefault();
          if (query) {
            setQuery("");
          }
          break;
      }
    },
    [results.length, executeAction, query]
  );

  const categoryLabel = (cat: string) => {
    const labels: Record<string, string> = {
      app: "App",
      history: "Recent",
      file: "File",
      ssh: "SSH",
      onepass: "1Pass",
      math: "Calc",
      vector: "Content",
      info: "Info",
      action: "Action",
    };
    return labels[cat] || cat;
  };

  return (
    <div className="launcher" onKeyDown={handleKeyDown}>
      <input
        ref={inputRef}
        className="search-input"
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search apps, files, SSH hosts..."
        autoFocus
        spellCheck={false}
      />
      {notification && (
        <div className="notification">{notification}</div>
      )}
      <ul className="results-list">
        {results.map((item, i) => (
          <li
            key={item.id}
            className={`result-item ${i === selectedIndex ? "selected" : ""}`}
            onMouseEnter={() => setSelectedIndex(i)}
            onClick={() => executeAction(null, item)}
          >
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
    </div>
  );
}

export default App;
