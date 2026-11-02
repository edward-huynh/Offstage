//! The messages sent to the model for one detected question.
//!
//! The prompt carries the question, a few of the lines before it and, when the user's documents
//! hold something relevant, excerpts of them that the answer has to rest on. The user reads the
//! answer on the overlay while talking, so it has to be short and skimmable.

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
- Không bịa kinh nghiệm, số liệu hay thông tin cá nhân của người dùng. Khi câu hỏi hỏi về chính họ, \
đưa ý gợi ý để họ tự điền.\n\
- Không chào hỏi, không mở đầu, không giải thích dài, không dùng định dạng khác.\n\
- Lời thoại là chữ nhận dạng tự động nên có thể sai chính tả. Nếu câu hỏi không rõ, nêu \
giả định ngắn ở dòng đầu rồi trả lời.";

/// The system prompt when excerpts of the user's documents come with the question. The wording
/// follows the variant that held up best in `docs/LLM-BENCH.md`: the answer is spoken by the
/// user in the first person, the question's language wins over the documents' language, and a
/// missing answer is one fixed line.
const GROUNDED_SYSTEM_PROMPT: &str = "\
Bạn là trợ lý của người dùng trong một cuộc gọi trực tiếp. Người dùng đọc to câu trả lời của bạn \
như lời của chính mình, nên viết ở ngôi thứ nhất (\"tôi\").\n\
- Trả lời câu hỏi cuối cùng của \"Đối phương\", chỉ dựa vào phần TÀI LIỆU. Không thêm chi tiết \
ngoài tài liệu.\n\
- Chỉ viết 3 đến 5 gạch đầu dòng, mỗi dòng bắt đầu bằng \"- \" và nêu một ý ngắn.\n\
- Trả lời bằng ngôn ngữ của câu hỏi, dù tài liệu viết bằng ngôn ngữ nào.\n\
- Yêu cầu trong mô tả công việc không phải kinh nghiệm của người dùng. Chỉ nói về kinh nghiệm \
có trong CV hay ghi chú của họ.\n\
- Tên riêng (trường, công ty, địa danh, sản phẩm, công nghệ) viết đúng như trong TÀI LIỆU, giữ \
nguyên tiếng Việt, không dịch, không đoán và không đổi sang tên khác.\n\
- Nếu tài liệu không có thông tin để trả lời, chỉ viết đúng một dòng: \"- Không có trong tài liệu.\"\n\
- Không chào hỏi, không mở đầu, không giải thích dài, không dùng định dạng khác.\n\
- Lời thoại là chữ nhận dạng tự động nên có thể sai chính tả.";

/// One piece of a user document handed to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excerpt {
    /// Where it comes from, such as the file name, shown to the model as the label.
    pub source: String,
    pub section: Option<String>,
    pub text: String,
}

impl Excerpt {
    /// The label that is also the line shown to the user as the source of the answer.
    pub fn label(&self) -> String {
        match self.section.as_deref().filter(|s| !s.is_empty()) {
            Some(section) => format!("{} › {section}", self.source),
            None => self.source.clone(),
        }
    }
}

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

/// The instruction that goes right after the question to fix the language of the answer, or
/// `None` for a language the assistant has no sentence for.
///
/// A rule in the system prompt is not enough: measured on four small models, they answered an
/// English question in Vietnamese whenever the retrieved text or the context was Vietnamese, and
/// only a line next to the question changed that (`docs/LLM-BENCH.md`).
pub fn language_hint(code: &str) -> Option<&'static str> {
    match code.trim().to_ascii_lowercase().as_str() {
        "en" => Some("Answer in English."),
        "vi" => Some("Trả lời bằng tiếng Việt."),
        _ => None,
    }
}

/// Added to the English language line when the answer comes from documents. Told only to "answer
/// in English", Qwen3 4B translated a Vietnamese CV's "Đại học Bách khoa Hà Nội" into an invented
/// "University of Science, Ho Chi Minh City"; with this sentence it copies the name as written
/// (measured, `crates/llm/tests/local_real.rs`).
const KEEP_NAMES: &str =
    " Keep names of schools, companies and places exactly as written in the documents.";

