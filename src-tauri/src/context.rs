use std::sync::Arc;

use crate::commands::history::DbState;
use crate::commands::vectors::VectorDbState;
use crate::indexer::IndexerState;

/// Application context that decouples backend logic from `tauri::AppHandle`.
/// Used by the test-server binary (no Tauri runtime) and by Tauri commands via thin wrappers.
pub struct AppContext {
    pub db: Arc<DbState>,
    pub vector_db: Arc<VectorDbState>,
    pub indexer: Arc<IndexerState>,
    /// Optional Tauri app handle for window operations (hide, emit events).
    /// `None` in test-server / CLI mode — window ops become no-ops.
    pub app_handle: Option<tauri::AppHandle>,
}

impl AppContext {
    pub fn new(db: DbState, vector_db: VectorDbState, indexer: IndexerState) -> Self {
        Self {
            db: Arc::new(db),
            vector_db: Arc::new(vector_db),
            indexer: Arc::new(indexer),
            app_handle: None,
        }
    }

    pub fn with_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.app_handle = Some(handle);
        self
    }

    /// Create from standard DB paths (test-server, CLI). Respects `BURROW_DATA_DIR`.
    pub fn from_disk() -> Result<Self, Box<dyn std::error::Error>> {
        use crate::commands::{history, vectors};
        Ok(Self::new(
            DbState::new(history::open_history_db()?),
            VectorDbState::new(vectors::open_vector_db()?),
            IndexerState::new(),
        ))
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
