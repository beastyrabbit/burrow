use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Progress bar wrapper for CLI indexing operations that provides
/// visual feedback during long-running operations.
pub struct IndexProgress {
    bar: ProgressBar,
}

impl IndexProgress {
    /// Create a new progress bar for indexing with a known total count.
    pub fn new(total: u64) -> Self {
        let bar = ProgressBar::new(total);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.cyan} [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                .expect("valid template")
                .progress_chars("█▓░"),
        );
        bar.enable_steady_tick(Duration::from_millis(100));

        Self { bar }
    }

    /// Create a spinner for indeterminate progress (single file operations).
    pub fn spinner(msg: &str) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .expect("valid template"),
        );
        bar.set_message(msg.to_string());
        bar.enable_steady_tick(Duration::from_millis(100));

        Self { bar }
    }

    /// Update current file being processed (shown in progress message).
    pub fn set_current(&self, filename: &str) {
        self.bar.set_message(filename.to_string());
    }

    /// Increment progress by one.
    pub fn inc(&self) {
        self.bar.inc(1);
    }

    /// Finish with success message.
    pub fn finish_success(&self, msg: &str) {
        self.bar
            .finish_with_message(format!("{} {}", style("✓").green().bold(), msg));
    }

    /// Finish with error message.
    pub fn finish_error(&self, msg: &str) {
        self.bar
            .finish_with_message(format!("{} {}", style("✗").red().bold(), msg));
    }

    /// Finish and clear the progress bar (for custom summary output).
    pub fn finish_clear(&self) {
        self.bar.finish_and_clear();
    }
}

/// Simple progress tracker without a bar (for quiet mode).
/// Tracks indexed count and error messages for final summary.
#[derive(Default)]
pub struct QuietProgress {
    indexed: u32,
    errors: Vec<String>,
}

impl QuietProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc(&mut self) {
        self.indexed += 1;
    }

    pub fn add_error(&mut self, msg: String) {
        self.errors.push(msg);
    }

    pub fn indexed(&self) -> u32 {
        self.indexed
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_progress_tracks_counts() {
        let mut p = QuietProgress::new();
        p.inc();
        p.inc();
        p.add_error("test error".into());
        assert_eq!(p.indexed(), 2);
        assert_eq!(p.errors().len(), 1);
    }

    #[test]
    fn quiet_progress_default() {
        let p = QuietProgress::default();
        assert_eq!(p.indexed(), 0);
        assert!(p.errors().is_empty());
    }
}
