use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

static CONFIG: OnceLock<AppConfig> = OnceLock::new();

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub models: ModelsConfig,
    pub ollama: OllamaConfig,
    pub openrouter: OpenRouterConfig,
    pub chat: ChatConfig,
    pub vector_search: VectorSearchConfig,
    pub indexer: IndexerConfig,
    pub history: HistoryConfig,
    pub search: SearchConfig,
    pub onepass: OnePassConfig,
    pub daemon: DaemonConfig,
}

/// A model specification with provider routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Model name (e.g., "qwen3-embedding:8b", "anthropic/claude-sonnet-4")
    pub name: String,
    /// Provider: "ollama" or "openrouter"
    pub provider: String,
}

impl ModelSpec {
    pub fn ollama(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: "ollama".into(),
        }
    }

    pub fn openrouter(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: "openrouter".into(),
        }
    }
}

/// Unified model configuration with provider routing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    /// Model for vector embeddings (typically local Ollama)
    pub embedding: ModelSpec,
    /// Small/fast chat model for quick responses
    pub chat: ModelSpec,
    /// Large/powerful chat model for complex queries
    pub chat_large: ModelSpec,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            embedding: ModelSpec::ollama("qwen3-embedding:8b"),
            chat: ModelSpec::ollama("gpt-oss:20b"),
            chat_large: ModelSpec::ollama("gpt-oss:120b"),
        }
    }
}

/// Chat behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatConfig {
    /// Enable RAG (retrieval-augmented generation) for chat-docs
    pub rag_enabled: bool,
    /// Maximum context snippets to include in RAG prompt
    pub max_context_snippets: usize,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            rag_enabled: true,
            max_context_snippets: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OnePassConfig {
    /// Minutes of idle time before the vault is cleared. Set to 0 to disable idle timeout.
    pub idle_timeout_minutes: u32,
}

impl Default for OnePassConfig {
    fn default() -> Self {
        Self {
            idle_timeout_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// Whether CLI commands should auto-start the daemon if not running.
    pub auto_start: bool,
    /// Timeout in seconds when waiting for daemon to start.
    pub startup_timeout_secs: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            auto_start: true,
            startup_timeout_secs: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub url: String,
    /// Timeout for embedding requests (seconds)
    pub timeout_secs: u64,
    /// Timeout for chat requests (seconds) - longer for complex generation
    pub chat_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VectorSearchConfig {
    pub enabled: bool,
    pub top_k: usize,
    pub min_score: f32,
    pub max_file_size_bytes: u64,
    /// Index mode: "all" (indexes home directory) or "custom" (uses index_dirs)
    pub index_mode: String,
    /// Directories to index when index_mode is "custom"
    pub index_dirs: Vec<String>,
    /// Glob patterns to exclude from indexing
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexerConfig {
    pub interval_hours: u64,
    pub file_extensions: Vec<String>,
    pub max_content_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub max_results: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub max_results: usize,
    pub debounce_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenRouterConfig {
    /// Never serialize back to config.toml to avoid leaking secrets to disk.
    #[serde(skip_serializing)]
    pub api_key: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:11434".into(),
            timeout_secs: 30,
            chat_timeout_secs: 120,
        }
    }
}

impl Default for VectorSearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            top_k: 10,
            min_score: 0.3,
            max_file_size_bytes: 1_000_000,
            index_mode: "all".into(),
            index_dirs: vec![
                "~/Documents".into(),
                "~/Projects".into(),
                "~/Downloads".into(),
            ],
            exclude_patterns: default_exclude_patterns(),
        }
    }
}

