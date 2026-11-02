//! Workspaces: a set of documents with a note for the model, and the choices that go with one
//! situation (an interview, a sales call): keep it off the network, or use another provider.
//!
//! There is always at least one. `default` exists from the start and takes the documents that
//! were added before workspaces did. Which one is in use is remembered in the key-value table.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Db, DbError};

/// Id of the workspace every database starts with.
pub const DEFAULT_WORKSPACE: &str = "default";

const ACTIVE_KEY: &str = "active_workspace";
const MAX_NAME_CHARS: usize = 80;
const MAX_NOTE_CHARS: usize = 2_000;
const MAX_MODEL_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    /// What the user wants the model to know about this situation, written in their own words.
    pub note: String,
    /// Nothing said in a call using this workspace may leave the machine, whatever the app-wide
    /// choice is.
    pub local_only: bool,
    /// Answer with this provider instead of the app-wide one, or `None` for the app-wide one.
    pub provider_id: Option<String>,
    /// The model of that provider, or `None` for the app-wide choice.
    pub model: Option<String>,
    /// Unix time in milliseconds.
    pub created_at: u64,
}

/// What can be changed about a workspace. Every field replaces the stored one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceUpdate {
    pub name: String,
    pub note: String,
    pub local_only: bool,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

/// The messages that the user can read are written in Vietnamese.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("workspace storage error: {0}")]
    Db(#[from] DbError),
    #[error("workspace not found: {0}")]
    NotFound(String),
    #[error("Tên workspace không được để trống")]
    EmptyName,
    #[error("Phải còn ít nhất một workspace")]
    LastOne,
}

impl From<rusqlite::Error> for WorkspaceError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.into())
    }
}

type Result<T> = std::result::Result<T, WorkspaceError>;

#[derive(Clone)]
pub struct WorkspaceStore {
    db: Db,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        note: row.get(2)?,
        local_only: row.get(3)?,
        provider_id: row.get(4)?,
        model: row.get(5)?,
        created_at: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
    })
}

const COLUMNS: &str = "id, name, note, local_only, provider_id, model, created_at";

fn clean_name(name: &str) -> Result<String> {
    let name: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    let name = name.trim_end().to_owned();
    if name.is_empty() {
        return Err(WorkspaceError::EmptyName);
    }
    Ok(name)
}

/// Blank is no choice.
fn clean_optional(value: Option<String>, max_chars: usize) -> Option<String> {
    value
        .map(|v| v.trim().chars().take(max_chars).collect::<String>())
        .filter(|v| !v.is_empty())
}

