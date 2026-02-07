use crate::window_manager::Stream;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

/// Shared state for output window buffers. Each running command writes lines here;
/// the frontend polls via `get_output` to retrieve new lines.
#[derive(Default)]
pub struct OutputBufferState(Mutex<HashMap<String, OutputBuffer>>);

struct OutputBuffer {
    lines: Vec<BufferedLine>,
    /// `None` while running, `Some(Some(code))` on normal exit, `Some(None)` if killed by signal.
    done: Option<Option<i32>>,
}

/// A single line of output with its stream origin.
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct BufferedLine {
    pub stream: Stream,
    pub text: String,
}

/// Snapshot returned to the frontend from a `get_output` poll.
#[derive(Clone, Default, Serialize, Debug, PartialEq)]
pub struct OutputSnapshot {
    /// New lines since the requested offset.
    pub lines: Vec<BufferedLine>,
    /// Whether the command has finished.
    pub done: bool,
    /// Process exit code when done. `None` while running or if killed by signal.
    pub exit_code: Option<i32>,
    /// Total number of lines so far (use as `since_index` for next poll).
    pub total: usize,
}

impl OutputBufferState {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, OutputBuffer>> {
        self.0.lock().expect("output buffer lock poisoned")
    }

    /// Create a new buffer for a window label.
    pub fn create(&self, label: String) {
        self.lock().insert(
            label,
            OutputBuffer {
                lines: Vec::new(),
                done: None,
            },
        );
    }

    /// Remove a buffer, freeing its memory. Called when the output window is closed
    /// after the command has finished.
    pub fn remove(&self, label: &str) {
        self.lock().remove(label);
    }

    /// Maximum number of lines per buffer. Oldest lines are dropped when exceeded.
    const MAX_LINES: usize = 50_000;

    /// Push a line of output into a buffer. Drops oldest lines if MAX_LINES exceeded.
    pub fn push_line(&self, label: &str, stream: Stream, text: String) {
        if let Some(buf) = self.lock().get_mut(label) {
            buf.lines.push(BufferedLine { stream, text });
            if buf.lines.len() > Self::MAX_LINES {
                let excess = buf.lines.len() - Self::MAX_LINES;
                buf.lines.drain(..excess);
            }
        }
    }

    /// Mark a buffer as done with an optional exit code.
    pub fn set_done(&self, label: &str, exit_code: Option<i32>) {
        if let Some(buf) = self.lock().get_mut(label) {
            buf.done = Some(exit_code);
        }
    }

    /// Get lines since `since_index`. Returns empty snapshot for unknown labels.
    pub fn get_since(&self, label: &str, since_index: usize) -> OutputSnapshot {
        self.lock()
            .get(label)
            .map_or(OutputSnapshot::default(), |buf| {
                let total = buf.lines.len();
                let start = since_index.min(total);
                let (done, exit_code) = match buf.done {
                    Some(code) => (true, code),
                    None => (false, None),
                };
                OutputSnapshot {
                    lines: buf.lines[start..].to_vec(),
                    done,
                    exit_code,
                    total,
                }
            })
    }
}

