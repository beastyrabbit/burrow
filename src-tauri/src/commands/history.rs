use crate::context::AppContext;
use crate::router::{Category, SearchResult};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::AppHandle;

/// Thread-safe wrapper for the history database connection.
/// Inner field is private to enforce access through the `lock()` method.
pub struct DbState(Mutex<Connection>);

impl DbState {
    /// Create a new DbState wrapping a database connection.
    pub fn new(conn: Connection) -> Self {
        Self(Mutex::new(conn))
    }

    /// Acquire a lock on the database connection.
    pub fn lock(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.0
            .lock()
            .map_err(|e| format!("history DB lock failed: {e}"))
    }
}

/// Get the history database path
pub fn db_path() -> PathBuf {
    let dir = super::data_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(path = %dir.display(), error = %e, "failed to create history data dir");
    }
    dir.join("history.db")
}

/// Open a standalone history database connection (for CLI use, no Tauri state)
pub fn open_history_db() -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(db_path())?;
    create_table(&conn)?;
    Ok(conn)
}

fn create_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS launches (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            exec TEXT NOT NULL,
            icon TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            count INTEGER NOT NULL DEFAULT 0,
            last_used REAL NOT NULL DEFAULT 0
        )",
    )
}

pub fn init_db(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    let conn = Connection::open(db_path())?;
    create_table(&conn)?;
    app.manage(DbState::new(conn));
    Ok(())
}

/// Query the most frequent/recent (frecent) entries from the database.
///
/// Returns up to 6 entries, ordered by frecency score (count weighted by recency).
/// Public for CLI use.
pub fn query_frecent(conn: &Connection) -> Result<Vec<SearchResult>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, exec, icon, description FROM launches
         ORDER BY count * (1.0 / (1.0 + (julianday('now') - last_used))) DESC
         LIMIT 6",
    )?;

    let results = stmt
        .query_map([], |row| {
            Ok(SearchResult {
                id: row.get(0)?,
                name: row.get(1)?,
                exec: row.get(2)?,
                icon: row.get(3)?,
                description: row.get(4)?,
                category: Category::History,
                input_spec: None,
            })
        })?
        .filter_map(|r| match r {
            Ok(val) => Some(val),
            Err(e) => {
                tracing::warn!(error = %e, "skipping corrupted history row");
                None
            }
        })
        .collect();

    Ok(results)
}

/// Get frecent results using AppContext (Tauri-free).
pub fn get_frecent(ctx: &AppContext) -> Result<Vec<SearchResult>, String> {
    let conn = ctx.db.lock()?;
    query_frecent(&conn).map_err(|e| e.to_string())
}

/// Returns a map of app id → frecency score for all entries in the history DB.
pub fn get_frecency_scores(
    ctx: &AppContext,
) -> Result<std::collections::HashMap<String, f64>, String> {
    let conn = ctx.db.lock()?;
    query_frecency_scores(&conn).map_err(|e| e.to_string())
}

fn query_frecency_scores(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, f64>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, count * (1.0 / (1.0 + (julianday('now') - last_used))) AS score
         FROM launches",
    )?;
    let map = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?
        .filter_map(|r| match r {
            Ok(val) => Some(val),
            Err(e) => {
                tracing::warn!(error = %e, "skipping corrupted history row");
                None
            }
        })
        .collect();
    Ok(map)
}

fn insert_launch(
    conn: &Connection,
    id: &str,
    name: &str,
    exec: &str,
    icon: &str,
    description: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO launches (id, name, exec, icon, description, count, last_used)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, julianday('now'))
         ON CONFLICT(id) DO UPDATE SET
           count = count + 1,
           last_used = julianday('now'),
           name = ?2, exec = ?3, icon = ?4, description = ?5",
        rusqlite::params![id, name, exec, icon, description],
    )?;
    Ok(())
}

/// Record a launch using AppContext (Tauri-free).
pub fn record_launch(
    id: &str,
    name: &str,
    exec: &str,
    icon: &str,
    description: &str,
    ctx: &AppContext,
) -> Result<(), String> {
    let conn = ctx.db.lock()?;
    insert_launch(&conn, id, name, exec, icon, description).map_err(|e| e.to_string())
}

/// Tauri command wrapper for record_launch.
#[tauri::command]
pub fn record_launch_cmd(
    id: String,
    name: String,
    exec: String,
    icon: String,
    description: String,
    app: AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    let ctx = app.state::<AppContext>();
    record_launch(&id, &name, &exec, &icon, &description, &ctx)
}

/// Clear all entries from the history database.
/// Returns the number of entries deleted.
pub fn clear_all_history(conn: &Connection) -> Result<usize, rusqlite::Error> {
    conn.execute("DELETE FROM launches", [])
}

/// Remove a specific entry from history by its ID.
/// Returns true if an entry was removed, false if not found.
pub fn remove_from_history(conn: &Connection, id: &str) -> Result<bool, rusqlite::Error> {
    let rows_affected = conn.execute("DELETE FROM launches WHERE id = ?1", [id])?;
    Ok(rows_affected > 0)
}

