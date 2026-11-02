//! A CV with Vietnamese text as a real PDF (printed by Chrome) and a real DOCX (made by macOS
//! `textutil`), read through the same path the app uses. The unit tests only build tiny files by
//! hand; this checks that what real programs write comes out with its accents intact.

use std::{fs, path::PathBuf};

use offstage_core::{db::Db, knowledge::KnowledgeKind};
use offstage_rag::{extract::extract_text, ingest::index_file, ChunkStore};
use rusqlite::params;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

const FILES: &[(&str, KnowledgeKind)] = &[
    ("cv.pdf", KnowledgeKind::Pdf),
    ("cv.docx", KnowledgeKind::Docx),
];

#[test]
fn vietnamese_text_comes_out_of_a_real_pdf_and_docx_with_its_accents() {
    for (name, kind) in FILES {
        let raw = extract_text(*kind, &fs::read(fixture(name)).unwrap()).unwrap();
        // A paragraph wrapped by the PDF may break between any two words.
        let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        // Words with the accents that break first: stacked marks and đ.
        for expected in [
            "Nguyễn Văn A",
            "Đại học Bách khoa Hà Nội",
            "nhận dạng giọng nói",
            "PostgreSQL",
            "Công ty Âm Sắc",
            // Spaces that a PDF reader wrongly puts after "ứ", "ờ", "ở" or capitals.
            "ứng dụng desktop",
            "người dùng",
            "cơ sở dữ liệu",
            "Công ty Bình Minh",
        ] {
            assert!(
                text.contains(expected),
                "{name}: {expected:?} is missing from\n{text}"
            );
        }
        // A heading stays on one line.
        assert!(
            text.contains("Kinh nghiệm\n") || text.contains("Kinh nghiệm "),
            "{name}\n{text}"
        );
        assert!(!text.contains("Kinh\nnghiệm"), "{name}\n{text}");
    }
}

#[test]
fn a_question_finds_the_right_part_of_a_real_pdf_and_docx() {
    for (name, kind) in FILES {
        let db = Db::open_in_memory().unwrap();
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
                 VALUES ('d1', ?1, 'pdf', 1, 1)",
                params![name],
            )
            .unwrap();
        let store = ChunkStore::new(db);
        let count = index_file(&store, "d1", name, *kind, &fixture(name)).unwrap();
        assert!(count >= 1, "{name}");

        let hits = store
            .search("Bạn tốt nghiệp trường đại học nào?", 4, 4000)
            .unwrap();
        assert!(
            hits.iter()
                .any(|h| h.chunk.content.contains("Bách khoa") && h.chunk.doc == *name),
            "{name}: {hits:?}"
        );
        let hits = store
            .search("Bạn có kinh nghiệm với PostgreSQL không?", 4, 4000)
            .unwrap();
        assert!(
            hits.iter().any(|h| h.chunk.content.contains("PostgreSQL")),
            "{name}: {hits:?}"
        );
    }
}
