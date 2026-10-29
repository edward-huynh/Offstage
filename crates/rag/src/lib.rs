//! Retrieval: ingest documents, chunk, embed, hybrid search (sqlite-vec + FTS5).
//! Phase 4 adds embeddings and storage; chunking lives here from day one.

pub mod chunk;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    pub doc: String,
    pub section: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk: Chunk,
    pub score: f32,
}
