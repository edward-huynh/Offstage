//! Knowledge documents (JD, CV, rubrics...): the user's own files. The database lists them
//! and the files themselves are copied into a folder next to it, since a 20 MB document
//! does not belong in a table.
//!
//! This only stores and lists files. Extracting text and indexing them is Phase 4 (RAG),
//! which will read the stored files through [`KnowledgeStore::path_of`].
//!
//! Stored files are named after the id the store generates, never after the user's file
//! name, and an id is only honoured when the database knows it, so a caller-supplied id
//! can never point outside the storage folder.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Db, DbError};

/// Largest file accepted, in bytes.
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

const DEFAULT_NAME: &str = "Tài liệu";
const MAX_NAME_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Pdf,
    Docx,
    Markdown,
    Text,
}

impl KnowledgeKind {
    /// Extensions accepted on import, lowercase and without the dot.
    pub const EXTENSIONS: [&'static str; 5] = ["pdf", "docx", "md", "markdown", "txt"];

    /// Kind for a file name, judged by its extension and ignoring case.
    /// Any folders in the name are ignored.
    pub fn from_file_name(file_name: &str) -> Option<Self> {
        let (_, ext) = base_name(file_name).rsplit_once('.')?;
        match ext.to_ascii_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            "md" | "markdown" => Some(Self::Markdown),
            "txt" => Some(Self::Text),
            _ => None,
        }
    }

    /// Name used in the database. Matches the serde name, so it is also what the UI sees.
    fn db_name(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Markdown => "markdown",
            Self::Text => "text",
        }
    }

    fn from_db_name(name: &str) -> Option<Self> {
        [Self::Pdf, Self::Docx, Self::Markdown, Self::Text]
            .into_iter()
            .find(|kind| kind.db_name() == name)
    }

    /// Extension of the stored copy. `.markdown` files are stored as `.md`.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Markdown => "md",
            Self::Text => "txt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeDoc {
    pub id: String,
    /// File name as shown to the user, without any folders.
    pub name: String,
    pub kind: KnowledgeKind,
    pub size_bytes: u64,
    /// Unix time in milliseconds.
    pub added_at: u64,
}

/// The messages of the three import rejections are shown to the user as they are,
/// so they are written in Vietnamese.
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeError {
    #[error("knowledge storage error: {0}")]
    Db(#[from] DbError),
    #[error("knowledge file error: {0}")]
    Io(#[from] io::Error),
    #[error("knowledge document not found: {0}")]
    NotFound(String),
    #[error("Chỉ nhận .pdf, .docx, .md, .txt")]
    UnsupportedType,
    #[error("Tệp quá lớn (tối đa 20 MB)")]
    TooLarge,
    #[error("Tệp rỗng")]
    Empty,
}

impl From<rusqlite::Error> for KnowledgeError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.into())
    }
}

type Result<T> = std::result::Result<T, KnowledgeError>;

fn doc_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDoc> {
    let kind: String = row.get(2)?;
    let kind = KnowledgeKind::from_db_name(&kind).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            format!("unknown document kind {kind:?}").into(),
        )
    })?;
    Ok(KnowledgeDoc {
        id: row.get(0)?,
        name: row.get(1)?,
        kind,
        size_bytes: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
        added_at: u64::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
    })
}

pub struct KnowledgeStore {
    db: Db,
    /// Where the files themselves are kept.
    dir: PathBuf,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl KnowledgeStore {
    /// Does not touch the disk: `dir` is created on the first import.
    pub fn new(db: Db, dir: impl Into<PathBuf>) -> Self {
        Self::with_clock(db, dir, system_millis)
    }

    /// Like [`KnowledgeStore::new`] with a custom time source, for tests.
    pub fn with_clock(
        db: Db,
        dir: impl Into<PathBuf>,
        clock: impl Fn() -> u64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            db,
            dir: dir.into(),
            clock: Box::new(clock),
        }
    }

    fn file_path(&self, doc: &KnowledgeDoc) -> PathBuf {
        self.dir
            .join(format!("{}.{}", doc.id, doc.kind.extension()))
    }

    fn get(&self, id: &str) -> Result<KnowledgeDoc> {
        self.db
            .lock()?
            .query_row(
                "SELECT id, name, kind, size_bytes, added_at FROM knowledge WHERE id = ?1",
                [id],
                doc_from_row,
            )
            .optional()?
            .ok_or_else(|| KnowledgeError::NotFound(id.to_owned()))
    }

