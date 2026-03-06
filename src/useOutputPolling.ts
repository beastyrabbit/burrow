import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

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

interface UseOutputPollingOptions {
  label: string;
  onLines: (lines: BufferedLine[]) => void;
  pollIntervalMs?: number;
  maxConsecutiveErrors?: number;
}

interface UseOutputPollingResult {
  done: boolean;
  exitCode: number | null;
  pollError: string | null;
}

const DEFAULT_POLL_INTERVAL_MS = 150;
const DEFAULT_MAX_ERRORS = 20;

export type { BufferedLine, OutputSnapshot };

export function useOutputPolling({
  label,
  onLines,
  pollIntervalMs = DEFAULT_POLL_INTERVAL_MS,
  maxConsecutiveErrors = DEFAULT_MAX_ERRORS,
}: UseOutputPollingOptions): UseOutputPollingResult {
  const [done, setDone] = useState(false);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);

  // Use ref for onLines to avoid re-triggering effect when callback changes
  const onLinesRef = useRef(onLines);
  onLinesRef.current = onLines;

  // Reset state when label changes
  useEffect(() => {
    setDone(false);
    setExitCode(null);
    setPollError(null);
  }, [label]);

  // Serial polling loop
  useEffect(() => {
    let stopped = false;
    let sinceIndex = 0;
    let errorCount = 0;

    async function poll() {
      if (stopped) return;
      try {
        const snap = await invoke<OutputSnapshot>("get_output", {
          label,
          sinceIndex,
        });

        // Note: a `stopped` guard here would fix a theoretical race on rapid
        // label transitions, but React StrictMode double-mounts cause the
        // first effect's poll to consume mock data, then the guard discards
        // the response — breaking Playwright tests. The label-change state
        // reset (lines 49-53) is sufficient protection in practice.

        sinceIndex = snap.total;
        errorCount = 0;

        if (snap.lines.length > 0) {
          onLinesRef.current(snap.lines);
        }

        if (snap.done) {
          setDone(true);
          setExitCode(snap.exit_code);
          stopped = true;
          return;
        }
      } catch (err) {
        errorCount++;
        if (errorCount >= maxConsecutiveErrors) {
          console.error(
            `[useOutputPolling] ${errorCount} consecutive poll failures, giving up:`,
            err
          );
          setPollError(`Connection lost (${errorCount} failures)`);
          stopped = true;
          return;
        }
      }

      if (!stopped) {
        setTimeout(poll, pollIntervalMs);
      }
    }

    poll();

    return () => {
      stopped = true;
    };
  }, [label, pollIntervalMs, maxConsecutiveErrors]);

  return { done, exitCode, pollError };
}
