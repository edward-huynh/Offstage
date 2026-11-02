//! Chooses where an answer is written: a provider over the network, the model on this machine, or
//! the first and then the second when the first fails.
//!
//! - `local_only` sends nothing over the network, ever: only the local model is asked.
//! - Otherwise the provider is asked first. If it fails, or says nothing for too long, and no piece
//!   of the answer has reached the user yet, the local model answers instead (when `fallback` is on
//!   and a local model is installed). Once a token has been shown, the answer cannot be swapped
//!   for another one, so an error then is an error.
//! - With no provider configured, the local model answers.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use crate::{
    local::{LocalLlm, MAX_ANSWER_TOKENS},
    openai_compat::OpenAiCompat,
    ChatMessage, LlmError,
};

/// Anything that can write an answer: the provider client, or the local model.
#[async_trait::async_trait]
pub trait ChatBackend: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<String, LlmError>;
}

#[async_trait::async_trait]
impl ChatBackend for OpenAiCompat {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<String, LlmError> {
        self.stream_chat_cancellable(messages, on_token, cancel)
            .await
    }
}

#[async_trait::async_trait]
impl ChatBackend for LocalLlm {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<String, LlmError> {
        LocalLlm::chat(self, messages, MAX_ANSWER_TOKENS, on_token, cancel).await
    }
}

/// Where the local model comes from. Starting it takes seconds and a few gigabytes of memory, so
/// it is only asked for when an answer needs it.
#[async_trait::async_trait]
pub trait LocalSource: Send + Sync {
    async fn get(&self) -> Result<Arc<dyn ChatBackend>, LlmError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Never ask the provider.
    pub local_only: bool,
    /// Answer with the local model when the provider fails.
    pub fallback: bool,
    /// How long the provider may take to write the first token before it is given up on.
    pub first_token_timeout: Duration,
}

/// Where the answer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Served {
    Provider,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    pub served: Served,
    /// The provider was asked first and failed, so the local model answered.
    pub fell_back: bool,
}

pub struct Router {
    provider: Option<Arc<dyn ChatBackend>>,
    local: Option<Arc<dyn LocalSource>>,
    policy: Policy,
}

impl Router {
    pub fn new(
        provider: Option<Arc<dyn ChatBackend>>,
        local: Option<Arc<dyn LocalSource>>,
        policy: Policy,
    ) -> Self {
        Self {
            provider,
            local,
            policy,
        }
    }

