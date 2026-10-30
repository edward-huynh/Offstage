//! The local database: one SQLite file for everything the app stores, created on first
//! use inside the app's data folder. It is the same file in development and in release
//! builds, and it never leaves the machine.
//!
//! To store something new, add a migration to [`MIGRATIONS`] (never edit one that has
//! shipped) and write a small store type on top of [`Db`], the way
//! [`crate::scripts::ScriptStore`] does. Small loose values (settings, flags) can go in
//! the key-value table through [`Db::kv_get`] and [`Db::kv_set`] without any migration.

use std::{
    fs, io,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

/// Name of the database file inside the data folder.
pub const DB_FILE: &str = "offstage.db";

/// Schema changes, oldest first. The position in this list is the schema version, kept in
/// SQLite's `user_version`. A migration that has shipped must never change: add a new one.
const MIGRATIONS: &[&str] = &[
    // 1: scripts, knowledge documents and loose key-value settings.
    "CREATE TABLE scripts (
         id         TEXT PRIMARY KEY NOT NULL,
         title      TEXT NOT NULL,
         body       TEXT NOT NULL,
         updated_at INTEGER NOT NULL
     );
     CREATE INDEX scripts_updated_at ON scripts (updated_at);
     CREATE TABLE knowledge (
         id         TEXT PRIMARY KEY NOT NULL,
         name       TEXT NOT NULL,
         kind       TEXT NOT NULL,
         size_bytes INTEGER NOT NULL,
         added_at   INTEGER NOT NULL
     );
     CREATE INDEX knowledge_added_at ON knowledge (added_at);
     CREATE TABLE kv (
         key   TEXT PRIMARY KEY NOT NULL,
         value TEXT NOT NULL
     );",
];

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database folder error: {0}")]
    Io(#[from] io::Error),
    #[error("stored value is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "the data was written by a newer version of Offstage (schema {found}, this version \
         understands {supported})"
    )]
    TooNew { found: u32, supported: u32 },
    #[error("database is unavailable")]
    Poisoned,
}

type Result<T> = std::result::Result<T, DbError>;

/// A handle to the database. Cloning is cheap and every clone shares one connection,
/// so the stores built on it never interleave their statements.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens `<dir>/offstage.db`, creating the folder and the file on first use, and brings
    /// the schema up to date. Fails instead of touching a database written by a newer version.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        fs::create_dir_all(dir.as_ref())?;
        let conn = Connection::open(dir.as_ref().join(DB_FILE))?;
        // A second process (a dev build next to an installed one) waits instead of failing.
        conn.busy_timeout(Duration::from_secs(5))?;
        // Readers do not block the writer, and a crash cannot leave the file half-written.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::from_connection(conn)
    }

    /// A database that lives in memory only, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn, MIGRATIONS)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| DbError::Poisoned)
    }

    /// Reads a value stored with [`Db::kv_set`]. `None` when the key was never set.
    pub fn kv_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .lock()?
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        raw.map(|text| serde_json::from_str(&text).map_err(DbError::from))
            .transpose()
    }

    pub fn kv_set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let text = serde_json::to_string(value)?;
        self.lock()?.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            (key, text),
        )?;
        Ok(())
    }

    pub fn kv_delete(&self, key: &str) -> Result<()> {
        self.lock()?
            .execute("DELETE FROM kv WHERE key = ?1", [key])?;
        Ok(())
    }
}

/// Applies the migrations the database has not seen yet, each in its own transaction
/// together with the version bump, so a failure leaves the previous version intact.
fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<()> {
    let supported = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    let found: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found > supported {
        return Err(DbError::TooNew { found, supported });
    }
    for (version, sql) in (found..).zip(&migrations[found as usize..]) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(db: &Db) -> u32 {
        db.lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn open_creates_the_folder_and_the_file_and_reaches_the_latest_version() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("nested").join("data");
        let db = Db::open(&data).unwrap();
        assert!(data.join(DB_FILE).exists());
        assert_eq!(version(&db), u32::try_from(MIGRATIONS.len()).unwrap());
    }

    #[test]
    fn reopening_keeps_the_data_and_does_not_migrate_again() {
        let dir = tempfile::tempdir().unwrap();
        Db::open(dir.path())
            .unwrap()
            .kv_set("k", &"v".to_owned())
            .unwrap();
        let db = Db::open(dir.path()).unwrap();
        assert_eq!(db.kv_get::<String>("k").unwrap().as_deref(), Some("v"));
    }

    #[test]
    fn a_database_from_a_newer_version_is_refused_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.lock()
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        drop(db);
        assert!(matches!(
            Db::open(dir.path()),
            Err(DbError::TooNew {
                found: 99,
                supported: 1
            })
        ));
    }

    #[test]
    fn migrations_run_in_order_and_only_the_missing_ones() {
        let mut conn = Connection::open_in_memory().unwrap();
        let first = ["CREATE TABLE a (x INTEGER);"];
        migrate(&mut conn, &first).unwrap();
        let both = ["CREATE TABLE a (x INTEGER);", "CREATE TABLE b (y INTEGER);"];
        // The first one would fail if it ran again: the table already exists.
        migrate(&mut conn, &both).unwrap();
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        conn.execute("INSERT INTO b (y) VALUES (1)", []).unwrap();
    }

    #[test]
    fn a_failing_migration_leaves_the_previous_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        let broken = [
            "CREATE TABLE a (x INTEGER);",
            "CREATE TABLE b (y INTEGER); NOT SQL;",
        ];
        assert!(migrate(&mut conn, &broken).is_err());
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
        // The half-applied second migration was rolled back.
        assert!(conn.execute("INSERT INTO b (y) VALUES (1)", []).is_err());
    }

    #[test]
    fn kv_round_trips_overwrites_and_deletes() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.kv_get::<Vec<u32>>("nums").unwrap(), None);
        db.kv_set("nums", &vec![1, 2]).unwrap();
        assert_eq!(db.kv_get::<Vec<u32>>("nums").unwrap(), Some(vec![1, 2]));
        db.kv_set("nums", &vec![3]).unwrap();
        assert_eq!(db.kv_get::<Vec<u32>>("nums").unwrap(), Some(vec![3]));
        db.kv_delete("nums").unwrap();
        assert_eq!(db.kv_get::<Vec<u32>>("nums").unwrap(), None);
    }

    #[test]
    fn kv_reports_a_value_of_the_wrong_shape() {
        let db = Db::open_in_memory().unwrap();
        db.kv_set("k", &"text").unwrap();
        assert!(matches!(db.kv_get::<u32>("k"), Err(DbError::Json(_))));
    }

    #[test]
    fn clones_share_one_connection() {
        let db = Db::open_in_memory().unwrap();
        let other = db.clone();
        db.kv_set("k", &1u32).unwrap();
        assert_eq!(other.kv_get::<u32>("k").unwrap(), Some(1));
    }
}
