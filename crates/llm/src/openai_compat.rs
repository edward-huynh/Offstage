//! Client for any OpenAI-compatible `chat/completions` endpoint: OpenAI, OpenCode Zen and Go,
//! OpenRouter, Groq, Ollama, LM Studio...
//!
//! Answers are streamed as server-sent events. A stream can be cancelled at any point, which
//! is how a newly detected question replaces the answer being written.

use std::{
    process,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, RequestBuilder, Response, Url};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{ChatMessage, LlmError, LlmProvider, ProviderConfig};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest silence tolerated before the first byte and between two chunks of a stream.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// How much of an unreadable error body is kept for the message.
const MAX_ERROR_CHARS: usize = 200;

/// Names the app to the provider, as OpenCode asks of third-party clients, instead of a
/// generic HTTP library.
const USER_AGENT: &str = concat!("offstage/", env!("CARGO_PKG_VERSION"));
/// OpenCode Go rejects requests without it. It tells the gateway which requests belong to one
/// conversation so it can route them together and reuse the prompt cache. Other providers
/// ignore it.
const SESSION_HEADER: &str = "x-opencode-session";

/// A fresh id for one conversation. It only has to differ from the others, so it is built
/// from the clock, the process and a counter rather than from a random number generator.
pub fn new_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ses_{nanos:x}{:x}{count:x}", process::id())
}

pub struct OpenAiCompat {
    config: ProviderConfig,
    api_key: Option<String>,
    client: Client,
    local: bool,
    /// Sent with every request. Replaced by [`OpenAiCompat::new_session`].
    session: Mutex<String>,
}

impl OpenAiCompat {
    /// `api_key` is the resolved key (see [`crate::secrets::resolve`]). A provider whose config
    /// has a `key_ref` fails with [`LlmError::MissingKey`] when it is `None`. An endpoint that
    /// does not use a key (a local Ollama) has no `key_ref`.
    ///
    /// Plain `http://` is only accepted for this machine: anything else would send what the
    /// other side of the call said across the network in the clear.
    pub fn new(config: ProviderConfig, api_key: Option<String>) -> Result<Self, LlmError> {
        let local = check_endpoint(&config.base_url)?;
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(request_error)?;
        Ok(Self {
            config,
            api_key: api_key.filter(|key| !key.trim().is_empty()),
            client,
            local,
            session: Mutex::new(new_session_id()),
        })
    }

    /// Starts a new conversation: later requests carry a new session id. Call it when a call ends.
    pub fn new_session(&self) {
        if let Ok(mut session) = self.session.lock() {
            *session = new_session_id();
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.config.base_url.trim_end_matches('/'))
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        let session = self
            .session
            .lock()
            .map(|session| session.clone())
            .unwrap_or_default();
        let request = if session.is_empty() {
            request
        } else {
            request.header(SESSION_HEADER, session)
        };
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }

    fn require_key(&self) -> Result<(), LlmError> {
        if self.config.key_ref.is_some() && self.api_key.is_none() {
            return Err(LlmError::MissingKey(self.config.id.clone()));
        }
        Ok(())
    }

