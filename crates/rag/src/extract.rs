//! Reads the text out of a stored document. Markdown and plain text are used as they are, a
//! DOCX is unzipped and its paragraphs read from `word/document.xml`, a PDF goes through
//! `pdf-extract`. Everything runs in Rust on the user's machine.
//!
//! The error messages are shown to the user (they explain why the AI cannot read a file), so
//! they are written in Vietnamese.

use std::{io::Read, panic};

use offstage_core::knowledge::KnowledgeKind;
use quick_xml::{events::Event, Reader};

/// Most bytes read out of a DOCX's `document.xml` after unzipping. A small zip can expand to
/// gigabytes; a real 20 MB document does not come close to this.
const MAX_XML_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExtractError {
    #[error("Tệp không có chữ để đọc. Nếu là PDF quét từ ảnh thì cần một bản có chữ.")]
    NoText,
    #[error("Không đọc được nội dung tệp: {0}")]
    Unreadable(String),
}

/// The text of a document of the given kind. Lines end in `\n`, and paragraphs are separated
/// by a blank line. Headings of a DOCX come out as Markdown `##` lines so they become sections.
pub fn extract_text(kind: KnowledgeKind, bytes: &[u8]) -> Result<String, ExtractError> {
    let text = match kind {
        KnowledgeKind::Markdown | KnowledgeKind::Text => plain(bytes),
        KnowledgeKind::Docx => docx(bytes)?,
        KnowledgeKind::Pdf => pdf(bytes)?,
    };
    let text = normalize(&text);
    if text.is_empty() {
        return Err(ExtractError::NoText);
    }
    Ok(text)
}

fn plain(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.trim_start_matches('\u{feff}').to_owned()
}

/// One newline style, no trailing spaces, at most one blank line in a row.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text
        .replace("\r\n", "\n")
        .replace(['\r', '\u{c}'], "\n")
        .lines()
    {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

fn pdf(bytes: &[u8]) -> Result<String, ExtractError> {
    // The parser can panic on a damaged file. That must end in a message, not a crash.
    panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes))
        .map_err(|_| ExtractError::Unreadable("tệp PDF bị hỏng".to_owned()))?
        .map_err(|e| ExtractError::Unreadable(e.to_string()))
}

fn docx(bytes: &[u8]) -> Result<String, ExtractError> {
    let unreadable = |e: &dyn std::fmt::Display| ExtractError::Unreadable(e.to_string());
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| unreadable(&e))?;
    let mut xml = Vec::new();
    archive
        .by_name("word/document.xml")
        .map_err(|_| ExtractError::Unreadable("không phải tệp DOCX".to_owned()))?
        .take(MAX_XML_BYTES)
        .read_to_end(&mut xml)
        .map_err(|e| unreadable(&e))?;
    paragraphs(&xml)
}

/// The paragraphs of a WordprocessingML body. Text sits in `w:t` runs, a tab or a line break
/// in `w:tab` and `w:br`, and a paragraph style named Heading or Title marks a heading.
fn paragraphs(xml: &[u8]) -> Result<String, ExtractError> {
    let mut reader = Reader::from_reader(xml);
    let mut out = String::new();
    let mut paragraph = String::new();
    let mut heading = false;
    let mut in_text = false;

    let finish = |paragraph: &mut String, heading: &mut bool, out: &mut String| {
        let text = paragraph.trim();
        if !text.is_empty() {
            if *heading {
                out.push_str("## ");
            }
            out.push_str(text);
            out.push_str("\n\n");
        }
        paragraph.clear();
        *heading = false;
    };

    loop {
        match reader
            .read_event()
            .map_err(|e| ExtractError::Unreadable(e.to_string()))?
        {
            Event::Start(e) | Event::Empty(e) => match e.local_name().as_ref() {
                "t" => in_text = true,
                "tab" => paragraph.push('\t'),
                "br" | "cr" => paragraph.push('\n'),
                "pStyle" => {
                    for attr in e.attributes().flatten() {
                        if attr.key.local_name().as_ref() == "val" {
                            let style = attr.value.to_lowercase();
                            heading = style.starts_with("heading") || style == "title";
                        }
                    }
                }
                _ => {}
            },
            Event::End(e) => match e.local_name().as_ref() {
                "t" => in_text = false,
                "p" => finish(&mut paragraph, &mut heading, &mut out),
                _ => {}
            },
            Event::Text(t) if in_text => {
                paragraph.push_str(&t.xml10_content());
            }
            Event::GeneralRef(r) if in_text => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    paragraph.push(c);
                } else if let Some(s) = quick_xml::escape::resolve_xml_entity(&r) {
                    paragraph.push_str(s);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    finish(&mut paragraph, &mut heading, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn docx_with(body: &str) -> Vec<u8> {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
        );
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn plain_text_is_normalized() {
        let text = extract_text(
            KnowledgeKind::Text,
            "\u{feff}Một  \r\n\r\n\r\n\r\nHai\r\n".as_bytes(),
        )
        .unwrap();
        assert_eq!(text, "Một\n\nHai");
    }

    #[test]
    fn blank_text_has_no_text() {
        assert_eq!(
            extract_text(KnowledgeKind::Markdown, b" \n\n "),
            Err(ExtractError::NoText)
        );
    }

    #[test]
    fn a_docx_gives_its_paragraphs_and_marks_headings() {
        let bytes = docx_with(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Kinh nghiệm</w:t></w:r></w:p>
               <w:p><w:r><w:t>Làm Rust </w:t></w:r><w:r><w:t xml:space="preserve">và Tauri &amp; SQLite</w:t></w:r></w:p>
               <w:p><w:r><w:t>Dòng</w:t><w:br/><w:t>hai</w:t></w:r></w:p>
               <w:p></w:p>"#,
        );
        let text = extract_text(KnowledgeKind::Docx, &bytes).unwrap();
        assert_eq!(
            text,
            "## Kinh nghiệm\n\nLàm Rust và Tauri & SQLite\n\nDòng\nhai"
        );
    }

    #[test]
    fn a_docx_without_text_has_no_text() {
        let bytes = docx_with("<w:p></w:p>");
        assert_eq!(
            extract_text(KnowledgeKind::Docx, &bytes),
            Err(ExtractError::NoText)
        );
    }

    #[test]
    fn a_file_that_is_not_a_docx_is_unreadable_not_a_panic() {
        assert!(matches!(
            extract_text(KnowledgeKind::Docx, b"not a zip"),
            Err(ExtractError::Unreadable(_))
        ));
        // A zip without the document part.
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            zip.start_file("other.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.finish().unwrap();
        }
        assert!(matches!(
            extract_text(KnowledgeKind::Docx, &buf),
            Err(ExtractError::Unreadable(_))
        ));
    }

    #[test]
    fn a_damaged_pdf_is_unreadable_not_a_panic() {
        assert!(matches!(
            extract_text(KnowledgeKind::Pdf, b"%PDF-1.7 garbage"),
            Err(ExtractError::Unreadable(_))
        ));
    }
}
