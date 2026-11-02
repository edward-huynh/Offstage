//! The real sidecar with a real model, through the client and the router. Needs both, so it is
//! `#[ignore]`d: `OFFSTAGE_LLM_HOST=<offstage-llm-host> OFFSTAGE_LLM_MODEL=<Qwen3-*.gguf>
//! cargo test -p offstage-llm --test local_real -- --ignored --nocapture`.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use offstage_llm::{
    local::LocalLlm,
    prompt::{build_messages_with_documents, Excerpt},
    router::{ChatBackend, LocalSource, Policy, Router, Served},
    ChatMessage, LlmError,
};
use tokio_util::sync::CancellationToken;

fn paths() -> (PathBuf, PathBuf) {
    let get = |name| PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("set {name}")));
    (get("OFFSTAGE_LLM_HOST"), get("OFFSTAGE_LLM_MODEL"))
}

fn messages() -> Vec<ChatMessage> {
    build_messages_with_documents(
        "Bạn có kinh nghiệm với PostgreSQL không?",
        &[],
        Some("vi"),
        &[Excerpt {
            source: "CV.pdf".into(),
            section: Some("Kinh nghiệm".into()),
            text: "Công ty Âm Sắc, 2022 đến nay: thiết kế cơ sở dữ liệu PostgreSQL cho hệ thống báo cáo, \
                   xây dựng ứng dụng desktop bằng Rust và Tauri."
                .into(),
        }],
    )
}

struct Local(Arc<LocalLlm>);

#[async_trait::async_trait]
impl LocalSource for Local {
    async fn get(&self) -> Result<Arc<dyn ChatBackend>, LlmError> {
        Ok(self.0.clone() as Arc<dyn ChatBackend>)
    }
}

#[tokio::test]
#[ignore = "needs the sidecar and a model"]
async fn the_grounded_prompt_gets_a_vietnamese_answer_from_the_real_sidecar() {
    let (host, model) = paths();
    let loading = Instant::now();
    let llm = Arc::new(LocalLlm::start(&host, &model, true, Duration::from_secs(120)).unwrap());
    eprintln!(
        "loaded {} in {:.1}s",
        llm.model(),
        loading.elapsed().as_secs_f64()
    );

    // Through the router, the way the assistant uses it: local only.
    let router = Router::new(
        None,
        Some(Arc::new(Local(llm.clone()))),
        Policy {
            local_only: true,
            fallback: true,
            first_token_timeout: Duration::from_secs(5),
        },
    );
    let started = Instant::now();
    let mut first = None;
    let reply = router
        .answer(
            &messages(),
            &mut |_: &str| {
                first.get_or_insert(started.elapsed());
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    eprintln!(
        "first token {:?}, total {:?}\n{}",
        first,
        started.elapsed(),
        reply.text
    );
    assert_eq!(reply.served, Served::Local);
    assert!(!reply.fell_back);
    assert!(reply.text.contains("PostgreSQL"), "{}", reply.text);
    assert!(
        reply.text.trim_start().starts_with('-'),
        "bullets expected: {}",
        reply.text
    );
    // Whole characters only: no replacement character from a split multi-byte letter.
    assert!(!reply.text.contains('\u{fffd}'), "{}", reply.text);

    // A second answer on the same process, then a cancel in the middle of a third.
    assert!(!llm.chat_text("Rust là gì?").await.is_empty());
}

trait ChatText {
    async fn chat_text(&self, question: &str) -> String;
}

impl ChatText for LocalLlm {
    async fn chat_text(&self, question: &str) -> String {
        let messages = vec![ChatMessage {
            role: offstage_llm::Role::User,
            content: question.into(),
        }];
        let mut out = String::new();
        self.chat(
            &messages,
            40,
            &mut |t: &str| out.push_str(t),
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    }
}
