use super::MarkdownExport;
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::editor_options;
use pulldown_cmark::{Event, Parser, Tag};

pub(crate) fn trim_end_newlines(s: &str) -> &str {
    s.trim_end_matches(['\n', '\r'])
}

pub(crate) fn phrasing_export<D: MarkdownExport>(doc: &D, id: NodeId) -> &str {
    let source = trim_end_newlines(doc.leaf_source(id));
    if source.is_empty() {
        doc.display(id)
    } else {
        source
    }
}

pub(crate) fn phrasing_source(doc: &Document, id: NodeId) -> String {
    phrasing_export(doc, id).to_string()
}

pub(crate) fn paragraph_export<D: MarkdownExport>(doc: &D, id: NodeId) -> String {
    escape_leading_fences(phrasing_export(doc, id))
}

fn is_fence_marker_line(line: &str) -> bool {
    let mut col = 0usize;
    for &b in line.as_bytes() {
        match b {
            b' ' => col += 1,
            b'\t' => col = (col / 4 + 1) * 4,
            _ => break,
        }
    }
    if col > 3 {
        return false;
    }
    let s = line.trim_start();
    let marker = match s.as_bytes().first() {
        Some(b @ (b'`' | b'~')) => *b,
        _ => return false,
    };
    let fence_len = s.bytes().take_while(|&b| b == marker).count();
    fence_len >= 3 && !s[fence_len..].contains(marker as char)
}

fn would_reparse_as_fence(source: &str) -> bool {
    Parser::new_ext(source, editor_options())
        .any(|event| matches!(event, Event::Start(Tag::CodeBlock(_))))
}

pub(crate) fn escape_leading_fences(source: &str) -> String {
    if !source.lines().any(is_fence_marker_line) {
        return source.to_string();
    }
    if !would_reparse_as_fence(source) {
        return source.to_string();
    }
    source
        .lines()
        .map(|line| {
            if is_fence_marker_line(line) {
                let indent = line.len() - line.trim_start().len();
                format!("{}\\{}", &line[..indent], &line[indent..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
