//! Teleprompter scripts, stored in the local database.
//!
//! Ids are generated here and every lookup goes through them as plain query parameters,
//! so a caller-supplied id can never reach anything but a script row.

use std::{
    fs, io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Db, DbError};

pub const SAMPLE_TITLE: &str = "Kịch bản mẫu";
pub const SAMPLE_BODY: &str = "Chào mọi người, cảm ơn đã dành thời gian hôm nay.

Đây là kịch bản mẫu. Nhấn E để sửa nội dung, Space để chạy hoặc dừng.

Mũi tên lên và xuống chỉnh tốc độ. Phím + và − chỉnh cỡ chữ.

Đặt cửa sổ này sát webcam để ánh mắt nhìn tự nhiên khi đọc.

Cửa sổ được loại khỏi luồng chia sẻ màn hình, nên người xem chỉ thấy bạn.";

const DEFAULT_TITLE: &str = "Kịch bản mới";
const MAX_TITLE_CHARS: usize = 80;
/// Set once the scripts of the file-based storage used before the database were copied over.
const LEGACY_IMPORTED_KEY: &str = "legacy_scripts_imported";
/// Characters of context shown before and after a match in a search result.
const SNIPPET_BEFORE: usize = 36;
const SNIPPET_AFTER: usize = 90;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScriptMeta {
    pub id: String,
    pub title: String,
    /// Unix time in milliseconds.
    pub updated_at: u64,
}

/// A search result: the script, and where the words were found in its text when they were.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScriptHit {
    pub id: String,
    pub title: String,
    /// A short stretch of the text around the first match, `None` when only the title matched.
    pub snippet: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script storage error: {0}")]
    Db(#[from] DbError),
    #[error("old script storage error: {0}")]
    Io(#[from] io::Error),
    #[error("old script index is unreadable: {0}")]
    Index(#[from] serde_json::Error),
    #[error("script not found: {0}")]
    NotFound(String),
}

impl From<rusqlite::Error> for ScriptError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.into())
    }
}

type Result<T> = std::result::Result<T, ScriptError>;