/// Default exclusion patterns for indexing (based on BackInTime/rsync best practices)
fn default_exclude_patterns() -> Vec<String> {
    vec![
        // System/virtual directories
        "/proc".into(),
        "/sys".into(),
        "/dev".into(),
        "/run".into(),
        "/tmp".into(),
        "/mnt".into(),
        "/media".into(),
        "/lost+found".into(),
        // User caches and trash
        ".cache".into(),
        "*[Cc]ache*".into(),
        ".thumbnails*".into(),
        ".local/share/Trash".into(),
        ".local/share/[Tt]rash*".into(),
        ".gvfs".into(),
        ".Private".into(),
        // Version control
        ".git".into(),
        // Build artifacts
        "node_modules".into(),
        "target".into(),
        "__pycache__".into(),
        ".venv".into(),
        "venv".into(),
        "*.pyc".into(),
        // Temporary/backup files
        "*.swp".into(),
        "*~".into(),
        "*.backup*".into(),
        "*.tmp".into(),
        // Large binary dirs
        ".steam".into(),
        ".local/share/Steam".into(),
        "snap".into(),
        ".snap".into(),
    ]
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            interval_hours: 24,
            file_extensions: vec![
                "txt", "md", "rs", "ts", "tsx", "js", "py", "toml", "yaml", "yml", "json", "sh",
                "css", "html", "pdf", "doc", "docx", "xlsx", "xls", "pptx", "odt", "ods", "odp",
                "csv", "rtf",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            max_content_chars: 4096,
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { max_results: 6 }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_results: 10,
            debounce_ms: 80,
        }
    }
}

pub fn config_dir() -> PathBuf {
    // Allow override via env var for testing
    if let Ok(dir) = std::env::var("BURROW_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("burrow")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn load_config() -> AppConfig {
    load_config_from_path(&config_path())
}

fn load_config_from_path(path: &PathBuf) -> AppConfig {
    match std::fs::read_to_string(path) {
        Ok(content) => parse_config(&content),
        Err(_) => {
            // Create default config file if dir exists or can be created
            let cfg = AppConfig::default();
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            if let Ok(toml_str) = toml::to_string_pretty(&cfg) {
                std::fs::write(path, &toml_str).ok();
            }
            cfg
        }
    }
}

fn parse_config(content: &str) -> AppConfig {
    toml::from_str(content).unwrap_or_default()
}

fn apply_env_overrides(mut cfg: AppConfig) -> AppConfig {
    // Ollama settings
    if let Ok(url) = std::env::var("BURROW_OLLAMA_URL") {
        cfg.ollama.url = url;
    }

    // Model overrides
    if let Ok(model) = std::env::var("BURROW_MODEL_EMBEDDING") {
        cfg.models.embedding.name = model;
    }
    if let Ok(model) = std::env::var("BURROW_MODEL_CHAT") {
        cfg.models.chat.name = model;
    }
    if let Ok(model) = std::env::var("BURROW_MODEL_CHAT_LARGE") {
        cfg.models.chat_large.name = model;
    }
    if let Ok(provider) = std::env::var("BURROW_MODEL_CHAT_LARGE_PROVIDER") {
        cfg.models.chat_large.provider = provider;
    }

    // Legacy env var support (maps to new config)
    if let Ok(model) = std::env::var("BURROW_OLLAMA_EMBEDDING_MODEL") {
        cfg.models.embedding.name = model;
    }

    // Vector search settings
    if let Ok(val) = std::env::var("BURROW_VECTOR_SEARCH_ENABLED") {
        cfg.vector_search.enabled = val == "true" || val == "1";
    }
    if let Ok(mode) = std::env::var("BURROW_INDEX_MODE") {
        cfg.vector_search.index_mode = mode;
    }

    // OpenRouter API key
    if let Ok(key) = std::env::var("BURROW_OPENROUTER_API_KEY") {
        cfg.openrouter.api_key = key;
    } else if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        cfg.openrouter.api_key = key;
    }
    cfg
}

pub fn init_config() -> &'static AppConfig {
    CONFIG.get_or_init(|| {
        let cfg = load_config();
        apply_env_overrides(cfg)
    })
}

pub fn get_config() -> &'static AppConfig {
    CONFIG
        .get()
        .expect("Config not initialized. Call init_config() first.")
}

/// Update a specific model in the config file
pub fn update_config_model(
    model_type: &str,
    provider: &str,
    model_name: &str,
) -> Result<(), String> {
    let path = config_path();

    // Read existing config or create default
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut cfg: AppConfig = toml::from_str(&content).unwrap_or_default();

    // Update the appropriate model
    match model_type {
        "embedding" => {
            cfg.models.embedding.name = model_name.into();
            cfg.models.embedding.provider = provider.into();
        }
        "chat" => {
            cfg.models.chat.name = model_name.into();
            cfg.models.chat.provider = provider.into();
        }
        "chat_large" => {
            cfg.models.chat_large.name = model_name.into();
            cfg.models.chat_large.provider = provider.into();
        }
        _ => return Err(format!("Unknown model type: {model_type}")),
    }

    // Write back to file
    let toml_str =
        toml::to_string_pretty(&cfg).map_err(|e| format!("Failed to serialize config: {e}"))?;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }

    std::fs::write(&path, toml_str).map_err(|e| format!("Failed to write config: {e}"))?;

    Ok(())
}