    /// Writes an answer to `messages`, passing each piece to `on_token`.
    pub async fn answer(
        &self,
        messages: &[ChatMessage],
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<Reply, LlmError> {
        let provider = match (&self.provider, self.policy.local_only) {
            (Some(provider), false) => provider,
            _ => return self.ask_local(messages, on_token, cancel, false).await,
        };

        // The provider, with a watchdog for the first token. The user's own cancel and the
        // watchdog's both end the request the same way, so they are told apart afterwards.
        let started = Arc::new(AtomicBool::new(false));
        let stop = cancel.child_token();
        let mut timed_out = false;
        let result = {
            let shown = Arc::clone(&started);
            // A reborrow, so `on_token` is still there if the local model has to answer.
            let on_token = &mut *on_token;
            let mut wrapped = move |token: &str| {
                shown.store(true, Ordering::SeqCst);
                on_token(token);
            };
            let chat = provider.chat(messages, &mut wrapped, &stop);
            tokio::pin!(chat);
            let timer = tokio::time::sleep(self.policy.first_token_timeout);
            tokio::pin!(timer);
            let mut timer_done = false;
            loop {
                tokio::select! {
                    result = &mut chat => break result,
                    () = &mut timer, if !timer_done => {
                        timer_done = true;
                        if !started.load(Ordering::SeqCst) {
                            timed_out = true;
                            stop.cancel();
                        }
                    }
                }
            }
        };

        let error = match result {
            Ok(text) => {
                return Ok(Reply {
                    text,
                    served: Served::Provider,
                    fell_back: false,
                })
            }
            Err(_) if cancel.is_cancelled() => return Err(LlmError::Cancelled),
            Err(LlmError::Cancelled) if timed_out => {
                LlmError::Request("the provider did not answer in time".into())
            }
            Err(error) => error,
        };
        // Part of an answer is already on screen, or there is nowhere to fall back to.
        if started.load(Ordering::SeqCst) || !self.policy.fallback || self.local.is_none() {
            return Err(error);
        }
        match self.ask_local(messages, on_token, cancel, true).await {
            Err(LlmError::Cancelled) => Err(LlmError::Cancelled),
            Err(local) => Err(LlmError::Request(format!(
                "{error}; the model on this machine is not available either: {local}"
            ))),
            reply => reply,
        }
    }

    async fn ask_local(
        &self,
        messages: &[ChatMessage],
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
        fell_back: bool,
    ) -> Result<Reply, LlmError> {
        let source = self
            .local
            .as_ref()
            .ok_or_else(|| LlmError::Request("no model on this machine is installed".into()))?;
        let local = tokio::select! {
            () = cancel.cancelled() => return Err(LlmError::Cancelled),
            local = source.get() => local?,
        };
        let text = local.chat(messages, on_token, cancel).await?;
        Ok(Reply {
            text,
            served: Served::Local,
            fell_back,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::Role;

    /// A backend that does what the test says: writes `tokens`, then fails with `error` or ends.
    /// `delay` is waited before the first token, and `hang` never writes anything.
    struct Fake {
        tokens: Vec<&'static str>,
        error: Option<&'static str>,
        delay: Duration,
        hang: bool,
        calls: AtomicUsize,
    }

    impl Fake {
        fn ok(tokens: &[&'static str]) -> Self {
            Self {
                tokens: tokens.to_vec(),
                error: None,
                delay: Duration::ZERO,
                hang: false,
                calls: AtomicUsize::new(0),
            }
        }
        fn failing(tokens: &[&'static str], error: &'static str) -> Self {
            Self {
                error: Some(error),
                ..Self::ok(tokens)
            }
        }
        fn hanging() -> Self {
            Self {
                hang: true,
                ..Self::ok(&[])
            }
        }
        fn slow(delay: Duration, tokens: &[&'static str]) -> Self {
            Self {
                delay,
                ..Self::ok(tokens)
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl ChatBackend for Fake {
        async fn chat(
            &self,
            _: &[ChatMessage],
            on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
            cancel: &CancellationToken,
        ) -> Result<String, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.hang {
                cancel.cancelled().await;
                return Err(LlmError::Cancelled);
            }
            tokio::select! {
                () = cancel.cancelled() => return Err(LlmError::Cancelled),
                () = tokio::time::sleep(self.delay) => {}
            }
            let mut text = String::new();
            for token in &self.tokens {
                on_token(token);
                text.push_str(token);
            }
            match self.error {
                Some(message) => Err(LlmError::Request(message.into())),
                None => Ok(text),
            }
        }
    }

    struct Source {
        backend: Result<Arc<Fake>, &'static str>,
        gets: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LocalSource for Source {
        async fn get(&self) -> Result<Arc<dyn ChatBackend>, LlmError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            match &self.backend {
                Ok(backend) => Ok(backend.clone()),
                Err(message) => Err(LlmError::Request((*message).into())),
            }
        }
    }

    fn source(backend: &Arc<Fake>) -> Arc<Source> {
        Arc::new(Source {
            backend: Ok(backend.clone()),
            gets: AtomicUsize::new(0),
        })
    }

    fn policy(local_only: bool, fallback: bool) -> Policy {
        Policy {
            local_only,
            fallback,
            first_token_timeout: Duration::from_millis(80),
        }
    }

    fn messages() -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: Role::User,
            content: "hi".into(),
        }]
    }

    async fn run(router: &Router) -> (Result<Reply, LlmError>, String) {
        let mut seen = String::new();
        let result = router
            .answer(
                &messages(),
                &mut |t: &str| seen.push_str(t),
                &CancellationToken::new(),
            )
            .await;
        (result, seen)
    }

    #[tokio::test]
    async fn the_provider_answers_and_the_local_model_is_not_even_started() {
        let provider = Arc::new(Fake::ok(&["a", "b"]));
        let local = Arc::new(Fake::ok(&["L"]));
        let src = source(&local);
        let router = Router::new(Some(provider), Some(src.clone()), policy(false, true));
        let (reply, seen) = run(&router).await;
        assert_eq!(
            reply.unwrap(),
            Reply {
                text: "ab".into(),
                served: Served::Provider,
                fell_back: false
            }
        );
        assert_eq!(seen, "ab");
        assert_eq!(src.gets.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn local_only_never_asks_the_provider() {
        let provider = Arc::new(Fake::ok(&["cloud"]));
        let local = Arc::new(Fake::ok(&["L1", "L2"]));
        let router = Router::new(
            Some(provider.clone()),
            Some(source(&local)),
            policy(true, true),
        );
        let (reply, seen) = run(&router).await;
        assert_eq!(
            reply.unwrap(),
            Reply {
                text: "L1L2".into(),
                served: Served::Local,
                fell_back: false
            }
        );
        assert_eq!(seen, "L1L2");
        assert_eq!(provider.calls(), 0);
    }

    #[tokio::test]
    async fn local_only_without_a_local_model_is_an_error_and_still_never_asks_the_provider() {
        let provider = Arc::new(Fake::ok(&["cloud"]));
        let router = Router::new(Some(provider.clone()), None, policy(true, true));
        let (reply, _) = run(&router).await;
        assert!(reply
            .unwrap_err()
            .to_string()
            .contains("no model on this machine"));
        assert_eq!(provider.calls(), 0);
    }

    #[tokio::test]
    async fn a_provider_error_before_any_token_falls_back_to_the_local_model() {
        let provider = Arc::new(Fake::failing(&[], "offline"));
        let local = Arc::new(Fake::ok(&["local answer"]));
        let router = Router::new(Some(provider), Some(source(&local)), policy(false, true));
        let (reply, seen) = run(&router).await;
        assert_eq!(
            reply.unwrap(),
            Reply {
                text: "local answer".into(),
                served: Served::Local,
                fell_back: true
            }
        );
        assert_eq!(seen, "local answer");
    }

    #[tokio::test]
    async fn an_error_after_a_token_is_shown_is_an_error_not_a_second_answer() {
        let provider = Arc::new(Fake::failing(&["partial"], "connection reset"));
        let local = Arc::new(Fake::ok(&["local answer"]));
        let src = source(&local);
        let router = Router::new(Some(provider), Some(src.clone()), policy(false, true));
        let (reply, seen) = run(&router).await;
        assert!(reply.unwrap_err().to_string().contains("connection reset"));
        assert_eq!(seen, "partial");
        assert_eq!(src.gets.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn with_fallback_off_a_provider_error_is_the_answer() {
        let provider = Arc::new(Fake::failing(&[], "offline"));
        let local = Arc::new(Fake::ok(&["local"]));
        let src = source(&local);
        let router = Router::new(Some(provider), Some(src.clone()), policy(false, false));
        assert!(run(&router)
            .await
            .0
            .unwrap_err()
            .to_string()
            .contains("offline"));
        assert_eq!(src.gets.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn without_a_local_model_the_provider_error_is_the_answer() {
        let provider = Arc::new(Fake::failing(&[], "offline"));
        let router = Router::new(Some(provider), None, policy(false, true));
        assert!(run(&router)
            .await
            .0
            .unwrap_err()
            .to_string()
            .contains("offline"));
    }

    #[tokio::test]
    async fn a_provider_that_says_nothing_in_time_falls_back() {
        let provider = Arc::new(Fake::hanging());
        let local = Arc::new(Fake::ok(&["local"]));
        let router = Router::new(Some(provider), Some(source(&local)), policy(false, true));
        let started = std::time::Instant::now();
        let (reply, _) = run(&router).await;
        assert_eq!(
            reply.unwrap(),
            Reply {
                text: "local".into(),
                served: Served::Local,
                fell_back: true
            }
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn a_timeout_without_a_fallback_says_the_provider_did_not_answer() {
        let provider = Arc::new(Fake::hanging());
        let router = Router::new(Some(provider), None, policy(false, true));
        let err = run(&router).await.0.unwrap_err();
        assert!(err.to_string().contains("did not answer in time"), "{err}");
    }

    #[tokio::test]
    async fn a_slow_start_that_beats_the_timeout_is_left_alone() {
        let provider = Arc::new(Fake::slow(Duration::from_millis(30), &["fine"]));
        let local = Arc::new(Fake::ok(&["local"]));
        let src = source(&local);
        let router = Router::new(Some(provider), Some(src.clone()), policy(false, true));
        assert_eq!(run(&router).await.0.unwrap().served, Served::Provider);
        assert_eq!(src.gets.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn the_user_cancelling_ends_the_answer_without_falling_back() {
        let provider = Arc::new(Fake::hanging());
        let local = Arc::new(Fake::ok(&["local"]));
        let src = source(&local);
        let router = Router::new(
            Some(provider),
            Some(src.clone()),
            Policy {
                first_token_timeout: Duration::from_secs(30),
                ..policy(false, true)
            },
        );
        let cancel = CancellationToken::new();
        let later = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            later.cancel();
        });
        let result = router.answer(&messages(), &mut |_: &str| {}, &cancel).await;
        assert!(matches!(result, Err(LlmError::Cancelled)));
        assert_eq!(src.gets.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn without_a_provider_the_local_model_answers_and_it_is_not_a_fallback() {
        let local = Arc::new(Fake::ok(&["local"]));
        let router = Router::new(None, Some(source(&local)), policy(false, true));
        let reply = run(&router).await.0.unwrap();
        assert_eq!(
            reply,
            Reply {
                text: "local".into(),
                served: Served::Local,
                fell_back: false
            }
        );
    }

    #[tokio::test]
    async fn when_both_fail_the_message_names_both() {
        let provider = Arc::new(Fake::failing(&[], "offline"));
        let src = Arc::new(Source {
            backend: Err("out of memory"),
            gets: AtomicUsize::new(0),
        });
        let router = Router::new(Some(provider), Some(src), policy(false, true));
        let message = run(&router).await.0.unwrap_err().to_string();
        assert!(
            message.contains("offline") && message.contains("out of memory"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn a_cancel_while_the_local_model_starts_ends_the_answer() {
        struct Never;
        #[async_trait::async_trait]
        impl LocalSource for Never {
            async fn get(&self) -> Result<Arc<dyn ChatBackend>, LlmError> {
                std::future::pending().await
            }
        }
        let router = Router::new(None, Some(Arc::new(Never)), policy(true, true));
        let cancel = CancellationToken::new();
        let later = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            later.cancel();
        });
        let result = router.answer(&messages(), &mut |_: &str| {}, &cancel).await;
        assert!(matches!(result, Err(LlmError::Cancelled)));
    }
}
