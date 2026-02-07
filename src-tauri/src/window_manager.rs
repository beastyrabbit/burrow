use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// Which stream a line of output came from.
#[derive(Clone, Copy, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Generate a unique window label for an output window.
/// Format: `output-{name}-{unix_ms}` to guarantee uniqueness.
pub fn make_output_label(name: &str) -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("output-{name}-{ms}")
}

/// Spawn a new output window using the Tauri WebviewWindow API.
/// Returns the window label on success.
pub fn spawn_output_window(app: &tauri::AppHandle, name: &str) -> Result<String, String> {
    use tauri::WebviewUrl;

    let label = make_output_label(name);
    let title = format!("Burrow - {name}");
    let url = format!(
        "index.html?view=output&label={}&title={}",
        urlencoded(&label),
        urlencoded(name),
    );

    tauri::WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title(&title)
        .inner_size(900.0, 700.0)
        .decorations(false)
        .resizable(true)
        .center()
        .build()
        .map_err(|e| format!("failed to create output window: {e}"))?;

    tracing::info!(label = %label, title = %title, "spawned output window");
    Ok(label)
}

/// Percent-encode a string for use in URL query parameters.
fn urlencoded(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_output_label_contains_name() {
        let label = make_output_label("kub-merge");
        assert!(
            label.starts_with("output-kub-merge-"),
            "label should start with output-kub-merge-, got: {label}"
        );
    }

    #[test]
    fn make_output_label_is_unique() {
        let a = make_output_label("test");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = make_output_label("test");
        assert_ne!(a, b, "labels should be unique across calls");
    }

    #[test]
    fn urlencoded_handles_special_chars() {
        // form_urlencoded uses + for spaces (standard application/x-www-form-urlencoded)
        assert_eq!(urlencoded("hello world"), "hello+world");
        assert_eq!(urlencoded("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencoded("foo#bar"), "foo%23bar");
        assert_eq!(urlencoded("100%"), "100%25");
    }

    #[test]
    fn urlencoded_passes_simple_text() {
        assert_eq!(urlencoded("kub-merge"), "kub-merge");
    }

    #[test]
    fn stream_serializes_to_lowercase() {
        let json = serde_json::to_string(&Stream::Stdout).unwrap();
        assert_eq!(json, "\"stdout\"");

        let json = serde_json::to_string(&Stream::Stderr).unwrap();
        assert_eq!(json, "\"stderr\"");
    }
}
