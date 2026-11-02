//! A stand-in for `offstage-llm-host` that the tests of `local.rs` start. It speaks the same
//! protocol without a model: a chat answers with the words of the last message, one per token.
//! Special last messages: `SLOW` writes a token every 20 ms until cancelled, `FAIL` answers with an
//! error, `CRASH` exits at once. `--never-ready` hangs before `Ready`, `--load-error` fails to load.

use std::{
    io::{self, BufRead, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use offstage_llm_proto::{decode, encode, Event, Request};

static CANCELLED: AtomicU64 = AtomicU64::new(0);

fn send(event: &Event) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{}", encode(event));
    let _ = out.flush();
}

fn main() {
    let flags: Vec<String> = std::env::args().collect();
    if flags.iter().any(|a| a == "--load-error") {
        send(&Event::Error {
            id: None,
            message: "no such model".into(),
        });
        std::process::exit(1);
    }
    if flags.iter().any(|a| a == "--never-ready") {
        thread::sleep(Duration::from_secs(60));
        return;
    }
    send(&Event::Ready {
        model: "fake".into(),
        load_ms: 1,
    });

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            match decode::<Request>(&line) {
                Ok(Request::Cancel { id }) => CANCELLED.store(id, Ordering::SeqCst),
                Ok(request) => {
                    let _ = tx.send(request);
                }
                Err(_) => {}
            }
        }
    });

    while let Ok(request) = rx.recv() {
        let Request::Chat { id, messages, .. } = request else {
            break;
        };
        let last = messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        match last.as_str() {
            "CRASH" => std::process::exit(3),
            "FAIL" => send(&Event::Error {
                id: Some(id),
                message: "out of memory".into(),
            }),
            "SLOW" => {
                for n in 0..500 {
                    if CANCELLED.load(Ordering::SeqCst) == id {
                        break;
                    }
                    send(&Event::Token {
                        id,
                        text: format!("t{n} "),
                    });
                    thread::sleep(Duration::from_millis(20));
                }
                send(&Event::Done { id });
            }
            _ => {
                for word in last.split_inclusive(' ') {
                    send(&Event::Token {
                        id,
                        text: word.to_owned(),
                    });
                }
                send(&Event::Done { id });
            }
        }
    }
}
