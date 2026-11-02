//! The local language model, run by the sidecar `offstage-llm-host` (see `offstage-llm-proto` for
//! why it is a separate process). [`LocalLlm`] starts it, sends it conversations and turns its
//! events back into tokens. Dropping it stops the process and gives the memory back.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use offstage_llm_proto::{decode, encode, Event, Message, Request};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{ChatMessage, LlmError, Role};

/// Longest silence tolerated while an answer is being written. Reading a long prompt on a
/// computer without a GPU takes several seconds before the first token.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a cancelled answer may take to end before it is abandoned.
const CANCEL_GRACE: Duration = Duration::from_secs(5);
/// How long the process gets to exit by itself before it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// Tokens an answer may have. The assistant writes three to five short bullets.
pub const MAX_ANSWER_TOKENS: u32 = 320;

type Pending = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Event>>>>;

pub struct LocalLlm {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Pending,
    next_id: AtomicU64,
    /// False once the process has gone, whatever the reason.
    alive: Arc<AtomicBool>,
    model: String,
}

impl LocalLlm {
    /// Starts the sidecar `host` on the model file and waits until the model is loaded. Blocks,
    /// for seconds with a large model: call it off the async runtime. `close_thinking` is for
    /// Qwen3 models.
    pub fn start(
        host: &Path,
        model: &Path,
        close_thinking: bool,
        ready_within: Duration,
    ) -> Result<Self, LlmError> {
        let mut command = Command::new(host);
        command.arg("--model").arg(model);
        if close_thinking {
            command.arg("--close-thinking");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // llama.cpp is chatty on stderr, and a GUI app has nobody to read it.
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window for a program that has no use for one.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|e| {
            LlmError::Request(format!(
                "cannot start the local model ({}): {e}",
                host.display()
            ))
        })?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(LlmError::Request("the local model has no pipes".into()));
        };

        let pending: Pending = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<String, String>>();
        {
            let pending = Arc::clone(&pending);
            let alive = Arc::clone(&alive);
            thread::spawn(move || read_events(stdout, &pending, &alive, &ready_tx));
        }

        let model_name = match ready_rx.recv_timeout(ready_within) {
            Ok(Ok(name)) => name,
            Ok(Err(message)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LlmError::Request(format!(
                    "the local model did not load: {message}"
                )));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LlmError::Request(
                    "the local model took too long to load".into(),
                ));
            }
        };
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending,
            next_id: AtomicU64::new(1),
            alive,
            model: model_name,
        })
    }

    /// The model file the sidecar loaded.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// False once the process has exited, for example because it ran out of memory. A new
    /// [`LocalLlm`] has to be started then.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn write(&self, request: &Request) -> Result<(), LlmError> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| LlmError::Request("the local model is unavailable".into()))?;
        writeln!(stdin, "{}", encode(request))
            .and_then(|()| stdin.flush())
            .map_err(|_| LlmError::Request("the local model has stopped".into()))
    }

    /// Writes an answer to `messages`, passing each piece to `on_token`, and returns the whole
    /// text. Stops with [`LlmError::Cancelled`] when `cancel` is triggered.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        max_tokens: u32,
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<String, LlmError> {
        if !self.is_alive() {
            return Err(LlmError::Request("the local model has stopped".into()));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, mut events) = mpsc::unbounded_channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(id, tx);
        }
        let result = self
            .run(id, messages, max_tokens, on_token, cancel, &mut events)
            .await;
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
        result
    }

    async fn run(
        &self,
        id: u64,
        messages: &[ChatMessage],
        max_tokens: u32,
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
        events: &mut mpsc::UnboundedReceiver<Event>,
    ) -> Result<String, LlmError> {
        self.write(&Request::Chat {
            id,
            messages: messages.iter().map(to_wire).collect(),
            max_tokens,
        })?;

        let mut answer = String::new();
        let mut cancelled = false;
        loop {
            let wait = if cancelled {
                CANCEL_GRACE
            } else {
                IDLE_TIMEOUT
            };
            let event = tokio::select! {
                () = cancel.cancelled(), if !cancelled => {
                    cancelled = true;
                    // The sidecar stops at the next token and ends the answer with `Done`.
                    let _ = self.write(&Request::Cancel { id });
                    continue;
                }
                next = tokio::time::timeout(wait, events.recv()) => match next {
                    Ok(event) => event,
                    Err(_) if cancelled => return Err(LlmError::Cancelled),
                    Err(_) => return Err(LlmError::Request("the local model stopped responding".into())),
                },
            };
            match event {
                Some(Event::Token { text, .. }) if !cancelled => {
                    on_token(&text);
                    answer.push_str(&text);
                }
                Some(Event::Token { .. }) => {}
                Some(Event::Done { .. }) if cancelled => return Err(LlmError::Cancelled),
                Some(Event::Done { .. }) => return Ok(answer),
                Some(Event::Error { message, .. }) => return Err(LlmError::Request(message)),
                Some(Event::Ready { .. }) => {}
                // The reader ended: the process is gone.
                None if cancelled => return Err(LlmError::Cancelled),
                None => return Err(LlmError::Request("the local model has stopped".into())),
            }
        }
    }
}