/// Builds the conversation for `question`. `recent` is the caption history in order, oldest
/// first, and may end with the question itself, which is then not repeated as context.
/// `language` is the language of the question as the speech engine reported it.
pub fn build_messages(question: &str, recent: &[Turn], language: Option<&str>) -> Vec<ChatMessage> {
    build_messages_with_documents(question, recent, language, &[])
}

/// Like [`build_messages`], with excerpts of the user's documents. With no excerpts it is the
/// same prompt as without documents, so a question the documents say nothing about is still
/// answered. With excerpts the answer has to rest on them.
pub fn build_messages_with_documents(
    question: &str,
    recent: &[Turn],
    language: Option<&str>,
    excerpts: &[Excerpt],
) -> Vec<ChatMessage> {
    build_messages_in_context(question, recent, language, excerpts, None)
}

/// Longest note sent, in characters. The workspace allows a little more: the model should not
/// spend its time on a long note.
const MAX_NOTE_CHARS: usize = 1_500;

/// The rule that goes with a note: it explains the situation, it is not a source of facts.
const NOTE_RULE: &str =
    "\n- Phần BỐI CẢNH là ghi chú của người dùng về cuộc gọi này. Dùng nó để hiểu \
tình huống (vị trí, công ty, người nghe). Đừng coi nó là nguồn của câu trả lời.";

