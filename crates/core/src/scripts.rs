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
        index.sort_by_key(|m| std::cmp::Reverse(m.updated_at));
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
        for meta in self.list()? {
            // A missing text file only means no text to search, not a failed search.
            let body = fs::read_to_string(self.body_path(&meta.id)).unwrap_or_default();
            let snippet = snippet_for(&body, &needle);
            let (title, _) = fold(&meta.title);
            let title_match = find(&title, &needle).is_some();
            if !title_match && snippet.is_none() {
                continue;
            }
            let hit = ScriptHit {
                id: meta.id,
                title: meta.title,
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

#[cfg(test)]
mod search_tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    fn store(dir: &tempfile::TempDir) -> ScriptStore {
        let next = AtomicU64::new(1000);
        ScriptStore::with_clock(dir.path().join("scripts"), move || {
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
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("Bài Thuyết Trình", "nội dung khác").unwrap();
        s.create("Ghi chú", "không liên quan").unwrap();

        let hits = s.search("thuyết trình", 10).unwrap();
        assert_eq!(ids(&hits), vec!["Bài Thuyết Trình"]);
        // The text does not hold the words, so there is nothing to quote.
        assert_eq!(hits[0].snippet, None);
    }

    #[test]
    fn the_text_is_searched_and_quoted() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
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
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("Một", "Hãy trình bày ngắn gọn về kinh nghiệm")
            .unwrap();

        assert_eq!(s.search("ngan gon", 10).unwrap().len(), 1);
        assert_eq!(s.search("ngắn gọn", 10).unwrap().len(), 1);
        assert_eq!(s.search("KINH NGHIEM", 10).unwrap().len(), 1);
        assert_eq!(s.search("nghiệm", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_long_text_is_quoted_with_ellipses_and_on_one_line() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
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
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        // The text-only match is the newest, yet it still comes second.
        s.create("Báo cáo quý", "không nhắc tới từ khoá").unwrap();
        s.create("Ghi chú", "báo cáo nằm trong nội dung").unwrap();

        let hits = s.search("báo cáo", 10).unwrap();
        assert_eq!(ids(&hits), vec!["Báo cáo quý", "Ghi chú"]);
    }

    #[test]
    fn within_a_group_the_newest_comes_first() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("Họp cũ", "").unwrap();
        s.create("Họp mới", "").unwrap();
        assert_eq!(
            ids(&s.search("họp", 10).unwrap()),
            vec!["Họp mới", "Họp cũ"]
        );
    }

    #[test]
    fn an_empty_or_blank_query_finds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("Một", "nội dung").unwrap();
        assert!(s.search("", 10).unwrap().is_empty());
        assert!(s.search("   \n ", 10).unwrap().is_empty());
    }

    #[test]
    fn no_match_and_an_empty_store_both_give_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        assert!(s.search("bất kỳ", 10).unwrap().is_empty());
        s.create("Một", "nội dung").unwrap();
        assert!(s.search("không có từ này", 10).unwrap().is_empty());
    }

    #[test]
    fn the_limit_is_respected() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        for i in 0..5 {
            s.create(&format!("Họp {i}"), "").unwrap();
        }
        assert_eq!(s.search("họp", 3).unwrap().len(), 3);
    }

    #[test]
    fn a_script_whose_text_file_is_missing_is_still_found_by_title() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let meta = s.create("Mất file", "nội dung").unwrap();
        fs::remove_file(dir.path().join("scripts").join(format!("{}.md", meta.id))).unwrap();

        assert_eq!(ids(&s.search("mất file", 10).unwrap()), vec!["Mất file"]);
        // And searching for its old text just finds nothing, instead of failing.
        assert!(s.search("nội dung", 10).unwrap().is_empty());
    }

    #[test]
    fn a_corrupt_index_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.create("Một", "x").unwrap();
        fs::write(dir.path().join("scripts").join("index.json"), "{ nope").unwrap();
        assert!(matches!(s.search("x", 10), Err(ScriptError::Index(_))));
    }
}
