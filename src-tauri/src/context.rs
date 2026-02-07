use std::sync::Arc;

use crate::commands::history::DbState;
use crate::commands::vectors::VectorDbState;
use crate::indexer::IndexerState;
use crate::output_buffers::OutputBufferState;

/// Application context that decouples backend logic from `tauri::AppHandle`.
/// Used by the test-server binary (no Tauri runtime) and by Tauri commands via thin wrappers.
pub struct AppContext {
    pub(crate) db: Arc<DbState>,
    pub(crate) vector_db: Arc<VectorDbState>,
    pub(crate) indexer: Arc<IndexerState>,
    pub(crate) output_buffers: Arc<OutputBufferState>,
    /// Optional Tauri app handle for window operations (hide, emit events).
    /// `None` in test-server / CLI mode — window ops become no-ops.
    app_handle: Option<tauri::AppHandle>,
}

impl AppContext {
    /// Create a new AppContext, wrapping each value in `Arc`.
    /// Use `from_arcs` when you already have `Arc` references (e.g., sharing with Tauri state).
    pub fn new(db: DbState, vector_db: VectorDbState, indexer: IndexerState) -> Self {
        Self {
            db: Arc::new(db),
            vector_db: Arc::new(vector_db),
            indexer: Arc::new(indexer),
            output_buffers: Arc::new(OutputBufferState::new()),
            app_handle: None,
        }
    }

    /// Create from pre-existing `Arc` references. Used in Tauri setup to share
    /// state between AppContext and Tauri's managed state / background indexer.
    pub fn from_arcs(
        db: Arc<DbState>,
        vector_db: Arc<VectorDbState>,
        indexer: Arc<IndexerState>,
        output_buffers: Arc<OutputBufferState>,
    ) -> Self {
        Self {
            db,
            vector_db,
            indexer,
            output_buffers,
            app_handle: None,
        }
    }

    pub fn with_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.app_handle = Some(handle);
        self
    }

    /// Clone the optional app handle (for moving into spawned threads).
    pub fn clone_app_handle(&self) -> Option<tauri::AppHandle> {
        self.app_handle.clone()
    }

    /// Create from standard DB paths (test-server, CLI). Respects `BURROW_DATA_DIR`.
    pub fn from_disk() -> Result<Self, Box<dyn std::error::Error>> {
        use crate::commands::{history, vectors};
        let db = DbState::new(
            history::open_history_db().map_err(|e| format!("failed to open history DB: {e}"))?,
        );
        let vector_db = VectorDbState::new(
            vectors::open_vector_db().map_err(|e| format!("failed to open vector DB: {e}"))?,
        );
        Ok(Self::new(db, vector_db, IndexerState::new()))
    }

    /// Hide the main window if a Tauri AppHandle is available.
    pub fn hide_window(&self) {
        if let Some(ref app) = self.app_handle {
            crate::actions::utils::hide_window(app);
        } else {
            tracing::debug!("[no-window] hide_window skipped (no AppHandle)");
        }
    }

    /// Emit an event if a Tauri AppHandle is available.
    pub fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        if let Some(ref app) = self.app_handle {
            use tauri::Emitter;
            app.emit(event, payload).map_err(|e| e.to_string())
        } else {
            tracing::debug!(event, "[no-window] emit skipped (no AppHandle)");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_ctx() -> AppContext {
        let db = DbState::new(Connection::open_in_memory().unwrap());
        let vector_db = VectorDbState::new(Connection::open_in_memory().unwrap());
        AppContext::new(db, vector_db, IndexerState::new())
    }

    #[test]
    fn new_creates_context_without_app_handle() {
        let ctx = in_memory_ctx();
        assert!(ctx.clone_app_handle().is_none());
    }

    #[test]
    fn from_arcs_shares_references() {
        let db = Arc::new(DbState::new(Connection::open_in_memory().unwrap()));
        let vector_db = Arc::new(VectorDbState::new(Connection::open_in_memory().unwrap()));
        let indexer = Arc::new(IndexerState::new());
        let output_buffers = Arc::new(OutputBufferState::new());

        let ctx = AppContext::from_arcs(
            db.clone(),
            vector_db.clone(),
            indexer.clone(),
            output_buffers.clone(),
        );

        // Arc::ptr_eq proves they share the same allocation
        assert!(Arc::ptr_eq(&ctx.db, &db));
        assert!(Arc::ptr_eq(&ctx.vector_db, &vector_db));
        assert!(Arc::ptr_eq(&ctx.indexer, &indexer));
        assert!(Arc::ptr_eq(&ctx.output_buffers, &output_buffers));
    }

    #[test]
    fn hide_window_noop_without_app_handle() {
        let ctx = in_memory_ctx();
        // Should not panic
        ctx.hide_window();
    }

    #[test]
    fn emit_noop_without_app_handle() {
        let ctx = in_memory_ctx();
        let result = ctx.emit("test-event", "payload");
        assert!(result.is_ok());
    }

    #[test]
    fn db_lock_works() {
        let ctx = in_memory_ctx();
        let conn = ctx.db.lock().expect("should lock DB");
        conn.execute_batch("SELECT 1")
            .expect("should execute query");
    }

    #[test]
    fn vector_db_lock_works() {
        let ctx = in_memory_ctx();
        let conn = ctx.vector_db.lock().expect("should lock vector DB");
        conn.execute_batch("SELECT 1")
            .expect("should execute query");
    }

    #[test]
    fn indexer_default_not_running() {
        let ctx = in_memory_ctx();
        let progress = ctx.indexer.get();
        assert!(!progress.running);
        assert_eq!(progress.phase, "idle");
    }
}
