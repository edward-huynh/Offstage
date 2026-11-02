//! Retrieval for the user's own documents: read their text, cut it into chunks, index the
//! chunks in the local database and find the ones that answer a question.
//!
//! Search has two halves. BM25 over an SQLite FTS5 index (`store`) catches names, tools and terms
//! the way they are written and needs no model. Embedding vectors (`semantic`) catch paraphrases
//! and other languages, and exist once the embedding model is downloaded. When both are there
//! their rankings are fused with reciprocal rank fusion. Either half works alone.

pub mod chunk;
pub mod extract;
pub mod ingest;
pub mod query;
pub mod semantic;
pub mod store;

use serde::{Deserialize, Serialize};

pub use semantic::SemanticQuery;
pub use store::{ChunkStore, IndexStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chunk {
    /// Name of the document the chunk comes from, as the user sees it.
    pub doc: String,
    pub section: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk: Chunk,
    /// Higher is more relevant. Only meaningful to compare hits of the same search.
    pub score: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum RagError {
    #[error("database error: {0}")]
    Db(#[from] offstage_core::db::DbError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The message is written for the user and stored as the reason a document is not indexed.
    #[error("{0}")]
    Extract(#[from] extract::ExtractError),
    #[error("{0}")]
    Embed(#[from] offstage_embed::EmbedError),
    #[error("cannot read the document: {0}")]
    Io(#[from] std::io::Error),
}
