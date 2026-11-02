//! The chunk index: where the chunks of every document live and how a question finds them.
//! Tables and triggers are created by the migrations in `offstage-core`.

use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use offstage_core::db::Db;
use rusqlite::params;
use serde::Serialize;

use crate::{query, semantic::SemanticQuery, Chunk, RagError, SearchHit};

type Result<T> = std::result::Result<T, RagError>;

/// Most chunks kept for one document. A guard against a huge file filling the database, 20 MB
/// of plain text would otherwise make about 13,000.
pub const MAX_CHUNKS_PER_DOC: usize = 3_000;

/// Constant of reciprocal rank fusion. 60 is the value from the paper that introduced it, and it
/// keeps one ranking's first place from drowning the other's second.
const RRF_K: f32 = 60.0;

/// Chunks read from the index before the relevance filter, per search.
const CANDIDATES: i64 = 24;

/// Whether a document could be read, for the Knowledge screen.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IndexStatus {
    pub doc_id: String,
    /// Chunks stored. Zero when the document could not be read.
    pub chunks: u32,
    /// Why the document could not be read, in words for the user.
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct ChunkStore {
    pub(crate) db: Db,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl ChunkStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Replaces the chunks of a document with `chunks`, and marks it as indexed. Returns how
    /// many were kept. The document has to exist in the knowledge table.
    pub fn replace(&self, doc_id: &str, chunks: &[Chunk]) -> Result<usize> {
        let kept = &chunks[..chunks.len().min(MAX_CHUNKS_PER_DOC)];
        let mut conn = self.db.lock()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM chunks WHERE doc_id = ?1", [doc_id])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO chunks (doc_id, ord, section, content) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (ord, chunk) in kept.iter().enumerate() {
                insert.execute(params![doc_id, ord as i64, chunk.section, chunk.content])?;
            }
        }
        tx.execute(
            "INSERT INTO doc_index (doc_id, chunks, error, indexed_at) VALUES (?1, ?2, NULL, ?3)
             ON CONFLICT (doc_id) DO UPDATE SET
                 chunks = excluded.chunks, error = NULL, indexed_at = excluded.indexed_at",
            params![doc_id, kept.len() as i64, now_ms()],
        )?;
        tx.commit()?;
        Ok(kept.len())
    }

    /// Records that a document could not be read, and drops any chunks it had.
    pub fn record_failure(&self, doc_id: &str, message: &str) -> Result<()> {
        let mut conn = self.db.lock()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM chunks WHERE doc_id = ?1", [doc_id])?;
        tx.execute(
            "INSERT INTO doc_index (doc_id, chunks, error, indexed_at) VALUES (?1, 0, ?2, ?3)
             ON CONFLICT (doc_id) DO UPDATE SET
                 chunks = 0, error = excluded.error, indexed_at = excluded.indexed_at",
            params![doc_id, message, now_ms()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The state of every document that has been looked at.
    pub fn statuses(&self) -> Result<Vec<IndexStatus>> {
        let conn = self.db.lock()?;
        let mut stmt = conn.prepare("SELECT doc_id, chunks, error FROM doc_index")?;
        let rows = stmt.query_map([], |row| {
            Ok(IndexStatus {
                doc_id: row.get(0)?,
                chunks: row.get(1)?,
                error: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Ids of the documents never looked at: added before indexing existed, or while the app
    /// was closed in the middle of it.
    pub fn unindexed(&self) -> Result<Vec<String>> {
        let conn = self.db.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM knowledge WHERE id NOT IN (SELECT doc_id FROM doc_index)
             ORDER BY added_at",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// True when at least one chunk is stored, so there is something to search.
    pub fn has_chunks(&self) -> Result<bool> {
        Ok(self
            .db
            .lock()?
            .query_row("SELECT EXISTS (SELECT 1 FROM chunks)", [], |row| row.get(0))?)
    }

    /// How common the question's words are in the documents: the terms that name something
    /// specific (not accented words, held by few chunks), and a weight for each pair of words
    /// that grows as fewer chunks hold the pair. A chunk holding such a term is relevant even when
    /// the rest of a long question matches nothing ("Bạn có kinh nghiệm với PostgreSQL không?"
    /// against "Thành thạo PostgreSQL"), and a rare pair ("server actions") outweighs one that
    /// every chunk of a document about it holds ("next js").
    fn statistics(&self, terms: &[String], pairs: &HashSet<query::Pair>) -> Result<Statistics> {
        let conn = self.db.lock()?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
        let holding = |phrase: String| -> Result<i64> {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH ?1",
                [phrase],
                |row| row.get(0),
            )?)
        };
        let most = (total / 4).max(1);
        let mut rare = Vec::new();
        for term in terms.iter().filter(|t| query::looks_distinctive(t)) {
            if holding(format!("\"{term}\""))? <= most {
                rare.push(term.clone());
            }
        }
        let mut pair_weight = HashMap::new();
        for pair in pairs {
            let count = holding(format!("\"{} {}\"", pair.0, pair.1))?;
            // 1 for a pair every chunk holds, up to about 400 for one chunk in a large set.
            let idf = (((total + 1) as f64) / ((count + 1) as f64)).ln();
            pair_weight.insert(pair.clone(), 1 + (idf * 100.0) as usize);
        }
        Ok(Statistics { rare, pair_weight })
    }

    /// Every chunk, in the order the documents were added and then in document order, when the
    /// documents together hold at most `max_chars` characters. `None` when there are no documents
    /// or more text than that.
    ///
    /// For a few short documents (a CV and a job description) searching for the right paragraph
    /// is the wrong tool: a question like "where did you study?" shares no word with a Vietnamese
    /// CV and scores no higher against it than an unrelated question does. Sending all of it is
    /// cheap, and the model decides what is relevant.
    pub fn everything_if_small(&self, max_chars: usize) -> Result<Option<Vec<SearchHit>>> {
        let conn = self.db.lock()?;
        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(content)), 0) FROM chunks",
            [],
            |row| row.get(0),
        )?;
        if total == 0 || usize::try_from(total).map_or(true, |total| total > max_chars) {
            return Ok(None);
        }
        let mut stmt = conn.prepare(
            "SELECT k.name, c.section, c.content
             FROM chunks c JOIN knowledge k ON k.id = c.doc_id
             ORDER BY k.added_at, k.id, c.ord",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SearchHit {
                chunk: Chunk {
                    doc: row.get(0)?,
                    section: row.get(1)?,
                    content: row.get(2)?,
                },
                score: 0.0,
            })
        })?;
        Ok(Some(rows.collect::<std::result::Result<_, _>>()?))
    }

    /// The chunks that best answer `question` by its words alone. See [`ChunkStore::search_with`].
    pub fn search(&self, question: &str, limit: usize, max_chars: usize) -> Result<Vec<SearchHit>> {
        self.search_with(question, None, limit, max_chars)
    }

    /// The chunks that best answer `question`, best first: at most `limit` of them, and at most
    /// `max_chars` characters of text in all (the last one is cut to fit, so a long chunk never
    /// crowds out the others).
    ///
    /// Two rankings are made and fused with reciprocal rank fusion: one by words (BM25, with the
    /// relevance gate of [`relevance`]) and, when `semantic` is given, one by the meaning of the
    /// question (see [`crate::semantic`]). A chunk both rankings like comes first. A question that
    /// neither finds anything for gets nothing, rather than the least bad paragraph.
    pub fn search_with(
        &self,
        question: &str,
        semantic: Option<&SemanticQuery<'_>>,
        limit: usize,
        max_chars: usize,
    ) -> Result<Vec<SearchHit>> {
        let by_words = self.by_words(question)?;
        let by_meaning = match semantic {
            Some(query) => self.by_meaning(query)?,
            None => Vec::new(),
        };
        // The best chunk of each ranking is always sent. Fusion alone can lose it: a chunk that
        // only one ranking likes scores half of one that both like, even when the other ranking
        // is only noise (the meaning search likes generic "FAQ" paragraphs for many questions).
        let guaranteed = [by_words.first(), by_meaning.first()].map(|top| top.map(|(id, _)| *id));

        // Reciprocal rank fusion: each ranking gives 1 / (K + rank) to its chunks.
        let mut fused: Vec<(i64, Chunk, f32)> = Vec::new();
        for ranking in [by_words, by_meaning] {
            for (rank, (id, chunk)) in ranking.into_iter().enumerate() {
                let share = 1.0 / (RRF_K + rank as f32 + 1.0);
                match fused.iter_mut().find(|(seen, _, _)| *seen == id) {
                    Some(entry) => entry.2 += share,
                    None => fused.push((id, chunk, share)),
                }
            }
        }
        // Stable: chunks with the same score keep the order of the word ranking.
        fused.sort_by(|a, b| b.2.total_cmp(&a.2));
        // Guaranteed chunks first, then the rest by score, up to the limit and the budget.
        let (first, rest): (Vec<_>, Vec<_>) = fused
            .into_iter()
            .partition(|(id, _, _)| guaranteed.contains(&Some(*id)));

        let mut hits = Vec::new();
        let mut budget = max_chars;
        for (_, mut chunk, score) in first.into_iter().chain(rest) {
            if hits.len() == limit || budget == 0 {
                break;
            }
            if chunk.content.chars().count() > budget {
                chunk.content = chunk.content.chars().take(budget).collect();
            }
            budget -= chunk.content.chars().count();
            hits.push(SearchHit { chunk, score });
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(hits)
    }

    /// The chunks that share the question's words, most relevant first, without the ones that
    /// merely share a word by chance.
    fn by_words(&self, question: &str) -> Result<Vec<(i64, Chunk)>> {
        let terms = query::terms(question);
        let pairs = query::pairs(question);
        let Some(fts) = query::fts_query(&terms, &pairs) else {
            return Ok(Vec::new());
        };

        let candidates: Vec<(i64, Chunk)> = {
            let conn = self.db.lock()?;
            // bm25() is lower for a better match, and the section counts double.
            let mut stmt = conn.prepare(
                "SELECT c.id, k.name, c.section, c.content
                 FROM chunks_fts
                 JOIN chunks c ON c.id = chunks_fts.rowid
                 JOIN knowledge k ON k.id = c.doc_id
                 WHERE chunks_fts MATCH ?1
                 ORDER BY bm25(chunks_fts, 1.0, 2.0)
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![fts, CANDIDATES], |row| {
                Ok((
                    row.get(0)?,
                    Chunk {
                        doc: row.get(1)?,
                        section: row.get(2)?,
                        content: row.get(3)?,
                    },
                ))
            })?;
            rows.collect::<std::result::Result<_, _>>()?
        };

        let stats = self.statistics(&terms, &pairs)?;

        // Keep the chunks that really bear on the question, then put the ones with the most
        // matching word pairs first. BM25 order decides between equals (the sort is stable).
        let mut relevant: Vec<(Strength, i64, Chunk)> = candidates
            .into_iter()
            .filter_map(|(id, chunk)| {
                relevance(&terms, &stats, &chunk).map(|strength| (strength, id, chunk))
            })
            .collect();
        relevant.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(relevant
            .into_iter()
            .map(|(_, id, chunk)| (id, chunk))
            .collect())
    }
}

/// What the documents say about how common the words of a question are.
struct Statistics {
    /// Terms that name something specific and few chunks hold.
    rare: Vec<String>,
    /// Weight of each neighbouring pair of the question; higher for pairs fewer chunks hold.
    pair_weight: HashMap<query::Pair, usize>,
}

/// How well a chunk answers a question: the weight of the word pairs it shares with the
/// question first (a pair in the section heading counts double), then how many words match.
/// Compared in that order.
type Strength = (usize, usize);

/// `None` when the chunk does not bear on the question. A chunk does when it holds a pair of
/// neighbouring words of the question, a name that few chunks hold, or every word of a short
/// question. A word or two matching by chance is not enough: in Vietnamese, "chạy bộ" would
/// otherwise match any chunk with "chạy" and "đồng bộ".
fn relevance(terms: &[String], stats: &Statistics, chunk: &Chunk) -> Option<Strength> {
    let section = chunk.section.as_deref().unwrap_or("");
    let in_section = query::word_set(section);
    let in_body = query::word_set(&chunk.content);
    let has = |term: &String| in_section.contains(term) || in_body.contains(term);

    let matched = terms.iter().filter(|t| has(t)).count();
    let weight = |pair: &query::Pair| stats.pair_weight.get(pair).copied().unwrap_or(0);
    let in_heading: HashSet<_> = query::pairs_in(section);
    let in_text: HashSet<_> = query::pairs_in(&chunk.content);
    let shared: Vec<&query::Pair> = stats
        .pair_weight
        .keys()
        .filter(|p| in_heading.contains(*p) || in_text.contains(*p))
        .collect();
    let pair_score: usize = shared
        .iter()
        .map(|p| weight(p) * (1 + usize::from(in_heading.contains(*p))))
        .sum();

    let every_word = terms.len() <= 3 && matched == terms.len();
    if shared.is_empty() && !every_word && !stats.rare.iter().any(has) {
        return None;
    }
    Some((pair_score, matched))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store over an in-memory database that already holds `ids` as knowledge documents.
    fn store_with(ids: &[(&str, &str)]) -> ChunkStore {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.lock().unwrap();
            for (n, (id, name)) in ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
                     VALUES (?1, ?2, 'text', 1, ?3)",
                    params![id, name, n as i64],
                )
                .unwrap();
            }
        }
        ChunkStore::new(db)
    }

    fn chunk(doc: &str, section: Option<&str>, content: &str) -> Chunk {
        Chunk {
            doc: doc.into(),
            section: section.map(Into::into),
            content: content.into(),
        }
    }

    fn cv() -> Vec<Chunk> {
        vec![
            chunk(
                "CV.md",
                Some("Kinh nghiệm"),
                "Ba năm viết Rust và Tauri cho ứng dụng desktop.",
            ),
            chunk(
                "CV.md",
                Some("Học vấn"),
                "Cử nhân Công nghệ thông tin, Đại học Bách khoa.",
            ),
            chunk("CV.md", None, "Sở thích: chạy bộ và đọc sách."),
        ]
    }

    #[test]
    fn a_question_finds_the_chunk_about_its_topic() {
        let s = store_with(&[("d1", "CV.md")]);
        s.replace("d1", &cv()).unwrap();
        let hits = s
            .search("Bạn có kinh nghiệm với Rust không?", 4, 4000)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk.doc, "CV.md");
        assert_eq!(hits[0].chunk.section.as_deref(), Some("Kinh nghiệm"));
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn vietnamese_accents_are_kept_apart() {
        let s = store_with(&[("d1", "a.txt")]);
        s.replace(
            "d1",
            &[
                chunk("a.txt", None, "Mã nguồn mở"),
                chunk("a.txt", None, "Má nấu cơm"),
            ],
        )
        .unwrap();
        let hits = s.search("mã", 4, 4000).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].chunk.content.starts_with("Mã"));
    }

    #[test]
    fn an_unrelated_question_finds_nothing() {
        let s = store_with(&[("d1", "CV.md")]);
        s.replace("d1", &cv()).unwrap();
        assert!(s
            .search("Kubernetes cluster autoscaling policy", 4, 4000)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn one_coincidental_word_in_a_long_question_is_not_enough() {
        let s = store_with(&[("d1", "CV.md")]);
        s.replace("d1", &cv()).unwrap();
        // "sách" is in the hobbies chunk, but nothing else of the question is.
        assert!(s
            .search(
                "Quy trình triển khai kubernetes và giám sát sách lược",
                4,
                4000
            )
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_question_of_only_filler_finds_nothing() {
        let s = store_with(&[("d1", "CV.md")]);
        s.replace("d1", &cv()).unwrap();
        assert!(s.search("Bạn có không?", 4, 4000).unwrap().is_empty());
        assert!(s.search("", 4, 4000).unwrap().is_empty());
    }

    #[test]
    fn one_rare_technology_name_is_enough_in_a_long_question() {
        let s = store_with(&[("d1", "CV.md")]);
        let mut chunks = vec![chunk(
            "CV.md",
            Some("Kỹ năng"),
            "Thành thạo PostgreSQL và Redis.",
        )];
        chunks.extend((0..4).map(|i| chunk("CV.md", None, &format!("Mục khác số {i}."))));
        s.replace("d1", &chunks).unwrap();
        let hits = s
            .search("Bạn có kinh nghiệm làm việc với PostgreSQL không?", 4, 4000)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk.section.as_deref(), Some("Kỹ năng"));
    }

    #[test]
    fn an_everyday_english_word_held_by_most_chunks_is_not_enough() {
        let s = store_with(&[("d1", "CV.md")]);
        let chunks: Vec<_> = (0..8)
            .map(|i| chunk("CV.md", None, &format!("Notes on project number {i}.")))
            .collect();
        s.replace("d1", &chunks).unwrap();
        assert!(s
            .search("Describe your last project deployment pipeline", 4, 4000)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_section_heading_counts_as_text() {
        let s = store_with(&[("d1", "CV.md")]);
        s.replace("d1", &cv()).unwrap();
        let hits = s.search("học vấn", 4, 4000).unwrap();
        assert_eq!(hits[0].chunk.section.as_deref(), Some("Học vấn"));
    }

    #[test]
    fn limit_and_character_budget_are_respected() {
        let s = store_with(&[("d1", "a.txt")]);
        let many: Vec<_> = (0..10)
            .map(|i| {
                chunk(
                    "a.txt",
                    None,
                    &format!("rust chunk number {i} {}", "x".repeat(100)),
                )
            })
            .collect();
        s.replace("d1", &many).unwrap();
        assert_eq!(s.search("rust", 3, 4000).unwrap().len(), 3);
        let hits = s.search("rust", 10, 250).unwrap();
        assert_eq!(
            hits.iter()
                .map(|h| h.chunk.content.chars().count())
                .sum::<usize>(),
            250
        );
    }

    #[test]
    fn reindexing_replaces_the_old_chunks() {
        let s = store_with(&[("d1", "a.txt")]);
        s.replace("d1", &[chunk("a.txt", None, "alpha beta")])
            .unwrap();
        s.replace("d1", &[chunk("a.txt", None, "gamma delta")])
            .unwrap();
        assert!(s.search("alpha", 4, 4000).unwrap().is_empty());
        assert_eq!(s.search("gamma", 4, 4000).unwrap().len(), 1);
        assert_eq!(s.statuses().unwrap()[0].chunks, 1);
    }

    #[test]
    fn deleting_a_document_removes_its_chunks_from_search() {
        let s = store_with(&[("d1", "a.txt"), ("d2", "b.txt")]);
        s.replace("d1", &[chunk("a.txt", None, "alpha beta")])
            .unwrap();
        s.replace("d2", &[chunk("b.txt", None, "alpha gamma")])
            .unwrap();
        s.db.lock()
            .unwrap()
            .execute("DELETE FROM knowledge WHERE id = 'd1'", [])
            .unwrap();
        let hits = s.search("alpha", 4, 4000).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk.doc, "b.txt");
        assert_eq!(s.statuses().unwrap().len(), 1);
    }

    #[test]
    fn unindexed_lists_documents_not_looked_at_yet() {
        let s = store_with(&[("d1", "a.txt"), ("d2", "b.txt"), ("d3", "c.txt")]);
        assert_eq!(s.unindexed().unwrap(), ["d1", "d2", "d3"]);
        s.replace("d1", &[chunk("a.txt", None, "alpha")]).unwrap();
        s.record_failure("d2", "Tệp không có chữ").unwrap();
        assert_eq!(s.unindexed().unwrap(), ["d3"]);
        let failed = s
            .statuses()
            .unwrap()
            .into_iter()
            .find(|x| x.doc_id == "d2")
            .unwrap();
        assert_eq!(failed.chunks, 0);
        assert_eq!(failed.error.as_deref(), Some("Tệp không có chữ"));
    }

    #[test]
    fn a_failure_drops_the_chunks_the_document_had() {
        let s = store_with(&[("d1", "a.txt")]);
        s.replace("d1", &[chunk("a.txt", None, "alpha beta")])
            .unwrap();
        s.record_failure("d1", "hỏng").unwrap();
        assert!(s.search("alpha", 4, 4000).unwrap().is_empty());
        assert!(!s.has_chunks().unwrap());
    }

    #[test]
    fn a_small_set_of_documents_can_be_taken_whole_in_order() {
        let s = store_with(&[("d1", "CV.md"), ("d2", "JD.md")]);
        s.replace(
            "d1",
            &[
                chunk("CV.md", Some("Học vấn"), "Đại học Bách khoa"),
                chunk("CV.md", Some("Kinh nghiệm"), "Ba năm Rust"),
            ],
        )
        .unwrap();
        s.replace("d2", &[chunk("JD.md", None, "Cần Tauri")])
            .unwrap();
        let all = s.everything_if_small(1000).unwrap().unwrap();
        let texts: Vec<_> = all.iter().map(|h| h.chunk.content.as_str()).collect();
        assert_eq!(texts, ["Đại học Bách khoa", "Ba năm Rust", "Cần Tauri"]);
        assert_eq!(all[0].chunk.section.as_deref(), Some("Học vấn"));
        assert_eq!(all[2].chunk.doc, "JD.md");
    }

    #[test]
    fn too_much_text_or_none_is_not_a_small_set() {
        let s = store_with(&[("d1", "a.txt")]);
        assert!(
            s.everything_if_small(1000).unwrap().is_none(),
            "no documents yet"
        );
        s.replace("d1", &[chunk("a.txt", None, &"x".repeat(100))])
            .unwrap();
        assert!(s.everything_if_small(99).unwrap().is_none());
        assert!(s.everything_if_small(100).unwrap().is_some());
    }

    #[test]
    fn the_size_counts_characters_not_bytes() {
        let s = store_with(&[("d1", "a.txt")]);
        // 10 characters, 30 bytes.
        s.replace("d1", &[chunk("a.txt", None, "ệệệệệệệệệệ")])
            .unwrap();
        assert!(s.everything_if_small(10).unwrap().is_some());
    }

    #[test]
    fn has_chunks_tells_whether_there_is_anything_to_search() {
        let s = store_with(&[("d1", "a.txt")]);
        assert!(!s.has_chunks().unwrap());
        s.replace("d1", &[chunk("a.txt", None, "alpha")]).unwrap();
        assert!(s.has_chunks().unwrap());
    }

    #[test]
    fn a_document_missing_from_the_knowledge_table_is_refused() {
        let s = store_with(&[]);
        assert!(s.replace("ghost", &[chunk("g", None, "alpha")]).is_err());
        assert!(!s.has_chunks().unwrap());
    }

    #[test]
    fn at_most_the_per_document_limit_is_kept() {
        let s = store_with(&[("d1", "a.txt")]);
        let many: Vec<_> = (0..MAX_CHUNKS_PER_DOC + 5)
            .map(|i| chunk("a.txt", None, &format!("alpha {i}")))
            .collect();
        assert_eq!(s.replace("d1", &many).unwrap(), MAX_CHUNKS_PER_DOC);
    }
}
