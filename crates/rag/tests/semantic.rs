//! The meaning search against the real multilingual-e5-small model and the sample documents.
//! Needs the model files, so it is `#[ignore]`d:
//! `OFFSTAGE_E5_DIR=<folder with the five files> cargo test -p offstage-rag --test semantic -- --ignored --nocapture`
//! (the files of `Xenova/multilingual-e5-small` under their Hugging Face names, `model_int8.onnx`
//! for the model).

use std::{fs, path::PathBuf};

use offstage_core::{db::Db, knowledge::KnowledgeKind};
use offstage_embed::{
    onnx::{E5Embedder, ModelFiles},
    TextEmbedder,
};
use offstage_rag::{ingest::index_file, ChunkStore, SemanticQuery};
use rusqlite::params;

const MODEL: &str = "multilingual-e5-small";

fn embedder() -> E5Embedder {
    let dir = PathBuf::from(std::env::var_os("OFFSTAGE_E5_DIR").expect("set OFFSTAGE_E5_DIR"));
    E5Embedder::load(&ModelFiles {
        model: dir.join("model_int8.onnx"),
        tokenizer: dir.join("tokenizer.json"),
        config: dir.join("config.json"),
        special_tokens_map: dir.join("special_tokens_map.json"),
        tokenizer_config: dir.join("tokenizer_config.json"),
    })
    .unwrap()
}

fn indexed(e: &E5Embedder) -> ChunkStore {
    let db = Db::open_in_memory().unwrap();
    let store = ChunkStore::new(db.clone());
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/knowledge");
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|f| f.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
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
    store.embed_pending(e, MODEL, |_| {}).unwrap();
    store
}

fn ask(e: &E5Embedder, store: &ChunkStore, question: &str) -> Vec<(String, String)> {
    let v = e.embed_query(question).unwrap();
    store
        .search_with(
            question,
            Some(&SemanticQuery {
                model: MODEL,
                vector: &v,
            }),
            4,
            4000,
        )
        .unwrap()
        .into_iter()
        .map(|h| (h.chunk.doc, h.chunk.section.unwrap_or_default()))
        .collect()
}

/// (question, document, section) that must be among the hits. Mixed languages, and
/// phrasings that share few words with the section.
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
    // No shared words, or another language.
    (
        "What is the difference between WHERE and HAVING in SQL?",
        "SQL.md",
        "WHERE và HAVING khác nhau thế nào",
    ),
    (
        "How does Node.js handle asynchronous work on a single thread?",
        "NodeJS.md",
        "Event loop",
    ),
    (
        "Làm sao để tránh component render lại không cần thiết?",
        "React.md",
        "Khi nào component render lại",
    ),
    (
        "Cách kiểm tra dữ liệu đầu vào của request trong Nest?",
        "NestJS.md",
        "Kiểm tra dữ liệu",
    ),
    (
        "Why is a join slow and how do indexes help?",
        "SQL.md",
        "Chỉ mục (index)",
    ),
];

/// Questions the documents do not answer. Printed, not asserted: see `MIN_SIMILARITY`.
const UNANSWERABLE: &[&str] = &[
    "Mức lương mong muốn của bạn là bao nhiêu?",
    "Hãy giải thích Terraform và Ansible",
    "Bạn có thích chạy bộ không?",
    "What is your favourite food?",
    "Bạn đã học đại học ở đâu?",
    "Thời tiết hôm nay thế nào?",
];

#[test]
#[ignore = "needs the model files"]
fn what_the_assistant_would_get_for_each_question() {
    let e = embedder();
    let store = indexed(&e);
    let mut missing = Vec::new();
    for (question, doc, section) in ANSWERABLE {
        let hits = ask(&e, &store, question);
        eprintln!("{question} -> {hits:?}");
        let words: Vec<_> = store
            .search(question, 6, 9000)
            .unwrap()
            .into_iter()
            .map(|h| h.chunk.section)
            .collect();
        eprintln!("   by words alone: {words:?}");
        // The assistant sends at most four excerpts, so the right section has to be among them.
        if !hits.iter().any(|(d, s)| d == doc && s == section) {
            missing.push(format!("{question} -> {hits:?}"));
        }
    }
    for question in UNANSWERABLE {
        eprintln!(
            "(no answer in the documents) {question} -> {:?}",
            ask(&e, &store, question)
        );
    }
    assert!(missing.is_empty(), "\n{}", missing.join("\n"));
}

#[test]
#[ignore = "needs the model files"]
fn a_question_far_from_every_document_gets_nothing() {
    let e = embedder();
    let store = indexed(&e);
    assert!(ask(&e, &store, "What is your favourite food?").is_empty());
}

/// What speech recognition actually wrote for a spoken English question about "PostgreSQL" (the
/// real model, `tests` in `offstage-stt`): "PostGur SQL". The CV is the real PDF fixture.
#[test]
#[ignore = "needs the model files"]
fn a_question_misheard_by_speech_recognition_still_finds_the_right_paragraph() {
    let e = embedder();
    let db = Db::open_in_memory().unwrap();
    let store = ChunkStore::new(db.clone());
    db.lock()
        .unwrap()
        .execute(
            "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
             VALUES ('cv', 'cv.pdf', 'pdf', 1, 1)",
            [],
        )
        .unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cv.pdf");
    index_file(&store, "cv", "cv.pdf", KnowledgeKind::Pdf, &path).unwrap();
    store.embed_pending(&e, MODEL, |_| {}).unwrap();

    for question in [
        "Can you tell me about your experience with PostGur SQL?",
        "Bạn có kinh nghiệm với cơ sở dữ liệu không?",
        "Bạn học trường nào?",
    ] {
        let hits = ask(&e, &store, question);
        eprintln!("{question} -> {hits:?}");
        assert!(!hits.is_empty(), "{question}");
    }
    let found = |q: &str, needle: &str| {
        let v = e.embed_query(q).unwrap();
        store
            .search_with(
                q,
                Some(&SemanticQuery {
                    model: MODEL,
                    vector: &v,
                }),
                4,
                4000,
            )
            .unwrap()
            .iter()
            .any(|h| h.chunk.content.contains(needle))
    };
    assert!(found(
        "Can you tell me about your experience with PostGur SQL?",
        "PostgreSQL"
    ));
    assert!(found("Bạn học trường nào?", "Bách khoa"));
    // Known gap, measured: an English question about a Vietnamese CV that is a single chunk
    // scores 0.76 to 0.79 against it, no higher than "what is the weather like today" (0.78), so
    // "Where did you study?" finds nothing. See CLAUDE.md.
}
