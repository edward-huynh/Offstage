//! Guesses whether a caption is a question, so the assistant knows when to answer.
//!
//! It is a heuristic on purpose: the speech models often drop punctuation (PhoWhisper writes
//! lowercase text with no `?` at all), so it looks for Vietnamese question words and for
//! English questions that open with a question word or an auxiliary verb. Text is compared
//! without accents and case, so a caption that lost its diacritics still matches.
//!
//! It leans towards missing a question over answering a statement: a wrong answer covers the
//! script the user is reading, a missed one costs a key press.

use crate::scripts::fold_char;

/// Phrases that make a Vietnamese sentence a question wherever they appear.
const VI_PHRASES: &[&[&str]] = &[
    &["tai", "sao"],
    &["vi", "sao"],
    &["the", "nao"],
    &["nhu", "nao"],
    &["ra", "sao"],
    &["lam", "sao"],
    &["bao", "nhieu"],
    &["bao", "lau"],
    &["khi", "nao"],
    &["luc", "nao"],
    &["o", "dau"],
    &["la", "gi"],
    &["la", "ai"],
    &["gi", "vay"],
];

/// Words that end a Vietnamese question ("bạn nghĩ sao", "là ai") or open one ("ai là…").
const VI_EDGE: &[&str] = &["sao", "gi", "ai", "nhi"];

/// Words that turn a trailing "không" into the "có … không" question form.
const VI_YES_NO: &[&str] = &["co", "phai", "dung", "duoc", "la"];

/// Words that make an edge word a statement ("không có gì", "chẳng ai").
const VI_NEGATION: &[&str] = &["khong", "chang"];

/// English words that open a question, a yes/no one included.
const EN_OPENERS: &[&str] = &[
    "what", "why", "how", "when", "where", "who", "whom", "whose", "which", "can", "could",
    "would", "will", "do", "does", "did", "is", "are", "was", "were", "have", "has", "should",
    "describe", "explain",
];

/// English openers that only ask something when followed by "me" or "us" ("tell me about…").
const EN_REQUESTS: &[&str] = &["tell", "walk", "give", "show"];

/// Spoken filler that may come before the question word ("so, what…").
const EN_FILLER: &[&str] = &[
    "so", "ok", "okay", "and", "but", "well", "um", "uh", "right", "now", "then", "alright",
];

/// True when `text` looks like a question in Vietnamese or English.
pub fn is_question(text: &str) -> bool {
    if text.contains('?') || text.contains('？') {
        return true;
    }
    let words = words(text);
    if words.is_empty() {
        return false;
    }
    english_question(&words) || vietnamese_question(&words)
}

/// Lowercase words without accents, split at anything that is not a letter or digit.
fn words(text: &str) -> Vec<String> {
    let folded: String = text
        .chars()
        .filter_map(fold_char)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    folded.split_whitespace().map(str::to_owned).collect()
}

fn english_question(words: &[String]) -> bool {
    let mut rest = words;
    // Skip a few filler words, never the whole text.
    for _ in 0..3 {
        match rest {
            [first, tail @ ..] if !tail.is_empty() && EN_FILLER.contains(&first.as_str()) => {
                rest = tail;
            }
            _ => break,
        }
    }
    match rest {
        [first, ..] if EN_OPENERS.contains(&first.as_str()) => true,
        [first, second, ..] if EN_REQUESTS.contains(&first.as_str()) => {
            matches!(second.as_str(), "me" | "us")
        }
        _ => false,
    }
}

