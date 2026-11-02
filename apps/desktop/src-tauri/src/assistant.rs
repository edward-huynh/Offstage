//! The assistant: when the other side of the call asks something, stream a short answer to the
//! overlay. The work happens in `offstage-llm` and `offstage_core::question`; this connects it
//! to Tauri: it watches the transcripts, calls the provider and emits the answer as events.
//!
//! It is off until the user turns it on, because it sends what the other side says to the
//! chosen provider. The key stays in the OS keychain, and every request is made from here,
//! never from the webview.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use offstage_core::{question::is_question, AppEvent, Speaker};
use offstage_embed::{onnx::E5Embedder, TextEmbedder};
use offstage_llm::{
    config::{self, AssistantConfig},
    history::History,
    openai_compat::{endpoint_is_local, OpenAiCompat},
    prompt::{build_messages_with_documents, Excerpt, Turn},
    router::{ChatBackend, Policy, Router, Served},
    secrets, LlmError, LlmProvider,
};
use offstage_rag::{ChunkStore, SearchHit, SemanticQuery};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use crate::{embedding, listening::emit, local_llm, rag::Rag, storage};

pub(crate) const CONFIG_KEY: &str = "assistant_config";

/// Most excerpts of the user's documents sent with a question.
const EXCERPTS: usize = 4;
/// Most characters of excerpts in all, about 1,000 tokens. More makes the first token slower
/// (`docs/LLM-BENCH.md`) without making the short answer better.
const EXCERPT_CHARS: usize = 4_000;
/// The same for the model on this machine, which reads the prompt at 400 tokens per second:
/// 400 to 600 tokens of documents keep the first token near two seconds (`docs/LLM-BENCH.md`).
const LOCAL_EXCERPTS: usize = 3;
const LOCAL_EXCERPT_CHARS: usize = 2_000;
/// When searching finds nothing but all the documents together are no longer than this, they are
/// all sent. A CV and a job description are a few thousand characters, and a question such as
/// "where did you study?" finds nothing in them by search (`docs/LLM-BENCH.md`, `offstage-rag`).
const WHOLE_CHARS: usize = 8_000;
/// The same for the model on this machine, which pays for every token it reads.
const LOCAL_WHOLE_CHARS: usize = 3_000;

/// How long a provider may take to write the first token before the model on this machine
/// takes over.
const FIRST_TOKEN_TIMEOUT: Duration = Duration::from_secs(5);

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

/// Whether the assistant is turned on, so the app knows to keep what it needs ready.
pub fn is_enabled(app: &AppHandle) -> bool {
    load_config(app).is_ok_and(|config| config.enabled)
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
        language,
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
        answer(app, text.clone(), language.clone(), history);
    }
}

