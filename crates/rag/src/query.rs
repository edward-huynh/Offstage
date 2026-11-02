//! Turns a spoken question into an FTS5 query.
//!
//! The question comes from speech recognition, so it is full of filler ("bạn có thể cho tôi
//! biết...", "can you tell me...") that would match every document. The words that carry the
//! topic are kept and joined with OR: BM25 then ranks chunks by how many of them they hold and
//! how rare those words are.

use std::collections::HashSet;

/// Most terms of one query. A long question adds noise faster than it adds recall.
const MAX_TERMS: usize = 12;

/// Words that say nothing about the topic. Vietnamese words are one syllable each here because
/// the index splits text the same way, so "tại sao" is two entries. Words with a topic meaning
/// in either language ("làm", "work") are deliberately not listed.
const STOPWORDS: &[&str] = &[
    // English
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "am", "do", "does", "did", "to",
    "of", "in", "on", "at", "for", "and", "or", "but", "with", "what", "which", "who", "whom",
    "how", "why", "when", "where", "you", "your", "yours", "me", "my", "we", "our", "they",
    "their", "it", "its", "this", "that", "these", "those", "can", "could", "would", "should",
    "will", "have", "has", "had", "tell", "about", "please", "any", "some", "as", "by", "from",
    // Vietnamese
    "và", "của", "là", "có", "không", "được", "cho", "với", "một", "các", "những", "này", "đó",
    "thì", "mà", "cũng", "để", "trong", "khi", "nào", "gì", "sao", "như", "thế", "bạn", "tôi",
    "anh", "chị", "em", "hãy", "xin", "vui", "lòng", "hỏi", "nói", "về", "ra", "đã", "sẽ", "đang",
    "bao", "nhiêu", "ở", "đâu", "ai", "tại", "vì", "nên", "hay", "hoặc", "rằng", "nhé", "thể",
    "biết", "giúp",
];

/// The lowercase words of `text`, the way the index splits it: runs of letters and digits.
pub fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// Whether a word of a question can be searched for: not a single letter, not filler.
fn is_topic_word(word: &str) -> bool {
    (word.chars().count() >= 2 || word.chars().all(|c| c.is_ascii_digit()))
        && !STOPWORDS.contains(&word)
}

/// The words of `question` worth searching for, in order, without repeats.
pub fn terms(question: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    words(question)
        .filter(|w| is_topic_word(w))
        .filter(|w| seen.insert(w.clone()))
        .take(MAX_TERMS)
        .collect()
}

/// Runs of words that follow each other without punctuation that ends a sentence.
fn segments(text: &str) -> Vec<Vec<String>> {
    let mut segments = vec![Vec::new()];
    let mut word = String::new();
    let mut chars = text.chars().peekable();
    let end_word = |word: &mut String, segments: &mut Vec<Vec<String>>| {
        if !word.is_empty() {
            if let Some(last) = segments.last_mut() {
                last.push(word.to_lowercase());
            }
            word.clear();
        }
    };
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() {
            word.push(c);
            continue;
        }
        end_word(&mut word, &mut segments);
        // "Node.js" is one name, "Rust. Tauri" is two sentences.
        let ends_sentence = match c {
            '\n' | '?' | '!' | ';' => true,
            '.' => chars.peek().is_none_or(|n| n.is_whitespace()),
            _ => false,
        };
        if ends_sentence && segments.last().is_some_and(|s| !s.is_empty()) {
            segments.push(Vec::new());
        }
    }
    end_word(&mut word, &mut segments);
    segments
}

/// A pair of words that follow each other.
pub type Pair = (String, String);

/// Pairs of topic words that are next to each other in `question`: "kinh nghiệm", "dự án",
/// "event loop". The index splits Vietnamese into syllables, so a word like "chạy bộ" is only
/// told apart from "chạy song song ... đồng bộ" by the two syllables standing together.
pub fn pairs(question: &str) -> HashSet<Pair> {
    segments(question)
        .iter()
        .flat_map(|s| s.windows(2))
        .filter(|w| is_topic_word(&w[0]) && is_topic_word(&w[1]))
        .map(|w| (w[0].clone(), w[1].clone()))
        .collect()
}

/// Every pair of neighbouring words in `text`.
pub fn pairs_in(text: &str) -> HashSet<Pair> {
    segments(text)
        .iter()
        .flat_map(|s| s.windows(2))
        .map(|w| (w[0].clone(), w[1].clone()))
        .collect()
}

