//! The assistant: when the other side of the call asks something, stream a short answer to the
//! overlay. The work happens in `offstage-llm` and `offstage_core::question`; this connects it
//! to Tauri: it watches the transcripts, calls the provider and emits the answer as events.
//!
//! It is off until the user turns it on, because it sends what the other side says to the
//! chosen provider. The key stays in the OS keychain, and every request is made from here,
//! never from the webview.

use std::sync::{Arc, Mutex};

use offstage_core::{question::is_question, AppEvent, Speaker};
use offstage_llm::{
    config::{self, AssistantConfig},
    history::History,
    openai_compat::{endpoint_is_local, OpenAiCompat},
    prompt::{build_messages, Turn},
    secrets, LlmError, LlmProvider,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use crate::{listening::emit, storage};

const CONFIG_KEY: &str = "assistant_config";

#[derive(Default)]
pub struct Assistant {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    history: History,
    /// The answer being written, so a new question can stop it.
    current: Option<CancellationToken>,
    /// The provider for the saved choice. Building it reads the keychain, which can ask the
    /// user, so it is kept until the choice or the key changes.
    provider: Option<Arc<OpenAiCompat>>,
}

fn load_config(app: &AppHandle) -> Result<AssistantConfig, String> {
    Ok(storage::get(app, CONFIG_KEY)?.unwrap_or_default())
}

fn with_state<T>(app: &AppHandle, f: impl FnOnce(&mut State) -> T) -> Option<T> {
    let assistant = app.try_state::<Assistant>()?;
    let mut state = assistant.state.lock().ok()?;
    Some(f(&mut state))
}

/// The provider for the saved choice, with its key read from the keychain.
fn provider(app: &AppHandle) -> Result<Arc<OpenAiCompat>, String> {
    if let Some(Some(cached)) = with_state(app, |state| state.provider.clone()) {
        return Ok(cached);
    }
    let mut chosen = load_config(app)?.provider().map_err(|e| e.to_string())?;
    let key = match chosen.key_ref.as_deref() {
        Some(key_ref) => secrets::resolve(key_ref).map_err(|e| e.to_string())?,
        None => None,
    };
    // A custom endpoint can do without a key (a local Ollama).
    if chosen.id == config::CUSTOM_ID && key.is_none() {
        chosen.key_ref = None;
    }
    let built = Arc::new(OpenAiCompat::new(chosen, key).map_err(|e| e.to_string())?);
    with_state(app, |state| state.provider = Some(Arc::clone(&built)));
    Ok(built)
}

/// Forgets the cached provider after the choice or a key changed.
fn invalidate(app: &AppHandle) {
    with_state(app, |state| state.provider = None);
}

/// Stops the answer being written, if any, and starts a new conversation for the provider.
/// Used when listening ends.
pub fn cancel_current(app: &AppHandle) {
    with_state(app, |state| {
        if let Some(token) = state.current.take() {
            token.cancel();
        }
        state.history.clear();
        if let Some(provider) = &state.provider {
            provider.new_session();
        }
    });
}

/// Sees every event the pipeline emits. A question from the other side starts an answer.
/// Called on a pipeline thread, so it only does the quick part and hands the rest to a task.
pub fn on_event(app: &AppHandle, event: &AppEvent) {
    let AppEvent::Transcript {
        speaker,
        text,
        is_final: true,
    } = event
    else {
        return;
    };
    let remote = *speaker == Speaker::Remote;
    let history = with_state(app, |state| {
        state.history.push(!remote, text);
        state.history.snapshot()
    });
    if let (true, Some(history)) = (remote && is_question(text), history) {
        answer(app, text.clone(), history);
    }
}

fn answer(app: &AppHandle, question: String, history: Vec<Turn>) {
    match load_config(app) {
        Ok(config) if config.enabled => {}
        Ok(_) => return,
        Err(message) => return emit(app, &AppEvent::Error { message }),
    }
    let provider = match provider(app) {
        Ok(provider) => provider,
        Err(message) => return emit(app, &AppEvent::Error { message }),
    };

    // A new question replaces the answer being written.
    let cancel = CancellationToken::new();
    with_state(app, |state| {
        if let Some(previous) = state.current.replace(cancel.clone()) {
            previous.cancel();
        }
    });
    emit(
        app,
        &AppEvent::QuestionDetected {
            text: question.clone(),
        },
    );

    let messages = build_messages(&question, &history);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut on_token = {
            let app = app.clone();
            let cancel = cancel.clone();
            move |token: &str| {
                // A chunk can still be in flight when the answer is replaced.
                if !cancel.is_cancelled() {
                    emit(
                        &app,
                        &AppEvent::AnswerToken {
                            token: token.to_owned(),
                        },
                    );
                }
            }
        };
        match provider
            .stream_chat_cancellable(&messages, &mut on_token, &cancel)
            .await
        {
            Ok(_) if cancel.is_cancelled() => {}
            Ok(_) => emit(
                &app,
                &AppEvent::AnswerDone {
                    sources: Vec::new(),
                },
            ),
            Err(LlmError::Cancelled) => {}
            Err(e) => emit(
                &app,
                &AppEvent::Error {
                    message: e.to_string(),
                },
            ),
        }
    });
}

/// What the UI needs to show whether the assistant will answer.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AssistantStatus {
    pub enabled: bool,
    pub provider_id: String,
    pub model: Option<String>,
    pub has_key: bool,
    /// The provider needs a key. A local endpoint may not.
    pub needs_key: bool,
    /// What is said in the call is sent to another machine.
    pub cloud: bool,
    /// Enabled, with a model and a usable key: the next question will be answered.
    pub ready: bool,
}

