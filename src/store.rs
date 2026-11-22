//! SQLite storage (WAL). One row per session. Platform independent.

use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub start: i64,
    pub end: i64,
    pub process: String,
    /// Scene / project file open in the app, when the window title shows one.
    pub project: Option<String>,
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
                 path TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS app_custom(
                 key TEXT PRIMARY KEY,
                 name TEXT,
                 icon BLOB,
                 icon_type TEXT);
             CREATE TABLE IF NOT EXISTS meta(
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL);",
        )?;
        // v0.3 added the project column.
        let has_project: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'project'")?
            .exists([])?;
        if !has_project {
            conn.execute_batch("ALTER TABLE sessions ADD COLUMN project TEXT")?;
        }
        Ok(Store { conn })
    }

    /// Starts a session row; returns its id. `end` is updated later.
    pub fn insert(
        &self,
        start: i64,
        end: i64,
        process: &str,
        project: Option<&str>,
    ) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions(start, end, process, kind, project) VALUES (?1, ?2, ?3, 'active', ?4)",
            params![start, end, process, project],
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
            "SELECT start, end, process, project FROM sessions
             WHERE start < ?2 AND end > ?1 AND kind = 'active' AND end > start ORDER BY start",
        )?;
        let rows = st.query_map(params![from, to], |r| {
            Ok(Session {
                start: r.get(0)?,
                end: r.get(1)?,
                process: r.get(2)?,
                project: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Every process ever recorded.
    pub fn processes(&self) -> rusqlite::Result<Vec<String>> {
        let mut st = self.conn.prepare(
            "SELECT DISTINCT process FROM sessions WHERE kind = 'active' ORDER BY process",
        )?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    /// Renames made in the dashboard: original name -> new name.
    pub fn renames(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        if let Ok(mut st) = self
            .conn
            .prepare_cached("SELECT key, name FROM app_custom WHERE name IS NOT NULL")
        {
            if let Ok(rows) =
                st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// `None` restores the original name.
    pub fn set_rename(&self, key: &str, name: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO app_custom(key, name) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET name = excluded.name",
            params![key, name],
        )?;
        Ok(())
    }

    /// Custom logo of an app: (bytes, content type).
    pub fn custom_icon(&self, key: &str) -> Option<(Vec<u8>, String)> {
        self.conn
            .query_row(
                "SELECT icon, icon_type FROM app_custom WHERE key = ?1 AND icon IS NOT NULL",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// `None` restores the app's own icon.
    pub fn set_custom_icon(&self, key: &str, icon: Option<(&[u8], &str)>) -> rusqlite::Result<()> {
        let (data, kind) = icon.map_or((None, None), |(d, k)| (Some(d), Some(k)));
        self.conn.execute(
            "INSERT INTO app_custom(key, icon, icon_type) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET icon = excluded.icon, icon_type = excluded.icon_type",
            params![key, data, kind],
        )?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_database() {
        let path = std::env::temp_dir().join(format!("at-mig-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE sessions(id INTEGER PRIMARY KEY, start INTEGER NOT NULL, end INTEGER NOT NULL,
                 process TEXT NOT NULL, kind TEXT NOT NULL);
                 INSERT INTO sessions(start, end, process, kind) VALUES (10, 20, 'maya.exe', 'active');
                 INSERT INTO sessions(start, end, process, kind) VALUES (20, 30, 'prman.exe', 'render');",
            )
            .unwrap();
        }
        let s = Store::open(&path).unwrap();
        let rows = s.sessions_between(0, 100).unwrap();
        assert_eq!(
            rows,
            vec![Session {
                start: 10,
                end: 20,
                process: "maya.exe".into(),
                project: None
            }]
        );
        s.insert(30, 40, "maya.exe", Some("robot.mb")).unwrap();
        assert_eq!(
            s.sessions_between(0, 100).unwrap()[1].project.as_deref(),
            Some("robot.mb")
        );
        s.set_rename("Maya", Some("Maya 2025")).unwrap();
        s.set_custom_icon("Maya", Some((b"png", "image/png")))
            .unwrap();
        s.set_rename("Chrome", Some("Web")).unwrap();
        s.set_rename("Chrome", None).unwrap();
        assert_eq!(
            s.renames(),
            HashMap::from([("Maya".to_string(), "Maya 2025".to_string())])
        );
        assert_eq!(
            s.custom_icon("Maya"),
            Some((b"png".to_vec(), "image/png".to_string())),
            "rename kept the icon"
        );
        s.set_custom_icon("Maya", None).unwrap();
        assert_eq!(s.custom_icon("Maya"), None);
        assert_eq!(s.renames().len(), 1, "icon reset kept the name");
        assert_eq!(s.processes().unwrap(), vec!["maya.exe".to_string()]);
        s.set_meta("k", "1").unwrap();
        s.set_meta("k", "2").unwrap();
        assert_eq!(s.meta("k").as_deref(), Some("2"));
        drop(s);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
