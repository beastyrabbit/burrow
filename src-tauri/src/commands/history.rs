use crate::router::SearchResult;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct DbState(pub Mutex<Connection>);

fn db_path() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("burrow");
    std::fs::create_dir_all(&dir).ok();
    dir.join("history.db")
}

pub fn init_db(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::open(db_path())?;
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
    )?;
    app.manage(DbState(Mutex::new(conn)));
    Ok(())
}

pub fn get_frecent(app: &AppHandle) -> Result<Vec<SearchResult>, String> {
    let state = app.state::<DbState>();
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    // Frecency: score = count * decay_factor where decay_factor favors recent usage
    let mut stmt = conn
        .prepare(
            "SELECT id, name, exec, icon, description FROM launches
             ORDER BY count * (1.0 / (1.0 + (julianday('now') - last_used))) DESC
             LIMIT 10",
        )
        .map_err(|e| e.to_string())?;

    let results = stmt
        .query_map([], |row| {
            Ok(SearchResult {
                id: row.get(0)?,
                name: row.get(1)?,
                exec: row.get(2)?,
                icon: row.get(3)?,
                description: row.get(4)?,
                category: "history".into(),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

#[tauri::command]
pub fn record_launch(
    id: String,
    name: String,
    exec: String,
    icon: String,
    description: String,
    app: AppHandle,
) -> Result<(), String> {
    let state = app.state::<DbState>();
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO launches (id, name, exec, icon, description, count, last_used)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, julianday('now'))
         ON CONFLICT(id) DO UPDATE SET
           count = count + 1,
           last_used = julianday('now'),
           name = ?2, exec = ?3, icon = ?4, description = ?5",
        rusqlite::params![id, name, exec, icon, description],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
