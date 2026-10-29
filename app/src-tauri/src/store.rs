use crate::plan::Uninstall;
use crate::util::now;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

pub struct Store {
    conn: Connection,
}

#[derive(Serialize, Clone, Debug)]
pub struct App {
    pub id: i64,
    pub name: String,
    pub version: Option<String>,
    pub kind: String,
    pub source: Option<String>,
    pub sha256: Option<String>,
    pub installed_at: i64,
    pub status: String,
    pub uninstall: Uninstall,
    /// The record that replaced this one in an update (status `updated`); a rollback reinstalls this one.
    pub replaced_by: Option<i64>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Event {
    pub ts: i64,
    pub level: String,
    pub message: String,
}

const SCHEMA_V1: &str = "
CREATE TABLE apps (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  version TEXT,
  kind TEXT NOT NULL,
  source TEXT,
  sha256 TEXT,
  installed_at INTEGER NOT NULL,
  status TEXT NOT NULL,
  uninstall TEXT NOT NULL
);
CREATE TABLE events (
  id INTEGER PRIMARY KEY,
  app_id INTEGER REFERENCES apps(id) ON DELETE CASCADE,
  ts INTEGER NOT NULL,
  level TEXT NOT NULL,
  message TEXT NOT NULL
);
CREATE INDEX events_app ON events(app_id);
";

impl Store {
    pub fn open(path: &Path) -> Result<Store, String> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        Self::init(Connection::open(path).map_err(|e| e.to_string())?)
    }

    pub fn memory() -> Result<Store, String> {
        Self::init(Connection::open_in_memory().map_err(|e| e.to_string())?)
    }

    fn init(conn: Connection) -> Result<Store, String> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;").map_err(|e| e.to_string())?;
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(|e| e.to_string())?;
        if v < 1 {
            conn.execute_batch(SCHEMA_V1).map_err(|e| e.to_string())?;
            conn.execute_batch("PRAGMA user_version = 1").map_err(|e| e.to_string())?;
        }
        if v < 2 {
            conn.execute_batch("ALTER TABLE apps ADD COLUMN replaced_by INTEGER; PRAGMA user_version = 2;").map_err(|e| e.to_string())?;
        }
        Ok(Store { conn })
    }

    pub fn add_app(&self, name: &str, version: Option<&str>, kind: &str, source: &str, sha256: Option<&str>, u: &Uninstall) -> Result<i64, String> {
        let json = serde_json::to_string(u).map_err(|e| e.to_string())?;
        self.conn
            .execute(
                "INSERT INTO apps (name, version, kind, source, sha256, installed_at, status, uninstall) VALUES (?1,?2,?3,?4,?5,?6,'installed',?7)",
                params![name, version, kind, source, sha256, now(), json],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_status(&self, id: i64, status: &str) -> Result<(), String> {
        self.conn.execute("UPDATE apps SET status = ?1 WHERE id = ?2", params![status, id]).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// `old` was updated to `new`.
    pub fn set_replaced(&self, old: i64, new: i64) -> Result<(), String> {
        self.conn.execute("UPDATE apps SET status = 'updated', replaced_by = ?1 WHERE id = ?2", params![new, old]).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The version `id` replaced in an update, newest first, if Unpack made that update.
    pub fn predecessor(&self, id: i64) -> Result<Option<App>, String> {
        let sql = format!("SELECT {} FROM apps WHERE replaced_by = ?1 ORDER BY id DESC LIMIT 1", Self::COLS);
        match self.conn.query_row(&sql, [id], Self::row) {
            Ok(a) => Ok(Some(a)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn event(&self, app_id: Option<i64>, level: &str, message: &str) {
        let _ = self.conn.execute("INSERT INTO events (app_id, ts, level, message) VALUES (?1,?2,?3,?4)", params![app_id, now(), level, message]);
    }

    fn row(r: &rusqlite::Row) -> rusqlite::Result<App> {
        let json: String = r.get(8)?;
        Ok(App {
            id: r.get(0)?,
            name: r.get(1)?,
            version: r.get(2)?,
            kind: r.get(3)?,
            source: r.get(4)?,
            sha256: r.get(5)?,
            installed_at: r.get(6)?,
            status: r.get(7)?,
            uninstall: serde_json::from_str(&json).unwrap_or(Uninstall::Manual { note: "unreadable record".into() }),
            replaced_by: r.get(9)?,
        })
    }

    const COLS: &'static str = "id,name,version,kind,source,sha256,installed_at,status,uninstall,replaced_by";

    pub fn list(&self) -> Result<Vec<App>, String> {
        let sql = format!("SELECT {} FROM apps ORDER BY installed_at DESC, id DESC", Self::COLS);
        let mut st = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = st.query_map([], Self::row).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    pub fn get(&self, id: i64) -> Result<App, String> {
        let sql = format!("SELECT {} FROM apps WHERE id = ?1", Self::COLS);
        self.conn.query_row(&sql, [id], Self::row).map_err(|e| format!("No such app: {e}"))
    }

    pub fn events(&self, app_id: Option<i64>) -> Result<Vec<Event>, String> {
        let mut st = self
            .conn
            .prepare("SELECT ts, level, message FROM events WHERE (?1 IS NULL OR app_id = ?1) ORDER BY id DESC LIMIT 500")
            .map_err(|e| e.to_string())?;
        let rows = st.query_map([app_id], |r| Ok(Event { ts: r.get(0)?, level: r.get(1)?, message: r.get(2)? })).map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = Store::memory().unwrap();
        let u = Uninstall::Files { paths: vec!["/a".into()] };
        let id = s.add_app("vlc", Some("3"), "deb", "x.deb", None, &u).unwrap();
        s.event(Some(id), "info", "installed");
        let a = s.get(id).unwrap();
        assert_eq!(a.name, "vlc");
        assert_eq!(a.uninstall, u);
        s.set_status(id, "removed").unwrap();
        assert_eq!(s.list().unwrap()[0].status, "removed");
        assert_eq!(s.events(Some(id)).unwrap().len(), 1);
        assert_eq!(s.events(None).unwrap().len(), 1);
        let new = s.add_app("vlc", Some("4"), "deb", "y.deb", None, &u).unwrap();
        assert!(s.predecessor(new).unwrap().is_none());
        s.set_replaced(id, new).unwrap();
        let old = s.predecessor(new).unwrap().unwrap();
        assert_eq!((old.id, old.status.as_str(), old.replaced_by), (id, "updated", Some(new)));
    }

    #[test]
    fn migrates_a_version_1_database() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("old.db");
        let c = Connection::open(&p).unwrap();
        c.execute_batch(SCHEMA_V1).unwrap();
        c.execute_batch("PRAGMA user_version = 1; INSERT INTO apps (name, kind, installed_at, status, uninstall) VALUES ('a','x',0,'installed','{\"type\":\"traced\"}');").unwrap();
        drop(c);
        let s = Store::open(&p).unwrap();
        assert_eq!(s.list().unwrap()[0].replaced_by, None);
    }
}
