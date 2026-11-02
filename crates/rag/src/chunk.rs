//! Paragraph-based chunker that tracks Markdown headings as sections.
//! Size is measured in characters as a cheap proxy for tokens.
//!
//! Plain text (a PDF, a pasted note) goes through the same function: it has no headings, so
//! the chunks only follow paragraphs, and a paragraph longer than the limit is cut at line or
//! sentence ends, because text from a PDF often has no blank lines at all.

use crate::Chunk;

pub struct ChunkOptions {
    pub max_chars: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        // Roughly 400 tokens of mixed Vietnamese/English text.
        Self { max_chars: 1600 }
    }
}

pub fn chunk_markdown(doc: &str, text: &str, opts: &ChunkOptions) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut section: Option<String> = None;
    let mut buf = String::new();

    let mut flush = |buf: &mut String, section: &Option<String>| {
        let content = buf.trim();
        if !content.is_empty() {
            chunks.push(Chunk {
                doc: doc.to_string(),
                section: section.clone(),
                content: content.to_string(),
            });
        }
        buf.clear();
    };

    for para in text.split("\n\n") {
        let mut p = para.trim().to_owned();
        if p.is_empty() {
            continue;
        }
        if let Some(h) = p.lines().next().filter(|l| l.starts_with('#')) {
            flush(&mut buf, &section);
            section = Some(h.trim_start_matches('#').trim().to_string());
            // What follows the heading in the same paragraph is body text like any other.
            p = p.lines().skip(1).collect::<Vec<_>>().join("\n");
            if p.trim().is_empty() {
                continue;
            }
        }
        let len = p.chars().count();
        if len > opts.max_chars {
            flush(&mut buf, &section);
            for piece in split_to_fit(&p, opts.max_chars) {
                buf.push_str(&piece);
                flush(&mut buf, &section);
            }
            continue;
        }
        if buf.chars().count() + len > opts.max_chars {
            flush(&mut buf, &section);
        }
        if !buf.is_empty() {
            buf.push_str("\n\n");
        }
        buf.push_str(&p);
    }
    flush(&mut buf, &section);
    chunks
}

/// Cuts `text` into pieces of at most `max` characters, at line ends or after sentence
/// punctuation. A stretch with neither is cut at the last space, and as a last resort at `max`.
fn split_to_fit(text: &str, max: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for part in text.split_inclusive(['\n', '.', '?', '!']) {
        let len = part.chars().count();
        if used + len > max && !current.is_empty() {
            pieces.push(std::mem::take(&mut current));
            used = 0;
        }
        if len > max {
            pieces.extend(cut_at_spaces(part, max));
        } else {
            current.push_str(part);
            used += len;
        }
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

fn cut_at_spaces(text: &str, max: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while rest.chars().count() > max {
        let limit = rest
            .char_indices()
            .nth(max)
            .map_or(rest.len(), |(index, _)| index);
        let cut = rest[..limit]
            .rfind(char::is_whitespace)
            .filter(|&i| i > 0)
            .unwrap_or(limit);
        pieces.push(rest[..cut].to_owned());
        rest = rest[cut..].trim_start();
    }
    if !rest.is_empty() {
        pieces.push(rest.to_owned());
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_by_heading_and_keeps_section() {
        let md = "# Giới thiệu\n\nĐoạn một.\n\n## Kinh nghiệm\n\nĐoạn hai.";
        let c = chunk_markdown("cv.md", md, &ChunkOptions::default());
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].section.as_deref(), Some("Giới thiệu"));
        assert_eq!(c[1].section.as_deref(), Some("Kinh nghiệm"));
        assert_eq!(c[1].content, "Đoạn hai.");
    }

    #[test]
    fn respects_max_size() {
        let md = ["a".repeat(100), "b".repeat(100), "c".repeat(100)].join("\n\n");
        let c = chunk_markdown("x.md", &md, &ChunkOptions { max_chars: 150 });
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn a_long_paragraph_without_blank_lines_is_cut_at_line_ends() {
        let line = "Một dòng của tệp PDF có độ dài vừa phải.";
        let text = vec![line; 30].join("\n");
        let c = chunk_markdown("cv.pdf", &text, &ChunkOptions { max_chars: 200 });
        assert!(c.len() > 5);
        for chunk in &c {
            assert!(chunk.content.chars().count() <= 200, "{}", chunk.content);
            // Cut at line ends, so no line is torn in two.
            assert!(
                chunk.content.lines().all(|l| l == line),
                "{}",
                chunk.content
            );
        }
    }

    #[test]
    fn a_single_run_of_words_is_cut_at_spaces_not_inside_a_word() {
        let text = vec!["Việt"; 100].join(" ");
        let c = chunk_markdown("x.txt", &text, &ChunkOptions { max_chars: 50 });
        assert!(c.len() > 1);
        for chunk in &c {
            assert!(chunk.content.chars().count() <= 50);
            assert!(chunk.content.split(' ').all(|w| w == "Việt"));
        }
    }

    #[test]
    fn a_word_longer_than_the_limit_is_cut_by_length_and_nothing_is_lost() {
        let text = "x".repeat(250);
        let c = chunk_markdown("x.txt", &text, &ChunkOptions { max_chars: 100 });
        assert_eq!(c.len(), 3);
        assert_eq!(c.iter().map(|c| c.content.len()).sum::<usize>(), 250);
    }

    #[test]
    fn sections_survive_a_long_paragraph() {
        let text = format!("# Kinh nghiệm\n\n{}", "Câu ngắn. ".repeat(60));
        let c = chunk_markdown("cv.md", &text, &ChunkOptions { max_chars: 100 });
        assert!(c.len() > 2);
        assert!(c
            .iter()
            .all(|c| c.section.as_deref() == Some("Kinh nghiệm")));
    }

    #[test]
    fn empty_and_blank_text_gives_no_chunks() {
        assert!(chunk_markdown("x", "", &ChunkOptions::default()).is_empty());
        assert!(chunk_markdown("x", " \n\n \n", &ChunkOptions::default()).is_empty());
    }
}
