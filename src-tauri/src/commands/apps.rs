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
    let section = entry.section("Desktop Entry");

    let name = section.attr("Name")?.to_string();
    let exec_raw = section.attr("Exec").unwrap_or("").to_string();
    let icon = section.attr("Icon").unwrap_or("").to_string();
    let comment = section.attr("Comment").unwrap_or("").to_string();
    let no_display = section.attr("NoDisplay").unwrap_or("false") == "true";
    let entry_type = section.attr("Type").unwrap_or("Application");

    if entry_type != "Application" {
        return None;
    }

    // Strip field codes from exec (%f, %F, %u, %U, etc.)
    let exec = exec_raw
        .split_whitespace()
        .filter(|s| !s.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ");

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

pub fn search_apps(query: &str) -> Result<Vec<SearchResult>, String> {
    let apps = APP_CACHE.get().ok_or("App cache not initialized")?;
    let mut matcher = Matcher::new(nucleo::Config::DEFAULT);
    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        nucleo::pattern::AtomKind::Fuzzy,
    );

    let mut scored: Vec<(u32, &DesktopEntry)> = apps
        .iter()
        .filter_map(|app| {
            let mut buf = Vec::new();
            let haystack = nucleo::Utf32Str::new(&app.name, &mut buf);
            let score = pattern.score(haystack, &mut matcher)?;
            Some((score, app))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));

    Ok(scored
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
        .collect())
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
