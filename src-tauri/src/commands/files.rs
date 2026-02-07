use crate::config;
use crate::indexer;
use crate::router::{Category, SearchResult};
use std::path::PathBuf;

fn match_files_in_dirs(dirs: &[PathBuf], query: &str, limit: usize) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().contains(&query_lower) {
                    let path = entry.path();
                    results.push(SearchResult {
                        id: path.display().to_string(),
                        name,
                        description: path
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        icon: "".into(),
                        category: Category::File,
                        // Security: exec intentionally empty. handle_file uses result.id
                        // with xdg_open via Command::arg() to prevent shell injection
                        exec: String::new(),
                        input_spec: None,
                        output_mode: None,
                    });
                }
                if results.len() >= limit {
                    return results;
                }
            }
        }
    }

    results
}

pub fn search_files(query: &str) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    let cfg = config::get_config();
    let search_dirs = indexer::get_search_directories(cfg);

    Ok(match_files_in_dirs(
        &search_dirs,
        query,
        cfg.search.max_results,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("readme.txt"), "hello").unwrap();
        fs::write(dir.path().join("notes.md"), "notes").unwrap();
        fs::write(dir.path().join("photo.png"), "img").unwrap();
        fs::write(dir.path().join("Report_2024.pdf"), "pdf").unwrap();
        dir
    }

    #[test]
    fn finds_matching_files() {
        let dir = setup_test_dir();
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "readme", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "readme.txt");
    }

    #[test]
    fn case_insensitive_search() {
        let dir = setup_test_dir();
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "README", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn partial_match() {
        let dir = setup_test_dir();
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "note", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "notes.md");
    }

    #[test]
    fn no_match_returns_empty() {
        let dir = setup_test_dir();
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "zzzzz", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn respects_limit() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            fs::write(dir.path().join(format!("file_{i}.txt")), "x").unwrap();
        }
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "file", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn results_have_file_category() {
        let dir = setup_test_dir();
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "readme", 10);
        assert_eq!(results[0].category, Category::File);
    }

    #[test]
    fn results_have_parent_as_description() {
        let dir = setup_test_dir();
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "readme", 10);
        assert_eq!(results[0].description, dir.path().display().to_string());
    }

    #[test]
    fn searches_multiple_dirs() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        fs::write(dir1.path().join("alpha.txt"), "a").unwrap();
        fs::write(dir2.path().join("alpha.md"), "b").unwrap();
        let dirs = vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, "alpha", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn nonexistent_dir_skipped() {
        let dirs = vec![PathBuf::from("/nonexistent_test_dir_12345")];
        let results = match_files_in_dirs(&dirs, "anything", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn search_files_empty_query() {
        let result = search_files("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn extension_match() {
        let dir = setup_test_dir();
        let dirs = vec![dir.path().to_path_buf()];
        let results = match_files_in_dirs(&dirs, ".pdf", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Report_2024.pdf");
    }
}
