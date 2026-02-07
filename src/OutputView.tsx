import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./OutputView.css";

interface BufferedLine {
  stream: "stdout" | "stderr";
  text: string;
}

interface OutputSnapshot {
  lines: BufferedLine[];
  done: boolean;
  exit_code: number | null;
  total: number;
}

interface OutputViewProps {
  label: string;
  title: string;
}

const MAX_LINES = 10_000;
const POLL_INTERVAL_MS = 150;
const MAX_CONSECUTIVE_ERRORS = 20;

function getStatus(done: boolean, exitCode: number | null): { className: string; text: string } {
  if (!done) return { className: "status-running", text: "Running..." };
  if (exitCode === 0) return { className: "status-success", text: "Done" };
  return { className: "status-error", text: `Exit ${exitCode ?? "?"}` };
}

function OutputView({ label, title }: OutputViewProps): React.JSX.Element {
  const [lines, setLines] = useState<BufferedLine[]>([]);
  const [done, setDone] = useState(false);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const outputRef = useRef<HTMLPreElement>(null);
  const autoScrollRef = useRef(true);
  const sinceIndexRef = useRef(0);
  const errorCountRef = useRef(0);

  // Track whether user has scrolled up (disable auto-scroll)
  useEffect(() => {
    const el = outputRef.current;
    if (!el) return;
    const onScroll = () => {
      const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      autoScrollRef.current = atBottom;
    };
    el.addEventListener("scroll", onScroll);
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  // Auto-scroll to bottom when new lines arrive
  useEffect(() => {
    if (autoScrollRef.current && outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [lines]);

  // Reset state when label changes (e.g. component reused for a different command)
  useEffect(() => {
    sinceIndexRef.current = 0;
    errorCountRef.current = 0;
    setLines([]);
    setDone(false);
    setExitCode(null);
    setPollError(null);
  }, [label]);

  // Poll for output
  useEffect(() => {
    let stopped = false;
    const id = setInterval(async () => {
      if (stopped) return;
      try {
        const snap = await invoke<OutputSnapshot>("get_output", {
          label,
          sinceIndex: sinceIndexRef.current,
        });

        if (snap.lines.length > 0) {
          setLines((prev) => {
            const next = [...prev, ...snap.lines];
            return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next;
          });
        }
        sinceIndexRef.current = snap.total;

        errorCountRef.current = 0;

        if (snap.done) {
          setDone(true);
          setExitCode(snap.exit_code);
          stopped = true;
          clearInterval(id);
        }
      } catch (err) {
        errorCountRef.current += 1;
        if (errorCountRef.current >= MAX_CONSECUTIVE_ERRORS) {
          console.error(`[OutputView] ${errorCountRef.current} consecutive poll failures, giving up:`, err);
          setPollError(`Connection lost (${errorCountRef.current} failures)`);
          stopped = true;
          clearInterval(id);
        }
      }
    }, POLL_INTERVAL_MS);

    return () => {
      stopped = true;
      clearInterval(id);
    };
  }, [label]);

  const status = pollError
    ? { className: "status-error", text: pollError }
    : getStatus(done, exitCode);

  return (
    <div className="output-view">
      <div className="output-titlebar" data-tauri-drag-region>
        <span className="output-title">{title}</span>
        <span className={`output-status ${status.className}`}>{status.text}</span>
      </div>
      <pre ref={outputRef} className="output-content">
        {lines.map((line, i) => (
          <span
            key={i}
            className={line.stream === "stderr" ? "line-stderr" : "line-stdout"}
          >
            {line.text}
            {"\n"}
          </span>
        ))}
      </pre>
    </div>
  );
}

export default OutputView;
