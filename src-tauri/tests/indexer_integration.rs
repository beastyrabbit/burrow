use burrow_lib::indexer::is_indexable_file;
use std::fs;
use tempfile::TempDir;

fn default_exts() -> Vec<String> {
    vec![
        "txt", "md", "rs", "ts", "tsx", "js", "py", "toml", "yaml", "yml", "json", "sh", "css",
        "html",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[test]
fn index_text_file_is_indexable() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("readme.txt");
    fs::write(&file, "Hello, world!").unwrap();
    assert!(is_indexable_file(&file, 1_000_000, &default_exts()));
}

#[test]
fn skip_large_file() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("big.txt");
    fs::write(&file, "x".repeat(2_000_000)).unwrap();
    assert!(!is_indexable_file(&file, 1_000_000, &default_exts()));
}

#[test]
fn skip_unknown_extension() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("binary.exe");
    fs::write(&file, "MZ").unwrap();
    assert!(!is_indexable_file(&file, 1_000_000, &default_exts()));
}

#[test]
fn skip_hidden_file() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join(".secret.rs");
    fs::write(&file, "fn main() {}").unwrap();
    assert!(!is_indexable_file(&file, 1_000_000, &default_exts()));
}

#[test]
fn skip_no_extension() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("Makefile");
    fs::write(&file, "all:").unwrap();
    assert!(!is_indexable_file(&file, 1_000_000, &default_exts()));
}

#[test]
fn all_common_extensions_indexable() {
    let tmp = TempDir::new().unwrap();
    let exts = default_exts();
    for ext in &exts {
        let file = tmp.path().join(format!("test.{ext}"));
        fs::write(&file, "content").unwrap();
        assert!(
            is_indexable_file(&file, 1_000_000, &exts),
            ".{ext} should be indexable"
        );
    }
}

#[test]
fn custom_extension_list() {
    let tmp = TempDir::new().unwrap();
    let custom = vec!["xyz".to_string()];

    let yes = tmp.path().join("data.xyz");
    let no = tmp.path().join("data.txt");
    fs::write(&yes, "y").unwrap();
    fs::write(&no, "n").unwrap();

    assert!(is_indexable_file(&yes, 1_000_000, &custom));
    assert!(!is_indexable_file(&no, 1_000_000, &custom));
}

#[test]
fn nonexistent_file_not_indexable() {
    let exts = default_exts();
    assert!(!is_indexable_file(
        std::path::Path::new("/nonexistent/file.rs"),
        1_000_000,
        &exts
    ));
}

#[test]
fn directory_not_indexable() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("subdir.rs");
    fs::create_dir_all(&dir).unwrap();
    // is_indexable_file checks is_file()
    assert!(!is_indexable_file(&dir, 1_000_000, &default_exts()));
}