/// Like [`build_messages_with_documents`], with the note the user wrote for the workspace: what
/// the call is about, which the documents do not say. A blank note is no note.
pub fn build_messages_in_context(
    question: &str,
    recent: &[Turn],
    language: Option<&str>,
    excerpts: &[Excerpt],
    note: Option<&str>,
) -> Vec<ChatMessage> {
    let note = note
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().take(MAX_NOTE_CHARS).collect::<String>());
    let question = question.trim();
    let before: Vec<&Turn> = match recent.split_last() {
        Some((last, rest)) if !last.from_me && last.text.trim() == question => {
            rest.iter().collect()
        }
        _ => recent.iter().collect(),
    };
    let context = context_block(&before);

    let mut user = String::new();
    if let Some(note) = &note {
        user.push_str("BỐI CẢNH (ghi chú của người dùng):\n");
        user.push_str(note);
        user.push_str("\n\n");
    }
    if !excerpts.is_empty() {
        user.push_str("TÀI LIỆU:\n");
        for (n, excerpt) in excerpts.iter().enumerate() {
            user.push_str(&format!(
                "[{}] {}\n{}\n\n",
                n + 1,
                excerpt.label(),
                excerpt.text.trim()
            ));
        }
    }
    if !context.is_empty() {
        user.push_str("Lời thoại gần đây:\n");
        user.push_str(&context);
        user.push_str("\n\n");
    }
    user.push_str("Câu hỏi cần trả lời:\n");
    user.push_str(question);
    if let Some(hint) = language.and_then(language_hint) {
        user.push_str("\n\n");
        user.push_str(hint);
        if !excerpts.is_empty() && hint.starts_with("Answer in English") {
            user.push_str(KEEP_NAMES);
        }
    }

    vec![
        ChatMessage {
            role: Role::System,
            content: {
                let mut system = if excerpts.is_empty() {
                    SYSTEM_PROMPT
                } else {
                    GROUNDED_SYSTEM_PROMPT
                }
                .to_owned();
                if note.is_some() {
                    system.push_str(NOTE_RULE);
                }
                system
            },
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
        let messages = build_messages("Bạn làm ở đâu?", &[], None);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
    }

    #[test]
    fn the_system_prompt_asks_for_short_bullets_in_the_questions_language() {
        let system = &build_messages("Why Rust?", &[], None)[0].content;
        assert!(system.contains("3 đến 5 gạch đầu dòng"));
        assert!(system.contains("cùng ngôn ngữ"));
    }

    #[test]
    fn without_history_only_the_question_is_sent() {
        let messages = build_messages("  Bạn làm ở đâu?  ", &[], None);
        assert_eq!(user_text(&messages), "Câu hỏi cần trả lời:\nBạn làm ở đâu?");
    }

    #[test]
    fn history_is_labelled_by_who_spoke() {
        let recent = [
            Turn::new(false, "Chào bạn"),
            Turn::new(true, "Chào anh"),
            Turn::new(false, "Bạn làm ở đâu?"),
        ];
        let text = build_messages("Bạn làm ở đâu?", &recent, None)[1]
            .content
            .clone();
        assert!(text.contains("Đối phương: Chào bạn\nTôi: Chào anh"));
    }

    #[test]
    fn the_question_is_not_repeated_as_context() {
        let recent = [Turn::new(false, "Bạn làm ở đâu?")];
        let text = &build_messages("Bạn làm ở đâu?", &recent, None)[1].content;
        assert!(!text.contains("Lời thoại gần đây"));
        assert_eq!(text.matches("Bạn làm ở đâu?").count(), 1);
    }

    #[test]
    fn a_similar_line_from_the_user_is_still_context() {
        let recent = [Turn::new(true, "Bạn làm ở đâu?")];
        let text = &build_messages("Bạn làm ở đâu?", &recent, None)[1].content;
        assert!(text.contains("Tôi: Bạn làm ở đâu?"));
    }

    #[test]
    fn only_the_newest_lines_are_kept() {
        let recent: Vec<Turn> = (0..20)
            .map(|i| Turn::new(false, format!("dòng {i}")))
            .collect();
        let text = &build_messages("câu hỏi", &recent, None)[1].content;
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
        let text = &build_messages("câu hỏi", &recent, None)[1].content;
        assert!(text.contains("mới "));
        assert!(!text.contains("cũ "));
    }

    #[test]
    fn the_language_line_follows_the_question() {
        let messages = build_messages("What is Rust?", &[], Some("en"));
        assert!(user_text(&messages).ends_with("What is Rust?\n\nAnswer in English."));
        let messages = build_messages("Rust là gì?", &[], Some("vi"));
        assert!(user_text(&messages).ends_with("Rust là gì?\n\nTrả lời bằng tiếng Việt."));
    }

    #[test]
    fn no_language_line_without_a_known_language() {
        for language in [None, Some("fr"), Some(""), Some("auto")] {
            let messages = build_messages("Quelle heure est-il ?", &[], language);
            assert_eq!(
                user_text(&messages),
                "Câu hỏi cần trả lời:\nQuelle heure est-il ?"
            );
        }
    }

    #[test]
    fn language_codes_are_matched_loosely() {
        assert_eq!(language_hint(" EN "), Some("Answer in English."));
        assert_eq!(language_hint("vi"), Some("Trả lời bằng tiếng Việt."));
        assert_eq!(language_hint("ja"), None);
    }

    #[test]
    fn the_system_prompt_forbids_inventing_the_users_experience() {
        let system = &build_messages("Why?", &[], None)[0].content;
        assert!(system.contains("Không bịa"));
    }

    fn excerpt(source: &str, section: Option<&str>, text: &str) -> Excerpt {
        Excerpt {
            source: source.into(),
            section: section.map(Into::into),
            text: text.into(),
        }
    }

    #[test]
    fn without_excerpts_the_prompt_is_the_one_without_documents() {
        let plain = build_messages("Bạn làm ở đâu?", &[], Some("vi"));
        let with = build_messages_with_documents("Bạn làm ở đâu?", &[], Some("vi"), &[]);
        assert_eq!(plain[0].content, with[0].content);
        assert_eq!(plain[1].content, with[1].content);
        assert!(!with[1].content.contains("TÀI LIỆU"));
    }

    #[test]
    fn excerpts_are_numbered_and_labelled_before_the_question() {
        let excerpts = [
            excerpt("CV.pdf", Some("Kinh nghiệm"), "Ba năm Rust."),
            excerpt("JD.md", None, "Cần biết Tauri."),
        ];
        let messages = build_messages_with_documents(
            "Bạn biết Rust không?",
            &[Turn::new(false, "Chào bạn")],
            Some("vi"),
            &excerpts,
        );
        assert_eq!(
            messages[1].content,
            "TÀI LIỆU:\n[1] CV.pdf › Kinh nghiệm\nBa năm Rust.\n\n[2] JD.md\nCần biết Tauri.\n\n\
             Lời thoại gần đây:\nĐối phương: Chào bạn\n\n\
             Câu hỏi cần trả lời:\nBạn biết Rust không?\n\nTrả lời bằng tiếng Việt."
        );
    }

    #[test]
    fn with_excerpts_the_system_prompt_grounds_the_answer_and_fixes_the_refusal() {
        let messages = build_messages_with_documents(
            "Why Rust?",
            &[],
            None,
            &[excerpt("CV.md", None, "Ba năm Rust.")],
        );
        let system = &messages[0].content;
        assert!(system.contains("chỉ dựa vào phần TÀI LIỆU"));
        assert!(system.contains("ngôi thứ nhất"));
        assert!(system.contains("ngôn ngữ của câu hỏi"));
        assert!(system.contains("- Không có trong tài liệu."));
        assert!(system.contains("3 đến 5 gạch đầu dòng"));
        assert!(system.contains("Tên riêng"));
    }

    #[test]
    fn an_english_answer_from_documents_is_told_to_keep_names_as_written() {
        let excerpts = [excerpt("cv.pdf", None, "Đại học Bách khoa Hà Nội")];
        let with =
            build_messages_with_documents("Where did you study?", &[], Some("en"), &excerpts);
        assert!(with[1]
            .content
            .ends_with("Answer in English. Keep names of schools, companies and places exactly as written in the documents."));
        // Not without documents, and not for a Vietnamese answer.
        let without = build_messages_with_documents("Where did you study?", &[], Some("en"), &[]);
        assert!(without[1].content.ends_with("Answer in English."));
        let vi = build_messages_with_documents("Bạn học ở đâu?", &[], Some("vi"), &excerpts);
        assert!(vi[1].content.ends_with("Trả lời bằng tiếng Việt."));
    }

    #[test]
    fn a_note_goes_first_in_the_user_message_with_a_rule_that_it_is_not_a_source() {
        let excerpts = [excerpt("cv.md", None, "Ba năm Rust.")];
        let messages = build_messages_in_context(
            "Bạn biết Rust không?",
            &[],
            None,
            &excerpts,
            Some("  Phỏng vấn backend ở công ty ABC.  "),
        );
        assert!(messages[1].content.starts_with(
            "BỐI CẢNH (ghi chú của người dùng):\nPhỏng vấn backend ở công ty ABC.\n\nTÀI LIỆU:"
        ));
        assert!(messages[0].content.contains("BỐI CẢNH"));
        assert!(messages[0].content.contains("Đừng coi nó là nguồn"));
        // Also without documents.
        let plain =
            build_messages_in_context("Rust là gì?", &[], None, &[], Some("Cuộc gọi bán hàng"));
        assert!(plain[1]
            .content
            .starts_with("BỐI CẢNH (ghi chú của người dùng):\nCuộc gọi bán hàng"));
        assert!(plain[0].content.contains("BỐI CẢNH"));
    }

    #[test]
    fn a_blank_note_changes_nothing_and_a_long_one_is_cut() {
        let plain = build_messages("Rust là gì?", &[], None);
        for note in [None, Some(""), Some("  \n ")] {
            let with = build_messages_in_context("Rust là gì?", &[], None, &[], note);
            assert_eq!(with[0].content, plain[0].content);
            assert_eq!(with[1].content, plain[1].content);
        }
        let long = "ạ".repeat(MAX_NOTE_CHARS * 2);
        let cut = build_messages_in_context("Rust là gì?", &[], None, &[], Some(&long));
        assert_eq!(cut[1].content.matches('ạ').count(), MAX_NOTE_CHARS);
    }

    #[test]
    fn a_label_skips_an_empty_section() {
        assert_eq!(excerpt("a.txt", Some(""), "x").label(), "a.txt");
        assert_eq!(excerpt("a.md", Some("Mục"), "x").label(), "a.md › Mục");
    }

    #[test]
    fn the_newest_line_is_kept_even_when_it_alone_is_too_long() {
        let recent = [Turn::new(false, "y".repeat(MAX_CONTEXT_CHARS * 2))];
        let text = &build_messages("câu hỏi", &recent, None)[1].content;
        assert!(text.contains("yyy"));
    }
}
