//! Built-in provider presets. Users can also add a custom OpenAI-compatible endpoint.

use crate::{ApiStyle, ProviderConfig};

pub fn builtin() -> Vec<ProviderConfig> {
    vec![
        preset(
            "openai",
            "OpenAI",
            "https://api.openai.com/v1",
            "keychain:openai",
        ),
        preset(
            "opencode-zen",
            "OpenCode Zen",
            "https://opencode.ai/zen/v1",
            "keychain:opencode",
        ),
        preset(
            "opencode-go",
            "OpenCode Go",
            "https://opencode.ai/zen/go/v1",
            "keychain:opencode",
        ),
    ]
}

fn preset(id: &str, label: &str, base_url: &str, key_ref: &str) -> ProviderConfig {
    ProviderConfig {
        id: id.into(),
        label: label.into(),
        base_url: base_url.into(),
        api_style: ApiStyle::ChatCompletions,
        model: None,
        key_ref: Some(key_ref.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_unique_ids_and_https_urls() {
        let p = builtin();
        let mut ids: Vec<_> = p.iter().map(|c| c.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), p.len());
        assert!(p.iter().all(|c| c.base_url.starts_with("https://")));
    }

    #[test]
    fn opencode_tiers_share_one_key() {
        let p = builtin();
        let key = |id: &str| p.iter().find(|c| c.id == id).unwrap().key_ref.clone();
        assert_eq!(key("opencode-zen"), key("opencode-go"));
    }
}
