//! What the user chose for the assistant: on or off, which provider and which model.
//!
//! This is the saved choice. The provider itself comes from the built-in presets or, for a
//! custom endpoint, from the address the user typed. The key is never part of it: only the
//! reference of [`key_ref_for`] is known here.

use serde::{Deserialize, Serialize};

use crate::{presets, ApiStyle, LlmError, ProviderConfig};

/// Id of the provider whose address the user types.
pub const CUSTOM_ID: &str = "custom";

/// Keychain entry of the custom endpoint's key.
const CUSTOM_KEY_REF: &str = "keychain:custom";

/// Missing fields fall back to the defaults, so a value saved by an older version still loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssistantConfig {
    /// Off until the user turns it on: turning it on sends what the other side says to the provider.
    pub enabled: bool,
    pub provider_id: String,
    pub model: Option<String>,
    /// Address of the endpoint when `provider_id` is [`CUSTOM_ID`].
    pub custom_base_url: Option<String>,
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider_id: "openai".into(),
            model: None,
            custom_base_url: None,
        }
    }
}

impl AssistantConfig {
    /// The provider this choice points to.
    pub fn provider(&self) -> Result<ProviderConfig, LlmError> {
        let model = self
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_owned);

        if self.provider_id == CUSTOM_ID {
            let base_url = self
                .custom_base_url
                .as_deref()
                .map(str::trim)
                .filter(|url| !url.is_empty())
                .ok_or_else(|| LlmError::Request("enter the endpoint address".into()))?;
            return Ok(ProviderConfig {
                id: CUSTOM_ID.into(),
                label: "Endpoint tùy chỉnh".into(),
                base_url: base_url.to_owned(),
                api_style: ApiStyle::ChatCompletions,
                model,
                key_ref: Some(CUSTOM_KEY_REF.into()),
            });
        }

        let mut provider = presets::builtin()
            .into_iter()
            .find(|p| p.id == self.provider_id)
            .ok_or_else(|| LlmError::Request(format!("unknown provider {:?}", self.provider_id)))?;
        provider.model = model;
        Ok(provider)
    }
}

/// Where the key of `provider_id` is kept, or `None` for an unknown provider.
pub fn key_ref_for(provider_id: &str) -> Option<String> {
    if provider_id == CUSTOM_ID {
        return Some(CUSTOM_KEY_REF.into());
    }
    presets::builtin()
        .into_iter()
        .find(|p| p.id == provider_id)
        .and_then(|p| p.key_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(provider_id: &str) -> AssistantConfig {
        AssistantConfig {
            provider_id: provider_id.into(),
            ..AssistantConfig::default()
        }
    }

    #[test]
    fn the_assistant_is_off_by_default() {
        let c = AssistantConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.provider_id, "openai");
        assert_eq!(c.model, None);
    }

    #[test]
    fn an_older_or_empty_saved_value_still_loads() {
        let c: AssistantConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c, AssistantConfig::default());
        let c: AssistantConfig = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(c.enabled);
        assert_eq!(c.provider_id, "openai");
    }

    #[test]
    fn a_preset_comes_with_the_chosen_model() {
        let provider = AssistantConfig {
            model: Some("  gpt-4o-mini ".into()),
            ..config("opencode-go")
        }
        .provider()
        .unwrap();
        assert_eq!(provider.id, "opencode-go");
        assert_eq!(provider.model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(provider.base_url, "https://opencode.ai/zen/go/v1");
    }

    #[test]
    fn a_blank_model_means_none_was_chosen() {
        let provider = AssistantConfig {
            model: Some("   ".into()),
            ..config("openai")
        }
        .provider()
        .unwrap();
        assert_eq!(provider.model, None);
    }

    #[test]
    fn an_unknown_provider_is_an_error() {
        assert!(config("nope").provider().is_err());
    }

    #[test]
    fn a_custom_endpoint_needs_its_address() {
        assert!(config(CUSTOM_ID).provider().is_err());
        let blank = AssistantConfig {
            custom_base_url: Some("  ".into()),
            ..config(CUSTOM_ID)
        };
        assert!(blank.provider().is_err());

        let provider = AssistantConfig {
            custom_base_url: Some(" http://localhost:11434/v1 ".into()),
            model: Some("llama3".into()),
            ..config(CUSTOM_ID)
        }
        .provider()
        .unwrap();
        assert_eq!(provider.base_url, "http://localhost:11434/v1");
        assert_eq!(provider.model.as_deref(), Some("llama3"));
        assert_eq!(provider.key_ref.as_deref(), Some("keychain:custom"));
    }

    #[test]
    fn key_references_follow_the_provider() {
        assert_eq!(key_ref_for("openai").as_deref(), Some("keychain:openai"));
        // Zen and Go are two tiers of one account, so they share one key.
        assert_eq!(key_ref_for("opencode-zen"), key_ref_for("opencode-go"));
        assert_eq!(key_ref_for(CUSTOM_ID).as_deref(), Some("keychain:custom"));
        assert_eq!(key_ref_for("nope"), None);
    }
}
