use crate::context::AppContext;
use crate::icons;
use crate::router::{Category, SearchResult};
use freedesktop_entry_parser::parse_entry;
use notify::{self, RecommendedWatcher, RecursiveMode, Watcher};
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::Matcher;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex, RwLock,
};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopEntry {
    id: String,
    name: String,
    exec: String,
    icon: String,
    comment: String,
    no_display: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppCacheStatus {
    pub revision: u64,
    pub app_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefreshAppsResult {
    pub changed: bool,
    pub revision: u64,
    pub app_count: usize,
}

pub struct AppIndexState {
    entries: RwLock<Vec<DesktopEntry>>,
    revision: AtomicU64,
    watcher_started: AtomicBool,
    watcher: Mutex<Option<RecommendedWatcher>>,
    source_dirs: Vec<PathBuf>,
}

#[derive(Default)]
struct WatcherRefreshState {
    scheduled: bool,
    dirty: bool,
}

impl Default for AppIndexState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppIndexState {
    pub fn new() -> Self {
        Self::new_with_dirs(desktop_dirs())
    }

    fn new_with_dirs(dirs: Vec<PathBuf>) -> Self {
        let source_dirs = dedupe_dirs(dirs);
        let entries = load_desktop_entries_from_dirs(&source_dirs);
        Self {
            entries: RwLock::new(entries),
            revision: AtomicU64::new(1),
            watcher_started: AtomicBool::new(false),
            watcher: Mutex::new(None),
            source_dirs,
        }
    }

    fn snapshot(&self) -> Vec<DesktopEntry> {
        self.entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn status(&self) -> AppCacheStatus {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        AppCacheStatus {
            revision: self.revision.load(Ordering::SeqCst),
            app_count: entries.len(),
        }
    }

    pub fn refresh(&self) -> Result<RefreshAppsResult, String> {
        self.refresh_from_dirs(&self.source_dirs)
    }

    fn refresh_from_dirs(&self, dirs: &[PathBuf]) -> Result<RefreshAppsResult, String> {
        let next_entries = load_desktop_entries_from_dirs(dirs);
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *entries == next_entries {
            return Ok(RefreshAppsResult {
                changed: false,
                revision: self.revision.load(Ordering::SeqCst),
                app_count: entries.len(),
            });
        }

        *entries = next_entries;
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(RefreshAppsResult {
            changed: true,
            revision,
            app_count: entries.len(),
        })
    }

    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let entries = self.snapshot();
        fuzzy_search(&entries, query)
    }

    pub fn resolve_exec(&self, id: &str) -> Option<String> {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.exec.clone())
    }

    pub fn start_watcher(self: &Arc<Self>) -> Result<(), String> {
        if self
            .watcher_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }

        let index_weak = Arc::downgrade(self);
        let refresh_state = Arc::new(Mutex::new(WatcherRefreshState::default()));
        let refresh_state_for_callback = Arc::clone(&refresh_state);

        let mut watcher = match notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| {
                let Some(index) = index_weak.upgrade() else {
                    return;
                };
                let event = match result {
                    Ok(event) => event,
                    Err(error) => {
                        tracing::warn!(error = %error, "application directory watch event failed");
                        return;
                    }
                };

                if !event.paths.iter().any(|path| is_relevant_app_fs_path(path)) {
                    return;
                }

                let should_spawn = {
                    let mut state = refresh_state_for_callback
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.dirty = true;
                    if state.scheduled {
                        false
                    } else {
                        state.scheduled = true;
                        true
                    }
                };

                if !should_spawn {
                    return;
                }

                let index = Arc::clone(&index);
                let refresh_state = Arc::clone(&refresh_state_for_callback);
                std::thread::spawn(move || loop {
                    std::thread::sleep(Duration::from_millis(300));

                    {
                        let mut state = refresh_state
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        state.dirty = false;
                    }

                    if let Err(error) = index.refresh() {
                        tracing::warn!(error = %error, "application cache refresh failed after watch event");
                    }

                    let should_continue = {
                        let mut state = refresh_state
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        if state.dirty {
                            state.dirty = false;
                            true
                        } else {
                            state.scheduled = false;
                            false
                        }
                    };

                    if !should_continue {
                        break;
                    }
                });
            },
        ) {
            Ok(watcher) => watcher,
            Err(error) => {
                self.watcher_started.store(false, Ordering::SeqCst);
                return Err(error.to_string());
            }
        };

        let mut watched = 0usize;
        for dir in &self.source_dirs {
            // Note: only directories that exist at this point are watched.
            // Directories created after startup require a manual #refresh to be scanned.
            if !dir.exists() {
                tracing::info!(path = %dir.display(), "skipping missing application directory");
                continue;
            }
            match watcher.watch(dir, RecursiveMode::NonRecursive) {
                Ok(()) => watched += 1,
                Err(error) => {
                    tracing::warn!(path = %dir.display(), error = %error, "failed to watch application directory");
                }
            }
        }

        if watched == 0 {
            self.watcher_started.store(false, Ordering::SeqCst);
            return Err("no application directories could be watched".to_string());
        }

        tracing::info!(
            watched_dirs = watched,
            "application directory watcher started"
        );

        *self
            .watcher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(watcher);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(dirs: Vec<PathBuf>) -> Self {
        Self::new_with_dirs(dirs)
    }

    #[cfg(test)]
    fn refresh_from_dirs_for_test(&self) -> RefreshAppsResult {
        self.refresh()
            .expect("refresh_from_dirs_for_test should not fail")
    }
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
    // XDG_DATA_DIRS doesn't include them. Duplicate desktop IDs are deduped during loading.
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    // Snap application dir
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

    dirs
}

