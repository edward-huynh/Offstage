//! Questions against the real sample documents in `samples/knowledge`, read through the same
//! path the app uses (file -> text -> chunks -> index -> search). A small stand-in for the
//! 50-question evaluation set of Phase 4: it catches a change that makes search worse.

use std::{fs, path::PathBuf};

use offstage_core::{db::Db, knowledge::KnowledgeKind};
use offstage_rag::{ingest::index_file, ChunkStore};
use rusqlite::params;

fn samples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/knowledge")
}

fn indexed_samples() -> ChunkStore {
    let db = Db::open_in_memory().unwrap();
    let store = ChunkStore::new(db.clone());
    let mut files: Vec<_> = fs::read_dir(samples())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 5, "the sample documents moved");
    for (n, path) in files.iter().enumerate() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let id = format!("d{n}");
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
                 VALUES (?1, ?2, 'markdown', 1, ?3)",
                params![id, name, n as i64],
            )
            .unwrap();
        index_file(&store, &id, &name, KnowledgeKind::Markdown, path).unwrap();
    }
    store
}

/// (question, document of the best hit, section of the best hit)
const ANSWERABLE: &[(&str, &str, &str)] = &[
    (
        "Event loop trong Node.js hoạt động như thế nào?",
        "NodeJS.md",
        "Event loop",
    ),
    (
        "Dependency injection trong NestJS là gì?",
        "NestJS.md",
        "Dependency injection",
    ),
    (
        "WHERE và HAVING khác nhau thế nào?",
        "SQL.md",
        "WHERE và HAVING khác nhau thế nào",
    ),
    (
        "Server Actions trong Next.js dùng để làm gì?",
        "Nextjs.md",
        "Server Actions",
    ),
    (
        "Key trong danh sách của React để làm gì?",
        "React.md",
        "Key trong danh sách",
    ),
    (
        "useEffect hay gặp những lỗi gì?",
        "React.md",
        "useEffect: các lỗi hay gặp",
    ),
    ("Bạn hiểu gì về middleware?", "Nextjs.md", "Middleware"),
];

#[test]
fn a_question_finds_the_right_section_of_the_right_document() {
    let store = indexed_samples();
    let mut wrong = Vec::new();
    for (question, doc, section) in ANSWERABLE {
        let hits = store.search(question, 4, 4000).unwrap();
        let top = hits
            .first()
            .map(|h| (h.chunk.doc.as_str(), h.chunk.section.as_deref()));
        if top != Some((doc, Some(section))) {
            wrong.push(format!("{question} -> {top:?}"));
        }
    }
    assert!(wrong.is_empty(), "wrong or missing:\n{}", wrong.join("\n"));
}

#[test]
fn questions_the_documents_do_not_cover_find_nothing() {
    let store = indexed_samples();
    for question in [
        "Mức lương mong muốn của bạn là bao nhiêu?",
        "Hãy giải thích Terraform và Ansible",
        "Bạn có thích chạy bộ không?",
    ] {
        let hits = store.search(question, 4, 4000).unwrap();
        assert!(
            hits.is_empty(),
            "{question} -> {:?}",
            hits.first().map(|h| &h.chunk)
        );
    }
}

#[test]
fn the_excerpts_stay_within_the_budget() {
    let store = indexed_samples();
    let hits = store
        .search("Node.js event loop async await Promise", 4, 4000)
        .unwrap();
    assert!(!hits.is_empty());
    assert!(hits.len() <= 4);
    let chars: usize = hits.iter().map(|h| h.chunk.content.chars().count()).sum();
    assert!(chars <= 4000);
}