    /// Like [`LlmProvider::stream_chat`], and stops with [`LlmError::Cancelled`] as soon as
    /// `cancel` is triggered, closing the connection.
    pub async fn stream_chat_cancellable(
        &self,
        messages: &[ChatMessage],
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<String, LlmError> {
        self.require_key()?;
        let model = self
            .config
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .ok_or_else(|| LlmError::Request("no model chosen".into()))?;
        let body = json!({ "model": model, "messages": messages, "stream": true });

        let send = self
            .authorize(self.client.post(self.url("chat/completions")))
            .json(&body)
            .send();
        let response = tokio::select! {
            () = cancel.cancelled() => return Err(LlmError::Cancelled),
            sent = send => sent.map_err(request_error)?,
        };
        let mut response = check(response).await?;

        let mut parser = SseParser::default();
        let mut answer = String::new();
        loop {
            let chunk = tokio::select! {
                () = cancel.cancelled() => return Err(LlmError::Cancelled),
                next = tokio::time::timeout(IDLE_TIMEOUT, response.chunk()) => match next {
                    Ok(chunk) => chunk.map_err(request_error)?,
                    Err(_) => {
                        return Err(LlmError::Request("the provider stopped responding".into()))
                    }
                },
            };
            let Some(chunk) = chunk else {
                // The server closed the stream without `[DONE]`. What arrived is the answer.
                return Ok(answer);
            };
            for event in parser.push(&chunk) {
                match event {
                    SseEvent::Done => return Ok(answer),
                    SseEvent::Data(data) => {
                        if let Some(token) = delta_token(&data)? {
                            on_token(&token);
                            answer.push_str(&token);
                        }
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompat {
    fn id(&self) -> &str {
        &self.config.id
    }

    /// True for an endpoint on this machine, where nothing leaves it.
    fn is_local(&self) -> bool {
        self.local
    }

    async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        self.require_key()?;
        let response = self
            .authorize(self.client.get(self.url("models")))
            .send()
            .await
            .map_err(request_error)?;
        let value: Value = check(response).await?.json().await.map_err(request_error)?;
        Ok(model_ids(&value))
    }

    async fn test_connection(&self) -> Result<(), LlmError> {
        self.list_models().await.map(|_| ())
    }

    async fn stream_chat(
        &self,
        messages: &[ChatMessage],
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> Result<String, LlmError> {
        self.stream_chat_cancellable(messages, on_token, &CancellationToken::new())
            .await
    }
}

/// True when `base_url` points at this machine, so nothing said in the call leaves it.
/// An address that cannot be read is treated as remote.
pub fn endpoint_is_local(base_url: &str) -> bool {
    check_endpoint(base_url).unwrap_or(false)
}

/// Accepts `https://` anywhere and `http://` only for this machine. Returns whether the
/// endpoint is on this machine.
fn check_endpoint(base_url: &str) -> Result<bool, LlmError> {
    let url = Url::parse(base_url)
        .map_err(|e| LlmError::Request(format!("invalid endpoint {base_url:?}: {e}")))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => Ok(local),
        "http" if local => Ok(true),
        _ => Err(LlmError::Request(
            "the endpoint must use https (http is only allowed for localhost)".into(),
        )),
    }
}

fn request_error(e: reqwest::Error) -> LlmError {
    // `without_url` keeps a query string, which could hold a token, out of the message.
    LlmError::Request(e.without_url().to_string())
}

/// Passes a successful response through and turns any other into an error.
async fn check(response: Response) -> Result<Response, LlmError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Err(error_from_body(status, &body))
}

/// Reads an error body. OpenAI answers `{"error":{"message":…,"type":…}}`; OpenCode wraps the
/// same object as `{"type":"error","error":{"type":"AuthError","message":…}}`. Anything else
/// falls back to the start of the body.
pub(crate) fn error_from_body(status: u16, body: &str) -> LlmError {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    error_from_value(Some(status), &value, body)
}

fn error_from_value(status: Option<u16>, value: &Value, raw: &str) -> LlmError {
    let error = value.get("error");
    let kind = error
        .and_then(|e| e.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = error
        .and_then(|e| e.get("message").or(Some(e)))
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| raw.trim().chars().take(MAX_ERROR_CHARS).collect());

    let denied = matches!(status, Some(401 | 403)) || kind.to_ascii_lowercase().contains("auth");
    if denied {
        return LlmError::Auth(message);
    }
    match status {
        Some(code) => LlmError::Request(format!("HTTP {code}: {message}")),
        None => LlmError::Request(message),
    }
}

fn model_ids(value: &Value) -> Vec<String> {
    let mut ids: Vec<String> = value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The text one stream event adds to the answer, if any. An error event ends the stream.
fn delta_token(data: &str) -> Result<Option<String>, LlmError> {
    // Not JSON: a provider's keep-alive or banner. Nothing to show.
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return Ok(None);
    };
    if value.get("error").is_some() {
        return Err(error_from_value(None, &value, data));
    }
    let token = value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty());
    Ok(token.map(str::to_owned))
}

#[derive(Debug, PartialEq, Eq)]
enum SseEvent {
    Data(String),
    Done,
}

/// Splits a byte stream into server-sent events. Chunks can end anywhere, even inside a
/// multi-byte character, so it buffers bytes and only decodes whole lines.
#[derive(Default)]
struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            // Comment lines (`: keep-alive`) and other fields are not data.
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data == "[DONE]" {
                events.push(SseEvent::Done);
            } else if !data.is_empty() {
                events.push(SseEvent::Data(data.to_owned()));
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &str = include_str!("../tests/fixtures/stream_answer.sse");
    const ERROR_OPENAI: &str = include_str!("../tests/fixtures/error_openai.json");
    const ERROR_OPENCODE: &str = include_str!("../tests/fixtures/error_opencode.json");

    fn tokens(events: Vec<SseEvent>) -> Vec<String> {
        events
            .into_iter()
            .filter_map(|e| match e {
                SseEvent::Data(d) => delta_token(&d).unwrap(),
                SseEvent::Done => None,
            })
            .collect()
    }

    #[test]
    fn a_stream_yields_its_tokens_and_ends_with_done() {
        let events = SseParser::default().push(STREAM.as_bytes());
        assert_eq!(events.last(), Some(&SseEvent::Done));
        assert_eq!(
            tokens(events),
            ["- Hai năm", " dùng Rust\n- Tập trung vào âm thanh"]
        );
    }

    #[test]
    fn splitting_the_stream_anywhere_gives_the_same_events() {
        let whole = SseParser::default().push(STREAM.as_bytes());
        // Every cut point, including ones inside a multi-byte Vietnamese character.
        for cut in 1..STREAM.len() {
            let mut parser = SseParser::default();
            let mut events = parser.push(&STREAM.as_bytes()[..cut]);
            events.extend(parser.push(&STREAM.as_bytes()[cut..]));
            assert_eq!(events, whole, "cut at byte {cut}");
        }
    }

    #[test]
    fn windows_line_endings_are_accepted() {
        let crlf = STREAM.replace('\n', "\r\n");
        let events = SseParser::default().push(crlf.as_bytes());
        assert_eq!(events.last(), Some(&SseEvent::Done));
        assert_eq!(tokens(events).len(), 2);
    }

    #[test]
    fn comments_and_blank_data_are_ignored() {
        let events = SseParser::default().push(b": ping\n\ndata:\n\ndata: {\"x\":1}\n");
        assert_eq!(events, [SseEvent::Data("{\"x\":1}".into())]);
    }

    #[test]
    fn a_data_line_without_its_newline_waits_for_the_rest() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"data: [DO").is_empty());
        assert_eq!(parser.push(b"NE]\n"), [SseEvent::Done]);
    }

    #[test]
    fn events_that_are_not_json_or_carry_no_text_yield_no_token() {
        assert_eq!(delta_token("not json").unwrap(), None);
        assert_eq!(delta_token(r#"{"choices":[{"delta":{}}]}"#).unwrap(), None);
        assert_eq!(
            delta_token(r#"{"choices":[{"delta":{"content":""}}]}"#).unwrap(),
            None
        );
        assert_eq!(delta_token(r#"{"choices":[]}"#).unwrap(), None);
    }

    #[test]
    fn an_error_event_in_the_stream_is_an_error() {
        let err = delta_token(ERROR_OPENCODE).unwrap_err();
        assert!(matches!(err, LlmError::Auth(m) if m == "Invalid API key."));
    }

    #[test]
    fn the_openai_error_shape_is_read() {
        let err = error_from_body(401, ERROR_OPENAI);
        assert!(matches!(&err, LlmError::Auth(m) if m == "Incorrect API key provided."));
    }

    #[test]
    fn the_opencode_error_shape_is_read() {
        let err = error_from_body(401, ERROR_OPENCODE);
        assert!(matches!(&err, LlmError::Auth(m) if m == "Invalid API key."));
    }

    #[test]
    fn an_auth_error_type_counts_even_without_a_401() {
        let err = error_from_body(400, ERROR_OPENCODE);
        assert!(matches!(err, LlmError::Auth(_)));
    }

    #[test]
    fn other_failures_keep_the_status_and_the_message() {
        let body = r#"{"error":{"message":"Rate limit reached","type":"rate_limit_error"}}"#;
        assert_eq!(
            error_from_body(429, body).to_string(),
            "request failed: HTTP 429: Rate limit reached"
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_shown_truncated() {
        let long = "x".repeat(1_000);
        let LlmError::Request(message) = error_from_body(502, &long) else {
            panic!("expected a request error");
        };
        assert!(message.starts_with("HTTP 502: xxx"));
        assert!(message.chars().count() < 260);
    }

    #[test]
    fn an_error_that_is_a_plain_string_is_read() {
        let err = error_from_body(500, r#"{"error":"upstream down"}"#);
        assert_eq!(err.to_string(), "request failed: HTTP 500: upstream down");
    }

    #[test]
    fn model_ids_are_sorted_and_unique() {
        let value = json!({"data":[{"id":"b"},{"id":"a"},{"id":"b"},{"object":"x"}]});
        assert_eq!(model_ids(&value), ["a", "b"]);
        assert!(model_ids(&json!({})).is_empty());
    }

    #[test]
    fn session_ids_look_like_opencode_ids_and_never_repeat() {
        let ids: Vec<String> = (0..1_000).map(|_| new_session_id()).collect();
        assert!(ids.iter().all(|id| id.starts_with("ses_")));
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn plain_http_is_only_allowed_for_this_machine() {
        assert!(!check_endpoint("https://api.openai.com/v1").unwrap());
        assert!(check_endpoint("http://localhost:11434/v1").unwrap());
        assert!(check_endpoint("http://127.0.0.1:1234/v1").unwrap());
        assert!(check_endpoint("https://localhost/v1").unwrap());
        assert!(check_endpoint("http://api.example.com/v1").is_err());
        assert!(check_endpoint("ftp://example.com").is_err());
        assert!(check_endpoint("not a url").is_err());
    }
}
