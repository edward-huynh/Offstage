//! What the overlay is playing. The main window starts a session and the overlay reads it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    /// Read one saved content aloud from the teleprompter.
    Content,
    /// Listen to the call and help answer, with live captions.
    Interview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaySession {
    pub mode: PlayMode,
    /// The content to read. Only meaningful for [`PlayMode::Content`].
    #[serde(default)]
    pub content_id: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlayError {
    #[error("choose a content to play")]
    MissingContent,
}

impl PlaySession {
    pub fn content(id: impl Into<String>) -> Self {
        Self {
            mode: PlayMode::Content,
            content_id: Some(id.into()),
        }
    }

    pub fn interview() -> Self {
        Self {
            mode: PlayMode::Interview,
            content_id: None,
        }
    }

    /// Checks a session that came from outside. Content mode needs an id; interview mode
    /// has no use for one, so it is dropped.
    pub fn validated(self) -> Result<Self, PlayError> {
        match self.mode {
            PlayMode::Content => {
                if self
                    .content_id
                    .as_deref()
                    .is_none_or(|id| id.trim().is_empty())
                {
                    Err(PlayError::MissingContent)
                } else {
                    Ok(self)
                }
            }
            PlayMode::Interview => Ok(Self::interview()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_serialize_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&PlayMode::Content).unwrap(),
            r#""content""#
        );
        assert_eq!(
            serde_json::to_string(&PlayMode::Interview).unwrap(),
            r#""interview""#
        );
    }

    #[test]
    fn a_session_round_trips_through_json() {
        let session = PlaySession::content("s1a2b");
        let json = serde_json::to_string(&session).unwrap();
        assert_eq!(json, r#"{"mode":"content","content_id":"s1a2b"}"#);
        assert_eq!(serde_json::from_str::<PlaySession>(&json).unwrap(), session);
    }

    #[test]
    fn the_content_id_may_be_left_out() {
        let session: PlaySession = serde_json::from_str(r#"{"mode":"interview"}"#).unwrap();
        assert_eq!(session, PlaySession::interview());
    }

    #[test]
    fn content_mode_needs_an_id() {
        for missing in [None, Some(String::new()), Some("   ".to_owned())] {
            let session = PlaySession {
                mode: PlayMode::Content,
                content_id: missing,
            };
            assert_eq!(session.validated(), Err(PlayError::MissingContent));
        }
        assert!(PlaySession::content("s1").validated().is_ok());
    }

    #[test]
    fn interview_mode_drops_any_content_id() {
        let session = PlaySession {
            mode: PlayMode::Interview,
            content_id: Some("s1".into()),
        };
        assert_eq!(session.validated(), Ok(PlaySession::interview()));
    }
}