fn status_of(config: &AssistantConfig, has_key: bool) -> AssistantStatus {
    let provider = config.provider().ok();
    let model = provider.as_ref().and_then(|p| p.model.clone());
    let local = provider
        .as_ref()
        .is_some_and(|p| endpoint_is_local(&p.base_url));
    // A custom endpoint on this machine usually needs no key; the others always do.
    let needs_key = provider
        .as_ref()
        .is_some_and(|p| p.key_ref.is_some() && !(p.id == config::CUSTOM_ID && local));
    AssistantStatus {
        enabled: config.enabled,
        provider_id: config.provider_id.clone(),
        has_key,
        needs_key,
        cloud: provider.is_some() && !local,
        ready: config.enabled && provider.is_some() && model.is_some() && (has_key || !needs_key),
        model,
    }
}

/// Runs blocking work (the keychain, which may wait for the user) off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

fn key_ref(provider_id: &str) -> Result<String, String> {
    config::key_ref_for(provider_id).ok_or_else(|| format!("unknown provider {provider_id:?}"))
}

#[tauri::command]
pub fn assistant_get_config(app: AppHandle) -> Result<AssistantConfig, String> {
    load_config(&app)
}

/// Saves the choice. Turning the assistant on needs a provider that can be reached at an
/// allowed address, so a bad one is refused here instead of failing at the first question.
#[tauri::command]
pub fn assistant_set_config(
    app: AppHandle,
    config: AssistantConfig,
) -> Result<AssistantConfig, String> {
    if config.enabled {
        let chosen = config.provider().map_err(|e| e.to_string())?;
        OpenAiCompat::new(chosen, None).map_err(|e| e.to_string())?;
    }
    storage::set(&app, CONFIG_KEY, &config)?;
    invalidate(&app);
    if !config.enabled {
        cancel_current(&app);
    }
    Ok(config)
}

#[tauri::command]
pub async fn assistant_status(app: AppHandle) -> Result<AssistantStatus, String> {
    blocking(move || {
        let config = load_config(&app)?;
        let has_key = match config::key_ref_for(&config.provider_id) {
            Some(reference) => secrets::exists(&reference).map_err(|e| e.to_string())?,
            None => false,
        };
        Ok(status_of(&config, has_key))
    })
    .await
}

/// Saves the key of a provider in the keychain. There is no command that reads it back.
#[tauri::command]
pub async fn assistant_set_key(
    app: AppHandle,
    provider_id: String,
    key: String,
) -> Result<(), String> {
    blocking(move || {
        secrets::store(&key_ref(&provider_id)?, &key).map_err(|e| e.to_string())?;
        invalidate(&app);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn assistant_clear_key(app: AppHandle, provider_id: String) -> Result<(), String> {
    blocking(move || {
        secrets::remove(&key_ref(&provider_id)?).map_err(|e| e.to_string())?;
        invalidate(&app);
        Ok(())
    })
    .await
}

/// Checks the saved provider, key and address with a call to `/models`.
#[tauri::command]
pub async fn assistant_test_connection(app: AppHandle) -> Result<(), String> {
    let handle = app.clone();
    let provider = blocking(move || provider(&handle)).await?;
    provider.test_connection().await.map_err(|e| e.to_string())
}

/// The models the saved provider offers, for the dropdown.
#[tauri::command]
pub async fn assistant_list_models(app: AppHandle) -> Result<Vec<String>, String> {
    let handle = app.clone();
    let provider = blocking(move || provider(&handle)).await?;
    provider.list_models().await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(provider_id: &str, model: Option<&str>, enabled: bool) -> AssistantConfig {
        AssistantConfig {
            enabled,
            provider_id: provider_id.into(),
            model: model.map(str::to_owned),
            custom_base_url: None,
        }
    }

    #[test]
    fn a_cloud_provider_needs_a_key_and_a_model_to_be_ready() {
        let c = config("openai", Some("gpt-4o-mini"), true);
        let s = status_of(&c, true);
        assert!(s.ready && s.cloud && s.needs_key && s.has_key);

        assert!(!status_of(&c, false).ready, "no key");
        assert!(
            !status_of(&config("openai", None, true), true).ready,
            "no model"
        );
        assert!(
            !status_of(&config("openai", Some("m"), false), true).ready,
            "off"
        );
    }

    #[test]
    fn the_status_says_when_nothing_leaves_the_machine() {
        let local = AssistantConfig {
            custom_base_url: Some("http://localhost:11434/v1".into()),
            ..config(config::CUSTOM_ID, Some("llama3"), true)
        };
        let s = status_of(&local, false);
        assert!(!s.cloud);
        assert!(!s.needs_key);
        assert!(s.ready, "a local endpoint needs no key");
    }

    #[test]
    fn a_remote_custom_endpoint_is_cloud_and_needs_its_key() {
        let remote = AssistantConfig {
            custom_base_url: Some("https://openrouter.ai/api/v1".into()),
            ..config(config::CUSTOM_ID, Some("m"), true)
        };
        let s = status_of(&remote, false);
        assert!(s.cloud && s.needs_key && !s.ready);
        assert!(status_of(&remote, true).ready);
    }

    #[test]
    fn an_unusable_choice_is_not_ready_and_not_called_cloud() {
        let unknown = status_of(&config("nope", Some("m"), true), true);
        assert!(!unknown.ready && !unknown.cloud);
        let no_address = status_of(&config(config::CUSTOM_ID, Some("m"), true), true);
        assert!(!no_address.ready);
    }
}
