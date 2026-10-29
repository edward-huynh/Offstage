use crate::ModelSpec;

/// whisper.cpp `small` q5_1 (multilingual), the default STT model.
///
/// Verified against the Hugging Face API for `ggerganov/whisper.cpp` (LFS oid = SHA-256, size)
/// and the `x-linked-etag` header of the resolve URL. The URL is pinned to a commit so the
/// hash cannot drift if the repo is updated; the model card declares `license: mit`.
pub fn whisper_small_q5() -> ModelSpec {
    ModelSpec {
        id: "whisper-small-q5_1".into(),
        file_name: "ggml-small-q5_1.bin".into(),
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/\
              5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small-q5_1.bin"
            .into(),
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb".into(),
        size: 190_085_487,
        license: "MIT".into(),
        license_url: Some("https://huggingface.co/ggerganov/whisper.cpp".into()),
    }
}

pub fn all() -> Vec<ModelSpec> {
    vec![whisper_small_q5()]
}

pub fn find(id: &str) -> Option<ModelSpec> {
    all().into_iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_well_formed() {
        for m in all() {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}", m.id);
            assert_eq!(m.sha256, m.sha256.to_lowercase(), "{}", m.id);
            assert!(m.url.starts_with("https://"), "{}", m.id);
            assert!(!m.file_name.contains(['/', '\\']), "{}", m.id);
            assert!(m.size > 0 && !m.license.is_empty(), "{}", m.id);
        }
    }

    #[test]
    fn preset_ids_are_unique_and_findable() {
        let ids: Vec<_> = all().into_iter().map(|m| m.id).collect();
        let mut dedup = ids.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(ids.len(), dedup.len());
        assert!(find("whisper-small-q5_1").is_some());
        assert!(find("nope").is_none());
    }
}
