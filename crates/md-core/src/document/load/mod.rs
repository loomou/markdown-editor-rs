use super::Document;
#[cfg(test)]
use super::text::{LeafText, TextPiece};
use crate::block::{AlertKind, CodeFenceMarker, ListMarker, NodeExtra};
use pulldown_cmark::{Alignment, BlockQuoteKind, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::ops::Range;

mod builder;
mod events;
mod stack;
mod tags;

use builder::Builder;

#[derive(Clone, Debug, Default)]
pub struct Link {
    pub dest: String,
    pub title: String,
}

pub fn load(md: &str, mut opts: Options) -> Document {
    opts.remove(Options::ENABLE_DEFINITION_LIST);
    let source = normalize_markdown_source(md);
    let mut builder = Builder::new();
    let mut parser = Parser::new_ext(&source, opts).into_offset_iter();
    for (event, range) in parser.by_ref() {
        builder.handle(&source, event, range);
    }
    let mut spans: Vec<_> = parser
        .reference_definitions()
        .iter()
        .map(|(_, definition)| definition.span.clone())
        .collect();
    spans.sort_unstable_by_key(|span| (span.start, span.end));
    spans.dedup();
    let reference_definitions = spans
        .into_iter()
        .filter_map(|span| source.get(span).map(str::to_string))
        .collect();
    builder.finish(source, reference_definitions)
}

pub(crate) fn fold_cr(md: &str) -> String {
    if !md.as_bytes().contains(&b'\r') {
        return md.to_string();
    }
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(i) = rest.find('\r') {
        out.push_str(&rest[..i]);
        out.push('\n');
        rest = &rest[i + 1..];
        if rest.starts_with('\n') {
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn normalize_markdown_source(md: &str) -> String {
    fold_cr(md.strip_prefix('\u{feff}').unwrap_or(md))
}

fn intern_push(intern: &mut String, s: &str) -> Range<u32> {
    let start = intern.len() as u32;
    intern.push_str(s);
    start..(intern.len() as u32)
}

fn covers_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Heading { .. }
            | Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. }
            | Tag::Image { .. }
            | Tag::MetadataBlock(_)
    )
}

fn covers_end(end: TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Heading(_)
            | TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
            | TagEnd::MetadataBlock(_)
    )
}

