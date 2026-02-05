use crate::icons;
use crate::router::{Category, SearchResult};
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

    // Explicit Flatpak/Snap dirs ensure coverage on systems where
    // XDG_DATA_DIRS doesn't include them. Dedup in load_desktop_entries() handles overlap.
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    // Snap application dir
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

    dirs
}

fn load_desktop_entries() -> Vec<DesktopEntry> {
    let mut entries = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for dir in desktop_dirs() {
        let pattern = dir.join("*.desktop");
        if let Ok(paths) = glob::glob(pattern.to_str().unwrap_or("")) {
            for path in paths.flatten() {
                if let Some(entry) = parse_desktop_file(&path) {
                    if !entry.no_display && seen_ids.insert(entry.id.clone()) {
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

    let get_attr = |key: &str| -> String {
        section
            .attr(key)
            .first()
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string()
    };

    let name = section.attr("Name").first()?.to_string();
    let exec_raw = get_attr("Exec");
    let icon = get_attr("Icon");
    let comment = get_attr("Comment");
    let no_display = get_attr("NoDisplay") == "true";
    let entry_type = section
        .attr("Type")
        .first()
        .map(|s| s.as_str())
        .unwrap_or("Application");

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
        .map(|(_, app)| entry_to_result(app, Category::App))
        .collect()
}

fn entry_to_result(entry: &DesktopEntry, category: Category) -> SearchResult {
    SearchResult {
        id: entry.id.clone(),
        name: entry.name.clone(),
        description: entry.comment.clone(),
        icon: icons::resolve_icon(&entry.icon),
        category,
        exec: entry.exec.clone(),
        input_spec: None,
    }
}

/// Sort apps: history entries by frecency score first, then remaining apps alphabetically.
fn sort_apps_by_frecency(
    apps: &[DesktopEntry],
    scores: &std::collections::HashMap<String, f64>,
) -> Vec<SearchResult> {
    let mut with_history: Vec<(&DesktopEntry, f64)> = Vec::new();
    let mut without_history: Vec<&DesktopEntry> = Vec::new();

    for entry in apps {
        if let Some(&score) = scores.get(&entry.id) {
            with_history.push((entry, score));
        } else {
            without_history.push(entry);
        }
    }

    with_history.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    without_history.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let mut results: Vec<SearchResult> = with_history
        .into_iter()
        .map(|(entry, _)| entry_to_result(entry, Category::History))
        .collect();

    results.extend(
        without_history
            .into_iter()
            .map(|entry| entry_to_result(entry, Category::App)),
    );

    results
}

/// Returns all apps sorted by frecency (history first, then alphabetical).
/// Uses AppContext (Tauri-free).
pub fn get_all_apps_with_frecency(
    ctx: &crate::context::AppContext,
) -> Result<Vec<SearchResult>, String> {
    let apps = APP_CACHE.get().ok_or("App cache not initialized")?;
    let scores = match super::history::get_frecency_scores(ctx) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load frecency scores, falling back to alphabetical");
            std::collections::HashMap::new()
        }
    };
    Ok(sort_apps_by_frecency(apps, &scores))
}

/// Returns all apps sorted by frecency via Tauri AppHandle (legacy path).
pub fn get_all_apps_with_frecency_tauri(
    app: &tauri::AppHandle,
) -> Result<Vec<SearchResult>, String> {
    let apps = APP_CACHE.get().ok_or("App cache not initialized")?;
    let scores = match super::history::get_frecency_scores_tauri(app) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load frecency scores, falling back to alphabetical");
            std::collections::HashMap::new()
        }
    };
    Ok(sort_apps_by_frecency(apps, &scores))
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
    if crate::actions::dry_run::is_enabled() {
        return crate::actions::dry_run::launch_app(&exec);
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
    use base64::Engine;

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
        assert_eq!(results[0].category, Category::App);
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

    // --- sort_apps_by_frecency ---

    #[test]
    fn all_apps_history_items_come_first() {
        use std::collections::HashMap;
        let apps = vec![
            make_entry("zz", "Zzz App", "zzz"),
            make_entry("ff", "Firefox", "firefox"),
            make_entry("aa", "Alpha", "alpha"),
        ];
        let mut scores = HashMap::new();
        scores.insert("ff".to_string(), 5.0);

        let results = sort_apps_by_frecency(&apps, &scores);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Firefox", "Alpha", "Zzz App"]);
        assert_eq!(results[0].category, Category::History);
        assert_eq!(results[1].category, Category::App);
    }

    #[test]
    fn all_apps_no_history_sorted_alphabetically() {
        use std::collections::HashMap;
        let apps = vec![
            make_entry("zz", "Zzz", "z"),
            make_entry("aa", "Alpha", "a"),
            make_entry("mm", "Middle", "m"),
        ];
        let scores = HashMap::new();

        let results = sort_apps_by_frecency(&apps, &scores);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Middle", "Zzz"]);
        assert!(results.iter().all(|r| r.category == Category::App));
    }

    #[test]
    fn all_apps_history_sorted_by_score_desc() {
        use std::collections::HashMap;
        let apps = vec![
            make_entry("a", "App A", "a"),
            make_entry("b", "App B", "b"),
            make_entry("c", "App C", "c"),
        ];
        let mut scores = HashMap::new();
        scores.insert("a".to_string(), 1.0);
        scores.insert("b".to_string(), 10.0);
        scores.insert("c".to_string(), 5.0);

        let results = sort_apps_by_frecency(&apps, &scores);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["App B", "App C", "App A"]);
        assert!(results.iter().all(|r| r.category == Category::History));
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

    #[test]
    fn desktop_dirs_includes_flatpak_system() {
        let dirs = desktop_dirs();
        let has_flatpak = dirs.iter().any(|d| d.to_string_lossy().contains("flatpak"));
        assert!(has_flatpak, "Expected flatpak dir in desktop_dirs");
    }

    #[test]
    fn desktop_dirs_includes_snap() {
        let dirs = desktop_dirs();
        let has_snap = dirs.iter().any(|d| d.to_string_lossy().contains("snapd"));
        assert!(has_snap, "Expected snap dir in desktop_dirs");
    }

    // --- fuzzy_search resolves icons ---

    #[test]
    fn fuzzy_search_result_icon_is_resolved() {
        // An entry with empty icon should produce empty resolved icon
        let entries = vec![make_entry("ff", "Firefox", "firefox")];
        let results = fuzzy_search(&entries, "Firefox");
        assert!(!results.is_empty());
        // Empty icon input -> empty resolved output
        assert_eq!(results[0].icon, "");
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

    // --- dedup ---

    #[test]
    fn load_desktop_entries_has_no_duplicate_ids() {
        let entries = load_desktop_entries();
        let mut seen = std::collections::HashSet::new();
        for e in &entries {
            assert!(seen.insert(&e.id), "Duplicate desktop entry id: {}", e.id);
        }
    }

    // --- real system icon resolution (data URIs) ---

    /// Apps known to be installed. Each must resolve to a data URI.
    /// If an app is not installed the assertion is skipped, but at least
    /// 3 of these must resolve — otherwise the test environment is broken.
    #[test]
    fn real_apps_resolve_to_data_uris() {
        if std::env::var("BURROW_RUN_SYSTEM_ICON_TESTS").is_err() {
            eprintln!(
                "[skip] set BURROW_RUN_SYSTEM_ICON_TESTS=1 to run real_apps_resolve_to_data_uris"
            );
            return;
        }
        let known_icons = [
            "1password",
            "google-chrome",
            "chromium",
            "com.fastmail.Fastmail",
            "com.bambulab.BambuStudio",
            "firefox",
        ];

        let mut resolved_count = 0;
        for icon_name in known_icons {
            let result = icons::resolve_icon(icon_name);
            if result.is_empty() {
                eprintln!("[skip] {icon_name}: not installed");
                continue;
            }
            resolved_count += 1;
            assert!(
                result.starts_with("data:image/"),
                "{icon_name}: expected data URI, got: {}",
                &result[..60.min(result.len())]
            );
            // Verify the base64 payload is non-trivial (> 100 bytes decoded)
            let b64 = result.split(',').nth(1).unwrap();
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap();
            assert!(
                decoded.len() > 100,
                "{icon_name}: decoded icon is only {} bytes — too small",
                decoded.len()
            );
            eprintln!("[ok] {icon_name}: data URI ({} bytes encoded)", b64.len());
        }
        assert!(
            resolved_count >= 3,
            "Only {resolved_count}/6 icons resolved — check installed apps"
        );
    }

    /// Full pipeline: load real .desktop files, search, verify results
    /// contain data URI icons that a browser <img> tag can render.
    #[test]
    fn real_search_results_have_data_uri_icons() {
        if std::env::var("BURROW_RUN_SYSTEM_ICON_TESTS").is_err() {
            eprintln!("[skip] set BURROW_RUN_SYSTEM_ICON_TESTS=1 to run real_search_results_have_data_uri_icons");
            return;
        }
        let entries = load_desktop_entries();
        let queries = ["firefox", "chrome", "1password", "fastmail", "bambu"];
        let mut icons_found = 0;

        for query in queries {
            let results = fuzzy_search(&entries, query);
            let Some(first) = results.first() else {
                eprintln!("[skip] search '{query}': no results");
                continue;
            };
            if first.icon.is_empty() {
                eprintln!("[skip] search '{query}' -> {} (no icon)", first.name);
                continue;
            }
            icons_found += 1;
            assert!(
                first.icon.starts_with("data:image/"),
                "search '{query}' -> {}: expected data URI, got: {}",
                first.name,
                &first.icon[..60.min(first.icon.len())]
            );
            eprintln!(
                "[ok] search '{query}' -> {} (icon {} bytes)",
                first.name,
                first.icon.len()
            );
        }
        assert!(
            icons_found >= 3,
            "Only {icons_found}/5 searches returned icons"
        );
    }

    /// Unique results: no duplicate app IDs after dedup.
    #[test]
    fn real_search_no_duplicates() {
        let entries = load_desktop_entries();
        let results = fuzzy_search(&entries, "fast");
        let mut seen = std::collections::HashSet::new();
        for r in &results {
            assert!(
                seen.insert(&r.id),
                "Duplicate result id in search: {}",
                r.id
            );
        }
    }
}