impl Drop for LocalLlm {
    fn drop(&mut self) {
        let _ = self.write(&Request::Shutdown);
        if let Ok(mut child) = self.child.lock() {
            // It exits on `Shutdown` or when its input closes, which happens as `stdin` drops.
            let deadline = std::time::Instant::now() + EXIT_GRACE;
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => thread::sleep(Duration::from_millis(25)),
                    Err(_) => break,
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn to_wire(message: &ChatMessage) -> Message {
    Message {
        role: match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
        .to_owned(),
        content: message.content.clone(),
    }
}

/// Reads the sidecar's output until it closes. `Ready` and a load error go to `ready`, the rest
/// to the answer they belong to. When the output closes, every answer still waiting is told.
fn read_events(
    stdout: impl std::io::Read,
    pending: &Pending,
    alive: &AtomicBool,
    ready: &std::sync::mpsc::Sender<Result<String, String>>,
) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(event) = decode::<Event>(&line) else {
            continue;
        };
        match event {
            Event::Ready { model, .. } => {
                let _ = ready.send(Ok(model));
            }
            Event::Error { id: None, message } => {
                let _ = ready.send(Err(message));
            }
            Event::Token { id, .. } | Event::Done { id } | Event::Error { id: Some(id), .. } => {
                if let Ok(pending) = pending.lock() {
                    if let Some(tx) = pending.get(&id) {
                        let _ = tx.send(event);
                    }
                }
            }
        }
    }
    alive.store(false, Ordering::SeqCst);
    let _ = ready.send(Err("the local model exited".into()));
    // Dropping the senders ends each waiting answer.
    if let Ok(mut pending) = pending.lock() {
        pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// The fake sidecar of `examples/fake_host.rs`. `cargo test` builds examples, but running one
    /// test target alone (`--lib`) does not, so it is built here when it is missing.
    fn fake_host() -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        let name = if cfg!(windows) {
            "fake_host.exe"
        } else {
            "fake_host"
        };
        let path = exe
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples")
            .join(name);
        if !path.exists() {
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let built = Command::new(cargo)
                .args([
                    "build",
                    "--quiet",
                    "--example",
                    "fake_host",
                    "--manifest-path",
                ])
                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
                .status()
                .unwrap();
            assert!(built.success(), "cannot build the fake sidecar");
        }
        assert!(path.exists(), "{} is missing", path.display());
        path
    }

    fn start(flag: Option<&str>) -> Result<LocalLlm, LlmError> {
        match flag {
            None => LocalLlm::start(
                &fake_host(),
                Path::new("model.gguf"),
                false,
                Duration::from_secs(10),
            ),
            // The fake reads its flags from the command line; `start` passes `--model <path>`, so a
            // flag can ride along as the "model path" and is seen as an argument.
            Some(flag) => {
                LocalLlm::start(&fake_host(), Path::new(flag), false, Duration::from_secs(2))
            }
        }
    }

    fn user(text: &str) -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: Role::User,
            content: text.into(),
        }]
    }

    async fn ask(llm: &LocalLlm, text: &str) -> (Result<String, LlmError>, Vec<String>) {
        let mut tokens = Vec::new();
        let result = llm
            .chat(
                &user(text),
                100,
                &mut |t: &str| tokens.push(t.to_owned()),
                &CancellationToken::new(),
            )
            .await;
        (result, tokens)
    }

    #[tokio::test]
    async fn an_answer_streams_token_by_token_and_returns_the_whole_text() {
        let llm = start(None).unwrap();
        assert_eq!(llm.model(), "fake");
        let (result, tokens) = ask(&llm, "xin chào các bạn").await;
        assert_eq!(result.unwrap(), "xin chào các bạn");
        assert_eq!(tokens, ["xin ", "chào ", "các ", "bạn"]);
    }

    #[tokio::test]
    async fn answers_can_follow_each_other_on_one_process() {
        let llm = start(None).unwrap();
        for text in ["một", "hai ba"] {
            assert_eq!(ask(&llm, text).await.0.unwrap(), text);
        }
    }

    #[tokio::test]
    async fn cancelling_stops_the_answer_and_the_process_stays_usable() {
        let llm = start(None).unwrap();
        let cancel = CancellationToken::new();
        let mut seen = 0;
        let canceller = cancel.clone();
        let result = llm
            .chat(
                &user("SLOW"),
                100,
                &mut |_: &str| {
                    seen += 1;
                    if seen == 3 {
                        canceller.cancel();
                    }
                },
                &cancel,
            )
            .await;
        assert!(matches!(result, Err(LlmError::Cancelled)));
        assert!(seen < 20, "kept writing after the cancel: {seen}");
        assert_eq!(ask(&llm, "vẫn chạy").await.0.unwrap(), "vẫn chạy");
    }

    #[tokio::test]
    async fn an_error_from_the_model_is_reported_and_the_process_stays_usable() {
        let llm = start(None).unwrap();
        let err = ask(&llm, "FAIL").await.0.unwrap_err();
        assert!(err.to_string().contains("out of memory"), "{err}");
        assert_eq!(ask(&llm, "ok").await.0.unwrap(), "ok");
    }

    #[tokio::test]
    async fn a_process_that_dies_mid_answer_is_an_error_and_is_no_longer_alive() {
        let llm = start(None).unwrap();
        let err = ask(&llm, "CRASH").await.0.unwrap_err();
        assert!(err.to_string().contains("stopped"), "{err}");
        assert!(!llm.is_alive());
        // A later question fails at once instead of waiting.
        assert!(ask(&llm, "x").await.0.is_err());
    }

    #[test]
    fn a_model_that_fails_to_load_is_reported_with_its_reason() {
        let err = start(Some("--load-error")).err().unwrap();
        assert!(err.to_string().contains("no such model"), "{err}");
    }

    #[test]
    fn a_model_that_never_gets_ready_times_out_and_the_process_is_killed() {
        // Built first, so the clock below measures the timeout and not a compile.
        let _ = fake_host();
        let started = std::time::Instant::now();
        let err = start(Some("--never-ready")).err().unwrap();
        assert!(err.to_string().contains("too long"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_missing_program_is_reported() {
        let err = LocalLlm::start(
            Path::new("/nonexistent/offstage-llm-host"),
            Path::new("m.gguf"),
            false,
            Duration::from_secs(1),
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("cannot start"), "{err}");
    }

    #[tokio::test]
    async fn dropping_stops_the_process() {
        let llm = start(None).unwrap();
        let pid = llm.child.lock().unwrap().id();
        drop(llm);
        // The pid is gone: `kill -0` fails. Only checked where there is a `kill` command.
        #[cfg(unix)]
        {
            let alive = std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .unwrap()
                .success();
            assert!(!alive, "the sidecar is still running");
        }
        let _ = pid;
    }
}