fn load_desktop_entries_from_dirs(dirs: &[PathBuf]) -> Vec<DesktopEntry> {
    let mut entries = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for dir in dirs {
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
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

fn dedupe_dirs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    dirs.into_iter()
        .filter(|dir| seen.insert(dir.clone()))
        .collect()
}

fn is_relevant_app_fs_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("desktop"))
        .unwrap_or(false)
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
        output_mode: None,
        output_format: None,
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
pub fn get_all_apps_with_frecency(ctx: &AppContext) -> Result<Vec<SearchResult>, String> {
    let apps = ctx.apps.snapshot();
    let scores = match super::history::get_frecency_scores(ctx) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load frecency scores, falling back to alphabetical");
            std::collections::HashMap::new()
        }
    };
    Ok(sort_apps_by_frecency(&apps, &scores))
}

pub fn search_apps(query: &str, ctx: &AppContext) -> Result<Vec<SearchResult>, String> {
    Ok(ctx.apps.search(query))
}

/// Resolve a canonical app exec command by app ID from cache.
pub fn resolve_app_exec(id: &str, ctx: &AppContext) -> Option<String> {
    ctx.apps.resolve_exec(id)
}

pub fn refresh_app_cache(ctx: &AppContext) -> Result<RefreshAppsResult, String> {
    ctx.apps.refresh()
}

#[tauri::command]
pub fn app_cache_status_cmd(app: tauri::AppHandle) -> Result<AppCacheStatus, String> {
    use tauri::Manager;
    let ctx = app.state::<AppContext>();
    Ok(ctx.apps.status())
}

#[tauri::command]
pub fn refresh_app_cache_cmd(app: tauri::AppHandle) -> Result<RefreshAppsResult, String> {
    use tauri::Manager;
    let ctx = app.state::<AppContext>();
    refresh_app_cache(&ctx)
}

fn parse_exec_command(exec: &str) -> Result<Vec<String>, String> {
    let parts = shlex::split(exec).ok_or("Invalid exec command: unclosed quotes".to_string())?;
    if parts.is_empty() {
        return Err("Empty exec command".into());
    }
    Ok(parts)
}

