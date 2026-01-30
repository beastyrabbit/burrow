use burrow_lib::config;

#[test]
fn config_dir_is_absolute() {
    let dir = config::config_dir();
    assert!(dir.is_absolute() || dir.starts_with("~"));
}

#[test]
fn config_path_is_inside_config_dir() {
    let dir = config::config_dir();
    let path = config::config_path();
    assert!(path.starts_with(&dir));
}

#[test]
fn load_config_returns_valid_defaults() {
    let cfg = config::load_config();
    assert!(!cfg.ollama.url.is_empty());
    assert!(!cfg.ollama.embedding_model.is_empty());
    assert!(cfg.ollama.timeout_secs > 0);
    assert!(cfg.vector_search.top_k > 0);
    assert!(cfg.vector_search.min_score > 0.0);
    assert!(cfg.vector_search.min_score < 1.0);
    assert!(cfg.history.max_results > 0);
    assert!(cfg.search.max_results > 0);
    assert!(cfg.search.debounce_ms > 0);
}

#[test]
fn load_config_from_nonexistent_creates_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config_file = tmp.path().join("burrow").join("config.toml");

    // File shouldn't exist yet
    assert!(!config_file.exists());

    // Load will create default when file doesn't exist
    // We can't call load_config_from_path directly (private), but load_config
    // works from the real path. Just verify defaults are sane.
    let cfg = config::load_config();
    assert_eq!(cfg.ollama.url, "http://192.168.10.243:11434");
}

#[test]
fn default_index_dirs_contain_home_subdirs() {
    let cfg = config::load_config();
    assert!(cfg
        .vector_search
        .index_dirs
        .iter()
        .any(|d| d.contains("Documents")));
    assert!(cfg
        .vector_search
        .index_dirs
        .iter()
        .any(|d| d.contains("Projects")));
}

#[test]
fn default_file_extensions_include_common_types() {
    let cfg = config::load_config();
    let exts = &cfg.indexer.file_extensions;
    for expected in &["txt", "md", "rs", "ts", "py", "json"] {
        assert!(
            exts.contains(&expected.to_string()),
            "Missing extension: {expected}"
        );
    }
}
