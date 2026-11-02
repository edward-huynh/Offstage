//! `offstage-llm-host`: runs a local language model for the Offstage app, in its own process.
//!
//! `llama.cpp` cannot share a process with `whisper.cpp` (each embeds its own `ggml`; loading
//! both aborts), so the app starts this program when it needs the model and stops it afterwards.
//! The two talk in JSON lines on standard input and output, see `offstage-llm-proto`.
//!
//! Usage: `offstage-llm-host --model <file.gguf> [--close-thinking]`

mod engine;
mod utf8;

use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    process::ExitCode,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

use offstage_llm_proto::{decode, encode, Event, Request};

/// The id of the answer the app asked to stop, or 0. Ids start at 1.
static CANCELLED: AtomicU64 = AtomicU64::new(0);

fn send(event: &Event) {
    let mut out = io::stdout().lock();
    // A closed pipe means the app is gone, and the read loop ends with it.
    let _ = writeln!(out, "{}", encode(event));
    let _ = out.flush();
}

struct Args {
    model: PathBuf,
    close_thinking: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut close_thinking = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--model" => model = args.next().map(PathBuf::from),
            "--close-thinking" => close_thinking = true,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Args {
        model: model.ok_or("usage: offstage-llm-host --model <file.gguf> [--close-thinking]")?,
        close_thinking,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            send(&Event::Error { id: None, message });
            return ExitCode::FAILURE;
        }
    };
    let engine = match engine::Engine::load(&args.model, args.close_thinking) {
        Ok(engine) => engine,
        Err(message) => {
            send(&Event::Error { id: None, message });
            return ExitCode::FAILURE;
        }
    };
    send(&Event::Ready {
        model: args
            .model
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        load_ms: engine.load_ms,
    });

    // Standard input is read on its own thread so a `cancel` arrives while an answer is written.
    // When the app goes away the pipe closes, the thread ends, and so does the program.
    let (requests, incoming) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match decode::<Request>(&line) {
                Ok(Request::Cancel { id }) => CANCELLED.store(id, Ordering::SeqCst),
                Ok(request) => {
                    if requests.send(request).is_err() {
                        break;
                    }
                }
                Err(e) => send(&Event::Error {
                    id: None,
                    message: format!("unreadable request: {e}"),
                }),
            }
        }
    });

    while let Ok(request) = incoming.recv() {
        match request {
            Request::Shutdown => break,
            // Handled on the reading thread, never queued.
            Request::Cancel { .. } => {}
            Request::Chat {
                id,
                messages,
                max_tokens,
            } => {
                let stop = || CANCELLED.load(Ordering::SeqCst) == id;
                let result = engine.answer(&messages, max_tokens, &stop, &mut |text| {
                    send(&Event::Token {
                        id,
                        text: text.to_owned(),
                    });
                });
                match result {
                    Ok(()) => send(&Event::Done { id }),
                    Err(message) => send(&Event::Error {
                        id: Some(id),
                        message,
                    }),
                }
            }
        }
    }
    ExitCode::SUCCESS
}