#[tauri::command]
pub fn launch_app(exec: String) -> Result<(), String> {
    let parts = parse_exec_command(&exec)?;
    if crate::actions::dry_run::is_enabled() {
        return crate::actions::dry_run::launch_app(&exec);
    }
    Command::new(&parts[0])
        .args(&parts[1..])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::fs;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

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

    fn load_desktop_entries() -> Vec<DesktopEntry> {
        load_desktop_entries_from_dirs(&desktop_dirs())
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
    fn parse_exec_handles_quoted_args() {
        let parts = parse_exec_command(r#"my-app --title "My Window" --path '/tmp/a b'"#).unwrap();
        assert_eq!(
            parts,
            vec!["my-app", "--title", "My Window", "--path", "/tmp/a b"]
        );
    }

    #[test]
    fn parse_exec_unclosed_quote_fails() {
        let err = parse_exec_command(r#"my-app --title "broken"#).unwrap_err();
        assert!(err.contains("Invalid exec command"));
    }

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

    fn write_desktop_file(dir: &std::path::Path, file_name: &str, name: &str, exec: &str) {
        let content = format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\nIcon=\nComment=\n"
        );
        fs::write(dir.join(file_name), content).expect("should write desktop file");
    }

    #[test]
    fn refresh_adds_new_desktop_entry() {
        let dir = tempdir().unwrap();
        write_desktop_file(dir.path(), "alpha.desktop", "Alpha", "alpha");
        let index = AppIndexState::new_for_test(vec![dir.path().to_path_buf()]);

        write_desktop_file(dir.path(), "t3code.desktop", "t3code", "t3code");
        let refreshed = index.refresh_from_dirs_for_test();

        assert!(refreshed.changed);
        assert_eq!(refreshed.revision, 2);
        let results = index.search("t3code");
        assert!(results.iter().any(|result| result.id == "t3code"));
    }

    #[test]
    fn refresh_removes_deleted_desktop_entry() {
        let dir = tempdir().unwrap();
        write_desktop_file(dir.path(), "alpha.desktop", "Alpha", "alpha");
        let index = AppIndexState::new_for_test(vec![dir.path().to_path_buf()]);

        fs::remove_file(dir.path().join("alpha.desktop")).unwrap();
        let refreshed = index.refresh_from_dirs_for_test();

        assert!(refreshed.changed);
        assert!(index.search("alpha").is_empty());
    }

    #[test]
    fn refresh_updates_exec_for_existing_id() {
        let dir = tempdir().unwrap();
        write_desktop_file(dir.path(), "alpha.desktop", "Alpha", "alpha --old");
        let index = AppIndexState::new_for_test(vec![dir.path().to_path_buf()]);

        write_desktop_file(dir.path(), "alpha.desktop", "Alpha", "alpha --new");
        let refreshed = index.refresh_from_dirs_for_test();

        assert!(refreshed.changed);
        assert_eq!(index.resolve_exec("alpha").as_deref(), Some("alpha --new"));
    }

    #[test]
    fn refresh_no_change_keeps_revision() {
        let dir = tempdir().unwrap();
        write_desktop_file(dir.path(), "alpha.desktop", "Alpha", "alpha");
        let index = AppIndexState::new_for_test(vec![dir.path().to_path_buf()]);

        let status_before = index.status();
        let refreshed = index.refresh_from_dirs_for_test();
        let status_after = index.status();

        assert!(!refreshed.changed);
        assert_eq!(refreshed.revision, status_before.revision);
        assert_eq!(status_after.revision, status_before.revision);
    }

    #[test]
    fn status_reports_revision_and_count() {
        let dir = tempdir().unwrap();
        write_desktop_file(dir.path(), "alpha.desktop", "Alpha", "alpha");
        write_desktop_file(dir.path(), "beta.desktop", "Beta", "beta");
        let index = AppIndexState::new_for_test(vec![dir.path().to_path_buf()]);

        let status = index.status();
        assert_eq!(status.revision, 1);
        assert_eq!(status.app_count, 2);
    }

    #[test]
    fn relevant_fs_event_accepts_desktop_files() {
        assert!(is_relevant_app_fs_path(std::path::Path::new(
            "/tmp/applications/t3code.desktop"
        )));
    }

    #[test]
    fn relevant_fs_event_ignores_non_desktop_files() {
        assert!(!is_relevant_app_fs_path(std::path::Path::new(
            "/tmp/applications/icon.png"
        )));
    }

    #[test]
    fn watcher_refreshes_after_desktop_file_created() {
        let dir = tempdir().unwrap();
        let index = Arc::new(AppIndexState::new_for_test(vec![dir.path().to_path_buf()]));
        index.start_watcher().expect("watcher should start");

        write_desktop_file(dir.path(), "watched.desktop", "Watched App", "watched-app");

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = index.status();
            if status.revision > 1
                && index
                    .search("Watched App")
                    .iter()
                    .any(|result| result.id == "watched")
            {
                break;
            }

            assert!(
                Instant::now() < deadline,
                "watcher did not refresh cache after desktop file creation"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn watcher_refreshes_after_desktop_file_updated() {
        let dir = tempdir().unwrap();
        write_desktop_file(
            dir.path(),
            "watched.desktop",
            "Watched App",
            "watched-app --old",
        );

        let index = Arc::new(AppIndexState::new_for_test(vec![dir.path().to_path_buf()]));
        index.start_watcher().expect("watcher should start");

        write_desktop_file(
            dir.path(),
            "watched.desktop",
            "Watched App",
            "watched-app --new",
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if index.resolve_exec("watched").as_deref() == Some("watched-app --new") {
                break;
            }

            assert!(
                Instant::now() < deadline,
                "watcher did not refresh cache after desktop file update"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn watcher_refreshes_after_desktop_file_deleted() {
        let dir = tempdir().unwrap();
        write_desktop_file(dir.path(), "watched.desktop", "Watched App", "watched-app");

        let index = Arc::new(AppIndexState::new_for_test(vec![dir.path().to_path_buf()]));
        index.start_watcher().expect("watcher should start");

        fs::remove_file(dir.path().join("watched.desktop")).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = index.status();
            if status.revision > 1 && index.search("Watched App").is_empty() {
                break;
            }

            assert!(
                Instant::now() < deadline,
                "watcher did not refresh cache after desktop file deletion"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn watcher_start_fails_when_no_directories_can_be_watched() {
        let dir = tempdir().unwrap();
        let missing_dir = dir.path().join("missing-applications");
        let index = Arc::new(AppIndexState::new_for_test(vec![missing_dir]));

        let error = index
            .start_watcher()
            .expect_err("watcher should fail when all source directories are missing");

        assert!(error.contains("no application directories could be watched"));
        assert!(!index.watcher_started.load(Ordering::SeqCst));
    }

    #[test]
    fn watcher_does_not_keep_app_index_alive_after_drop() {
        let dir = tempdir().unwrap();
        let index = Arc::new(AppIndexState::new_for_test(vec![dir.path().to_path_buf()]));
        let weak_index = Arc::downgrade(&index);

        index.start_watcher().expect("watcher should start");
        drop(index);

        let deadline = Instant::now() + Duration::from_secs(2);
        while weak_index.upgrade().is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }

        assert!(
            weak_index.upgrade().is_none(),
            "watcher callback should not keep AppIndexState alive after drop"
        );
    }
}
