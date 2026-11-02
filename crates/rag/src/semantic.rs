//! Search by meaning: every chunk has an embedding vector, and a question is matched to the
//! chunks whose vectors point the same way.
//!
//! Vectors are stored as blobs next to the chunks and compared in code. A few thousand chunks
//! of 384 numbers take a few milliseconds, which is why no vector extension is loaded.

use std::collections::HashMap;

use offstage_embed::{dot, TextEmbedder};
use rusqlite::params;

use crate::{store::ChunkStore, Chunk, RagError};

type Result<T> = std::result::Result<T, RagError>;

/// Chunks read, embedded and stored per round, so a long document shows progress and a closing
/// app loses little.
const BATCH: usize = 32;

/// Chunks the meaning search puts forward. Fusion and the budget trim them further.
const NEAREST: usize = 8;

/// Least cosine similarity for a chunk to count as related to a question. multilingual-e5
/// scores unrelated text, even in another language, around 0.75 to 0.82, not 0.
///
/// Measured on the sample documents (`tests/semantic.rs`): questions the documents answer score
/// 0.83 to 0.92 at best, but so do off-topic questions about the same field (0.87 for "explain
/// Terraform" against Node.js notes), so no line separates them. The line only removes what is
/// plainly unrelated; telling "the documents do not say" from "they do" is left to the prompt,
/// which makes the model answer "Không có trong tài liệu." in that case.
pub const MIN_SIMILARITY: f32 = 0.83;

/// How far below the best match a chunk may score and still be put forward. The best match is
/// the answer and the rest are its neighbours; this keeps a weak tail out.
const MAX_GAP: f32 = 0.03;

/// A question as a vector, with the name of the model that made it. Only vectors of the same
/// model are compared with it.
pub struct SemanticQuery<'a> {
    pub model: &'a str,
    /// Unit length, from [`TextEmbedder::embed_query`].
    pub vector: &'a [f32],
}

