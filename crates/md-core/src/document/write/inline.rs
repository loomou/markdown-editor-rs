use super::MarkdownExport;
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::editor_options;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

pub(crate) fn trim_end_newlines(s: &str) -> &str {
    s.trim_end_matches(['\n', '\r'])
}

pub(crate) fn phrasing_export<D: MarkdownExport>(doc: &D, id: NodeId) -> &str {
    let source = doc.leaf_source(id);
    if source.is_empty() {
        doc.display(id)
    } else {
        source
    }
}

pub(crate) fn phrasing_source(doc: &Document, id: NodeId) -> String {
    phrasing_export(doc, id).to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeadingBlock {
    Paragraph,
    Heading,
}

pub(crate) fn paragraph_export<D: MarkdownExport>(doc: &D, id: NodeId) -> String {
    escape_leading_block_markers(phrasing_export(doc, id), LeadingBlock::Paragraph)
}

fn block_marker_body(line: &str) -> Option<&str> {
    let mut col = 0usize;
    let mut index = 0usize;
    for (i, b) in line.bytes().enumerate() {
        match b {
            b' ' => col += 1,
            b'\t' => col = (col / 4 + 1) * 4,
            _ => break,
        }
        index = i + 1;
    }
    if col > 3 {
        return None;
    }
    Some(&line[index..])
}

fn is_thematic_break_start(s: &str, marker: u8) -> bool {
    let mut count = 0usize;
    for b in s.bytes() {
        if b == marker {
            count += 1;
        } else if b != b' ' && b != b'\t' {
            return false;
        }
    }
    count >= 3
}

fn is_block_marker_line(line: &str) -> bool {
    let Some(s) = block_marker_body(line) else {
        return false;
    };
    let bytes = s.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    match first {
        b'>' => true,
        b'#' => {
            let hashes = bytes.iter().take_while(|&&b| b == b'#').count();
            hashes <= 6 && matches!(bytes.get(hashes), None | Some(b' ') | Some(b'\t'))
        }
        b'`' | b'~' => {
            let len = bytes.iter().take_while(|&&b| b == first).count();
            len >= 3 && !s[len..].contains(first as char)
        }
        b'-' | b'+' | b'*' => {
            if matches!(bytes.get(1), None | Some(b' ') | Some(b'\t')) {
                return true;
            }
            is_thematic_break_start(s, first)
        }
        b'_' => is_thematic_break_start(s, b'_'),
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            digits <= 9
                && matches!(bytes.get(digits), Some(b'.') | Some(b')'))
                && matches!(bytes.get(digits + 1), None | Some(b' ') | Some(b'\t'))
        }
        _ => false,
    }
}

fn reparse_keeps_the_text(source: &str, expected: LeadingBlock) -> bool {
    crate::document::metrics::note_parser();
    let events: Vec<Event<'_>> = Parser::new_ext(source, editor_options()).collect();
    let (Some(first), Some(last)) = (events.first(), events.last()) else {
        return false;
    };
    matches!(
        (expected, first, last),
        (
            LeadingBlock::Paragraph,
            Event::Start(Tag::Paragraph | Tag::HtmlBlock),
            Event::End(TagEnd::Paragraph | TagEnd::HtmlBlock),
        ) | (
            LeadingBlock::Heading,
            Event::Start(Tag::Heading { .. }),
            Event::End(TagEnd::Heading(_)),
        )
    )
}

fn escape_first_punctuation(line: &str) -> String {
    let Some(index) = line.find(|c: char| c.is_ascii_punctuation()) else {
        return line.to_string();
    };
    format!("{}\\{}", &line[..index], &line[index..])
}

pub(crate) fn escape_leading_block_markers(source: &str, expected: LeadingBlock) -> String {
    if !source.lines().any(is_block_marker_line) {
        return source.to_string();
    }
    if reparse_keeps_the_text(source, expected) {
        return source.to_string();
    }
    source
        .lines()
        .map(|line| {
            if is_block_marker_line(line) {
                escape_first_punctuation(line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