/// Get the total count of entries in the history database.
pub fn get_launch_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT COUNT(*) FROM launches", [], |row| row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn).unwrap();
        conn
    }

    #[test]
    fn create_table_succeeds() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(create_table(&conn).is_ok());
    }

    #[test]
    fn create_table_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn).unwrap();
        assert!(create_table(&conn).is_ok());
    }

    #[test]
    fn insert_and_query() {
        let conn = test_db();
        insert_launch(
            &conn,
            "firefox",
            "Firefox",
            "firefox",
            "firefox-icon",
            "Web Browser",
        )
        .unwrap();
        let results = query_frecent(&conn).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "firefox");
        assert_eq!(results[0].name, "Firefox");
    }

    #[test]
    fn insert_increments_count() {
        let conn = test_db();
        insert_launch(&conn, "ff", "Firefox", "firefox", "", "").unwrap();
        insert_launch(&conn, "ff", "Firefox", "firefox", "", "").unwrap();
        insert_launch(&conn, "ff", "Firefox", "firefox", "", "").unwrap();

        let count: i64 = conn
            .query_row("SELECT count FROM launches WHERE id = 'ff'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn upsert_updates_metadata() {
        let conn = test_db();
        insert_launch(
            &conn, "app1", "Old Name", "old-exec", "old-icon", "old desc",
        )
        .unwrap();
        insert_launch(
            &conn, "app1", "New Name", "new-exec", "new-icon", "new desc",
        )
        .unwrap();

        let results = query_frecent(&conn).unwrap();
        assert_eq!(results[0].name, "New Name");
        assert_eq!(results[0].exec, "new-exec");
    }

    #[test]
    fn empty_db_returns_empty() {
        let conn = test_db();
        let results = query_frecent(&conn).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn results_have_history_category() {
        let conn = test_db();
        insert_launch(&conn, "a", "App", "app", "", "").unwrap();
        let results = query_frecent(&conn).unwrap();
        assert_eq!(results[0].category, Category::History);
    }

    #[test]
    fn frecency_ordering() {
        let conn = test_db();
        // Launch "rare" once
        insert_launch(&conn, "rare", "Rare App", "rare", "", "").unwrap();
        // Launch "frequent" many times
        for _ in 0..10 {
            insert_launch(&conn, "freq", "Frequent App", "freq", "", "").unwrap();
        }

        let results = query_frecent(&conn).unwrap();
        assert_eq!(results[0].id, "freq");
        assert_eq!(results[1].id, "rare");
    }

    #[test]
    fn limit_to_6_results() {
        let conn = test_db();
        for i in 0..20 {
            insert_launch(
                &conn,
                &format!("app{i}"),
                &format!("App {i}"),
                "exec",
                "",
                "",
            )
            .unwrap();
        }
        let results = query_frecent(&conn).unwrap();
        assert_eq!(results.len(), 6);
    }

    #[test]
    fn frecency_scores_returns_all_entries() {
        let conn = test_db();
        insert_launch(&conn, "a", "App A", "a", "", "").unwrap();
        insert_launch(&conn, "b", "App B", "b", "", "").unwrap();
        let scores = query_frecency_scores(&conn).unwrap();
        assert_eq!(scores.len(), 2);
        assert!(scores.contains_key("a"));
        assert!(scores.contains_key("b"));
        assert!(*scores.get("a").unwrap() > 0.0);
    }

    #[test]
    fn frecency_scores_empty_db() {
        let conn = test_db();
        let scores = query_frecency_scores(&conn).unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn stores_all_fields() {
        let conn = test_db();
        insert_launch(&conn, "id1", "Name1", "exec1", "icon1", "desc1").unwrap();
        let results = query_frecent(&conn).unwrap();
        assert_eq!(results[0].id, "id1");
        assert_eq!(results[0].name, "Name1");
        assert_eq!(results[0].exec, "exec1");
        assert_eq!(results[0].icon, "icon1");
        assert_eq!(results[0].description, "desc1");
    }

    #[test]
    fn clear_all_history_removes_all_entries() {
        let conn = test_db();
        insert_launch(&conn, "a", "A", "a", "", "").unwrap();
        insert_launch(&conn, "b", "B", "b", "", "").unwrap();
        let count = clear_all_history(&conn).unwrap();
        assert_eq!(count, 2);
        assert!(query_frecent(&conn).unwrap().is_empty());
    }

    #[test]
    fn clear_all_history_empty_db() {
        let conn = test_db();
        let count = clear_all_history(&conn).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn remove_from_history_removes_single_entry() {
        let conn = test_db();
        insert_launch(&conn, "keep", "Keep", "k", "", "").unwrap();
        insert_launch(&conn, "remove", "Remove", "r", "", "").unwrap();
        let removed = remove_from_history(&conn, "remove").unwrap();
        assert!(removed);
        let results = query_frecent(&conn).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "keep");
    }

    #[test]
    fn remove_from_history_returns_false_for_missing() {
        let conn = test_db();
        let removed = remove_from_history(&conn, "nonexistent").unwrap();
        assert!(!removed);
    }

    #[test]
    fn get_launch_count_returns_total() {
        let conn = test_db();
        insert_launch(&conn, "a", "A", "a", "", "").unwrap();
        insert_launch(&conn, "b", "B", "b", "", "").unwrap();
        assert_eq!(get_launch_count(&conn).unwrap(), 2);
    }

    #[test]
    fn get_launch_count_empty_db() {
        let conn = test_db();
        assert_eq!(get_launch_count(&conn).unwrap(), 0);
    }
}