fn to_sql_int(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn from_sql_int(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn meta_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScriptMeta> {
    Ok(ScriptMeta {
        id: row.get(0)?,
        title: row.get(1)?,
        updated_at: from_sql_int(row.get(2)?),
    })
}

pub struct ScriptStore {
    db: Db,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl ScriptStore {
    pub fn new(db: Db) -> Self {
        Self::with_clock(db, system_millis)
    }

    /// Like [`ScriptStore::new`] with a custom time source, for tests.
    pub fn with_clock(db: Db, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        Self {
            db,
            clock: Box::new(clock),
        }
    }

    fn get_meta(&self, id: &str) -> Result<ScriptMeta> {
        self.db
            .lock()?
            .query_row(
                "SELECT id, title, updated_at FROM scripts WHERE id = ?1",
                [id],
                meta_from_row,
            )
            .optional()?
            .ok_or_else(|| ScriptError::NotFound(id.to_owned()))
    }

    /// All scripts, most recently saved first.
    pub fn list(&self) -> Result<Vec<ScriptMeta>> {
        let conn = self.db.lock()?;
        // `rowid` breaks ties so the newest of two scripts saved in the same millisecond is first.
        let mut stmt = conn.prepare(
            "SELECT id, title, updated_at FROM scripts ORDER BY updated_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([], meta_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn create(&self, title: &str, body: &str) -> Result<ScriptMeta> {
        let conn = self.db.lock()?;
        let now = (self.clock)();
        let mut stamp = now;
        let id = loop {
            let candidate = format!("s{stamp:x}");
            let taken = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM scripts WHERE id = ?1)",
                [&candidate],
                |row| row.get::<_, bool>(0),
            )?;
            if !taken {
                break candidate;
            }
            stamp += 1;
        };
        let meta = ScriptMeta {
            id,
            title: clean_title(title),
            updated_at: now,
        };
        conn.execute(
            "INSERT INTO scripts (id, title, body, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![meta.id, meta.title, body, to_sql_int(now)],
        )?;
        Ok(meta)
    }

    pub fn read(&self, id: &str) -> Result<String> {
        self.db
            .lock()?
            .query_row("SELECT body FROM scripts WHERE id = ?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .ok_or_else(|| ScriptError::NotFound(id.to_owned()))
    }

    pub fn save(&self, id: &str, body: &str) -> Result<ScriptMeta> {
        let changed = self.db.lock()?.execute(
            "UPDATE scripts SET body = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, body, to_sql_int((self.clock)())],
        )?;
        if changed == 0 {
            return Err(ScriptError::NotFound(id.to_owned()));
        }
        self.get_meta(id)
    }

    /// Renaming does not count as an edit, so it keeps the script's place in the list.
    pub fn rename(&self, id: &str, title: &str) -> Result<ScriptMeta> {
        let changed = self.db.lock()?.execute(
            "UPDATE scripts SET title = ?2 WHERE id = ?1",
            params![id, clean_title(title)],
        )?;
        if changed == 0 {
            return Err(ScriptError::NotFound(id.to_owned()));
        }
        self.get_meta(id)
    }

    /// Finds scripts whose title or text contains `query`, ignoring case and Vietnamese accents,
    /// so "ngan gon" finds "ngắn gọn". Titles that match come first, then text-only matches, each
    /// group newest first. An empty query finds nothing.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ScriptHit>> {
        let (needle, _) = fold(query.trim());
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let mut in_title = Vec::new();
        let mut in_text = Vec::new();
        let candidates: Vec<(String, String, String)> = {
            let conn = self.db.lock()?;
            let mut stmt = conn.prepare(
                "SELECT id, title, body FROM scripts ORDER BY updated_at DESC, rowid DESC",
            )?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (id, title_text, body) in candidates {
            let snippet = snippet_for(&body, &needle);
            let (title, _) = fold(&title_text);
            let title_match = find(&title, &needle).is_some();
            if !title_match && snippet.is_none() {
                continue;
            }
            let hit = ScriptHit {
                id,
                title: title_text,
                snippet,
            };
            if title_match {
                in_title.push(hit);
            } else {
                in_text.push(hit);
            }
        }
        in_title.extend(in_text);
        in_title.truncate(limit);
        Ok(in_title)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let changed = self
            .db
            .lock()?
            .execute("DELETE FROM scripts WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(ScriptError::NotFound(id.to_owned()));
        }
        Ok(())
    }

    /// Copies the scripts of the file-based storage used before the database (one `<id>.md`
    /// per script plus `index.json` in `legacy_dir`) into the database, once. Returns how
    /// many were copied. The old files are left where they are. A script whose file is
    /// missing or whose id is not a plain identifier is skipped. When the old index cannot
    /// be read nothing is marked as done, so the next start tries again.
    pub fn import_legacy(&self, legacy_dir: &Path) -> Result<usize> {
        if self
            .db
            .kv_get::<bool>(LEGACY_IMPORTED_KEY)?
            .unwrap_or(false)
        {
            return Ok(0);
        }
        let metas: Vec<ScriptMeta> = match fs::read_to_string(legacy_dir.join("index.json")) {
            Ok(text) => serde_json::from_str(&text)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        let mut copied = 0;
        {
            let mut conn = self.db.lock()?;
            let tx = conn.transaction()?;
            for meta in metas {
                // The id becomes a file name below, so it must not be able to leave the folder.
                if meta.id.is_empty() || !meta.id.chars().all(|c| c.is_ascii_alphanumeric()) {
                    continue;
                }
                let Ok(body) = fs::read_to_string(legacy_dir.join(format!("{}.md", meta.id)))
                else {
                    continue;
                };
                copied += tx.execute(
                    "INSERT OR IGNORE INTO scripts (id, title, body, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        meta.id,
                        clean_title(&meta.title),
                        body,
                        to_sql_int(meta.updated_at)
                    ],
                )?;
            }
            tx.commit()?;
        }
        self.db.kv_set(LEGACY_IMPORTED_KEY, &true)?;
        Ok(copied)
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

/// Lowercases a letter and strips its Vietnamese accent. Returns `None` for a combining accent
/// mark, which is dropped (so accents typed as separate marks are ignored too).
fn fold_char(c: char) -> Option<char> {
    if ('\u{0300}'..='\u{036f}').contains(&c) {
        return None;
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    Some(match lower {
        'à' | 'á' | 'ạ' | 'ả' | 'ã' | 'â' | 'ầ' | 'ấ' | 'ậ' | 'ẩ' | 'ẫ' | 'ă' | 'ằ' | 'ắ' | 'ặ'
        | 'ẳ' | 'ẵ' => 'a',
        'è' | 'é' | 'ẹ' | 'ẻ' | 'ẽ' | 'ê' | 'ề' | 'ế' | 'ệ' | 'ể' | 'ễ' => 'e',
        'ì' | 'í' | 'ị' | 'ỉ' | 'ĩ' => 'i',
        'ò' | 'ó' | 'ọ' | 'ỏ' | 'õ' | 'ô' | 'ồ' | 'ố' | 'ộ' | 'ổ' | 'ỗ' | 'ơ' | 'ờ' | 'ớ' | 'ợ'
        | 'ở' | 'ỡ' => 'o',
        'ù' | 'ú' | 'ụ' | 'ủ' | 'ũ' | 'ư' | 'ừ' | 'ứ' | 'ự' | 'ử' | 'ữ' => 'u',
        'ỳ' | 'ý' | 'ỵ' | 'ỷ' | 'ỹ' => 'y',
        'đ' => 'd',
        other => other,
    })
}

/// The folded text, plus for each folded character the index of the character it came from,
/// so a match can be mapped back to the original text.
fn fold(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut folded = Vec::new();
    let mut origin = Vec::new();
    for (index, c) in text.chars().enumerate() {
        if let Some(f) = fold_char(c) {
            folded.push(f);
            origin.push(index);
        }
    }
    (folded, origin)
}

fn find(haystack: &[char], needle: &[char]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A short, single-line stretch of `text` around the first match of `needle` (already folded).
fn snippet_for(text: &str, needle: &[char]) -> Option<String> {
    let (folded, origin) = fold(text);
    let at = find(&folded, needle)?;
    let chars: Vec<char> = text.chars().collect();
    let start = origin[at];
    let end = origin[at + needle.len() - 1] + 1;
    let from = start.saturating_sub(SNIPPET_BEFORE);
    let to = (end + SNIPPET_AFTER).min(chars.len());

    let window: String = chars[from..to].iter().collect();
    let mut snippet = window.split_whitespace().collect::<Vec<_>>().join(" ");
    if from > 0 {
        snippet.insert(0, '…');
    }
    if to < chars.len() {
        snippet.push('…');
    }
    Some(snippet)
}

fn system_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
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

    fn store() -> ScriptStore {
        ScriptStore::with_clock(Db::open_in_memory().unwrap(), counter())
    }

    #[test]
    fn empty_store_lists_nothing() {
        assert!(store().list().unwrap().is_empty());
    }

    #[test]
    fn create_then_read_round_trips() {
        let s = store();
        let meta = s
            .create("Bài thuyết trình", "Xin chào\n\nTạm biệt")
            .unwrap();
        assert_eq!(meta.title, "Bài thuyết trình");
        assert_eq!(s.read(&meta.id).unwrap(), "Xin chào\n\nTạm biệt");
        assert_eq!(s.list().unwrap(), vec![meta]);
    }

    #[test]
    fn an_empty_body_is_kept_as_empty() {
        let s = store();
        let meta = s.create("Trống", "").unwrap();
        assert_eq!(s.read(&meta.id).unwrap(), "");
    }

    #[test]
    fn data_survives_reopening_the_database_file() {
        let dir = tempfile::tempdir().unwrap();
        let meta = ScriptStore::new(Db::open(dir.path()).unwrap())
            .create("Lưu lại", "nội dung")
            .unwrap();
        let again = ScriptStore::new(Db::open(dir.path()).unwrap());
        assert_eq!(again.read(&meta.id).unwrap(), "nội dung");
        assert_eq!(again.list().unwrap(), vec![meta]);
    }

    #[test]
    fn save_replaces_the_body_and_moves_the_script_to_the_top() {
        let s = store();
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
        let s = store();
        let first = s.create("Một", "a").unwrap();
        let second = s.create("Hai", "b").unwrap();

        let renamed = s.rename(&first.id, "  Tên\nmới  \t ").unwrap();
        assert_eq!(renamed.title, "Tên mới");
        assert_eq!(renamed.updated_at, first.updated_at);
        assert_eq!(s.list().unwrap()[0].id, second.id);
    }

    #[test]
    fn delete_removes_the_script() {
        let s = store();
        let keep = s.create("Giữ", "k").unwrap();
        let meta = s.create("Xoá tôi", "x").unwrap();

        s.delete(&meta.id).unwrap();
        assert_eq!(s.list().unwrap(), vec![keep]);
        assert!(matches!(s.read(&meta.id), Err(ScriptError::NotFound(_))));
    }

    #[test]
    fn unknown_ids_are_rejected_everywhere() {
        let s = store();
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
    fn ids_that_look_like_sql_or_paths_are_just_unknown_ids() {
        let s = store();
        let real = s.create("Có thật", "x").unwrap();
        for id in ["../escape", "' OR 1=1 --", "%", "s%"] {
            assert!(matches!(s.save(id, "pwned"), Err(ScriptError::NotFound(_))));
            assert!(matches!(s.delete(id), Err(ScriptError::NotFound(_))));
        }
        assert_eq!(s.read(&real.id).unwrap(), "x");
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn ids_stay_unique_and_the_newest_stays_first_when_the_clock_does_not_advance() {
        let s = ScriptStore::with_clock(Db::open_in_memory().unwrap(), || 5000);
        let a = s.create("A", "").unwrap();
        let b = s.create("B", "").unwrap();
        assert_ne!(a.id, b.id);
        let listed = s.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, b.id);
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

    /// Writes scripts the way the file-based storage did.
    fn write_legacy(dir: &Path, scripts: &[(&str, &str, &str, u64)]) {
        fs::create_dir_all(dir).unwrap();
        let index: Vec<ScriptMeta> = scripts
            .iter()
            .map(|(id, title, _, updated_at)| ScriptMeta {
                id: (*id).to_owned(),
                title: (*title).to_owned(),
                updated_at: *updated_at,
            })
            .collect();
        fs::write(
            dir.join("index.json"),
            serde_json::to_string(&index).unwrap(),
        )
        .unwrap();
        for (id, _, body, _) in scripts {
            fs::write(dir.join(format!("{id}.md")), body).unwrap();
        }
    }

    #[test]
    fn legacy_scripts_are_copied_once_and_keep_their_dates() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("scripts");
        write_legacy(
            &legacy,
            &[("s1", "Cũ một", "a", 100), ("s2", "Cũ hai", "b", 200)],
        );
        let s = store();

        assert_eq!(s.import_legacy(&legacy).unwrap(), 2);
        let listed = s.list().unwrap();
        assert_eq!(listed[0].title, "Cũ hai");
        assert_eq!(listed[0].updated_at, 200);
        assert_eq!(s.read("s1").unwrap(), "a");

        // Scripts deleted afterwards must not come back on the next start.
        s.delete("s1").unwrap();
        assert_eq!(s.import_legacy(&legacy).unwrap(), 0);
        assert_eq!(s.list().unwrap().len(), 1);
        // And the old files are left alone.
        assert!(legacy.join("s1.md").exists());
    }

    #[test]
    fn no_legacy_folder_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let s = store();
        assert_eq!(s.import_legacy(&dir.path().join("nothing")).unwrap(), 0);
    }

    #[test]
    fn legacy_entries_with_odd_ids_or_missing_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("scripts");
        write_legacy(&legacy, &[("s1", "Tốt", "ok", 1)]);
        // Rewrite the index with a path-like id and an entry without a file.
        let index = r#"[
            {"id":"s1","title":"Tốt","updated_at":1},
            {"id":"../secret","title":"Xấu","updated_at":2},
            {"id":"s9","title":"Mất file","updated_at":3},
            {"id":"","title":"Rỗng","updated_at":4}
        ]"#;
        fs::write(legacy.join("index.json"), index).unwrap();
        fs::write(dir.path().join("secret.md"), "should not be read").unwrap();

        let s = store();
        assert_eq!(s.import_legacy(&legacy).unwrap(), 1);
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn a_broken_legacy_index_is_reported_and_retried_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("scripts");
        write_legacy(&legacy, &[("s1", "Cũ", "a", 1)]);
        fs::write(legacy.join("index.json"), "{ not json").unwrap();
        let s = store();
        assert!(matches!(
            s.import_legacy(&legacy),
            Err(ScriptError::Index(_))
        ));

        write_legacy(&legacy, &[("s1", "Cũ", "a", 1)]);
        assert_eq!(s.import_legacy(&legacy).unwrap(), 1);
    }
}

#[cfg(test)]
mod search_tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    fn store() -> ScriptStore {
        let next = AtomicU64::new(1000);
        ScriptStore::with_clock(Db::open_in_memory().unwrap(), move || {
            next.fetch_add(10, Ordering::SeqCst)
        })
    }

    fn ids(hits: &[ScriptHit]) -> Vec<&str> {
        hits.iter().map(|h| h.title.as_str()).collect()
    }

    #[test]
    fn folding_ignores_case_and_vietnamese_accents() {
        let folded = |s: &str| fold(s).0.into_iter().collect::<String>();
        assert_eq!(folded("Ngắn GỌN"), "ngan gon");
        assert_eq!(folded("Đường đi"), "duong di");
        assert_eq!(
            folded("Tiếng Việt: ưu tiên, ơi, ỳ"),
            "tieng viet: uu tien, oi, y"
        );
        assert_eq!(folded("plain ascii 123"), "plain ascii 123");
    }

    #[test]
    fn accents_typed_as_separate_marks_are_ignored_too() {
        // "ngắn" written as n, g, a, breve, acute, n, with the marks as separate characters.
        let decomposed = "nga\u{0306}\u{0301}n";
        let (folded, origin) = fold(decomposed);
        assert_eq!(folded.iter().collect::<String>(), "ngan");
        // The last letter still points at its real position (index 5), after the two marks.
        assert_eq!(origin, vec![0, 1, 2, 5]);
    }

    #[test]
    fn a_title_match_is_found_without_caring_about_case() {
        let s = store();
        s.create("Bài Thuyết Trình", "nội dung khác").unwrap();
        s.create("Ghi chú", "không liên quan").unwrap();

        let hits = s.search("thuyết trình", 10).unwrap();
        assert_eq!(ids(&hits), vec!["Bài Thuyết Trình"]);
        // The text does not hold the words, so there is nothing to quote.
        assert_eq!(hits[0].snippet, None);
    }

    #[test]
    fn the_text_is_searched_and_quoted() {
        let s = store();
        s.create(
            "Một",
            "Xin chào mọi người. Hôm nay chúng ta nói về doanh thu quý ba.",
        )
        .unwrap();

        let hits = s.search("doanh thu", 10).unwrap();
        assert_eq!(hits.len(), 1);
        let snippet = hits[0].snippet.as_deref().unwrap();
        assert!(snippet.contains("doanh thu quý ba"), "{snippet}");
    }

    #[test]
    fn typing_without_accents_finds_accented_text_and_the_other_way_round() {
        let s = store();
        s.create("Một", "Hãy trình bày ngắn gọn về kinh nghiệm")
            .unwrap();

        assert_eq!(s.search("ngan gon", 10).unwrap().len(), 1);
        assert_eq!(s.search("ngắn gọn", 10).unwrap().len(), 1);
        assert_eq!(s.search("KINH NGHIEM", 10).unwrap().len(), 1);
        assert_eq!(s.search("nghiệm", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_long_text_is_quoted_with_ellipses_and_on_one_line() {
        let s = store();
        let body = format!(
            "{}\n\nTừ khoá cần tìm\n\n{}",
            "a ".repeat(60),
            "b ".repeat(80)
        );
        s.create("Dài", &body).unwrap();

        let snippet = s.search("khoá", 10).unwrap().remove(0).snippet.unwrap();
        assert!(
            snippet.starts_with('…') && snippet.ends_with('…'),
            "{snippet}"
        );
        assert!(snippet.contains("Từ khoá cần tìm"));
        assert!(!snippet.contains('\n'));
        assert!(snippet.chars().count() < 160, "{}", snippet.chars().count());
    }

    #[test]
    fn titles_that_match_come_before_text_only_matches() {
        let s = store();
        // The text-only match is the newest, yet it still comes second.
        s.create("Báo cáo quý", "không nhắc tới từ khoá").unwrap();
        s.create("Ghi chú", "báo cáo nằm trong nội dung").unwrap();

        let hits = s.search("báo cáo", 10).unwrap();
        assert_eq!(ids(&hits), vec!["Báo cáo quý", "Ghi chú"]);
    }

    #[test]
    fn within_a_group_the_newest_comes_first() {
        let s = store();
        s.create("Họp cũ", "").unwrap();
        s.create("Họp mới", "").unwrap();
        assert_eq!(
            ids(&s.search("họp", 10).unwrap()),
            vec!["Họp mới", "Họp cũ"]
        );
    }

    #[test]
    fn an_empty_or_blank_query_finds_nothing() {
        let s = store();
        s.create("Một", "nội dung").unwrap();
        assert!(s.search("", 10).unwrap().is_empty());
        assert!(s.search("   \n ", 10).unwrap().is_empty());
    }

    #[test]
    fn no_match_and_an_empty_store_both_give_an_empty_list() {
        let s = store();
        assert!(s.search("bất kỳ", 10).unwrap().is_empty());
        s.create("Một", "nội dung").unwrap();
        assert!(s.search("không có từ này", 10).unwrap().is_empty());
    }

    #[test]
    fn the_limit_is_respected() {
        let s = store();
        for i in 0..5 {
            s.create(&format!("Họp {i}"), "").unwrap();
        }
        assert_eq!(s.search("họp", 3).unwrap().len(), 3);
    }
}
