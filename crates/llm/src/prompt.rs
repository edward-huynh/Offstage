//! The messages sent to the model for one detected question.
//!
//! There are no documents to ground the answer yet (Phase 4), so the prompt only carries the
//! question and a few of the lines before it. The user reads the answer on the overlay while
//! talking, so it has to be short and skimmable.

use crate::{ChatMessage, Role};

/// Most recent caption lines sent along with the question.
pub const MAX_CONTEXT_LINES: usize = 6;
/// Budget for the context, in characters. The oldest lines are dropped first.
pub const MAX_CONTEXT_CHARS: usize = 1_500;

const SYSTEM_PROMPT: &str = "\
Bạn là trợ lý của người dùng trong một cuộc gọi trực tiếp. Người dùng đọc câu trả lời của bạn \
trên màn hình ngay khi đang nói chuyện.\n\
- Trả lời câu hỏi cuối cùng của \"Đối phương\".\n\
- Chỉ viết 3 đến 5 gạch đầu dòng, mỗi dòng bắt đầu bằng \"- \" và nêu một ý ngắn.\n\
- Dùng cùng ngôn ngữ với câu hỏi.\n\
- Không chào hỏi, không mở đầu, không giải thích dài, không dùng định dạng khác.\n\
- Lời thoại là chữ nhận dạng tự động nên có thể sai chính tả. Nếu câu hỏi không rõ, nêu \
giả định ngắn ở dòng đầu rồi trả lời.";

/// One caption line before the question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// True for the user's microphone, false for the other side of the call.
    pub from_me: bool,
    pub text: String,
}

impl Turn {
    pub fn new(from_me: bool, text: impl Into<String>) -> Self {
        Self {
            from_me,
            text: text.into(),
        }
    }

    fn render(&self) -> String {
        let who = if self.from_me {
            "Tôi"
        } else {
            "Đối phương"
        };
        format!("{who}: {}", self.text.trim())
    }
}

/// Builds the conversation for `question`. `recent` is the caption history in order, oldest
/// first, and may end with the question itself, which is then not repeated as context.
pub fn build_messages(question: &str, recent: &[Turn]) -> Vec<ChatMessage> {
    let question = question.trim();
    let before: Vec<&Turn> = match recent.split_last() {
        Some((last, rest)) if !last.from_me && last.text.trim() == question => {
            rest.iter().collect()
        }
        _ => recent.iter().collect(),
    };
    let context = context_block(&before);

    let mut user = String::new();
    if !context.is_empty() {
        user.push_str("Lời thoại gần đây:\n");
        user.push_str(&context);
        user.push_str("\n\n");
    }
    user.push_str("Câu hỏi cần trả lời:\n");
    user.push_str(question);

    vec![
        ChatMessage {
            role: Role::System,
            content: SYSTEM_PROMPT.to_owned(),
        },
        ChatMessage {
            role: Role::User,
            content: user,
        },
    ]
}

/// The newest lines that fit the limits, in their original order.
fn context_block(turns: &[&Turn]) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut chars = 0;
    for turn in turns.iter().rev().take(MAX_CONTEXT_LINES) {
        let line = turn.render();
        let len = line.chars().count();
        // Always keep the newest line, even when it alone is over budget.
        if !kept.is_empty() && chars + len > MAX_CONTEXT_CHARS {
            break;
        }
        chars += len;
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_text(messages: &[ChatMessage]) -> &str {
        &messages[1].content
    }

    #[test]
    fn a_system_message_comes_first_then_the_user_message() {
        let messages = build_messages("Bạn làm ở đâu?", &[]);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
    }

    #[test]
    fn the_system_prompt_asks_for_short_bullets_in_the_questions_language() {
        let system = &build_messages("Why Rust?", &[])[0].content;
        assert!(system.contains("3 đến 5 gạch đầu dòng"));
        assert!(system.contains("cùng ngôn ngữ"));
    }

    #[test]
    fn without_history_only_the_question_is_sent() {
        let messages = build_messages("  Bạn làm ở đâu?  ", &[]);
        assert_eq!(user_text(&messages), "Câu hỏi cần trả lời:\nBạn làm ở đâu?");
    }

    #[test]
    fn history_is_labelled_by_who_spoke() {
        let recent = [
            Turn::new(false, "Chào bạn"),
            Turn::new(true, "Chào anh"),
            Turn::new(false, "Bạn làm ở đâu?"),
        ];
        let text = build_messages("Bạn làm ở đâu?", &recent)[1].content.clone();
        assert!(text.contains("Đối phương: Chào bạn\nTôi: Chào anh"));
    }

    #[test]
    fn the_question_is_not_repeated_as_context() {
        let recent = [Turn::new(false, "Bạn làm ở đâu?")];
        let text = &build_messages("Bạn làm ở đâu?", &recent)[1].content;
        assert!(!text.contains("Lời thoại gần đây"));
        assert_eq!(text.matches("Bạn làm ở đâu?").count(), 1);
    }

    #[test]
    fn a_similar_line_from_the_user_is_still_context() {
        let recent = [Turn::new(true, "Bạn làm ở đâu?")];
        let text = &build_messages("Bạn làm ở đâu?", &recent)[1].content;
        assert!(text.contains("Tôi: Bạn làm ở đâu?"));
    }

    #[test]
    fn only_the_newest_lines_are_kept() {
        let recent: Vec<Turn> = (0..20)
            .map(|i| Turn::new(false, format!("dòng {i}")))
            .collect();
        let text = &build_messages("câu hỏi", &recent)[1].content;
        assert!(text.contains("dòng 19"));
        assert!(text.contains(&format!("dòng {}", 20 - MAX_CONTEXT_LINES)));
        assert!(!text.contains(&format!("dòng {}\n", 19 - MAX_CONTEXT_LINES)));
    }

    #[test]
    fn long_history_is_cut_to_the_character_budget_oldest_first() {
        let long = "x".repeat(800);
        let recent = [
            Turn::new(false, format!("cũ {long}")),
            Turn::new(false, format!("giữa {long}")),
            Turn::new(false, format!("mới {long}")),
        ];
        let text = &build_messages("câu hỏi", &recent)[1].content;
        assert!(text.contains("mới "));
        assert!(!text.contains("cũ "));
    }

    #[test]
    fn the_newest_line_is_kept_even_when_it_alone_is_too_long() {
        let recent = [Turn::new(false, "y".repeat(MAX_CONTEXT_CHARS * 2))];
        let text = &build_messages("câu hỏi", &recent)[1].content;
        assert!(text.contains("yyy"));
    }
}
