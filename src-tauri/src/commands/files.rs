use crate::router::SearchResult;
use std::path::PathBuf;

pub fn search_files(query: &str) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    let search_dirs = [
        home.join("Documents"),
        home.join("Downloads"),
        home.join("Desktop"),
        home.join("Projects"),
        home.clone(),
    ];

    for dir in &search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().contains(&query_lower) {
                    let path = entry.path();
                    results.push(SearchResult {
                        id: path.display().to_string(),
                        name: name.clone(),
                        description: path
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        icon: "".into(),
                        category: "file".into(),
                        exec: format!("xdg-open {}", path.display()),
                    });
                }
                if results.len() >= 10 {
                    return Ok(results);
                }
            }
        }
    }

    Ok(results)
}
