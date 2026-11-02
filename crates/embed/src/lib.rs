//! Text embeddings for document search: a text becomes a vector so that texts with a similar
//! meaning, in any of the model's languages, end up close together. The search side compares the
//! vector of a question with the vectors of the stored paragraphs.
//!
//! [`TextEmbedder`] is what the rest of the app depends on. [`onnx::E5Embedder`] (feature `onnx`)
//! runs multilingual-e5-small on ONNX Runtime, on the CPU, entirely on the user's machine.

#[cfg(feature = "onnx")]
pub mod onnx;

/// Size of the vectors the default model makes.
pub const E5_SMALL_DIMS: usize = 384;

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    /// The model files are missing, damaged or not usable on this machine.
    #[error("cannot load the embedding model: {0}")]
    Load(String),
    #[error("embedding failed: {0}")]
    Run(String),
}

pub trait TextEmbedder: Send + Sync {
    /// Length of every vector this embedder returns.
    fn dims(&self) -> usize;

    /// Vectors for paragraphs of a document, in the order given. Length 1 (unit length), so a
    /// dot product is the cosine similarity.
    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;

    /// The vector of a question, to compare with passage vectors. Not interchangeable with
    /// [`TextEmbedder::embed_passages`]: e5 models are trained with a different prefix for each.
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError>;
}

/// How e5 models want a question written before embedding.
pub fn query_text(text: &str) -> String {
    format!("query: {}", text.trim())
}

/// How e5 models want a paragraph written before embedding.
pub fn passage_text(text: &str) -> String {
    format!("passage: {}", text.trim())
}

/// Scales a vector to length 1. A zero vector is returned as it is.
pub fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

/// Dot product. For two unit vectors this is their cosine similarity. Vectors of different
/// lengths compare over the shorter one, which only happens with a corrupt index.
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e5_prefixes_differ_for_questions_and_paragraphs() {
        assert_eq!(query_text("  Rust là gì? "), "query: Rust là gì?");
        assert_eq!(passage_text("Ba năm Rust.\n"), "passage: Ba năm Rust.");
    }

    #[test]
    fn normalize_makes_unit_length_and_leaves_zero_alone() {
        let v = normalize(vec![3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        assert!((dot(&v, &v) - 1.0).abs() < 1e-6);
        assert_eq!(normalize(vec![0.0, 0.0]), vec![0.0, 0.0]);
    }

    #[test]
    fn dot_of_unit_vectors_is_the_cosine() {
        let a = normalize(vec![1.0, 0.0]);
        let b = normalize(vec![1.0, 1.0]);
        assert!((dot(&a, &b) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert_eq!(dot(&a, &[0.0, 1.0]), 0.0);
    }
}
