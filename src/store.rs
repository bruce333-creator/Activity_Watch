//! SQLite storage (WAL). One row per session. Platform independent.

use rusqlite::{Connection, params};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Active,
    Render,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Active => "active",
            Kind::Render => "render",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub start: i64,
    pub end: i64,
    pub process: String,
    pub kind: Kind,
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
        conn.execute_batch(
            "PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=2000;
             CREATE TABLE IF NOT EXISTS sessions(
                 id INTEGER PRIMARY KEY,
                 start INTEGER NOT NULL,
                 end INTEGER NOT NULL,
                 process TEXT NOT NULL,
                 kind TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS sessions_start ON sessions(start);",
        )?;
        Ok(Store { conn })
    }

    /// Starts a session row; returns its id. `end` is updated later.
    pub fn insert(&self, start: i64, end: i64, process: &str, kind: Kind) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions(start, end, process, kind) VALUES (?1, ?2, ?3, ?4)",
            params![start, end, process, kind.as_str()],
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

    /// Sessions overlapping [from, to).
    pub fn sessions_between(&self, from: i64, to: i64) -> rusqlite::Result<Vec<Session>> {
        let mut st = self.conn.prepare_cached(
            "SELECT start, end, process, kind FROM sessions
             WHERE start < ?2 AND end > ?1 ORDER BY start",
        )?;
        let rows = st.query_map(params![from, to], |r| {
            let kind: String = r.get(3)?;
            Ok(Session {
                start: r.get(0)?,
                end: r.get(1)?,
                process: r.get(2)?,
                kind: if kind == "render" {
                    Kind::Render
                } else {
                    Kind::Active
                },
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
