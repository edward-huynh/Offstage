//! What the app and the local language model sidecar (`offstage-llm-host`) say to each other.
//!
//! The sidecar is a separate process because `llama.cpp` and `whisper.cpp` cannot live in one: each
//! embeds its own `ggml`, and loading both aborts the process. They talk through the sidecar's
//! standard input and output, one JSON object per line. There is no socket, so nothing else on the
//! machine can reach the model and the firewall never asks anything.
//!
//! The app sends [`Request`]s. The sidecar answers with [`Event`]s: `Ready` once the model is
//! loaded, then for each `Chat` any number of `Token`s and a final `Done` or `Error`.

use serde::{Deserialize, Serialize};

/// One message of a conversation. `role` is `system`, `user` or `assistant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Answer a conversation. `id` names the answer in the events that follow.
    Chat {
        id: u64,
        messages: Vec<Message>,
        /// Most tokens to write.
        max_tokens: u32,
    },
    /// Stop writing the answer `id`. It ends with `Done`.
    Cancel { id: u64 },
    /// Exit. The sidecar also exits when its standard input closes, so it never outlives the app.
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The model is loaded and requests are accepted.
    Ready { model: String, load_ms: u64 },
    /// A piece of the answer `id`. Always whole characters.
    Token { id: u64, text: String },
    /// The answer `id` is finished, or was cancelled.
    Done { id: u64 },
    /// Something failed. `id` is the answer it belongs to, or `None` when no answer is running
    /// (the model did not load).
    Error { id: Option<u64>, message: String },
}

/// The line to write for `value`, without the newline.
pub fn encode<T: Serialize>(value: &T) -> String {
    // These types hold only strings and numbers, which always serialize.
    serde_json::to_string(value).unwrap_or_default()
}

/// Parses one line.
pub fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(line.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_as_one_line() {
        let request = Request::Chat {
            id: 7,
            messages: vec![Message {
                role: "user".into(),
                content: "Xin chào\n\"bạn\"".into(),
            }],
            max_tokens: 200,
        };
        let line = encode(&request);
        assert!(!line.contains('\n'), "{line}");
        assert_eq!(decode::<Request>(&line).unwrap(), request);
        assert_eq!(
            decode::<Request>(&encode(&Request::Shutdown)).unwrap(),
            Request::Shutdown
        );
        assert_eq!(
            decode::<Request>(r#"{"type":"cancel","id":3}"#).unwrap(),
            Request::Cancel { id: 3 }
        );
    }

    #[test]
    fn events_round_trip() {
        for event in [
            Event::Ready {
                model: "m".into(),
                load_ms: 1200,
            },
            Event::Token {
                id: 1,
                text: "ạ".into(),
            },
            Event::Done { id: 1 },
            Event::Error {
                id: None,
                message: "no model".into(),
            },
            Event::Error {
                id: Some(2),
                message: "too long".into(),
            },
        ] {
            assert_eq!(decode::<Event>(&encode(&event)).unwrap(), event);
        }
    }

    #[test]
    fn a_line_that_is_not_a_message_is_an_error() {
        assert!(decode::<Request>("hello").is_err());
        assert!(decode::<Request>(r#"{"type":"nope"}"#).is_err());
    }
}