fn answer(app: &AppHandle, question: String, language: Option<String>, history: Vec<Turn>) {
    let config = match load_config(app) {
        Ok(config) if config.enabled => config,
        Ok(_) => return,
        Err(message) => return emit(app, &AppEvent::Error { message }),
    };
    let local = local_llm::source(app, &config);
    // Local-only never builds the provider: no key is read and nothing can be sent.
    let provider: Option<Arc<dyn ChatBackend>> = if config.local_only {
        None
    } else {
        match provider(app) {
            Ok(provider) => Some(provider),
            // A provider that cannot be built (no key yet) is not fatal when the model on this
            // machine can answer in its place.
            Err(_) if config.fallback_to_local && local.is_some() => None,
            Err(message) => return emit(app, &AppEvent::Error { message }),
        }
    };
    if provider.is_none() && local.is_none() {
        return emit(
            app,
            &AppEvent::Error {
                message: "Chưa tải model chạy trên máy. Mở Cài đặt, mục Trợ lý AI, để tải về."
                    .to_owned(),
            },
        );
    }
    let router = Router::new(
        provider,
        local,
        Policy {
            local_only: config.local_only,
            fallback: config.fallback_to_local,
            first_token_timeout: FIRST_TOKEN_TIMEOUT,
        },
    );
    let budget = if config.local_only {
        Budget {
            excerpts: LOCAL_EXCERPTS,
            chars: LOCAL_EXCERPT_CHARS,
            whole_chars: LOCAL_WHOLE_CHARS,
        }
    } else {
        Budget {
            excerpts: EXCERPTS,
            chars: EXCERPT_CHARS,
            whole_chars: WHOLE_CHARS,
        }
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

    let index = app.try_state::<Rag>().map(|rag| rag.0.clone());
    let embedder = embedding::current(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let excerpts = retrieve(index, embedder, question.clone(), budget).await;
        // A newer question can arrive while the documents are searched.
        if cancel.is_cancelled() {
            return;
        }
        let messages =
            build_messages_with_documents(&question, &history, language.as_deref(), &excerpts);
        let sources = sources_of(&excerpts);
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
        match router.answer(&messages, &mut on_token, &cancel).await {
            Ok(_) if cancel.is_cancelled() => {}
            Ok(reply) => emit(
                &app,
                &AppEvent::AnswerDone {
                    sources,
                    fell_back: reply.fell_back,
                    local: reply.served == Served::Local,
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

/// The excerpts of the user's documents that bear on `question`, best first. Empty when there
/// are no documents, nothing relevant, or the search failed: the answer is then given without
/// them, because a broken index must not silence the assistant.
async fn retrieve(
    index: Option<ChunkStore>,
    embedder: Option<Arc<E5Embedder>>,
    question: String,
    budget: Budget,
) -> Vec<Excerpt> {
    let Some(index) = index else {
        return Vec::new();
    };
    let found = tauri::async_runtime::spawn_blocking(move || {
        // The question as a vector, when the model is loaded. If it fails the search still has
        // the words.
        let vector = embedder.and_then(|e| match e.embed_query(&question) {
            Ok(vector) => Some(vector),
            Err(e) => {
                eprintln!("question not embedded: {e}");
                None
            }
        });
        let semantic = vector.as_deref().map(|vector| SemanticQuery {
            model: embedding::MODEL_ID,
            vector,
        });
        gather(&index, &question, semantic.as_ref(), budget)
    })
    .await;
    match found {
        Ok(Ok(hits)) => excerpts_of(hits),
        Ok(Err(e)) => {
            eprintln!("documents not searched: {e}");
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

/// How much of the user's documents goes with a question.
#[derive(Debug, Clone, Copy)]
struct Budget {
    /// Most excerpts found by searching.
    excerpts: usize,
    /// Most characters of them in all.
    chars: usize,
    /// All the documents are sent when searching finds nothing and they are this short.
    whole_chars: usize,
}

/// The chunks for `question`: what search finds, or, when it finds nothing and the documents are
/// short, all of them.
fn gather(
    index: &ChunkStore,
    question: &str,
    semantic: Option<&SemanticQuery<'_>>,
    budget: Budget,
) -> Result<Vec<SearchHit>, offstage_rag::RagError> {
    let found = index.search_with(question, semantic, budget.excerpts, budget.chars)?;
    if !found.is_empty() {
        return Ok(found);
    }
    Ok(index
        .everything_if_small(budget.whole_chars)?
        .unwrap_or_default())
}

fn excerpts_of(hits: Vec<SearchHit>) -> Vec<Excerpt> {
    hits.into_iter()
        .map(|hit| Excerpt {
            source: hit.chunk.doc,
            section: hit.chunk.section,
            text: hit.chunk.content,
        })
        .collect()
}

/// What is shown under the answer: each document and section used, once, best first.
fn sources_of(excerpts: &[Excerpt]) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    for label in excerpts.iter().map(Excerpt::label) {
        if !sources.contains(&label) {
            sources.push(label);
        }
    }
    sources
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
    /// Enabled, with something that can answer: a provider with a model and a usable key, or the
    /// model on this machine.
    pub ready: bool,
    /// Answers are written on this machine only, and nothing is sent over the network.
    pub local_only: bool,
    /// The model on this machine chosen in the settings is downloaded.
    pub local_installed: bool,
    /// When the provider fails, the model on this machine answers.
    pub fallback: bool,
}

fn status_of(config: &AssistantConfig, has_key: bool, local_installed: bool) -> AssistantStatus {
    let provider = config.provider().ok();
    let model = provider.as_ref().and_then(|p| p.model.clone());
    let local = provider
        .as_ref()
        .is_some_and(|p| endpoint_is_local(&p.base_url));
    // A custom endpoint on this machine usually needs no key; the others always do.
    let needs_key = provider
        .as_ref()
        .is_some_and(|p| p.key_ref.is_some() && !(p.id == config::CUSTOM_ID && local));
    let provider_ready = provider.is_some() && model.is_some() && (has_key || !needs_key);
    // The model on this machine answers instead when it is asked to, or when the provider cannot.
    let ready = config.enabled
        && if config.local_only {
            local_installed
        } else {
            provider_ready || (config.fallback_to_local && local_installed)
        };
    AssistantStatus {
        enabled: config.enabled,
        provider_id: config.provider_id.clone(),
        has_key,
        needs_key,
        cloud: !config.local_only && provider.is_some() && !local,
        ready,
        local_only: config.local_only,
        local_installed,
        fallback: config.fallback_to_local,
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
    // Local-only has no provider to check.
    if config.enabled && !config.local_only {
        let chosen = config.provider().map_err(|e| e.to_string())?;
        OpenAiCompat::new(chosen, None).map_err(|e| e.to_string())?;
    }
    if config.enabled && !local_llm::installed(&app, &config.local_model) && config.local_only {
        return Err("Tải model chạy trên máy trước khi bật chế độ chỉ chạy trên máy.".to_owned());
    }
    let previous = load_config(&app)?;
    storage::set(&app, CONFIG_KEY, &config)?;
    invalidate(&app);
    // Another model, or a switch to the provider, frees what the model on this machine holds.
    if previous.local_model != config.local_model || previous.local_only != config.local_only {
        local_llm::release(&app);
    }
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
        let local_installed = local_llm::installed(&app, &config.local_model);
        Ok(status_of(&config, has_key, local_installed))
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
            ..AssistantConfig::default()
        }
    }

    fn hit(doc: &str, section: Option<&str>, content: &str) -> SearchHit {
        SearchHit {
            chunk: offstage_rag::Chunk {
                doc: doc.into(),
                section: section.map(Into::into),
                content: content.into(),
            },
            score: 1.0,
        }
    }

    fn index_with(texts: &[(&str, &str)]) -> ChunkStore {
        let db = offstage_core::db::Db::open_in_memory().unwrap();
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO knowledge (id, name, kind, size_bytes, added_at)
                 VALUES ('d1', 'CV.md', 'markdown', 1, 1)",
                [],
            )
            .unwrap();
        let index = ChunkStore::new(db);
        let chunks: Vec<_> = texts
            .iter()
            .map(|(section, text)| offstage_rag::Chunk {
                doc: "CV.md".into(),
                section: Some((*section).into()),
                content: (*text).into(),
            })
            .collect();
        index.replace("d1", &chunks).unwrap();
        index
    }

    const BUDGET: Budget = Budget {
        excerpts: 4,
        chars: 4_000,
        whole_chars: 500,
    };

    #[test]
    fn when_search_finds_the_paragraph_only_that_is_sent() {
        let index = index_with(&[
            ("Học vấn", "Đại học Bách khoa Hà Nội"),
            ("Sở thích", "Chạy bộ và đọc sách"),
        ]);
        let hits = gather(&index, "Bạn tốt nghiệp đại học nào?", None, BUDGET).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk.section.as_deref(), Some("Học vấn"));
    }

    #[test]
    fn when_search_finds_nothing_short_documents_are_sent_whole() {
        let index = index_with(&[
            ("Học vấn", "Đại học Bách khoa Hà Nội"),
            ("Sở thích", "Chạy bộ và đọc sách"),
        ]);
        // English question, Vietnamese CV: no word in common.
        let hits = gather(&index, "Where did you study?", None, BUDGET).unwrap();
        let sections: Vec<_> = hits.iter().map(|h| h.chunk.section.as_deref()).collect();
        assert_eq!(sections, [Some("Học vấn"), Some("Sở thích")]);
    }

    #[test]
    fn when_search_finds_nothing_long_documents_are_not_dumped() {
        let index = index_with(&[("Dài", &"chữ ".repeat(300))]);
        assert!(gather(&index, "Where did you study?", None, BUDGET)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn without_documents_nothing_is_sent() {
        let index = index_with(&[]);
        assert!(gather(&index, "Where did you study?", None, BUDGET)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn hits_become_excerpts_in_the_same_order() {
        let excerpts = excerpts_of(vec![
            hit("CV.pdf", Some("Kinh nghiệm"), "Ba năm Rust."),
            hit("JD.md", None, "Cần Tauri."),
        ]);
        assert_eq!(excerpts.len(), 2);
        assert_eq!(excerpts[0].source, "CV.pdf");
        assert_eq!(excerpts[0].section.as_deref(), Some("Kinh nghiệm"));
        assert_eq!(excerpts[1].text, "Cần Tauri.");
    }

    #[test]
    fn sources_name_each_document_and_section_once() {
        let excerpts = excerpts_of(vec![
            hit("CV.pdf", Some("Kinh nghiệm"), "a"),
            hit("CV.pdf", Some("Kinh nghiệm"), "b"),
            hit("CV.pdf", Some("Học vấn"), "c"),
            hit("JD.md", None, "d"),
        ]);
        assert_eq!(
            sources_of(&excerpts),
            ["CV.pdf › Kinh nghiệm", "CV.pdf › Học vấn", "JD.md"]
        );
        assert!(sources_of(&[]).is_empty());
    }

    #[test]
    fn a_cloud_provider_needs_a_key_and_a_model_to_be_ready() {
        let c = config("openai", Some("gpt-4o-mini"), true);
        let s = status_of(&c, true, false);
        assert!(s.ready && s.cloud && s.needs_key && s.has_key);

        assert!(!status_of(&c, false, false).ready, "no key");
        assert!(
            !status_of(&config("openai", None, true), true, false).ready,
            "no model"
        );
        assert!(
            !status_of(&config("openai", Some("m"), false), true, false).ready,
            "off"
        );
    }

    #[test]
    fn the_status_says_when_nothing_leaves_the_machine() {
        let local = AssistantConfig {
            custom_base_url: Some("http://localhost:11434/v1".into()),
            ..config(config::CUSTOM_ID, Some("llama3"), true)
        };
        let s = status_of(&local, false, false);
        assert!(!s.cloud);
        assert!(!s.needs_key);
        assert!(s.ready, "a local endpoint needs no key");
    }

    #[test]
    fn local_only_is_ready_with_the_model_installed_and_never_cloud() {
        let c = AssistantConfig {
            local_only: true,
            ..config("openai", Some("gpt-4o-mini"), true)
        };
        let s = status_of(&c, true, true);
        assert!(s.ready && !s.cloud && s.local_only && s.local_installed);
        assert!(!status_of(&c, true, false).ready, "model not downloaded");
        assert!(
            status_of(
                &AssistantConfig {
                    local_only: true,
                    ..config("openai", None, true)
                },
                false,
                true
            )
            .ready,
            "no provider needed"
        );
    }

    #[test]
    fn a_provider_that_is_not_ready_is_covered_by_the_local_model_when_falling_back() {
        let c = config("openai", Some("gpt-4o-mini"), true);
        // No key: not ready alone, ready with a model to fall back to.
        assert!(!status_of(&c, false, false).ready);
        assert!(status_of(&c, false, true).ready);
        let no_fallback = AssistantConfig {
            fallback_to_local: false,
            ..c
        };
        assert!(!status_of(&no_fallback, false, true).ready);
    }

    #[test]
    fn a_remote_custom_endpoint_is_cloud_and_needs_its_key() {
        let remote = AssistantConfig {
            custom_base_url: Some("https://openrouter.ai/api/v1".into()),
            ..config(config::CUSTOM_ID, Some("m"), true)
        };
        let s = status_of(&remote, false, false);
        assert!(s.cloud && s.needs_key && !s.ready);
        assert!(status_of(&remote, true, false).ready);
    }

    #[test]
    fn an_unusable_choice_is_not_ready_and_not_called_cloud() {
        let unknown = status_of(&config("nope", Some("m"), true), true, false);
        assert!(!unknown.ready && !unknown.cloud);
        let no_address = status_of(&config(config::CUSTOM_ID, Some("m"), true), true, false);
        assert!(!no_address.ready);
    }
}
