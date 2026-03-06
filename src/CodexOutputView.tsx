import { useState, useEffect, useRef, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useOutputPolling, type BufferedLine } from "./useOutputPolling";
import { useAutoScroll } from "./useAutoScroll";
import "./CodexOutputView.css";

// --- Types ---

interface CodexItem {
  id: string;
  type: string;
  status?: string;
  // command_execution fields
  command?: string;
  aggregated_output?: string;
  exit_code?: number | null;
  // agent_message fields
  text?: string;
  // raw JSON for unknown types
  raw?: string;
}

interface RawLine {
  stream: "stdout" | "stderr";
  text: string;
}

interface CodexState {
  items: Map<string, CodexItem>;
  itemOrder: string[];
  turnStatus: "idle" | "running" | "completed" | "failed";
  turnUsage: { input_tokens: number; output_tokens: number; cached_input_tokens?: number } | null;
  turnError: string | null;
  hasWarnings: boolean;
  rawLines: RawLine[];
}

interface CodexOutputViewProps {
  label: string;
  title: string;
}

// --- Reducer ---

function initialState(): CodexState {
  return {
    items: new Map(),
    itemOrder: [],
    turnStatus: "idle",
    turnUsage: null,
    turnError: null,
    hasWarnings: false,
    rawLines: [],
  };
}

function processLines(state: CodexState, lines: BufferedLine[]): CodexState {
  if (lines.length === 0) return state;

  const items = new Map(state.items);
  const itemOrder = [...state.itemOrder];
  const rawLines = [...state.rawLines];
  let { turnStatus, turnUsage, turnError, hasWarnings } = state;

  for (const line of lines) {
    rawLines.push({ stream: line.stream, text: line.text });

    if (line.stream !== "stdout") continue;

    let event: any;
    try {
      event = JSON.parse(line.text);
    } catch {
      continue;
    }

    if (!event || typeof event.type !== "string") continue;

    const eventType: string = event.type;

    if (eventType === "turn.started") {
      turnStatus = "running";
      turnError = null;
      turnUsage = null;
    } else if (eventType === "turn.completed") {
      turnStatus = "completed";
      if (event.usage) {
        turnUsage = event.usage;
      }
    } else if (eventType === "turn.failed") {
      turnStatus = "failed";
      turnError = event.error?.message ?? "Unknown error";
    } else if (
      eventType === "item.started" ||
      eventType === "item.updated" ||
      eventType === "item.completed"
    ) {
      const item = event.item;
      if (item && typeof item.id === "string") {
        const existing = items.get(item.id);
        const merged: CodexItem = {
          ...(existing || {}),
          ...item,
        };
        items.set(item.id, merged);

        if (!existing) {
          itemOrder.push(item.id);
        }

        if (item.type === "error") {
          hasWarnings = true;
        }
      }
    }
  }

  const MAX_RAW_LINES = 10_000;
  const trimmedRawLines =
    rawLines.length > MAX_RAW_LINES ? rawLines.slice(-MAX_RAW_LINES) : rawLines;
  return { items, itemOrder, turnStatus, turnUsage, turnError, hasWarnings, rawLines: trimmedRawLines };
}

// --- Status logic ---

type StatusInfo = { className: string; text: string };

function getCodexStatus(
  state: CodexState,
  processDone: boolean,
  processExitCode: number | null,
): StatusInfo {
  if (processExitCode !== null && processExitCode !== 0) {
    return { className: "status-error", text: `Exit ${processExitCode}` };
  }
  if (state.turnStatus === "failed") {
    return { className: "status-failed", text: "Failed" };
  }
  if (state.hasWarnings && state.turnStatus === "completed") {
    return { className: "status-warnings", text: "Completed with warnings" };
  }
  if (state.turnStatus === "completed" || (processDone && state.turnStatus === "idle")) {
    return { className: "status-completed", text: "Completed" };
  }
  return { className: "status-running", text: "Running..." };
}

// --- Buffer-expired detection ---
const EXPIRED_POLL_THRESHOLD = 150;

// --- Sub-components ---

function cardStatusClass(isRunning: boolean, exitCode: number | null): string {
  if (isRunning) return "running";
  if (exitCode === 0) return "exit-ok";
  if (exitCode !== null) return "exit-fail";
  return "";
}

function ExitBadge({ isRunning, exitCode }: { isRunning: boolean; exitCode: number | null }) {
  if (isRunning) {
    return <span className="codex-card-exit pending">running</span>;
  }
  if (exitCode !== null) {
    return (
      <span className={`codex-card-exit ${exitCode === 0 ? "ok" : "fail"}`}>
        exit {exitCode}
      </span>
    );
  }
  return null;
}

function CommandCard({ item }: { item: CodexItem }) {
  const [expanded, setExpanded] = useState(false);
  const hasOutput = item.aggregated_output && item.aggregated_output.length > 0;
  const isRunning = item.status === "in_progress";
  const exitCode = item.exit_code ?? null;

  return (
    <div className={`codex-card ${cardStatusClass(isRunning, exitCode)}`}>
      <div className="codex-card-header" style={hasOutput ? undefined : { cursor: "default" }} onClick={() => hasOutput && setExpanded(!expanded)}>
        <span className="codex-card-command">{item.command ?? "command"}</span>
        <ExitBadge isRunning={isRunning} exitCode={exitCode} />
      </div>
      {expanded && hasOutput && (
        <div className="codex-card-output">{item.aggregated_output}</div>
      )}
    </div>
  );
}