/// Tauri command: poll for new output lines.
#[tauri::command]
pub fn get_output_cmd(
    label: String,
    since_index: usize,
    state: tauri::State<'_, std::sync::Arc<OutputBufferState>>,
) -> OutputSnapshot {
    state.get_since(&label, since_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_get_empty_buffer() {
        let state = OutputBufferState::new();
        state.create("test-label".into());

        let snap = state.get_since("test-label", 0);
        assert!(snap.lines.is_empty(), "new buffer should have no lines");
        assert!(!snap.done, "new buffer should not be done");
        assert_eq!(snap.exit_code, None);
        assert_eq!(snap.total, 0);
    }

    #[test]
    fn push_lines_and_get_all() {
        let state = OutputBufferState::new();
        state.create("buf1".into());

        state.push_line("buf1", Stream::Stdout, "line 1".into());
        state.push_line("buf1", Stream::Stderr, "err 1".into());
        state.push_line("buf1", Stream::Stdout, "line 2".into());

        let snap = state.get_since("buf1", 0);
        assert_eq!(snap.lines.len(), 3, "should have 3 lines");
        assert_eq!(snap.total, 3);
        assert_eq!(snap.lines[0].text, "line 1");
        assert_eq!(snap.lines[1].text, "err 1");
        assert_eq!(snap.lines[2].text, "line 2");
        assert!(!snap.done);
    }

    #[test]
    fn get_since_returns_slice() {
        let state = OutputBufferState::new();
        state.create("buf2".into());

        state.push_line("buf2", Stream::Stdout, "a".into());
        state.push_line("buf2", Stream::Stdout, "b".into());
        state.push_line("buf2", Stream::Stdout, "c".into());

        // Get since index 1 — should return b and c
        let snap = state.get_since("buf2", 1);
        assert_eq!(snap.lines.len(), 2, "should have 2 new lines");
        assert_eq!(snap.lines[0].text, "b");
        assert_eq!(snap.lines[1].text, "c");
        assert_eq!(snap.total, 3);
    }

    #[test]
    fn get_since_at_total_returns_empty() {
        let state = OutputBufferState::new();
        state.create("buf3".into());

        state.push_line("buf3", Stream::Stdout, "x".into());

        let snap = state.get_since("buf3", 1);
        assert!(snap.lines.is_empty(), "should have no new lines");
        assert_eq!(snap.total, 1);
    }

    #[test]
    fn get_since_beyond_total_returns_empty() {
        let state = OutputBufferState::new();
        state.create("buf4".into());

        state.push_line("buf4", Stream::Stdout, "x".into());

        let snap = state.get_since("buf4", 999);
        assert!(
            snap.lines.is_empty(),
            "index beyond total should return empty"
        );
        assert_eq!(snap.total, 1);
    }

    #[test]
    fn set_done_marks_buffer_complete() {
        let state = OutputBufferState::new();
        state.create("done1".into());

        state.push_line("done1", Stream::Stdout, "output".into());
        state.set_done("done1", Some(0));

        let snap = state.get_since("done1", 0);
        assert!(snap.done, "should be done");
        assert_eq!(snap.exit_code, Some(0), "should have exit code 0");
    }

    #[test]
    fn set_done_with_none_exit_code() {
        let state = OutputBufferState::new();
        state.create("done2".into());
        state.set_done("done2", None);

        let snap = state.get_since("done2", 0);
        assert!(snap.done, "should be done");
        assert_eq!(snap.exit_code, None, "killed process has no exit code");
    }

    #[test]
    fn unknown_label_returns_empty_snapshot() {
        let state = OutputBufferState::new();

        let snap = state.get_since("nonexistent", 0);
        assert!(snap.lines.is_empty());
        assert!(!snap.done);
        assert_eq!(snap.exit_code, None);
        assert_eq!(snap.total, 0);
    }

    #[test]
    fn push_to_unknown_label_is_noop() {
        let state = OutputBufferState::new();
        // Should not panic
        state.push_line("nonexistent", Stream::Stdout, "ignored".into());
    }

    #[test]
    fn set_done_on_unknown_label_is_noop() {
        let state = OutputBufferState::new();
        // Should not panic
        state.set_done("nonexistent", Some(1));
    }

    #[test]
    fn multiple_buffers_are_independent() {
        let state = OutputBufferState::new();
        state.create("a".into());
        state.create("b".into());

        state.push_line("a", Stream::Stdout, "from a".into());
        state.push_line("b", Stream::Stderr, "from b".into());

        let snap_a = state.get_since("a", 0);
        let snap_b = state.get_since("b", 0);

        assert_eq!(snap_a.lines.len(), 1);
        assert_eq!(snap_a.lines[0].text, "from a");
        assert_eq!(snap_b.lines.len(), 1);
        assert_eq!(snap_b.lines[0].text, "from b");
    }

    #[test]
    fn incremental_polling_pattern() {
        let state = OutputBufferState::new();
        state.create("poll".into());

        // First batch
        state.push_line("poll", Stream::Stdout, "line 1".into());
        state.push_line("poll", Stream::Stdout, "line 2".into());

        let snap1 = state.get_since("poll", 0);
        assert_eq!(snap1.lines.len(), 2);
        assert_eq!(snap1.total, 2);

        // Second batch
        state.push_line("poll", Stream::Stdout, "line 3".into());

        let snap2 = state.get_since("poll", snap1.total);
        assert_eq!(snap2.lines.len(), 1);
        assert_eq!(snap2.lines[0].text, "line 3");
        assert_eq!(snap2.total, 3);

        // Mark done
        state.set_done("poll", Some(0));

        let snap3 = state.get_since("poll", snap2.total);
        assert!(snap3.lines.is_empty());
        assert!(snap3.done);
        assert_eq!(snap3.exit_code, Some(0));
    }

    #[test]
    fn remove_frees_buffer() {
        let state = OutputBufferState::new();
        state.create("rm-me".into());
        state.push_line("rm-me", Stream::Stdout, "line".into());

        state.remove("rm-me");

        let snap = state.get_since("rm-me", 0);
        assert!(snap.lines.is_empty(), "buffer should be gone after remove");
        assert_eq!(snap.total, 0);
    }

    #[test]
    fn remove_unknown_label_is_noop() {
        let state = OutputBufferState::new();
        // Should not panic
        state.remove("nonexistent");
    }

    #[test]
    fn push_line_caps_at_max_lines() {
        let state = OutputBufferState::new();
        state.create("cap".into());

        // Push MAX_LINES + 100 lines
        let max = OutputBufferState::MAX_LINES;
        for i in 0..max + 100 {
            state.push_line("cap", Stream::Stdout, format!("line {i}"));
        }

        let snap = state.get_since("cap", 0);
        assert_eq!(
            snap.lines.len(),
            max,
            "should be capped at MAX_LINES ({max})"
        );
        // First line should be line 100 (oldest 100 were dropped)
        assert_eq!(snap.lines[0].text, "line 100");
    }

    #[test]
    fn output_snapshot_serializes_correctly() {
        let snap = OutputSnapshot {
            lines: vec![
                BufferedLine {
                    stream: Stream::Stdout,
                    text: "hello".into(),
                },
                BufferedLine {
                    stream: Stream::Stderr,
                    text: "error".into(),
                },
            ],
            done: true,
            exit_code: Some(0),
            total: 2,
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"stream\":\"stdout\""), "got: {json}");
        assert!(json.contains("\"stream\":\"stderr\""), "got: {json}");
        assert!(json.contains("\"text\":\"hello\""), "got: {json}");
        assert!(json.contains("\"done\":true"), "got: {json}");
        assert!(json.contains("\"exit_code\":0"), "got: {json}");
        assert!(json.contains("\"total\":2"), "got: {json}");
    }
}
