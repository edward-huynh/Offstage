//! Reading a stored document into the index.

use std::{fs, path::Path};

use offstage_core::knowledge::{KnowledgeKind, MAX_FILE_BYTES};

use crate::{
    chunk::{chunk_markdown, ChunkOptions},
    extract::extract_text,
    ChunkStore, RagError,
};

/// Reads `path` (the stored copy of the document `doc_id`), cuts its text into chunks and
/// stores them in place of any earlier ones. Returns the number of chunks.
///
/// A document that cannot be read (a scanned PDF, a damaged file) is recorded as failed with a
/// reason the user can read, and the same error is returned, so the Knowledge screen can show
/// why the AI will not use it. Slow: call it off the UI thread and without holding any lock.
pub fn index_file(
    chunks: &ChunkStore,
    doc_id: &str,
    name: &str,
    kind: KnowledgeKind,
    path: &Path,
) -> Result<usize, RagError> {
    match read_chunks(name, kind, path) {
        Ok(found) => chunks.replace(doc_id, &found),
        Err(e @ (RagError::Extract(_) | RagError::Io(_))) => {
            chunks.record_failure(doc_id, &e.to_string())?;
            Err(e)
        }
        Err(e) => Err(e),
    }
}

fn read_chunks(
    name: &str,
    kind: KnowledgeKind,
    path: &Path,
) -> Result<Vec<crate::Chunk>, RagError> {
    // The store caps files already; this keeps a file swapped on disk from being loaded whole.
    if fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Err(RagError::Io(std::io::Error::other("tệp quá lớn")));
    }
    let text = extract_text(kind, &fs::read(path)?)?;
    Ok(chunk_markdown(name, &text, &ChunkOptions::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use offstage_core::db::Db;
    use rusqlite::params;

    fn setup(id: &str, name: &str) -> ChunkStore {
        let db = Db::open_in_memory().unwrap();
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
                 VALUES (?1, ?2, 'markdown', 1, 1)",
                params![id, name],
            )
            .unwrap();
        ChunkStore::new(db)
    }

    #[test]
    fn a_markdown_file_is_indexed_with_its_sections_and_found_by_search() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d1.md");
        fs::write(
            &path,
            "# Kỹ năng\n\nThành thạo PostgreSQL và Redis.\n\n# Sở thích\n\nChạy bộ.",
        )
        .unwrap();
        let store = setup("d1", "CV.md");

        assert_eq!(
            index_file(&store, "d1", "CV.md", KnowledgeKind::Markdown, &path).unwrap(),
            2
        );
        let hits = store.search("kinh nghiệm PostgreSQL", 4, 4000).unwrap();
        assert_eq!(hits[0].chunk.doc, "CV.md");
        assert_eq!(hits[0].chunk.section.as_deref(), Some("Kỹ năng"));
        assert!(store.unindexed().unwrap().is_empty());
    }

    #[test]
    fn a_file_without_text_is_recorded_as_failed_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d1.md");
        fs::write(&path, "  \n ").unwrap();
        let store = setup("d1", "rong.md");

        let err = index_file(&store, "d1", "rong.md", KnowledgeKind::Markdown, &path).unwrap_err();
        assert!(matches!(err, RagError::Extract(_)));
        let status = &store.statuses().unwrap()[0];
        assert_eq!(status.chunks, 0);
        assert_eq!(status.error.as_deref(), Some(err.to_string().as_str()));
        // It counts as looked at, so it is not retried at every start.
        assert!(store.unindexed().unwrap().is_empty());
    }

    #[test]
    fn a_missing_file_is_recorded_as_failed() {
        let dir = tempfile::tempdir().unwrap();
        let store = setup("d1", "a.md");
        let path = dir.path().join("gone.md");
        assert!(matches!(
            index_file(&store, "d1", "a.md", KnowledgeKind::Markdown, &path),
            Err(RagError::Io(_))
        ));
        assert!(store.statuses().unwrap()[0].error.is_some());
    }
}