    /// All documents, most recently added first.
    pub fn list(&self) -> Result<Vec<KnowledgeDoc>> {
        let conn = self.db.lock()?;
        // `rowid` breaks ties so the newest of two documents added in the same millisecond is first.
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, size_bytes, added_at FROM knowledge
             ORDER BY added_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([], doc_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Copies a file into the store. `file_name` may carry folders (it is whatever the
    /// caller got from the system), only its last part is used.
    pub fn import(&self, file_name: &str, bytes: &[u8]) -> Result<KnowledgeDoc> {
        let kind =
            KnowledgeKind::from_file_name(file_name).ok_or(KnowledgeError::UnsupportedType)?;
        let size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size_bytes > MAX_FILE_BYTES {
            return Err(KnowledgeError::TooLarge);
        }
        if bytes.is_empty() {
            return Err(KnowledgeError::Empty);
        }

        let conn = self.db.lock()?;
        let now = (self.clock)();
        let mut stamp = now;
        let id = loop {
            let candidate = format!("k{stamp:x}");
            let taken = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM knowledge WHERE id = ?1)",
                [&candidate],
                |row| row.get::<_, bool>(0),
            )?;
            if !taken {
                break candidate;
            }
            stamp += 1;
        };
        let doc = KnowledgeDoc {
            id,
            name: clean_name(file_name),
            kind,
            size_bytes,
            added_at: now,
        };

        fs::create_dir_all(&self.dir)?;
        let path = self.file_path(&doc);
        write_atomic(&path, bytes)?;
        let inserted = conn.execute(
            "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                doc.id,
                doc.name,
                doc.kind.db_name(),
                i64::try_from(doc.size_bytes).unwrap_or(i64::MAX),
                i64::try_from(doc.added_at).unwrap_or(i64::MAX),
            ],
        );
        if let Err(e) = inserted {
            // Without its row the copy would be unreachable.
            let _ = fs::remove_file(&path);
            return Err(KnowledgeError::from(e));
        }
        Ok(doc)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let doc = self.get(id)?;
        // Row first: if removing the file fails, the document is gone from the list
        // and the leftover file is harmless.
        self.db
            .lock()?
            .execute("DELETE FROM knowledge WHERE id = ?1", [id])?;
        match fs::remove_file(self.file_path(&doc)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// Where the stored copy of a document lives, for the code that will read it later.
    pub fn path_of(&self, id: &str) -> Result<PathBuf> {
        Ok(self.file_path(&self.get(id)?))
    }
}

/// The part of a path after the last `/` or `\`, whichever system it came from.
fn base_name(file_name: &str) -> &str {
    file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .trim()
}

/// Display name: no folders, whitespace collapsed, capped in length, and a default when
/// nothing is left.
fn clean_name(file_name: &str) -> String {
    let collapsed = base_name(file_name)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let capped: String = collapsed.chars().take(MAX_NAME_CHARS).collect();
    let capped = capped.trim_end().to_owned();
    if capped.is_empty() {
        DEFAULT_NAME.to_owned()
    } else {
        capped
    }
}

fn system_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Writes to a sibling temp file and renames it, so a crash never leaves a half-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// A clock that moves forward by 10 ms on every call.
    fn counter() -> impl Fn() -> u64 + Send + Sync {
        let next = AtomicU64::new(1000);
        move || next.fetch_add(10, Ordering::SeqCst)
    }

    fn store(dir: &tempfile::TempDir) -> KnowledgeStore {
        KnowledgeStore::with_clock(
            Db::open_in_memory().unwrap(),
            dir.path().join("knowledge"),
            counter(),
        )
    }

    fn stored(dir: &tempfile::TempDir, file: &str) -> PathBuf {
        dir.path().join("knowledge").join(file)
    }

    #[test]
    fn empty_store_lists_nothing_and_creates_no_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(store(&dir).list().unwrap().is_empty());
        assert!(!dir.path().join("knowledge").exists());
    }

    #[test]
    fn import_then_list_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let doc = s.import("CV Nguyễn Văn A.pdf", b"%PDF-1.7 data").unwrap();
        assert_eq!(doc.name, "CV Nguyễn Văn A.pdf");
        assert_eq!(doc.kind, KnowledgeKind::Pdf);
        assert_eq!(doc.size_bytes, 13);
        assert_eq!(doc.added_at, 1000);
        assert_eq!(s.list().unwrap(), vec![doc.clone()]);

