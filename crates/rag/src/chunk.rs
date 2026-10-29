//! Paragraph-based chunker that tracks Markdown headings as sections.
//! Size is measured in characters as a cheap proxy for tokens.

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
        let p = para.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(h) = p.lines().next().filter(|l| l.starts_with('#')) {
            flush(&mut buf, &section);
            section = Some(h.trim_start_matches('#').trim().to_string());
            let rest = p.lines().skip(1).collect::<Vec<_>>().join("\n");
            buf.push_str(&rest);
            continue;
        }
        if buf.chars().count() + p.chars().count() > opts.max_chars {
            flush(&mut buf, &section);
        }
        if !buf.is_empty() {
            buf.push_str("\n\n");
        }
        buf.push_str(p);
    }
    flush(&mut buf, &section);
    chunks
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
}
