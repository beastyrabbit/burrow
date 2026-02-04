use super::handlers::{
    ChatRequest, ChatResponse, DaemonStatus, IndexerStartRequest, IndexerStartResponse,
    ModelsListResponse, StatsResponse,
};
use super::socket::socket_path;
use crate::commands::health::HealthStatus;
use crate::config;
use crate::indexer::IndexerProgress;
use hyper_util::rt::TokioIo;
use std::time::Duration;
use tokio::net::UnixStream;

/// Client for communicating with the daemon over Unix socket.
pub struct DaemonClient {
    timeout: Duration,
}

impl Default for DaemonClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Default timeout for most requests (5 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default chat timeout in seconds (matches config default).
const DEFAULT_CHAT_TIMEOUT_SECS: u64 = 120;

/// Extra buffer time for client-side timeout beyond server-side chat timeout.
const CHAT_TIMEOUT_BUFFER_SECS: u64 = 10;

impl DaemonClient {
    pub fn new() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Create a client with longer timeout for chat operations.
    /// Reads from config with a buffer to ensure client doesn't timeout before server.
    /// Falls back to default timeout if config not initialized.
    pub fn with_chat_timeout() -> Self {
        let timeout_secs = config::try_get_config()
            .map(|cfg| cfg.ollama.chat_timeout_secs)
            .unwrap_or(DEFAULT_CHAT_TIMEOUT_SECS);
        Self {
            timeout: Duration::from_secs(timeout_secs + CHAT_TIMEOUT_BUFFER_SECS),
        }
    }

    /// Check if the daemon socket exists.
    pub fn socket_exists() -> bool {
        socket_path().exists()
    }

    /// Try to connect to the daemon.
    pub async fn connect() -> Result<(), String> {
        let path = socket_path();
        UnixStream::connect(&path)
            .await
            .map(|_| ())
            .map_err(|e| format!("failed to connect to daemon at {}: {e}", path.display()))
    }

    /// Send an HTTP request to the daemon and parse the JSON response.
    async fn request<T, B>(
        &self,
        method: hyper::Method,
        path: &str,
        body: B,
        content_type: Option<&str>,
    ) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
        B: hyper::body::Body + Send + 'static,
        B::Data: Send,
        B::Error: std::error::Error + Send + Sync,
    {
        let sock_path = socket_path();
        let stream = tokio::time::timeout(self.timeout, UnixStream::connect(&sock_path))
            .await
            .map_err(|_| "connection timeout".to_string())?
            .map_err(|e| format!("failed to connect: {e}"))?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| format!("handshake failed: {e}"))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::debug!(error = %e, "daemon client connection closed");
            }
        });

        let mut builder = hyper::Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, "localhost");

        if let Some(ct) = content_type {
            builder = builder.header(hyper::header::CONTENT_TYPE, ct);
        }

        let req = builder
            .body(body)
            .map_err(|e| format!("failed to build request: {e}"))?;

        let resp = tokio::time::timeout(self.timeout, sender.send_request(req))
            .await
            .map_err(|_| "request timeout".to_string())?
            .map_err(|e| format!("request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("daemon returned status {}", resp.status()));
        }

        let body = http_body_util::BodyExt::collect(resp.into_body())
            .await
            .map_err(|e| format!("failed to read body: {e}"))?
            .to_bytes();

        serde_json::from_slice(&body).map_err(|e| format!("failed to parse response: {e}"))
    }

    /// Send a GET request to the daemon.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.request(
            hyper::Method::GET,
            path,
            http_body_util::Empty::<hyper::body::Bytes>::new(),
            None,
        )
        .await
    }

    /// Send a POST request to the daemon with a JSON body.
    async fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let body_bytes =
            serde_json::to_vec(body).map_err(|e| format!("failed to serialize body: {e}"))?;
        self.request(
            hyper::Method::POST,
            path,
            http_body_util::Full::new(hyper::body::Bytes::from(body_bytes)),
            Some("application/json"),
        )
        .await
    }

    /// Get daemon status.
    pub async fn status(&self) -> Result<DaemonStatus, String> {
        self.get("/daemon/status").await
    }

    /// Request daemon shutdown.
    pub async fn shutdown(&self) -> Result<(), String> {
        self.post::<(), _>("/daemon/shutdown", &()).await
    }

    /// Get indexer progress.
    pub async fn progress(&self) -> Result<IndexerProgress, String> {
        self.get("/indexer/progress").await
    }

    /// Start indexer (full or incremental).
    pub async fn start_indexer(&self, full: bool) -> Result<IndexerStartResponse, String> {
        self.post("/indexer/start", &IndexerStartRequest { full })
            .await
    }

    /// Get health status.
    pub async fn health(&self) -> Result<HealthStatus, String> {
        self.get("/health").await
    }

    /// Get stats.
    pub async fn stats(&self) -> Result<StatsResponse, String> {
        self.get("/stats").await
    }

    /// Chat without document context.
    pub async fn chat(&self, query: &str, small: bool) -> Result<ChatResponse, String> {
        self.post(
            "/chat",
            &ChatRequest {
                query: query.to_string(),
                small,
            },
        )
        .await
    }

    /// Chat with document context (RAG).
    pub async fn chat_docs(&self, query: &str, small: bool) -> Result<ChatResponse, String> {
        self.post(
            "/chat/docs",
            &ChatRequest {
                query: query.to_string(),
                small,
            },
        )
        .await
    }

    /// Get model configuration.
    pub async fn models(&self) -> Result<ModelsListResponse, String> {
        self.get("/models").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_default_timeout() {
        let client = DaemonClient::new();
        assert_eq!(client.timeout, DEFAULT_TIMEOUT);
    }

    #[test]
    fn client_custom_timeout() {
        let client = DaemonClient::new().with_timeout(Duration::from_secs(10));
        assert_eq!(client.timeout, Duration::from_secs(10));
    }

    #[test]
    fn client_chat_timeout_reads_config() {
        // Initialize config for test
        config::init_config();
        let client = DaemonClient::with_chat_timeout();
        let cfg = config::get_config();
        let expected = Duration::from_secs(cfg.ollama.chat_timeout_secs + CHAT_TIMEOUT_BUFFER_SECS);
        assert_eq!(client.timeout, expected);
    }

    #[test]
    fn client_chat_timeout_fallback_without_config() {
        // with_chat_timeout should not panic even if config is not initialized
        // It falls back to DEFAULT_CHAT_TIMEOUT_SECS
        let client = DaemonClient::with_chat_timeout();
        // Should have some reasonable timeout (either from config or default)
        assert!(client.timeout.as_secs() >= DEFAULT_CHAT_TIMEOUT_SECS);
    }

    #[test]
    fn socket_exists_returns_false_when_no_socket() {
        // Assuming the daemon is not running in tests
        // This test just verifies the function doesn't panic
        let _ = DaemonClient::socket_exists();
    }
}