        let path = s.path_of(&doc.id).unwrap();
        assert_eq!(path, stored(&dir, &format!("{}.pdf", doc.id)));
        assert_eq!(fs::read(path).unwrap(), b"%PDF-1.7 data");
    }

    #[test]
    fn every_accepted_extension_maps_to_a_kind_and_a_stored_extension() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let cases = [
            ("a.pdf", KnowledgeKind::Pdf, "pdf"),
            ("a.docx", KnowledgeKind::Docx, "docx"),
            ("a.md", KnowledgeKind::Markdown, "md"),
            ("a.markdown", KnowledgeKind::Markdown, "md"),
            ("a.txt", KnowledgeKind::Text, "txt"),
        ];
        for (name, kind, ext) in cases {
            let doc = s.import(name, b"x").unwrap();
            assert_eq!(doc.kind, kind, "{name}");
            assert_eq!(
                s.path_of(&doc.id).unwrap(),
                stored(&dir, &format!("{}.{ext}", doc.id))
            );
        }
        for ext in KnowledgeKind::EXTENSIONS {
            assert!(KnowledgeKind::from_file_name(&format!("f.{ext}")).is_some());
        }
    }

    #[test]
    fn extensions_are_case_insensitive_and_stored_in_lowercase() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let doc = s.import("JD.PDF", b"x").unwrap();
        assert_eq!(doc.kind, KnowledgeKind::Pdf);
        assert_eq!(doc.name, "JD.PDF");
        assert!(s
            .path_of(&doc.id)
            .unwrap()
            .to_string_lossy()
            .ends_with(".pdf"));
        assert_eq!(
            KnowledgeKind::from_file_name("Notes.MarkDown"),
            Some(KnowledgeKind::Markdown)
        );
    }

    #[test]
    fn unsupported_extensions_are_rejected_and_write_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        for name in ["virus.exe", "noext", "archive.pdf.zip", "trailing.", ""] {
            assert!(
                matches!(s.import(name, b"x"), Err(KnowledgeError::UnsupportedType)),
                "{name:?}"
            );
        }
        assert!(!dir.path().join("knowledge").exists());
    }

    #[test]
    fn files_over_the_limit_are_rejected_but_the_limit_itself_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let limit = usize::try_from(MAX_FILE_BYTES).unwrap();
        assert!(matches!(
            s.import("big.txt", &vec![b'a'; limit + 1]),
            Err(KnowledgeError::TooLarge)
        ));
        assert!(s.list().unwrap().is_empty());

        let doc = s.import("edge.txt", &vec![b'a'; limit]).unwrap();
        assert_eq!(doc.size_bytes, MAX_FILE_BYTES);
    }

    #[test]
    fn empty_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        assert!(matches!(
            s.import("empty.md", b""),
            Err(KnowledgeError::Empty)
        ));
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn rejection_messages_are_shown_to_the_user_in_vietnamese() {
        assert_eq!(
            KnowledgeError::UnsupportedType.to_string(),
            "Chỉ nhận .pdf, .docx, .md, .txt"
        );
        assert_eq!(
            KnowledgeError::TooLarge.to_string(),
            "Tệp quá lớn (tối đa 20 MB)"
        );
    }

    #[test]
    fn hostile_file_names_lose_their_folders_and_never_decide_where_the_file_goes() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);

        let up = s.import("../../x.txt", b"x").unwrap();
        assert_eq!(up.name, "x.txt");
        let win = s.import("C:\\a\\b.pdf", b"x").unwrap();
        assert_eq!(win.name, "b.pdf");
        assert_eq!(win.kind, KnowledgeKind::Pdf);
        let mixed = s.import("/etc/..\\..\\passwd.md", b"x").unwrap();
        assert_eq!(mixed.name, "passwd.md");

        // Everything landed inside the storage directory, named after the ids.
        let root = dir.path().join("knowledge");
        for doc in [&up, &win, &mixed] {
            assert_eq!(s.path_of(&doc.id).unwrap().parent().unwrap(), root);
        }
        assert!(!dir.path().join("x.txt").exists());
    }

    #[test]
    fn a_name_that_is_only_a_folder_is_not_a_document() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        assert!(matches!(
            s.import("docs.txt/", b"x"),
            Err(KnowledgeError::UnsupportedType)
        ));
        assert!(matches!(
            s.import("docs\\", b"x"),
            Err(KnowledgeError::UnsupportedType)
        ));
    }

    #[test]
    fn display_names_are_cleaned() {
        assert_eq!(clean_name("a   b\t\nc.txt"), "a b c.txt");
        assert_eq!(clean_name("   "), DEFAULT_NAME);
        assert_eq!(clean_name("dir/"), DEFAULT_NAME);
        let long = format!("{}.pdf", "x".repeat(300));
        assert_eq!(clean_name(&long).chars().count(), MAX_NAME_CHARS);
        // Multi-byte characters are counted, not bytes.
        let vi = "ệ".repeat(300);
        assert_eq!(clean_name(&vi).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn a_very_long_name_is_capped_but_the_kind_is_still_known() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let doc = s
            .import(&format!("{}.docx", "x".repeat(300)), b"x")
            .unwrap();
        assert_eq!(doc.name.chars().count(), MAX_NAME_CHARS);
        assert_eq!(doc.kind, KnowledgeKind::Docx);
    }

    #[test]
    fn list_puts_the_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let a = s.import("a.txt", b"a").unwrap();
        let b = s.import("b.txt", b"b").unwrap();
        let c = s.import("c.txt", b"c").unwrap();
        let ids: Vec<_> = s.list().unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![c.id, b.id, a.id]);
    }

    #[test]
    fn ids_stay_unique_and_the_newest_stays_first_when_the_clock_does_not_advance() {
        let dir = tempfile::tempdir().unwrap();
        let s = KnowledgeStore::with_clock(
            Db::open_in_memory().unwrap(),
            dir.path().join("knowledge"),
            || 5000,
        );
        let a = s.import("a.txt", b"a").unwrap();
        let b = s.import("b.txt", b"b").unwrap();
        assert_ne!(a.id, b.id);
        let listed = s.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, b.id);
    }

    #[test]
    fn delete_removes_the_entry_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let keep = s.import("keep.txt", b"k").unwrap();
        let doc = s.import("drop.pdf", b"d").unwrap();
        let path = s.path_of(&doc.id).unwrap();
        assert!(path.exists());

        s.delete(&doc.id).unwrap();
        assert!(!path.exists());
        assert_eq!(s.list().unwrap(), vec![keep]);
        assert!(matches!(
            s.path_of(&doc.id),
            Err(KnowledgeError::NotFound(_))
        ));
    }

    #[test]
    fn delete_tolerates_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let doc = s.import("gone.md", b"x").unwrap();
        fs::remove_file(s.path_of(&doc.id).unwrap()).unwrap();
        s.delete(&doc.id).unwrap();
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn unknown_ids_are_rejected_on_delete_and_path_of() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.import("real.txt", b"x").unwrap();
        assert!(matches!(s.delete("nope"), Err(KnowledgeError::NotFound(_))));
        assert!(matches!(
            s.path_of("nope"),
            Err(KnowledgeError::NotFound(_))
        ));
    }

    #[test]
    fn ids_that_try_to_leave_the_directory_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.import("real.txt", b"x").unwrap();
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, "keep me").unwrap();

        for id in ["../victim", "../victim.txt", "..", "/etc/passwd", "index"] {
            assert!(
                matches!(s.path_of(id), Err(KnowledgeError::NotFound(_))),
                "{id}"
            );
            assert!(
                matches!(s.delete(id), Err(KnowledgeError::NotFound(_))),
                "{id}"
            );
        }
        assert_eq!(fs::read_to_string(victim).unwrap(), "keep me");
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn documents_survive_reopening_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("files");
        let doc = KnowledgeStore::new(Db::open(dir.path()).unwrap(), &files)
            .import("CV.docx", b"data")
            .unwrap();
        let again = KnowledgeStore::new(Db::open(dir.path()).unwrap(), &files);
        assert_eq!(again.list().unwrap(), vec![doc.clone()]);
        assert_eq!(fs::read(again.path_of(&doc.id).unwrap()).unwrap(), b"data");
    }

    #[test]
    fn a_failed_database_write_leaves_no_orphan_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let s = KnowledgeStore::with_clock(db.clone(), dir.path().join("knowledge"), || 7);
        // The id lookup works but every insert is refused, as if the disk were full.
        db.lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER refuse BEFORE INSERT ON knowledge
                 BEGIN SELECT RAISE(ABORT, 'refused'); END;",
            )
            .unwrap();
        assert!(matches!(
            s.import("a.txt", b"x"),
            Err(KnowledgeError::Db(_))
        ));
        let leftovers = fs::read_dir(dir.path().join("knowledge")).unwrap().count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn kinds_have_distinct_database_names_that_read_back() {
        for kind in [
            KnowledgeKind::Pdf,
            KnowledgeKind::Docx,
            KnowledgeKind::Markdown,
            KnowledgeKind::Text,
        ] {
            assert_eq!(KnowledgeKind::from_db_name(kind.db_name()), Some(kind));
            // The database name is also the serde name.
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.db_name())
            );
        }
        assert_eq!(KnowledgeKind::from_db_name("exe"), None);
    }

    #[test]
    fn kinds_serialize_as_snake_case() {
        let json = serde_json::to_string(&KnowledgeKind::Markdown).unwrap();
        assert_eq!(json, r#""markdown""#);
        let back: KnowledgeKind = serde_json::from_str(r#""docx""#).unwrap();
        assert_eq!(back, KnowledgeKind::Docx);
    }
}