fn vietnamese_question(words: &[String]) -> bool {
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    let (Some(first), Some(last)) = (words.first(), words.last()) else {
        return false;
    };

    // "có … không": a yes/no question ends on "không" after a word that sets it up.
    if last == "khong" && words.len() > 1 && has(VI_YES_NO) {
        return true;
    }
    if VI_PHRASES
        .iter()
        .any(|phrase| words.windows(phrase.len()).any(|w| w == *phrase))
    {
        return true;
    }
    // A lone question word is weak evidence, so a negation cancels it.
    if has(VI_NEGATION) {
        return false;
    }
    // "nào" asks "which" wherever it stands ("bạn thích dự án nào hơn").
    if has(&["nao"]) && words.len() > 1 {
        return true;
    }
    let edge = |word: &String| VI_EDGE.contains(&word.as_str());
    (edge(last) && words.len() > 1) || (edge(first) && matches!(first.as_str(), "sao" | "ai"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_mark_is_enough() {
        assert!(is_question("Really?"));
        assert!(is_question("Bạn làm ở đâu?"));
        assert!(is_question("你好？"));
    }

    #[test]
    fn vietnamese_questions_without_a_question_mark() {
        for text in [
            "Bạn có thể nói qua về kinh nghiệm làm việc với Rust không",
            "Tại sao bạn muốn làm vị trí này",
            "Vì sao bạn rời công ty cũ",
            "Bạn xử lý xung đột trong nhóm như thế nào",
            "Bạn mong muốn mức lương bao nhiêu",
            "Bạn đã làm việc ở đâu trước đây",
            "Còn về hiệu năng thì sao",
            "Điểm yếu lớn nhất của bạn là gì",
            "Bạn thích dự án nào hơn",
            "Sao bạn lại chọn Rust",
            "Ai là người quyết định kiến trúc",
            "Bạn có kinh nghiệm với Kubernetes không",
            "Bạn làm việc đó được không",
            "Kể từ khi nào bạn dùng Rust",
        ] {
            assert!(is_question(text), "{text}");
        }
    }

    #[test]
    fn vietnamese_questions_that_lost_their_accents_still_match() {
        assert!(is_question("tai sao ban muon lam vi tri nay"));
        assert!(is_question("ban co kinh nghiem voi docker khong"));
        assert!(is_question("diem yeu cua ban la gi"));
    }

    #[test]
    fn vietnamese_statements_are_not_questions() {
        for text in [
            "Tôi đã dùng Rust hai năm cho các dịch vụ xử lý âm thanh",
            "Không có gì đâu, cảm ơn bạn",
            "Chẳng ai biết điều đó",
            "Tôi không biết gì về chuyện này",
            "Cảm ơn bạn đã dành thời gian",
            "Chúng tôi làm việc từ chín giờ sáng",
        ] {
            assert!(!is_question(text), "{text}");
        }
    }

    #[test]
    fn english_questions_by_their_opening_word() {
        for text in [
            "What is your biggest weakness",
            "Why do you want this job",
            "How do you handle conflict in a team",
            "Can you walk me through your last project",
            "Do you have experience with Kubernetes",
            "Tell me about yourself",
            "Walk me through your resume",
            "Describe a time you failed",
            "Explain how a hash map works",
        ] {
            assert!(is_question(text), "{text}");
        }
    }

    #[test]
    fn a_little_filler_before_the_question_is_skipped() {
        assert!(is_question("So, what is your biggest weakness"));
        assert!(is_question("Okay and then how do you handle conflict"));
    }

    #[test]
    fn english_statements_are_not_questions() {
        for text in [
            "I have used Rust for two years",
            "That is a great answer, thank you",
            "We shipped the project last month",
            "Tell the team I said hello",
            "Thanks for your time today",
        ] {
            assert!(!is_question(text), "{text}");
        }
    }

    #[test]
    fn filler_alone_is_not_a_question() {
        assert!(!is_question("so"));
        assert!(!is_question("okay"));
        assert!(!is_question(""));
        assert!(!is_question("   "));
        assert!(!is_question("..."));
    }

    #[test]
    fn a_trailing_edge_word_needs_something_before_it() {
        // A lone "sao" or "gì" is more likely a fragment than a question.
        assert!(!is_question("gì"));
        assert!(!is_question("nào"));
    }
}
