use std::time::{Duration, Instant};

use offstage_llm::{
    openai_compat::OpenAiCompat, ApiStyle, ChatMessage, LlmError, LlmProvider, ProviderConfig, Role,
};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const STREAM: &str = include_str!("fixtures/stream_answer.sse");
const ERROR_OPENCODE: &str = include_str!("fixtures/error_opencode.json");

fn config(server: &MockServer, model: Option<&str>, key_ref: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        id: "test".into(),
        label: "Test".into(),
        base_url: format!("{}/v1", server.uri()),
        api_style: ApiStyle::ChatCompletions,
        model: model.map(str::to_owned),
        key_ref: key_ref.map(str::to_owned),
    }
}

fn ask() -> Vec<ChatMessage> {
    vec![ChatMessage {
        role: Role::User,
        content: "Bạn làm ở đâu?".into(),
    }]
}

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

#[tokio::test]
async fn tokens_are_streamed_in_order_and_joined_into_the_answer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .and(body_partial_json(serde_json::json!({
            "model": "gpt-test",
            "stream": true,
            "messages": [{"role": "user", "content": "Bạn làm ở đâu?"}],
        })))
        .respond_with(sse(STREAM))
        .expect(1)
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(
        config(&server, Some("gpt-test"), Some("env:X")),
        Some("sk-test".into()),
    )
    .unwrap();
    let mut seen = Vec::new();
    let answer = llm
        .stream_chat(&ask(), &mut |token| seen.push(token.to_owned()))
        .await
        .unwrap();

    assert_eq!(seen, ["- Hai năm", " dùng Rust\n- Tập trung vào âm thanh"]);
    assert_eq!(answer, seen.concat());
}

#[tokio::test]
async fn a_rejected_key_is_an_auth_error_with_the_providers_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string(ERROR_OPENCODE))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(
        config(&server, Some("m"), Some("env:X")),
        Some("sk-wrong".into()),
    )
    .unwrap();
    let err = llm.stream_chat(&ask(), &mut |_| {}).await.unwrap_err();
    assert!(matches!(&err, LlmError::Auth(m) if m == "Invalid API key."));
    assert!(!err.to_string().contains("sk-wrong"));
}

#[tokio::test]
async fn a_provider_that_needs_a_key_fails_before_any_request_without_one() {
    let server = MockServer::start().await;
    let llm = OpenAiCompat::new(config(&server, Some("m"), Some("keychain:openai")), None).unwrap();
    let err = llm.stream_chat(&ask(), &mut |_| {}).await.unwrap_err();
    assert!(matches!(err, LlmError::MissingKey(id) if id == "test"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_endpoint_without_a_key_reference_sends_no_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(STREAM))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    llm.stream_chat(&ask(), &mut |_| {}).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert!(!requests[0].headers.contains_key("authorization"));
}

#[tokio::test]
async fn no_model_is_an_error_before_any_request() {
    let server = MockServer::start().await;
    let llm = OpenAiCompat::new(config(&server, None, None), None).unwrap();
    let err = llm.stream_chat(&ask(), &mut |_| {}).await.unwrap_err();
    assert!(matches!(err, LlmError::Request(m) if m.contains("no model")));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn cancelling_before_the_call_returns_cancelled_without_a_request() {
    let server = MockServer::start().await;
    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = llm
        .stream_chat_cancellable(&ask(), &mut |_| {}, &cancel)
        .await
        .unwrap_err();
    assert!(matches!(err, LlmError::Cancelled));
}

#[tokio::test]
async fn cancelling_a_slow_response_returns_promptly() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(STREAM).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    let cancel = CancellationToken::new();
    let canceller = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel.cancel();
        })
    };

    let started = Instant::now();
    let err = llm
        .stream_chat_cancellable(&ask(), &mut |_| {}, &cancel)
        .await
        .unwrap_err();
    canceller.await.unwrap();

    assert!(matches!(err, LlmError::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn an_error_event_inside_the_stream_stops_the_answer() {
    let server = MockServer::start().await;
    let body = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"- a\"}}}}]}}\n\ndata: {}\n\n",
        ERROR_OPENCODE.trim()
    );
    Mock::given(method("POST"))
        .respond_with(sse(&body))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    let mut seen = Vec::new();
    let err = llm
        .stream_chat(&ask(), &mut |t| seen.push(t.to_owned()))
        .await
        .unwrap_err();
    assert_eq!(seen, ["- a"]);
    assert!(matches!(err, LlmError::Auth(_)));
}

#[tokio::test]
async fn a_stream_that_closes_without_done_still_returns_what_arrived() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"- một\"}}]}\n\n",
        ))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    let answer = llm.stream_chat(&ask(), &mut |_| {}).await.unwrap();
    assert_eq!(answer, "- một");
}

#[tokio::test]
async fn models_are_listed_sorted_and_the_connection_test_uses_the_same_call() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"id": "gpt-b"}, {"id": "gpt-a"}]
        })))
        .expect(2)
        .mount(&server)
        .await;

    let llm =
        OpenAiCompat::new(config(&server, None, Some("env:X")), Some("sk-test".into())).unwrap();
    assert_eq!(llm.list_models().await.unwrap(), ["gpt-a", "gpt-b"]);
    llm.test_connection().await.unwrap();
}

#[tokio::test]
async fn a_failed_connection_test_reports_the_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_string(ERROR_OPENCODE))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(
        config(&server, None, Some("env:X")),
        Some("sk-wrong".into()),
    )
    .unwrap();
    assert!(matches!(
        llm.test_connection().await,
        Err(LlmError::Auth(_))
    ));
}

#[tokio::test]
async fn an_endpoint_on_this_machine_is_reported_as_local() {
    let server = MockServer::start().await;
    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    // The mock server listens on 127.0.0.1.
    assert!(llm.is_local());
    assert_eq!(llm.id(), "test");
}

fn header_of(request: &wiremock::Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

#[tokio::test]
async fn every_request_names_the_app_and_carries_the_same_session_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(STREAM))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    llm.stream_chat(&ask(), &mut |_| {}).await.unwrap();
    llm.stream_chat(&ask(), &mut |_| {}).await.unwrap();
    llm.list_models().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    let sessions: Vec<_> = requests
        .iter()
        .map(|r| header_of(r, "x-opencode-session").expect("session header"))
        .collect();
    assert!(sessions[0].starts_with("ses_"));
    assert!(sessions.iter().all(|s| s == &sessions[0]));
    for request in &requests {
        let agent = header_of(request, "user-agent").unwrap();
        assert!(agent.starts_with("offstage/"), "{agent}");
    }
}

#[tokio::test]
async fn a_new_session_changes_the_id_for_later_requests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(STREAM))
        .mount(&server)
        .await;

    let llm = OpenAiCompat::new(config(&server, Some("m"), None), None).unwrap();
    llm.stream_chat(&ask(), &mut |_| {}).await.unwrap();
    llm.new_session();
    llm.stream_chat(&ask(), &mut |_| {}).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let first = header_of(&requests[0], "x-opencode-session").unwrap();
    let second = header_of(&requests[1], "x-opencode-session").unwrap();
    assert_ne!(first, second);
}