function AgentMessage({ item, defaultExpanded }: { item: CodexItem; defaultExpanded: boolean }) {
  const [collapsed, setCollapsed] = useState(!defaultExpanded);
  const userToggledRef = useRef(false);

  // Collapse previously-expanded messages when a newer message becomes the last,
  // but only if the user hasn't manually toggled this message
  useEffect(() => {
    if (!defaultExpanded && !userToggledRef.current) setCollapsed(true);
  }, [defaultExpanded]);

  return (
    <div className={`codex-agent-message ${collapsed ? "collapsed" : ""}`}>
      <div className="codex-agent-toggle" onClick={() => { userToggledRef.current = true; setCollapsed(!collapsed); }}>
        <span>{collapsed ? "▸" : "▾"}</span>
        <span>Agent message</span>
      </div>
      <div className="codex-message-content">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{item.text ?? ""}</ReactMarkdown>
      </div>
    </div>
  );
}

function ReasoningItem({ item }: { item: CodexItem }) {
  const [collapsed, setCollapsed] = useState(true);

  return (
    <div className={`codex-reasoning ${collapsed ? "collapsed" : ""}`} onClick={() => setCollapsed(!collapsed)}>
      <span>{collapsed ? "▸" : "▾"} reasoning</span>
      <div className="codex-reasoning-text">{item.text}</div>
    </div>
  );
}

// --- Main Component ---

function CodexOutputView({ label, title }: CodexOutputViewProps): React.JSX.Element {
  const [state, setState] = useState<CodexState>(initialState);
  const [activeTab, setActiveTab] = useState<"output" | "events">("output");
  const outputScrollRef = useRef<HTMLDivElement>(null);
  const eventsScrollRef = useRef<HTMLPreElement>(null);
  const emptyPollCountRef = useRef(0);
  const [bufferExpired, setBufferExpired] = useState(false);

  // Reset on label change
  useEffect(() => {
    setState(initialState());
    setBufferExpired(false);
    emptyPollCountRef.current = 0;
  }, [label]);

  const handleLines = useCallback((newLines: BufferedLine[]) => {
    emptyPollCountRef.current = 0;
    setState((prev) => processLines(prev, newLines));
  }, []);

  const { done, exitCode, pollError } = useOutputPolling({
    label,
    onLines: handleLines,
  });

  // Buffer-expired detection: if no data arrives after many polls
  useEffect(() => {
    if (done || pollError) return;
    const id = setInterval(() => {
      emptyPollCountRef.current++;
      if (emptyPollCountRef.current >= EXPIRED_POLL_THRESHOLD) {
        setBufferExpired(true);
      }
    }, 200);
    return () => clearInterval(id);
  }, [done, pollError]);

  useAutoScroll(outputScrollRef, [state.itemOrder.length, state.items]);
  useAutoScroll(eventsScrollRef, [state.rawLines.length]);

  // Find last agent_message id for default-expand logic
  const agentMessageIds = state.itemOrder.filter(
    (id) => state.items.get(id)?.type === "agent_message"
  );
  const lastAgentMessageId = agentMessageIds[agentMessageIds.length - 1] ?? null;

  const status = pollError
    ? { className: "status-error", text: pollError }
    : getCodexStatus(state, done, exitCode);

  if (bufferExpired && state.rawLines.length === 0) {
    return (
      <div className="codex-output">
        <div className="output-titlebar" data-tauri-drag-region>
          <span className="output-title">{title}</span>
        </div>
        <div className="codex-expired">Output expired or unavailable</div>
      </div>
    );
  }

  return (
    <div className="codex-output">
      <div className="output-titlebar" data-tauri-drag-region>
        <span className="output-title">{title}</span>
        <span className={`output-status ${status.className}`}>{status.text}</span>
      </div>

      {state.turnError && (
        <div className="codex-error-banner">{state.turnError}</div>
      )}

      <div className="codex-tabs">
        <button
          className={`codex-tab ${activeTab === "output" ? "active" : ""}`}
          onClick={() => setActiveTab("output")}
        >
          Output
        </button>
        <button
          className={`codex-tab ${activeTab === "events" ? "active" : ""}`}
          onClick={() => setActiveTab("events")}
        >
          Events
        </button>
      </div>

      {activeTab === "output" && (
        <div className="codex-tab-content" ref={outputScrollRef}>
          {state.itemOrder.map((id) => {
            const item = state.items.get(id);
            if (!item) return null;

            if (item.type === "command_execution") {
              return <CommandCard key={id} item={item} />;
            }
            if (item.type === "agent_message") {
              return (
                <AgentMessage
                  key={id}
                  item={item}
                  defaultExpanded={id === lastAgentMessageId}
                />
              );
            }
            if (item.type === "reasoning") {
              return <ReasoningItem key={id} item={item} />;
            }
            return (
              <div key={id} className="codex-unknown">
                {item.raw ?? JSON.stringify(item)}
              </div>
            );
          })}

          {state.turnUsage && (
            <div className="codex-usage">
              Tokens: {state.turnUsage.input_tokens.toLocaleString()} in / {state.turnUsage.output_tokens.toLocaleString()} out
              {state.turnUsage.cached_input_tokens != null && (
                <> ({state.turnUsage.cached_input_tokens.toLocaleString()} cached)</>
              )}
            </div>
          )}
        </div>
      )}

      {activeTab === "events" && (
        <pre className="codex-tab-content codex-events" ref={eventsScrollRef}>
          {state.rawLines.map((line, i) => (
            <span
              key={i}
              className={line.stream === "stderr" ? "event-stderr" : "event-stdout"}
            >
              {line.text}
              {"\n"}
            </span>
          ))}
        </pre>
      )}
    </div>
  );
}

export default CodexOutputView;
