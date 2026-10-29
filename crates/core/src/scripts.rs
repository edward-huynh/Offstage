//! Teleprompter scripts: one Markdown file per script plus a small JSON index.
//!
//! Ids are only ever taken from the index, so a caller-supplied id can never point
//! outside the storage directory.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const SAMPLE_TITLE: &str = "Kịch bản mẫu";
pub const SAMPLE_BODY: &str = "Chào mọi người, cảm ơn đã dành thời gian hôm nay.

Đây là kịch bản mẫu. Nhấn E để sửa nội dung, Space để chạy hoặc dừng.

Mũi tên lên và xuống chỉnh tốc độ. Phím + và − chỉnh cỡ chữ.

Đặt cửa sổ này sát webcam để ánh mắt nhìn tự nhiên khi đọc.

Cửa sổ được loại khỏi luồng chia sẻ màn hình, nên người xem chỉ thấy bạn.";

const INDEX_FILE: &str = "index.json";
const DEFAULT_TITLE: &str = "Kịch bản mới";
const MAX_TITLE_CHARS: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScriptMeta {
    pub id: String,
    pub title: String,
    /// Unix time in milliseconds.
    pub updated_at: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script storage error: {0}")]
    Io(#[from] io::Error),
    #[error("script index is unreadable: {0}")]
    Index(#[from] serde_json::Error),
    #[error("script not found: {0}")]
    NotFound(String),
}

type Result<T> = std::result::Result<T, ScriptError>;

pub struct ScriptStore {
    dir: PathBuf,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl ScriptStore {
    /// Does not touch the disk: the directory is created on the first write.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self::with_clock(dir, system_millis)
    }

    /// Like [`ScriptStore::new`] with a custom time source, for tests.
    pub fn with_clock(
        dir: impl Into<PathBuf>,
        clock: impl Fn() -> u64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            dir: dir.into(),
            clock: Box::new(clock),
        }
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join(INDEX_FILE)
    }

    fn body_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.md"))
    }

    fn load_index(&self) -> Result<Vec<ScriptMeta>> {
        match fs::read_to_string(self.index_path()) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn save_index(&self, index: &[ScriptMeta]) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(index)?;
        write_atomic(&self.index_path(), json.as_bytes())?;
        Ok(())
    }

    fn position(index: &[ScriptMeta], id: &str) -> Result<usize> {
        index
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| ScriptError::NotFound(id.to_owned()))
    }

    /// All scripts, most recently saved first.
    pub fn list(&self) -> Result<Vec<ScriptMeta>> {
        let mut index = self.load_index()?;
        index.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(index)
    }

    pub fn create(&self, title: &str, body: &str) -> Result<ScriptMeta> {
        let mut index = self.load_index()?;
        let now = (self.clock)();
        let mut stamp = now;
        let id = loop {
            let candidate = format!("s{stamp:x}");
            if index.iter().all(|m| m.id != candidate) {
                break candidate;
            }
            stamp += 1;
        };
        fs::create_dir_all(&self.dir)?;
        write_atomic(&self.body_path(&id), body.as_bytes())?;
        let meta = ScriptMeta {
            id,
            title: clean_title(title),
            updated_at: now,
        };
        index.push(meta.clone());
        self.save_index(&index)?;
        Ok(meta)
    }

    pub fn read(&self, id: &str) -> Result<String> {
        let index = self.load_index()?;
        Self::position(&index, id)?;
        Ok(fs::read_to_string(self.body_path(id))?)
    }

    pub fn save(&self, id: &str, body: &str) -> Result<ScriptMeta> {
        let mut index = self.load_index()?;
        let pos = Self::position(&index, id)?;
        write_atomic(&self.body_path(id), body.as_bytes())?;
        index[pos].updated_at = (self.clock)();
        let meta = index[pos].clone();
        self.save_index(&index)?;
        Ok(meta)
    }

    /// Renaming does not count as an edit, so it keeps the script's place in the list.
    pub fn rename(&self, id: &str, title: &str) -> Result<ScriptMeta> {
        let mut index = self.load_index()?;
        let pos = Self::position(&index, id)?;
        index[pos].title = clean_title(title);
        let meta = index[pos].clone();
        self.save_index(&index)?;
        Ok(meta)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut index = self.load_index()?;
        let pos = Self::position(&index, id)?;
        index.remove(pos);
        // Index first: if removing the file fails, the script is gone from the list
        // and the leftover file is harmless.
        self.save_index(&index)?;
        match fs::remove_file(self.body_path(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

/// Collapses whitespace (including newlines), caps the length, and falls back to a
/// default when nothing is left.
fn clean_title(title: &str) -> String {
    let collapsed = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(MAX_TITLE_CHARS).collect();
    let capped = capped.trim_end().to_owned();
    if capped.is_empty() {
        DEFAULT_TITLE.to_owned()
    } else {
        capped
    }
}

/// Title for an imported file: the first Markdown heading when the text starts with
/// one, otherwise the file name without its extension.
pub fn title_for_import(file_stem: &str, content: &str) -> String {
    let first_line = content.lines().map(str::trim).find(|l| !l.is_empty());
    if let Some(line) = first_line.filter(|l| l.starts_with('#')) {
        let heading = line.trim_start_matches('#').trim();
        if !heading.is_empty() {
            return clean_title(heading);
        }
    }
    clean_title(file_stem)
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

    fn store(dir: &tempfile::TempDir) -> ScriptStore {
        ScriptStore::with_clock(dir.path().join("scripts"), counter())
    }

    #[test]
    fn empty_store_lists_nothing_and_creates_no_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(store(&dir).list().unwrap().is_empty());
        assert!(!dir.path().join("scripts").exists());
    }

    #[test]
    fn create_then_read_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let meta = s
            .create("Bài thuyết trình", "Xin chào\n\nTạm biệt")
            .unwrap();
        assert_eq!(meta.title, "Bài thuyết trình");
        assert_eq!(s.read(&meta.id).unwrap(), "Xin chào\n\nTạm biệt");
        assert_eq!(s.list().unwrap(), vec![meta]);
    }

    #[test]
    fn save_replaces_the_body_and_moves_the_script_to_the_top() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let first = s.create("Một", "a").unwrap();
        let second = s.create("Hai", "b").unwrap();
        assert_eq!(s.list().unwrap()[0].id, second.id);

        let saved = s.save(&first.id, "a mới").unwrap();
        assert!(saved.updated_at > second.updated_at);
        assert_eq!(s.read(&first.id).unwrap(), "a mới");
        assert_eq!(s.list().unwrap()[0].id, first.id);
    }

    #[test]
    fn rename_cleans_the_title_and_keeps_the_order() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let first = s.create("Một", "a").unwrap();
        let second = s.create("Hai", "b").unwrap();

        let renamed = s.rename(&first.id, "  Tên\nmới  \t ").unwrap();
        assert_eq!(renamed.title, "Tên mới");
        assert_eq!(renamed.updated_at, first.updated_at);
        assert_eq!(s.list().unwrap()[0].id, second.id);
    }

    #[test]
    fn delete_removes_the_entry_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let meta = s.create("Xoá tôi", "x").unwrap();
        let body = dir.path().join("scripts").join(format!("{}.md", meta.id));
        assert!(body.exists());

        s.delete(&meta.id).unwrap();
        assert!(!body.exists());
        assert!(s.list().unwrap().is_empty());
        assert!(matches!(s.read(&meta.id), Err(ScriptError::NotFound(_))));
    }

    #[test]
    fn delete_tolerates_a_missing_body_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let meta = s.create("Mất file", "x").unwrap();
        fs::remove_file(dir.path().join("scripts").join(format!("{}.md", meta.id))).unwrap();
        s.delete(&meta.id).unwrap();
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn unknown_ids_are_rejected_everywhere() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("Có thật", "x").unwrap();
        assert!(matches!(s.read("nope"), Err(ScriptError::NotFound(_))));
        assert!(matches!(s.save("nope", "x"), Err(ScriptError::NotFound(_))));
        assert!(matches!(
            s.rename("nope", "x"),
            Err(ScriptError::NotFound(_))
        ));
        assert!(matches!(s.delete("nope"), Err(ScriptError::NotFound(_))));
    }

    #[test]
    fn ids_that_try_to_leave_the_directory_are_rejected_and_write_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("Có thật", "x").unwrap();
        assert!(matches!(
            s.save("../escape", "pwned"),
            Err(ScriptError::NotFound(_))
        ));
        assert!(!dir.path().join("escape.md").exists());
    }

    #[test]
    fn ids_stay_unique_when_the_clock_does_not_advance() {
        let dir = tempfile::tempdir().unwrap();
        let s = ScriptStore::with_clock(dir.path().join("scripts"), || 5000);
        let a = s.create("A", "").unwrap();
        let b = s.create("B", "").unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(s.list().unwrap().len(), 2);
    }

    #[test]
    fn corrupt_index_is_reported_instead_of_silently_emptied() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("A", "x").unwrap();
        fs::write(dir.path().join("scripts").join("index.json"), "{ not json").unwrap();
        assert!(matches!(s.list(), Err(ScriptError::Index(_))));
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_title(""), DEFAULT_TITLE);
        assert_eq!(clean_title("   \n "), DEFAULT_TITLE);
        assert_eq!(clean_title("a   b"), "a b");
        let long = "x".repeat(200);
        assert_eq!(clean_title(&long).chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn import_title_prefers_the_first_heading() {
        assert_eq!(
            title_for_import("file", "# Giới thiệu\n\nNội dung"),
            "Giới thiệu"
        );
        assert_eq!(title_for_import("file", "\n\n## Phụ đề\ntext"), "Phụ đề");
    }

    #[test]
    fn import_title_falls_back_to_the_file_name() {
        assert_eq!(title_for_import("kich-ban", "Không có tiêu đề"), "kich-ban");
        assert_eq!(title_for_import("kich-ban", "#\ntext"), "kich-ban");
        assert_eq!(title_for_import("", "text"), DEFAULT_TITLE);
    }
}
