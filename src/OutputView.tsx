import { useState, useEffect, useRef, useCallback } from "react";
import { useOutputPolling, type BufferedLine } from "./useOutputPolling";
import { useAutoScroll } from "./useAutoScroll";
import "./OutputView.css";

interface StableLine extends BufferedLine {
  id: number;
}

interface OutputViewProps {
  label: string;
  title: string;
}

const MAX_LINES = 10_000;

function getStatus(done: boolean, exitCode: number | null): { className: string; text: string } {
  if (!done) return { className: "status-running", text: "Running..." };
  if (exitCode === 0) return { className: "status-success", text: "Done" };
  return { className: "status-error", text: `Exit ${exitCode ?? "?"}` };
}

function OutputView({ label, title }: OutputViewProps): React.JSX.Element {
  const [lines, setLines] = useState<StableLine[]>([]);
  const outputRef = useRef<HTMLPreElement>(null);
  const lineCounterRef = useRef(0);

  // Reset lines when label changes
  useEffect(() => {
    setLines([]);
    lineCounterRef.current = 0;
  }, [label]);

  const handleLines = useCallback((newLines: BufferedLine[]) => {
    setLines((prev) => {
      const stamped = newLines.map((l) => ({ ...l, id: lineCounterRef.current++ }));
      const next = [...prev, ...stamped];
      return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next;
    });
  }, []);

  const { done, exitCode, pollError } = useOutputPolling({
    label,
    onLines: handleLines,
  });

  useAutoScroll(outputRef, [lines]);

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
        {lines.map((line) => (
          <span
            key={line.id}
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
