use base64::Engine;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

static ICON_CACHE: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Resolve a freedesktop icon name to a data URI.
/// Returns `data:image/png;base64,...` or `data:image/svg+xml;base64,...`,
/// or empty string if not found, unreadable, or in an unsupported format
/// (only PNG and SVG are supported).
///
/// Results are cached for the lifetime of the process.
pub fn resolve_icon(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }

    if let Some(cached) = ICON_CACHE.lock().unwrap().get(name) {
        return cached.clone();
    }

    let result = resolve_icon_uncached(name);
    ICON_CACHE
        .lock()
        .unwrap()
        .insert(name.to_string(), result.clone());
    result
}

fn resolve_icon_uncached(name: &str) -> String {
    let path = if name.starts_with('/') {
        let p = Path::new(name);
        if p.exists() {
            p.to_path_buf()
        } else {
            return String::new();
        }
    } else {
        match freedesktop_icons::lookup(name)
            .with_size(32)
            .with_cache()
            .find()
        {
            Some(p) => p,
            None => return String::new(),
        }
    };

    file_to_data_uri(&path)
}

fn file_to_data_uri(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let mime = match ext.as_deref() {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("xpm") => return String::new(),
        Some(ext) => {
            eprintln!("[icons] unsupported format .{ext} for {}", path.display());
            return String::new();
        }
        None => return String::new(),
    };

    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[icons] failed to read {}: {e}", path.display());
            return String::new();
        }
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    format!("data:{mime};base64,{b64}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_name_returns_empty() {
        assert_eq!(resolve_icon(""), "");
    }

    #[test]
    fn absolute_path_nonexistent_returns_empty() {
        assert_eq!(resolve_icon("/nonexistent/icon.png"), "");
    }

    #[test]
    fn nonexistent_icon_name_returns_empty() {
        assert_eq!(resolve_icon("__totally_fake_icon_zzz__"), "");
    }

    #[test]
    fn absolute_path_non_image_returns_empty() {
        // /etc/hostname exists but isn't a png/svg — should return empty
        let path = "/etc/hostname";
        if Path::new(path).exists() {
            assert_eq!(resolve_icon(path), "");
        }
    }

    // --- file_to_data_uri deterministic tests (tempfile) ---

    #[test]
    fn file_to_data_uri_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n fake png data here").unwrap();
        let result = file_to_data_uri(&path);
        assert!(
            result.starts_with("data:image/png;base64,"),
            "Expected png data URI, got: {}",
            &result[..50.min(result.len())]
        );
        // Verify round-trip
        let b64 = result.split(',').nth(1).unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert_eq!(&decoded[..4], b"\x89PNG");
    }

    #[test]
    fn file_to_data_uri_svg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.svg");
        std::fs::write(&path, b"<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        let result = file_to_data_uri(&path);
        assert!(
            result.starts_with("data:image/svg+xml;base64,"),
            "Expected svg data URI, got: {}",
            &result[..50.min(result.len())]
        );
    }

    #[test]
    fn file_to_data_uri_xpm_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icon.xpm");
        std::fs::write(&path, b"/* XPM */").unwrap();
        assert_eq!(file_to_data_uri(&path), "");
    }

    #[test]
    fn file_to_data_uri_unknown_extension_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icon.bmp");
        std::fs::write(&path, b"BM").unwrap();
        assert_eq!(file_to_data_uri(&path), "");
    }

    #[test]
    fn file_to_data_uri_no_extension_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icon");
        std::fs::write(&path, b"data").unwrap();
        assert_eq!(file_to_data_uri(&path), "");
    }

    #[test]
    fn file_to_data_uri_nonexistent_returns_empty() {
        let path = Path::new("/tmp/__nonexistent_icon_test__.png");
        assert_eq!(file_to_data_uri(path), "");
    }

    // --- resolve_icon with real system icons ---

    #[test]
    fn resolve_returns_data_uri_for_png() {
        let result = resolve_icon("firefox");
        if !result.is_empty() {
            assert!(
                result.starts_with("data:image/png;base64,"),
                "Expected png data URI, got prefix: {}",
                &result[..50.min(result.len())]
            );
        }
    }

    #[test]
    fn resolve_data_uri_is_valid_base64() {
        let result = resolve_icon("firefox");
        if !result.is_empty() {
            let b64 = match result.split(',').nth(1) {
                Some(b) => b,
                None => panic!("no comma in data URI: {}", &result[..50.min(result.len())]),
            };
            assert!(!b64.is_empty(), "Base64 payload is empty");
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .expect("Invalid base64");
            assert!(decoded.len() > 100, "Decoded icon too small");
        }
    }

    #[test]
    fn resolve_icon_is_cached() {
        // Calling resolve_icon twice with the same name should return identical results
        // without re-reading the file (we can't easily test the "no re-read" part,
        // but we verify the cache returns the same value).
        let first = resolve_icon("firefox");
        let second = resolve_icon("firefox");
        assert_eq!(first, second, "Cached result should match first call");
        // Also test with a name that doesn't resolve
        let empty1 = resolve_icon("__nonexistent__");
        let empty2 = resolve_icon("__nonexistent__");
        assert_eq!(empty1, "");
        assert_eq!(empty2, "");
    }

    #[test]
    fn resolve_flatpak_icon_returns_data_uri() {
        let result = resolve_icon("com.fastmail.Fastmail");
        if !result.is_empty() {
            assert!(
                result.starts_with("data:image/"),
                "Flatpak icon should return data URI, got prefix: {}",
                &result[..50.min(result.len())]
            );
        }
    }
}
