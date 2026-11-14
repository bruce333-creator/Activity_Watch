//! SQLite storage (WAL). One row per session. Platform independent.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub start: i64,
    pub end: i64,
    pub process: String,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Store> {
        Store::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Store> {
        // journal_mode returns a row, so it is queried rather than executed.
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        // `kind` is kept for databases written by v0.1 (which also recorded 'render' rows).
        conn.execute_batch(
            "PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=2000;
             CREATE TABLE IF NOT EXISTS sessions(
                 id INTEGER PRIMARY KEY,
                 start INTEGER NOT NULL,
                 end INTEGER NOT NULL,
                 process TEXT NOT NULL,
                 kind TEXT NOT NULL DEFAULT 'active');
             CREATE INDEX IF NOT EXISTS sessions_start ON sessions(start);
             CREATE TABLE IF NOT EXISTS apps(
                 process TEXT PRIMARY KEY,
                 path TEXT NOT NULL);",
        )?;
        Ok(Store { conn })
    }

    /// Starts a session row; returns its id. `end` is updated later.
    pub fn insert(&self, start: i64, end: i64, process: &str) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions(start, end, process, kind) VALUES (?1, ?2, ?3, 'active')",
            params![start, end, process],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_end(&self, id: i64, end: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE sessions SET end = ?1 WHERE id = ?2",
            params![end, id],
        )?;
        Ok(())
    }

    /// Remembers where an app's exe lives (for its icon).
    pub fn set_app_path(&self, process: &str, path: &str) -> rusqlite::Result<()> {
        if !path.is_empty() {
            self.conn.execute(
                "INSERT INTO apps(process, path) VALUES (?1, ?2)
                 ON CONFLICT(process) DO UPDATE SET path = excluded.path",
                params![process, path],
            )?;
        }
        Ok(())
    }

    pub fn app_path(&self, process: &str) -> Option<String> {
        self.conn
            .query_row("SELECT path FROM apps WHERE process = ?1", [process], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    /// Active sessions overlapping [from, to).
    pub fn sessions_between(&self, from: i64, to: i64) -> rusqlite::Result<Vec<Session>> {
        let mut st = self.conn.prepare_cached(
            "SELECT start, end, process FROM sessions
             WHERE start < ?2 AND end > ?1 AND kind = 'active' ORDER BY start",
        )?;
        let rows = st.query_map(params![from, to], |r| {
            Ok(Session {
                start: r.get(0)?,
                end: r.get(1)?,
                process: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Current local UTC offset in seconds, as SQLite sees it (uses the OS time zone).
    pub fn local_offset(&self) -> i64 {
        self.conn
            .query_row(
                "SELECT CAST(strftime('%s','now','localtime') AS INTEGER) - CAST(strftime('%s','now') AS INTEGER)",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }
}
