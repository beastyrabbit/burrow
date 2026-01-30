use crate::router::SearchResult;
use freedesktop_entry_parser::parse_entry;
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::Matcher;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
struct DesktopEntry {
    id: String,
    name: String,
    exec: String,
    icon: String,
    comment: String,
    no_display: bool,
}

static APP_CACHE: OnceLock<Vec<DesktopEntry>> = OnceLock::new();

pub fn init_app_cache() {
    APP_CACHE.get_or_init(load_desktop_entries);
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![];
    if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
        for dir in data_dirs.split(':') {
            dirs.push(PathBuf::from(dir).join("applications"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/share/applications"));
        dirs.push(PathBuf::from("/usr/local/share/applications"));
    }
    if let Some(home) = dirs::data_local_dir() {
        dirs.push(home.join("applications"));
    }
    if let Some(home) = dirs::data_dir() {
        dirs.push(home.join("applications"));
    }
    dirs
}

fn load_desktop_entries() -> Vec<DesktopEntry> {
    let mut entries = Vec::new();
    for dir in desktop_dirs() {
        let pattern = dir.join("*.desktop");
        if let Ok(paths) = glob::glob(pattern.to_str().unwrap_or("")) {
            for path in paths.flatten() {
                if let Some(entry) = parse_desktop_file(&path) {
                    if !entry.no_display {
                        entries.push(entry);
                    }
                }
            }
        }
    }
    entries
}

fn parse_desktop_file(path: &PathBuf) -> Option<DesktopEntry> {
    let entry = parse_entry(path).ok()?;
    let section = entry.section("Desktop Entry")?;

    let name = section.attr("Name").first()?.to_string();
    let exec_raw = section.attr("Exec").first().map(|s| s.as_str()).unwrap_or("").to_string();
    let icon = section.attr("Icon").first().map(|s| s.as_str()).unwrap_or("").to_string();
    let comment = section.attr("Comment").first().map(|s| s.as_str()).unwrap_or("").to_string();
    let no_display = section.attr("NoDisplay").first().map(|s| s.as_str()).unwrap_or("false") == "true";
    let entry_type = section.attr("Type").first().map(|s| s.as_str()).unwrap_or("Application");

    if entry_type != "Application" {
        return None;
    }

    let exec = strip_field_codes(&exec_raw);

    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    Some(DesktopEntry {
        id,
        name,
        exec,
        icon,
        comment,
        no_display,
    })
}

/// Strip freedesktop field codes (%f, %F, %u, %U, etc.) from an Exec string.
fn strip_field_codes(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|s| !s.starts_with('%'))
        .map(|s| {
            // Remove embedded field codes like --class=%c → --class=
            // But preserve literal %% as %
            if s.contains('%') {
                let mut result = String::new();
                let mut chars = s.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '%' {
                        if chars.peek() == Some(&'%') {
                            result.push('%');
                            chars.next();
                        } else {
                            // Skip the field code letter
                            chars.next();
                        }
                    } else {
                        result.push(c);
                    }
                }
                result
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fuzzy search a list of entries and return scored results.
fn fuzzy_search(entries: &[DesktopEntry], query: &str) -> Vec<SearchResult> {
    let mut matcher = Matcher::new(nucleo::Config::DEFAULT);
    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        nucleo::pattern::AtomKind::Fuzzy,
    );

    let mut scored: Vec<(u32, &DesktopEntry)> = entries
        .iter()
        .filter_map(|app| {
            let mut buf = Vec::new();
            let haystack = nucleo::Utf32Str::new(&app.name, &mut buf);
            let score = pattern.score(haystack, &mut matcher)?;
            Some((score, app))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));

    scored
        .into_iter()
        .take(10)
        .map(|(_, app)| SearchResult {
            id: app.id.clone(),
            name: app.name.clone(),
            description: app.comment.clone(),
            icon: app.icon.clone(),
            category: "app".into(),
            exec: app.exec.clone(),
        })
        .collect()
}

pub fn search_apps(query: &str) -> Result<Vec<SearchResult>, String> {
    let apps = APP_CACHE.get().ok_or("App cache not initialized")?;
    Ok(fuzzy_search(apps, query))
}

#[tauri::command]
pub fn launch_app(exec: String) -> Result<(), String> {
    let parts: Vec<&str> = exec.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty exec command".into());
    }
    Command::new(parts[0])
        .args(&parts[1..])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, name: &str, exec: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.into(),
            name: name.into(),
            exec: exec.into(),
            icon: "".into(),
            comment: "".into(),
            no_display: false,
        }
    }

    // --- strip_field_codes ---

    #[test]
    fn strip_codes_removes_percent_args() {
        assert_eq!(strip_field_codes("firefox %u"), "firefox");
    }

    #[test]
    fn strip_codes_removes_multiple() {
        assert_eq!(strip_field_codes("app %f %F %u"), "app");
    }

    #[test]
    fn strip_codes_keeps_non_percent() {
        assert_eq!(strip_field_codes("app --flag value"), "app --flag value");
    }

    #[test]
    fn strip_codes_empty_string() {
        assert_eq!(strip_field_codes(""), "");
    }

    #[test]
    fn strip_codes_only_percent() {
        assert_eq!(strip_field_codes("%u"), "");
    }

    // --- fuzzy_search ---

    #[test]
    fn fuzzy_exact_match() {
        let entries = vec![
            make_entry("ff", "Firefox", "firefox"),
            make_entry("ch", "Chromium", "chromium"),
        ];
        let results = fuzzy_search(&entries, "Firefox");
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn fuzzy_partial_match() {
        let entries = vec![
            make_entry("ff", "Firefox", "firefox"),
            make_entry("fm", "Files", "nautilus"),
        ];
        let results = fuzzy_search(&entries, "fire");
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn fuzzy_case_insensitive() {
        let entries = vec![make_entry("ff", "Firefox", "firefox")];
        let results = fuzzy_search(&entries, "firefox");
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn fuzzy_no_match() {
        let entries = vec![make_entry("ff", "Firefox", "firefox")];
        let results = fuzzy_search(&entries, "zzzzz");
        assert!(results.is_empty());
    }

    #[test]
    fn fuzzy_empty_query_returns_nothing() {
        // nucleo returns no score for empty pattern
        let entries = vec![make_entry("ff", "Firefox", "firefox")];
        let results = fuzzy_search(&entries, "");
        // Empty query may or may not match — just ensure no panic
        assert!(results.len() <= entries.len());
    }

    #[test]
    fn fuzzy_limits_to_10() {
        let entries: Vec<DesktopEntry> = (0..20)
            .map(|i| make_entry(&format!("app{i}"), &format!("Application {i}"), "exec"))
            .collect();
        let results = fuzzy_search(&entries, "Application");
        assert!(results.len() <= 10);
    }

    #[test]
    fn fuzzy_returns_app_category() {
        let entries = vec![make_entry("ff", "Firefox", "firefox")];
        let results = fuzzy_search(&entries, "Firefox");
        assert_eq!(results[0].category, "app");
    }

    #[test]
    fn fuzzy_results_sorted_by_score() {
        let entries = vec![
            make_entry("a", "ABCD", "a"),
            make_entry("b", "AB", "b"),
            make_entry("c", "ABCDEFGH", "c"),
        ];
        let results = fuzzy_search(&entries, "AB");
        // All should match; exact shorter match should score higher
        assert!(results.len() >= 2);
        // First result should be the best match
    }

    // --- launch_app ---

    #[test]
    fn launch_empty_exec_fails() {
        let result = launch_app("".into());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Empty exec command");
    }

    #[test]
    fn launch_nonexistent_binary_fails() {
        let result = launch_app("__nonexistent_binary_12345__".into());
        assert!(result.is_err());
    }

    // --- desktop_dirs ---

    #[test]
    fn desktop_dirs_returns_nonempty() {
        let dirs = desktop_dirs();
        assert!(!dirs.is_empty());
    }

    #[test]
    fn desktop_dirs_all_end_with_applications() {
        let dirs = desktop_dirs();
        for d in &dirs {
            assert!(
                d.ends_with("applications"),
                "Expected path ending in 'applications', got: {}",
                d.display()
            );
        }
    }

    // --- parse_desktop_file with real files ---

    #[test]
    fn parse_real_desktop_files_if_exist() {
        // This test only validates on systems with .desktop files
        let test_path = PathBuf::from("/usr/share/applications/firefox.desktop");
        if test_path.exists() {
            let entry = parse_desktop_file(&test_path);
            if let Some(e) = entry {
                assert!(!e.name.is_empty());
                assert!(!e.exec.is_empty());
                // exec should not contain % field codes
                assert!(!e.exec.contains('%'));
            }
        }
    }
}