/// The bytes stored for a vector.
pub fn encode(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// The vector in stored bytes. A length that is not a multiple of 4 gives `None`.
pub fn decode(bytes: &[u8]) -> Option<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

impl ChunkStore {
    /// Chunks with no vector from `model` yet, as (id, text), oldest first.
    pub fn pending_vectors(&self, model: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let conn = self.db.lock()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.content FROM chunks c
             LEFT JOIN chunk_vectors v ON v.chunk_id = c.id AND v.model = ?1
             WHERE v.chunk_id IS NULL
             ORDER BY c.id
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![model, limit as i64], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// How many chunks still wait for a vector from `model`.
    pub fn pending_vector_count(&self, model: &str) -> Result<usize> {
        let count: i64 = self.db.lock()?.query_row(
            "SELECT COUNT(*) FROM chunks c
             LEFT JOIN chunk_vectors v ON v.chunk_id = c.id AND v.model = ?1
             WHERE v.chunk_id IS NULL",
            [model],
            |row| row.get(0),
        )?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    /// Stores vectors made by `model`, replacing any the chunks had.
    pub fn put_vectors(&self, model: &str, vectors: &[(i64, Vec<f32>)]) -> Result<()> {
        let mut conn = self.db.lock()?;
        let tx = conn.transaction()?;
        {
            let mut put = tx.prepare(
                "INSERT INTO chunk_vectors (chunk_id, model, vec) VALUES (?1, ?2, ?3)
                 ON CONFLICT (chunk_id) DO UPDATE SET model = excluded.model, vec = excluded.vec",
            )?;
            for (id, vector) in vectors {
                put.execute(params![id, model, encode(vector)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Makes vectors for every chunk that has none from `model`, in rounds, and returns how many.
    /// `on_round` gets the count so far after each round. Slow with many chunks: run it off the
    /// UI thread. A chunk added while it runs is picked up by the next call.
    pub fn embed_pending(
        &self,
        embedder: &dyn TextEmbedder,
        model: &str,
        mut on_round: impl FnMut(usize),
    ) -> Result<usize> {
        let mut done = 0;
        loop {
            let batch = self.pending_vectors(model, BATCH)?;
            if batch.is_empty() {
                return Ok(done);
            }
            let texts: Vec<String> = batch.iter().map(|(_, text)| text.clone()).collect();
            let vectors = embedder.embed_passages(&texts)?;
            let stored: Vec<(i64, Vec<f32>)> =
                batch.iter().map(|(id, _)| *id).zip(vectors).collect();
            self.put_vectors(model, &stored)?;
            done += stored.len();
            on_round(done);
        }
    }

    /// Every chunk that has a vector from the question's model, with its cosine similarity to the
    /// question, most similar first. Mostly for measuring: search uses [`ChunkStore::search_with`].
    pub fn similarities(&self, query: &SemanticQuery<'_>) -> Result<Vec<(i64, f32)>> {
        let conn = self.db.lock()?;
        let mut stmt = conn.prepare(
            "SELECT v.chunk_id, v.vec FROM chunk_vectors v
             JOIN chunks c ON c.id = v.chunk_id
             JOIN knowledge k ON k.id = c.doc_id
             WHERE v.model = ?1 AND (?2 IS NULL OR k.workspace_id = ?2)",
        )?;
        let rows = stmt.query_map(params![query.model, self.workspace.as_deref()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut scored = Vec::new();
        for row in rows {
            let (id, bytes) = row?;
            if let Some(vector) = decode(&bytes).filter(|v| v.len() == query.vector.len()) {
                scored.push((id, dot(query.vector, &vector)));
            }
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(scored)
    }

    /// The chunks nearest in meaning to the question, closest first, as (id, chunk). Chunks
    /// without a vector from the question's model are not considered. Chunks that are not
    /// similar enough ([`MIN_SIMILARITY`]) or fall far behind the best one are left out.
    pub(crate) fn by_meaning(&self, query: &SemanticQuery<'_>) -> Result<Vec<(i64, Chunk)>> {
        let mut scored = self.similarities(query)?;
        let Some(&(_, best)) = scored.first() else {
            return Ok(Vec::new());
        };
        scored.retain(|&(_, score)| score >= MIN_SIMILARITY && score >= best - MAX_GAP);
        scored.truncate(NEAREST);
        let ids: Vec<i64> = scored.iter().map(|(id, _)| *id).collect();
        let mut chunks = self.chunks_by_id(&ids)?;
        Ok(ids
            .into_iter()
            .filter_map(|id| chunks.remove(&id).map(|chunk| (id, chunk)))
            .collect())
    }

    fn chunks_by_id(&self, ids: &[i64]) -> Result<HashMap<i64, Chunk>> {
        let conn = self.db.lock()?;
        let mut stmt = conn.prepare(
            "SELECT c.section, c.content, k.name FROM chunks c
             JOIN knowledge k ON k.id = c.doc_id WHERE c.id = ?1",
        )?;
        let mut found = HashMap::new();
        for &id in ids {
            let chunk = stmt.query_row([id], |row| {
                Ok(Chunk {
                    section: row.get(0)?,
                    content: row.get(1)?,
                    doc: row.get(2)?,
                })
            });
            match chunk {
                Ok(chunk) => {
                    found.insert(id, chunk);
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use offstage_core::db::Db;
    use offstage_embed::{normalize, EmbedError};

    /// An embedder for tests: a text's vector is made from which marker words it contains, so
    /// "meaning" is fully known. Words: rust, sport, cook.
    struct Fake;

    fn vector_of(text: &str) -> Vec<f32> {
        let t = text.to_lowercase();
        let has = |w: &str| if t.contains(w) { 1.0 } else { 0.0 };
        normalize(vec![has("rust"), has("sport"), has("cook"), 0.01])
    }

    impl TextEmbedder for Fake {
        fn dims(&self) -> usize {
            4
        }
        fn embed_passages(
            &self,
            texts: &[String],
        ) -> std::result::Result<Vec<Vec<f32>>, EmbedError> {
            Ok(texts.iter().map(|t| vector_of(t)).collect())
        }
        fn embed_query(&self, text: &str) -> std::result::Result<Vec<f32>, EmbedError> {
            Ok(vector_of(text))
        }
    }

    fn setup(chunks: &[&str]) -> ChunkStore {
        let db = Db::open_in_memory().unwrap();
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
                 VALUES ('d1', 'CV.md', 'markdown', 1, 1)",
                [],
            )
            .unwrap();
        let store = ChunkStore::new(db);
        let chunks: Vec<Chunk> = chunks
            .iter()
            .map(|c| Chunk {
                doc: "CV.md".into(),
                section: None,
                content: (*c).into(),
            })
            .collect();
        store.replace("d1", &chunks).unwrap();
        store
    }

    #[test]
    fn vectors_survive_the_trip_through_bytes() {
        let v = vec![0.5, -1.25, 3.0e-3];
        assert_eq!(decode(&encode(&v)).unwrap(), v);
        assert!(decode(&[1, 2, 3]).is_none());
    }

    #[test]
    fn pending_chunks_are_embedded_once_and_in_rounds() {
        let texts: Vec<String> = (0..70).map(|i| format!("rust note {i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let s = setup(&refs);
        let mut rounds = Vec::new();
        assert_eq!(
            s.embed_pending(&Fake, "fake", |n| rounds.push(n)).unwrap(),
            70
        );
        assert_eq!(rounds, [32, 64, 70]);
        assert!(s.pending_vectors("fake", 10).unwrap().is_empty());
        assert_eq!(s.pending_vector_count("fake").unwrap(), 0);
        assert_eq!(s.pending_vector_count("other").unwrap(), 70);
        // Nothing left, so nothing is redone.
        assert_eq!(s.embed_pending(&Fake, "fake", |_| {}).unwrap(), 0);
        // Another model starts from nothing.
        assert_eq!(s.pending_vectors("other", 100).unwrap().len(), 70);
    }

    #[test]
    fn a_question_finds_the_chunk_by_meaning_without_sharing_a_word() {
        let s = setup(&[
            "Viết rust cho desktop",
            "Chơi sport mỗi tuần",
            "Hay cook món Việt",
        ]);
        s.embed_pending(&Fake, "fake", |_| {}).unwrap();
        let q = Fake.embed_query("Bạn dùng rust không").unwrap();
        let hits = s
            .search_with(
                "zzz",
                Some(&SemanticQuery {
                    model: "fake",
                    vector: &q,
                }),
                4,
                4000,
            )
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].chunk.content.contains("rust"));
    }

    #[test]
    fn vectors_of_another_model_are_ignored() {
        let s = setup(&["Viết rust cho desktop"]);
        s.embed_pending(&Fake, "old-model", |_| {}).unwrap();
        let q = Fake.embed_query("rust").unwrap();
        let hits = s
            .search_with(
                "zzz",
                Some(&SemanticQuery {
                    model: "new-model",
                    vector: &q,
                }),
                4,
                4000,
            )
            .unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn an_unrelated_question_finds_nothing_by_meaning() {
        let s = setup(&["Viết rust cho desktop", "Chơi sport mỗi tuần"]);
        s.embed_pending(&Fake, "fake", |_| {}).unwrap();
        // Points the way of none of the marker words.
        let q = normalize(vec![0.0, 0.0, 0.0, 1.0]);
        let hits = s
            .search_with(
                "zzz",
                Some(&SemanticQuery {
                    model: "fake",
                    vector: &q,
                }),
                4,
                4000,
            )
            .unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn a_chunk_both_rankings_like_comes_first() {
        let s = setup(&[
            "Chơi sport mỗi tuần và cook",
            "Viết rust cho desktop với Tauri",
            "Tauri là khung ứng dụng",
        ]);
        s.embed_pending(&Fake, "fake", |_| {}).unwrap();
        // By words "Tauri" ranks the two last chunks; by meaning only the rust one.
        let q = Fake.embed_query("rust").unwrap();
        let hits = s
            .search_with(
                "Bạn biết Tauri không?",
                Some(&SemanticQuery {
                    model: "fake",
                    vector: &q,
                }),
                4,
                4000,
            )
            .unwrap();
        assert_eq!(hits[0].chunk.content, "Viết rust cho desktop với Tauri");
        assert_eq!(hits.len(), 2);
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn the_best_chunk_by_words_is_sent_even_when_meaning_crowds_it_out() {
        let mut chunks = vec!["Tauri Tauri".to_owned()];
        chunks.extend((0..5).map(|i| format!("Viết rust với Tauri cho ứng dụng desktop số {i}")));
        let refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        let s = setup(&refs);
        s.embed_pending(&Fake, "fake", |_| {}).unwrap();
        let q = Fake.embed_query("rust").unwrap();
        let semantic = SemanticQuery {
            model: "fake",
            vector: &q,
        };
        let hits = s
            .search_with("Bạn biết Tauri không?", Some(&semantic), 4, 4000)
            .unwrap();
        assert_eq!(hits.len(), 4);
        // Found by words only, and shortest, so first by BM25, but behind chunks both like.
        assert!(
            hits.iter().any(|h| h.chunk.content == "Tauri Tauri"),
            "{hits:?}"
        );
        // Shown best first.
        assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
    }

    #[test]
    fn the_best_chunk_by_meaning_is_sent_even_when_words_crowd_it_out() {
        let mut chunks: Vec<String> = (0..6).map(|i| format!("Tauri ghi chú {i}")).collect();
        chunks.push("Viết rust".to_owned());
        let refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        let s = setup(&refs);
        s.embed_pending(&Fake, "fake", |_| {}).unwrap();
        let q = Fake.embed_query("rust").unwrap();
        let semantic = SemanticQuery {
            model: "fake",
            vector: &q,
        };
        let hits = s
            .search_with("Bạn biết Tauri không?", Some(&semantic), 3, 4000)
            .unwrap();
        assert!(
            hits.iter().any(|h| h.chunk.content == "Viết rust"),
            "{hits:?}"
        );
    }

    #[test]
    fn searching_by_meaning_inside_a_workspace_never_sees_another_ones_documents() {
        let s = setup(&["Viết rust cho desktop"]);
        s.db.lock()
            .unwrap()
            .execute(
                "INSERT INTO knowledge (id, name, kind, size_bytes, added_at, workspace_id)
                 VALUES ('d2', 'JD.md', 'markdown', 1, 2, 'w2')",
                [],
            )
            .unwrap();
        s.replace(
            "d2",
            &[Chunk {
                doc: "JD.md".into(),
                section: None,
                content: "Cần rust cho máy chủ".into(),
            }],
        )
        .unwrap();
        s.embed_pending(&Fake, "fake", |_| {}).unwrap();
        let q = Fake.embed_query("rust").unwrap();
        let semantic = SemanticQuery {
            model: "fake",
            vector: &q,
        };
        let docs = |scope: &ChunkStore| -> Vec<String> {
            scope
                .search_with("zzz", Some(&semantic), 4, 4000)
                .unwrap()
                .into_iter()
                .map(|h| h.chunk.doc)
                .collect()
        };
        assert_eq!(docs(&s).len(), 2);
        assert_eq!(docs(&s.in_workspace("default")), ["CV.md"]);
        assert_eq!(docs(&s.in_workspace("w2")), ["JD.md"]);
    }

    #[test]
    fn replacing_a_documents_chunks_drops_their_vectors() {
        let s = setup(&["Viết rust cho desktop"]);
        s.embed_pending(&Fake, "fake", |_| {}).unwrap();
        s.replace(
            "d1",
            &[Chunk {
                doc: "CV.md".into(),
                section: None,
                content: "Chơi sport".into(),
            }],
        )
        .unwrap();
        assert_eq!(s.pending_vectors("fake", 10).unwrap().len(), 1);
        let q = Fake.embed_query("rust").unwrap();
        assert!(s
            .search_with(
                "zzz",
                Some(&SemanticQuery {
                    model: "fake",
                    vector: &q
                }),
                4,
                4000
            )
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_vector_of_the_wrong_length_is_skipped_not_a_crash() {
        let s = setup(&["Viết rust cho desktop"]);
        let (id, _) = s.pending_vectors("fake", 1).unwrap().remove(0);
        s.put_vectors("fake", &[(id, vec![1.0, 0.0])]).unwrap();
        let q = Fake.embed_query("rust").unwrap();
        assert!(s
            .search_with(
                "zzz",
                Some(&SemanticQuery {
                    model: "fake",
                    vector: &q
                }),
                4,
                4000
            )
            .unwrap()
            .is_empty());
    }
}