/// Reload config from file (useful after update_config_model)
pub fn reload_config() -> AppConfig {
    let cfg = load_config();
    apply_env_overrides(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sane_values() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.ollama.url, "http://localhost:11434");
        assert_eq!(cfg.ollama.timeout_secs, 30);
        assert_eq!(cfg.ollama.chat_timeout_secs, 120);
        assert!(cfg.vector_search.enabled);
        assert_eq!(cfg.vector_search.top_k, 10);
        assert_eq!(cfg.history.max_results, 6);
        assert_eq!(cfg.search.max_results, 10);
    }

    #[test]
    fn default_models_config() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.models.embedding.name, "qwen3-embedding:8b");
        assert_eq!(cfg.models.embedding.provider, "ollama");
        assert_eq!(cfg.models.chat.name, "gpt-oss:20b");
        assert_eq!(cfg.models.chat.provider, "ollama");
        assert_eq!(cfg.models.chat_large.name, "gpt-oss:120b");
        assert_eq!(cfg.models.chat_large.provider, "ollama");
    }

    #[test]
    fn default_chat_config() {
        let cfg = AppConfig::default();
        assert!(cfg.chat.rag_enabled);
        assert_eq!(cfg.chat.max_context_snippets, 5);
    }

    #[test]
    fn default_vector_search_has_index_mode() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.vector_search.index_mode, "all");
        assert!(!cfg.vector_search.exclude_patterns.is_empty());
        assert!(cfg
            .vector_search
            .exclude_patterns
            .contains(&".git".to_string()));
        assert!(cfg
            .vector_search
            .exclude_patterns
            .contains(&"node_modules".to_string()));
    }

    #[test]
    fn parse_empty_string_returns_defaults() {
        let cfg = parse_config("");
        assert_eq!(cfg.ollama.url, "http://localhost:11434");
        assert_eq!(cfg.models.embedding.name, "qwen3-embedding:8b");
    }

    #[test]
    fn parse_partial_config_fills_defaults() {
        let cfg = parse_config(
            r#"
[ollama]
url = "http://192.168.10.120:11434"
"#,
        );
        assert_eq!(cfg.ollama.url, "http://192.168.10.120:11434");
        assert_eq!(cfg.models.embedding.name, "qwen3-embedding:8b"); // default preserved
    }

    #[test]
    fn parse_full_config() {
        let cfg = parse_config(
            r#"
[models.embedding]
name = "nomic-embed-text"
provider = "ollama"

[models.chat]
name = "llama3:8b"
provider = "ollama"

[models.chat_large]
name = "anthropic/claude-sonnet-4"
provider = "openrouter"

[ollama]
url = "http://myserver:11434"
timeout_secs = 60
chat_timeout_secs = 180

[chat]
rag_enabled = false
max_context_snippets = 10

[vector_search]
enabled = false
top_k = 5
min_score = 0.5
max_file_size_bytes = 500000
index_mode = "custom"
index_dirs = ["~/Documents", "~/Code"]
exclude_patterns = [".git", "node_modules"]

[history]
max_results = 20

[search]
max_results = 15
debounce_ms = 100
"#,
        );
        assert_eq!(cfg.models.embedding.name, "nomic-embed-text");
        assert_eq!(cfg.models.chat.name, "llama3:8b");
        assert_eq!(cfg.models.chat_large.name, "anthropic/claude-sonnet-4");
        assert_eq!(cfg.models.chat_large.provider, "openrouter");
        assert_eq!(cfg.ollama.url, "http://myserver:11434");
        assert_eq!(cfg.ollama.timeout_secs, 60);
        assert_eq!(cfg.ollama.chat_timeout_secs, 180);
        assert!(!cfg.chat.rag_enabled);
        assert_eq!(cfg.chat.max_context_snippets, 10);
        assert!(!cfg.vector_search.enabled);
        assert_eq!(cfg.vector_search.top_k, 5);
        assert_eq!(cfg.vector_search.index_mode, "custom");
        assert_eq!(cfg.vector_search.index_dirs.len(), 2);
        assert_eq!(cfg.vector_search.exclude_patterns.len(), 2);
        assert_eq!(cfg.history.max_results, 20);
        assert_eq!(cfg.search.max_results, 15);
        assert_eq!(cfg.search.debounce_ms, 100);
    }

    #[test]
    fn parse_invalid_toml_returns_defaults() {
        let cfg = parse_config("this is not valid toml {{{}}}");
        assert_eq!(cfg.ollama.url, "http://localhost:11434");
    }

    #[test]
    fn config_dir_ends_with_burrow() {
        let dir = config_dir();
        assert!(dir.ends_with("burrow"));
    }

    #[test]
    fn config_path_ends_with_toml() {
        let path = config_path();
        assert_eq!(path.extension().unwrap(), "toml");
    }

    #[test]
    fn load_nonexistent_returns_defaults() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cfg = load_config_from_path(&tmp.path().join("burrow/config.toml"));
        assert_eq!(cfg.ollama.url, "http://localhost:11434");
    }

    #[test]
    fn env_override_ollama_url() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_OLLAMA_URL", "http://custom:11434");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.ollama.url, "http://custom:11434");
        std::env::remove_var("BURROW_OLLAMA_URL");
    }

    #[test]
    fn env_override_embedding_model_legacy() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_OLLAMA_EMBEDDING_MODEL", "custom-model");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.models.embedding.name, "custom-model");
        std::env::remove_var("BURROW_OLLAMA_EMBEDDING_MODEL");
    }

    #[test]
    fn env_override_model_embedding() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_MODEL_EMBEDDING", "nomic-embed-text");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.models.embedding.name, "nomic-embed-text");
        std::env::remove_var("BURROW_MODEL_EMBEDDING");
    }

    #[test]
    fn env_override_model_chat() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_MODEL_CHAT", "llama3:8b");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.models.chat.name, "llama3:8b");
        std::env::remove_var("BURROW_MODEL_CHAT");
    }

    #[test]
    fn env_override_model_chat_large() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_MODEL_CHAT_LARGE", "claude-opus");
        std::env::set_var("BURROW_MODEL_CHAT_LARGE_PROVIDER", "openrouter");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.models.chat_large.name, "claude-opus");
        assert_eq!(cfg.models.chat_large.provider, "openrouter");
        std::env::remove_var("BURROW_MODEL_CHAT_LARGE");
        std::env::remove_var("BURROW_MODEL_CHAT_LARGE_PROVIDER");
    }

    #[test]
    fn env_override_index_mode() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_INDEX_MODE", "custom");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.vector_search.index_mode, "custom");
        std::env::remove_var("BURROW_INDEX_MODE");
    }

    #[test]
    fn env_override_openrouter_api_key() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_OPENROUTER_API_KEY", "sk-burrow-test");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.openrouter.api_key, "sk-burrow-test");
        std::env::remove_var("BURROW_OPENROUTER_API_KEY");
    }

    #[test]
    fn env_override_openrouter_fallback() {
        let saved = std::env::var("OPENROUTER_API_KEY").ok();
        let mut cfg = AppConfig::default();
        std::env::remove_var("BURROW_OPENROUTER_API_KEY");
        std::env::set_var("OPENROUTER_API_KEY", "sk-fallback-test");
        cfg = apply_env_overrides(cfg);
        assert_eq!(cfg.openrouter.api_key, "sk-fallback-test");
        // Restore original value
        match saved {
            Some(v) => std::env::set_var("OPENROUTER_API_KEY", v),
            None => std::env::remove_var("OPENROUTER_API_KEY"),
        }
    }

    #[test]
    fn env_override_vector_enabled() {
        let mut cfg = AppConfig::default();
        std::env::set_var("BURROW_VECTOR_SEARCH_ENABLED", "false");
        cfg = apply_env_overrides(cfg);
        assert!(!cfg.vector_search.enabled);
        std::env::remove_var("BURROW_VECTOR_SEARCH_ENABLED");
    }

    #[test]
    fn serializes_to_valid_toml() {
        let cfg = AppConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        assert!(toml_str.contains("[ollama]"));
        assert!(toml_str.contains("url = "));
        assert!(toml_str.contains("[models.embedding]"));
        assert!(toml_str.contains("[models.chat]"));
        assert!(toml_str.contains("[models.chat_large]"));
        // Round-trip
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.ollama.url, cfg.ollama.url);
        assert_eq!(parsed.models.embedding.name, cfg.models.embedding.name);
    }

    #[test]
    fn update_config_model_creates_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let custom_dir = tmp.path().join("burrow");
        std::env::set_var("BURROW_CONFIG_DIR", &custom_dir);

        // Should create config file with updated model
        update_config_model("chat_large", "openrouter", "anthropic/claude-opus").unwrap();

        // Verify the file was created and contains the update
        let content = std::fs::read_to_string(custom_dir.join("config.toml")).unwrap();
        assert!(content.contains("anthropic/claude-opus"));
        assert!(content.contains("openrouter"));

        std::env::remove_var("BURROW_CONFIG_DIR");
    }

    #[test]
    fn update_config_model_invalid_type() {
        let result = update_config_model("invalid_type", "ollama", "model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown model type"));
    }

    #[test]
    fn min_score_is_reasonable() {
        let cfg = AppConfig::default();
        assert!(cfg.vector_search.min_score > 0.0);
        assert!(cfg.vector_search.min_score < 1.0);
    }

    #[test]
    fn max_file_size_is_1mb() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.vector_search.max_file_size_bytes, 1_000_000);
    }

    #[test]
    fn default_index_dirs_has_entries() {
        let cfg = AppConfig::default();
        assert!(!cfg.vector_search.index_dirs.is_empty());
    }

    #[test]
    fn default_indexer_config() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.indexer.interval_hours, 24);
        assert_eq!(cfg.indexer.max_content_chars, 4096);
        assert!(cfg.indexer.file_extensions.contains(&"rs".to_string()));
        assert!(cfg.indexer.file_extensions.contains(&"md".to_string()));
        assert!(cfg.indexer.file_extensions.len() >= 10);
    }

    #[test]
    fn parse_indexer_config() {
        let cfg = parse_config(
            r#"
[indexer]
interval_hours = 12
file_extensions = ["rs", "py"]
max_content_chars = 2048
"#,
        );
        assert_eq!(cfg.indexer.interval_hours, 12);
        assert_eq!(cfg.indexer.file_extensions, vec!["rs", "py"]);
        assert_eq!(cfg.indexer.max_content_chars, 2048);
    }

    #[test]
    fn default_onepass_config() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.onepass.idle_timeout_minutes, 10);
    }

    #[test]
    fn parse_onepass_config() {
        let cfg = parse_config(
            r#"
[onepass]
idle_timeout_minutes = 30
"#,
        );
        assert_eq!(cfg.onepass.idle_timeout_minutes, 30);
    }

    #[test]
    fn default_openrouter_config() {
        let cfg = AppConfig::default();
        assert!(cfg.openrouter.api_key.is_empty());
    }

    #[test]
    fn parse_openrouter_config() {
        let cfg = parse_config(
            r#"
[openrouter]
api_key = "sk-test-key"
"#,
        );
        assert_eq!(cfg.openrouter.api_key, "sk-test-key");
    }

    #[test]
    fn partial_indexer_config_fills_defaults() {
        let cfg = parse_config(
            r#"
[indexer]
interval_hours = 6
"#,
        );
        assert_eq!(cfg.indexer.interval_hours, 6);
        assert_eq!(cfg.indexer.max_content_chars, 4096); // default
        assert!(!cfg.indexer.file_extensions.is_empty()); // default
    }

    #[test]
    fn default_daemon_config() {
        let cfg = AppConfig::default();
        assert!(cfg.daemon.auto_start);
        assert_eq!(cfg.daemon.startup_timeout_secs, 5);
    }

    #[test]
    fn parse_daemon_config() {
        let cfg = parse_config(
            r#"
[daemon]
auto_start = false
startup_timeout_secs = 10
"#,
        );
        assert!(!cfg.daemon.auto_start);
        assert_eq!(cfg.daemon.startup_timeout_secs, 10);
    }

    #[test]
    fn env_override_config_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let custom_dir = tmp.path().join("custom-burrow");

        std::env::set_var("BURROW_CONFIG_DIR", &custom_dir);
        let dir = config_dir();
        assert_eq!(dir, custom_dir);
        std::env::remove_var("BURROW_CONFIG_DIR");
    }
}