fn source_piece(source: &str, text: &str, range: Range<usize>) -> Option<Range<u32>> {
    let slice = source.get(range.start..range.end)?;
    if slice == text {
        Some(range.start as u32..range.end as u32)
    } else {
        None
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn align_bits(a: &Alignment) -> u8 {
    match a {
        Alignment::None => 0,
        Alignment::Left => 1,
        Alignment::Center => 2,
        Alignment::Right => 3,
    }
}

fn list_marker(source: &str, range: &Range<usize>, ordered: bool) -> ListMarker {
    let line = source
        .get(range.clone())
        .and_then(|fragment| fragment.lines().next())
        .unwrap_or("")
        .trim_start_matches([' ', '\t']);
    if ordered {
        let delimiter = line.trim_start_matches(|ch: char| ch.is_ascii_digit());
        if delimiter.starts_with(')') {
            ListMarker::Parenthesis
        } else {
            ListMarker::Period
        }
    } else {
        match line.as_bytes().first().copied() {
            Some(b'+') => ListMarker::Plus,
            Some(b'*') => ListMarker::Star,
            _ => ListMarker::Dash,
        }
    }
}

fn code_fence_style(source: &str, range: &Range<usize>) -> (CodeFenceMarker, u16) {
    let line = source
        .get(range.clone())
        .and_then(|fragment| fragment.lines().next())
        .unwrap_or("")
        .trim_start_matches(' ');
    let marker = if line.starts_with('~') {
        CodeFenceMarker::Tilde
    } else {
        CodeFenceMarker::Backtick
    };
    let byte = if marker == CodeFenceMarker::Tilde {
        b'~'
    } else {
        b'`'
    };
    let len = line
        .bytes()
        .take_while(|candidate| *candidate == byte)
        .count()
        .max(3)
        .min(u16::MAX as usize) as u16;
    (marker, len)
}

fn quote_alert(source: &str, range: &Range<usize>, kind: BlockQuoteKind) -> NodeExtra {
    let kind = match kind {
        BlockQuoteKind::Note => AlertKind::Note,
        BlockQuoteKind::Tip => AlertKind::Tip,
        BlockQuoteKind::Important => AlertKind::Important,
        BlockQuoteKind::Warning => AlertKind::Warning,
        BlockQuoteKind::Caution => AlertKind::Caution,
    };
    let fragment = source.get(range.clone()).unwrap_or("");
    let label = fragment
        .lines()
        .next()
        .and_then(|line| line.find("[!").map(|start| &line[start + 2..]))
        .and_then(|rest| rest.find(']').map(|end| &rest[..end]))
        .unwrap_or("");
    let lowercase_mask = label.bytes().enumerate().fold(0u16, |mask, (index, byte)| {
        if index < u16::BITS as usize && byte.is_ascii_lowercase() {
            mask | (1 << index)
        } else {
            mask
        }
    });
    let blank_after_marker = fragment.lines().nth(1).is_some_and(empty_quote_line);
    NodeExtra::QuoteAlert {
        kind,
        lowercase_mask,
        blank_after_marker,
    }
}

fn empty_quote_line(line: &str) -> bool {
    let mut rest = line.trim_start();
    let mut quoted = false;
    while let Some(after) = rest.strip_prefix('>') {
        quoted = true;
        rest = after.strip_prefix(' ').unwrap_or(after).trim_start();
    }
    quoted && rest.is_empty()
}

pub(crate) fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut last_space = true;
    #[derive(PartialEq)]
    enum St {
        Text,
        Tag,
        Comment,
    }
    let mut st = St::Text;
    let mut quote = b'\0';
    let mut i = 0;
    while i < s.len() {
        match st {
            St::Comment => {
                if s.is_char_boundary(i) && s[i..].starts_with("-->") {
                    st = St::Text;
                    i += 3;
                } else {
                    i += 1;
                }
            }
            St::Tag => {
                let c = s.as_bytes()[i];
                if quote != b'\0' {
                    if c == quote {
                        quote = b'\0';
                    }
                    i += 1;
                } else if c == b'"' || c == b'\'' {
                    quote = c;
                    i += 1;
                } else if c == b'>' {
                    st = St::Text;
                    if !last_space {
                        out.push(' ');
                        last_space = true;
                    }
                    i += 1;
                } else {
                    i += 1;
                }
            }
            St::Text => {
                if s[i..].starts_with("<!--") {
                    st = St::Comment;
                    i += 4;
                } else if s.as_bytes()[i] == b'<' {
                    st = St::Tag;
                    quote = b'\0';
                    i += 1;
                } else {
                    let c = s[i..].chars().next().unwrap();
                    if c.is_whitespace() {
                        if !last_space {
                            out.push(' ');
                            last_space = true;
                        }
                    } else {
                        out.push(c);
                        last_space = false;
                    }
                    i += c.len_utf8();
                }
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
pub(crate) fn piece_str<'a>(doc: &'a Document, piece: &'a TextPiece) -> &'a str {
    match piece {
        TextPiece::Source(r) => {
            let start = r.start as usize;
            let end = r.end as usize;
            doc.source.get(start..end).unwrap_or("")
        }
        TextPiece::Intern(r) => {
            let start = r.start as usize;
            let end = r.end as usize;
            doc.intern.get(start..end).unwrap_or("")
        }
        TextPiece::Owned => "",
    }
}

#[cfg(test)]
pub(crate) fn display_matches_pieces(doc: &Document, leaf: &LeafText) -> bool {
    if leaf.pieces.iter().any(|p| matches!(p, TextPiece::Owned)) {
        return leaf.pieces.len() == 1;
    }
    let mut acc = String::new();
    for p in &leaf.pieces {
        acc.push_str(piece_str(doc, p));
    }
    acc == leaf.display()
}

#[cfg(test)]
mod normalize_tests {
    use crate::document::{editor_options, load_markdown};

    #[test]
    fn crlf_source_and_markdown_have_no_cr() {
        let doc = load_markdown("a\r\nb\r\n", editor_options());
        assert!(
            !doc.source.contains('\r'),
            "source still has CR: {}",
            doc.source.escape_debug()
        );
        let md = doc.to_markdown();
        assert!(
            !md.contains('\r'),
            "to_markdown still has CR: {}",
            md.escape_debug()
        );
        assert!(!md.contains('\u{feff}'));
    }

    #[test]
    fn lone_cr_becomes_lf() {
        let doc = load_markdown("a\rb\r", editor_options());
        assert!(!doc.source.contains('\r'));
        assert!(!doc.to_markdown().contains('\r'));
    }

    #[test]
    fn leading_utf8_bom_is_stripped() {
        let doc = load_markdown("\u{feff}# hi\n", editor_options());
        assert!(
            !doc.source.contains('\u{feff}'),
            "source still has BOM: {}",
            doc.source.escape_debug()
        );
        let md = doc.to_markdown();
        assert!(!md.contains('\u{feff}'));
        assert!(!md.contains('\r'));
    }

    #[test]
    fn bom_then_crlf_normalizes_both() {
        let doc = load_markdown("\u{feff}a\r\nb\r\n", editor_options());
        assert!(!doc.source.contains('\u{feff}'));
        assert!(!doc.source.contains('\r'));
        assert!(!doc.to_markdown().contains('\r'));
        assert!(!doc.to_markdown().contains('\u{feff}'));
    }
}