/// An FTS5 `MATCH` expression for the terms and the pairs, or `None` when there is nothing to
/// search for. Each term is quoted, so it is never read as FTS syntax. The pairs are searched as
/// phrases next to the single words: without them, a Vietnamese question made of common syllables
/// ("kiểm tra dữ liệu đầu vào") ranks the many chunks that hold some of the syllables above the one
/// that holds the phrase, and the phrase's chunk can fall outside the candidates.
pub fn fts_query(terms: &[String], pairs: &HashSet<Pair>) -> Option<String> {
    if terms.is_empty() {
        return None;
    }
    let mut phrases: Vec<String> = pairs.iter().map(|(a, b)| format!("\"{a} {b}\"")).collect();
    // A set has no order, and the same question should give the same query.
    phrases.sort();
    Some(
        terms
            .iter()
            .map(|t| format!("\"{t}\""))
            .chain(phrases)
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// The distinct words of `text`.
pub fn word_set(text: &str) -> HashSet<String> {
    words(text).collect()
}

/// Whether a term looks like a name or a technology (PostgreSQL, Tauri, k8s) rather than an
/// everyday word. Such a word is strong evidence by itself when few chunks hold it. Plain ASCII
/// is the sign: Vietnamese words carry accents, and the English ones that survive the stopword
/// list are checked against how common they are in the documents.
pub fn looks_distinctive(term: &str) -> bool {
    term.is_ascii() && (term.len() >= 3 || term.chars().any(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filler_words_are_dropped_and_topic_words_kept() {
        assert_eq!(
            terms("Bạn có thể cho tôi biết kinh nghiệm làm việc với Rust không?"),
            ["kinh", "nghiệm", "làm", "việc", "rust"]
        );
        assert_eq!(
            terms("Can you tell me about your experience with PostgreSQL?"),
            ["experience", "postgresql"]
        );
    }

    #[test]
    fn terms_are_lowercase_unique_and_capped() {
        assert_eq!(terms("Rust RUST rust"), ["rust"]);
        let long = (0..40)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(terms(&long).len(), MAX_TERMS);
    }

    #[test]
    fn single_letters_are_dropped_but_numbers_are_kept() {
        assert_eq!(terms("a b 5 go"), ["5", "go"]);
    }

    #[test]
    fn punctuation_never_reaches_the_query() {
        let t = terms(r#"what's "NEAR" (a OR b) AND c*"#);
        let q = fts_query(&t, &HashSet::new()).unwrap();
        assert!(q.chars().all(|c| c != '(' && c != '*' && c != '\''));
        assert_eq!(q, r#""near" OR "or""#.replace(" OR \"or\"", ""));
    }

    #[test]
    fn nothing_to_search_gives_no_query() {
        assert_eq!(fts_query(&terms("bạn có không?"), &HashSet::new()), None);
        assert_eq!(fts_query(&[], &HashSet::new()), None);
    }

    #[test]
    fn terms_join_with_or_and_pairs_follow_as_phrases() {
        let question = "kinh nghiệm";
        assert_eq!(
            fts_query(&terms(question), &pairs(question)).as_deref(),
            Some(r#""kinh" OR "nghiệm" OR "kinh nghiệm""#)
        );
        assert_eq!(
            fts_query(&["kinh".into(), "nghiệm".into()], &HashSet::new()).as_deref(),
            Some(r#""kinh" OR "nghiệm""#)
        );
    }

    #[test]
    fn neighbouring_topic_words_make_pairs_and_filler_breaks_them() {
        let p = pairs("Bạn có kinh nghiệm làm dự án Event loop không?");
        assert!(p.contains(&("kinh".into(), "nghiệm".into())));
        assert!(p.contains(&("làm".into(), "dự".into())));
        assert!(p.contains(&("event".into(), "loop".into())));
        // "có" is filler, so "bạn có kinh" yields nothing across it.
        assert!(!p.iter().any(|(a, b)| a == "có" || b == "có"));
        assert!(!p.contains(&("nghiệm".into(), "làm".into())) || p.len() > 3);
    }

    #[test]
    fn pairs_do_not_cross_a_sentence_end_but_cross_a_dot_inside_a_name() {
        let p = pairs_in("Thích chạy. Bộ nhớ lớn. Dùng Node.js nhiều.");
        assert!(!p.contains(&("chạy".into(), "bộ".into())));
        assert!(p.contains(&("bộ".into(), "nhớ".into())));
        assert!(p.contains(&("node".into(), "js".into())));
    }

    #[test]
    fn pairs_of_the_text_include_all_neighbours() {
        let p = pairs_in("Ba năm viết Rust");
        assert_eq!(p.len(), 3);
        assert!(p.contains(&("năm".into(), "viết".into())));
    }
}