impl WorkspaceStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Every workspace, oldest first, so the list does not reorder as they are edited.
    pub fn list(&self) -> Result<Vec<Workspace>> {
        let conn = self.db.lock()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM workspaces ORDER BY created_at, rowid"
        ))?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get(&self, id: &str) -> Result<Workspace> {
        self.db
            .lock()?
            .query_row(
                &format!("SELECT {COLUMNS} FROM workspaces WHERE id = ?1"),
                [id],
                from_row,
            )
            .optional()?
            .ok_or_else(|| WorkspaceError::NotFound(id.to_owned()))
    }

    /// A new, empty workspace.
    pub fn create(&self, name: &str) -> Result<Workspace> {
        let name = clean_name(name)?;
        let conn = self.db.lock()?;
        let now = now_millis();
        let mut stamp = now;
        let id = loop {
            let candidate = format!("w{stamp:x}");
            let taken: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM workspaces WHERE id = ?1)",
                [&candidate],
                |row| row.get(0),
            )?;
            if !taken {
                break candidate;
            }
            stamp += 1;
        };
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, i64::try_from(now).unwrap_or(i64::MAX)],
        )?;
        drop(conn);
        self.get(&id)
    }

    /// Replaces the name, note and choices of a workspace.
    pub fn update(&self, id: &str, update: WorkspaceUpdate) -> Result<Workspace> {
        let name = clean_name(&update.name)?;
        let note: String = update.note.trim().chars().take(MAX_NOTE_CHARS).collect();
        let provider_id = clean_optional(update.provider_id, MAX_NAME_CHARS);
        // A model only means something with the provider it belongs to.
        let model = provider_id
            .as_ref()
            .and_then(|_| clean_optional(update.model, MAX_MODEL_CHARS));
        let changed = self.db.lock()?.execute(
            "UPDATE workspaces
             SET name = ?2, note = ?3, local_only = ?4, provider_id = ?5, model = ?6
             WHERE id = ?1",
            params![id, name, note, update.local_only, provider_id, model],
        )?;
        if changed == 0 {
            return Err(WorkspaceError::NotFound(id.to_owned()));
        }
        self.get(id)
    }

    /// Removes the workspace row. The documents of the workspace are not touched: the caller
    /// deletes them first (they have files to remove). The last workspace cannot be deleted. When
    /// the one in use goes, the oldest remaining one takes its place.
    pub fn delete(&self, id: &str) -> Result<()> {
        let was_active = self.active()?.id == id;
        let mut conn = self.db.lock()?;
        let tx = conn.transaction()?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM workspaces", [], |row| row.get(0))?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM workspaces WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(WorkspaceError::NotFound(id.to_owned()));
        }
        if count <= 1 {
            return Err(WorkspaceError::LastOne);
        }
        tx.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
        tx.commit()?;
        drop(conn);
        if was_active {
            self.db.kv_delete(ACTIVE_KEY)?;
        }
        Ok(())
    }

    /// The workspace in use. Falls back to the oldest one when none was chosen or the chosen one
    /// is gone.
    pub fn active(&self) -> Result<Workspace> {
        if let Some(id) = self.db.kv_get::<String>(ACTIVE_KEY)? {
            if let Ok(workspace) = self.get(&id) {
                return Ok(workspace);
            }
        }
        self.list()?
            .into_iter()
            .next()
            .ok_or_else(|| WorkspaceError::NotFound(DEFAULT_WORKSPACE.to_owned()))
    }

    /// Makes `id` the workspace in use.
    pub fn set_active(&self, id: &str) -> Result<Workspace> {
        let workspace = self.get(id)?;
        self.db.kv_set(ACTIVE_KEY, &workspace.id)?;
        Ok(workspace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> WorkspaceStore {
        WorkspaceStore::new(Db::open_in_memory().unwrap())
    }

    fn update(name: &str) -> WorkspaceUpdate {
        WorkspaceUpdate {
            name: name.into(),
            note: String::new(),
            local_only: false,
            provider_id: None,
            model: None,
        }
    }

    #[test]
    fn a_new_database_has_the_default_workspace_and_it_is_in_use() {
        let s = store();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, DEFAULT_WORKSPACE);
        assert_eq!(s.active().unwrap().id, DEFAULT_WORKSPACE);
        assert!(!all[0].local_only && all[0].provider_id.is_none() && all[0].note.is_empty());
    }

    #[test]
    fn creating_adds_an_empty_workspace_after_the_others() {
        let s = store();
        let a = s.create("  Phỏng vấn   Rust ").unwrap();
        let b = s.create("Cuộc gọi bán hàng").unwrap();
        assert_eq!(a.name, "Phỏng vấn Rust");
        assert_ne!(a.id, b.id);
        let names: Vec<_> = s.list().unwrap().into_iter().map(|w| w.name).collect();
        assert_eq!(names, ["Mặc định", "Phỏng vấn Rust", "Cuộc gọi bán hàng"]);
    }

    #[test]
    fn a_blank_name_is_refused_and_a_long_one_is_cut() {
        let s = store();
        assert!(matches!(s.create("   "), Err(WorkspaceError::EmptyName)));
        assert!(matches!(
            s.update(DEFAULT_WORKSPACE, update(" ")),
            Err(WorkspaceError::EmptyName)
        ));
        assert_eq!(
            s.create(&"a".repeat(200)).unwrap().name.chars().count(),
            MAX_NAME_CHARS
        );
    }

    #[test]
    fn updating_replaces_the_note_and_the_choices() {
        let s = store();
        let w = s.create("Phỏng vấn").unwrap();
        let saved = s
            .update(
                &w.id,
                WorkspaceUpdate {
                    name: "Phỏng vấn Rust".into(),
                    note: "  Vị trí: backend, công ty ABC.  ".into(),
                    local_only: true,
                    provider_id: Some("opencode-go".into()),
                    model: Some(" kimi-k2.6 ".into()),
                },
            )
            .unwrap();
        assert_eq!(saved.name, "Phỏng vấn Rust");
        assert_eq!(saved.note, "Vị trí: backend, công ty ABC.");
        assert!(saved.local_only);
        assert_eq!(saved.provider_id.as_deref(), Some("opencode-go"));
        assert_eq!(saved.model.as_deref(), Some("kimi-k2.6"));
        assert_eq!(s.get(&w.id).unwrap(), saved);
    }

    #[test]
    fn a_model_without_a_provider_is_dropped_and_blanks_mean_no_choice() {
        let s = store();
        let w = s.create("x").unwrap();
        let saved = s
            .update(
                &w.id,
                WorkspaceUpdate {
                    provider_id: Some("  ".into()),
                    model: Some("gpt-4o-mini".into()),
                    ..update("x")
                },
            )
            .unwrap();
        assert_eq!((saved.provider_id, saved.model), (None, None));
    }

    #[test]
    fn a_note_is_cut_to_its_limit() {
        let s = store();
        let saved = s
            .update(
                DEFAULT_WORKSPACE,
                WorkspaceUpdate {
                    note: "ạ".repeat(MAX_NOTE_CHARS + 50),
                    ..update("Mặc định")
                },
            )
            .unwrap();
        assert_eq!(saved.note.chars().count(), MAX_NOTE_CHARS);
    }

    #[test]
    fn an_unknown_workspace_is_not_found() {
        let s = store();
        assert!(matches!(s.get("nope"), Err(WorkspaceError::NotFound(_))));
        assert!(matches!(
            s.update("nope", update("x")),
            Err(WorkspaceError::NotFound(_))
        ));
        assert!(matches!(
            s.set_active("nope"),
            Err(WorkspaceError::NotFound(_))
        ));
        assert!(matches!(s.delete("nope"), Err(WorkspaceError::NotFound(_))));
    }

    #[test]
    fn the_workspace_in_use_is_remembered_and_changes_on_request() {
        let s = store();
        let w = s.create("Phỏng vấn").unwrap();
        assert_eq!(s.set_active(&w.id).unwrap().id, w.id);
        assert_eq!(s.active().unwrap().id, w.id);
        s.set_active(DEFAULT_WORKSPACE).unwrap();
        assert_eq!(s.active().unwrap().id, DEFAULT_WORKSPACE);
    }

    #[test]
    fn deleting_the_one_in_use_hands_over_to_the_oldest_and_the_last_cannot_go() {
        let s = store();
        let w = s.create("Phỏng vấn").unwrap();
        s.set_active(&w.id).unwrap();
        s.delete(&w.id).unwrap();
        assert_eq!(s.active().unwrap().id, DEFAULT_WORKSPACE);
        assert!(matches!(s.get(&w.id), Err(WorkspaceError::NotFound(_))));
        assert!(matches!(
            s.delete(DEFAULT_WORKSPACE),
            Err(WorkspaceError::LastOne)
        ));
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn deleting_one_that_is_not_in_use_keeps_the_choice() {
        let s = store();
        let a = s.create("A").unwrap();
        let b = s.create("B").unwrap();
        s.set_active(&a.id).unwrap();
        s.delete(&b.id).unwrap();
        assert_eq!(s.active().unwrap().id, a.id);
    }

    #[test]
    fn a_choice_pointing_at_a_vanished_workspace_falls_back() {
        let db = Db::open_in_memory().unwrap();
        db.kv_set(ACTIVE_KEY, &"ghost".to_owned()).unwrap();
        assert_eq!(
            WorkspaceStore::new(db).active().unwrap().id,
            DEFAULT_WORKSPACE
        );
    }
}
