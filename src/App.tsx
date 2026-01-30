import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
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
  const [chatAnswer, setChatAnswer] = useState("");
  const [chatLoading, setChatLoading] = useState(false);
  const [healthOk, setHealthOk] = useState(true);
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

  const launchSelected = useCallback(async () => {
    const item = results[selectedIndex];
    if (!item) return;

    if (item.category === "math" || item.category === "info") return;

    if (item.category === "chat") {
      setChatLoading(true);
      setChatAnswer("");
      try {
        const answer = await invoke<string>("chat_ask", { query });
        setChatAnswer(answer);
      } catch (e) {
        const errMsg = e instanceof Error ? e.message : String(e);
        setChatAnswer(`Error: ${errMsg}`);
      } finally {
        setChatLoading(false);
      }
      return;
    }

    if (item.category === "action") {
      try {
        setNotification("Running...");
        const msg = await invoke<string>("run_setting", { action: item.id });
        setNotification(msg);
        if (notificationTimer.current) clearTimeout(notificationTimer.current);
        notificationTimer.current = setTimeout(() => setNotification(""), 4000);
      } catch (e) {
        const errMsg = e instanceof Error ? e.message : String(e);
        setNotification(`Error: ${errMsg}`);
        if (notificationTimer.current) clearTimeout(notificationTimer.current);
        notificationTimer.current = setTimeout(() => setNotification(""), 4000);
      }
      return;
    }

    try {
      await invoke("record_launch", {
        id: item.id,
        name: item.name,
        exec: item.exec,
        icon: item.icon,
        description: item.description,
      });
      await invoke("launch_app", { exec: item.exec });
    } catch (e) {
      console.error("Launch failed:", e);
    }
  }, [results, selectedIndex, query]);

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
          launchSelected();
          break;
        case "Escape":
          e.preventDefault();
          if (query) {
            setQuery("");
          }
          break;
      }
    },
    [results.length, launchSelected, query]
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
      chat: "Chat",
      info: "Info",
      action: "Action",
    };
    return labels[cat] || cat;
  };

  return (
    <div className="launcher" onKeyDown={handleKeyDown}>
      <div className="search-container">
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
        {!healthOk && (
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
      <ul className="results-list">
        {results.map((item, i) => (
          <li
            key={item.id}
            className={`result-item ${i === selectedIndex ? "selected" : ""}`}
            onMouseEnter={() => setSelectedIndex(i)}
            onClick={() => launchSelected()}
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
