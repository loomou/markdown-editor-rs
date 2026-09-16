use super::{Document, NodeId};
use crate::block::{BlockId, BlockKind};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct HeadingAnchor {
    pub block: BlockId,
    pub slug: String,
}

pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            out.push('-');
        } else if ch == '-' || ch == '_' || ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

pub fn decode_anchor(raw: &str) -> String {
    let raw = raw.trim();
    if !raw.contains('%') {
        return raw.to_string();
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (hex_digit(bytes.get(i + 1)), hex_digit(bytes.get(i + 2)))
        {
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

fn hex_digit(byte: Option<&u8>) -> Option<u8> {
    match *byte? {
        b @ b'0'..=b'9' => Some(b - b'0'),
        b @ b'a'..=b'f' => Some(b - b'a' + 10),
        b @ b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn heading_anchors(doc: &Document) -> Vec<HeadingAnchor> {
    let headings: Vec<(BlockId, String)> = doc
        .preorder()
        .into_iter()
        .filter_map(|id| heading_text(doc, id).map(|text| (id.index, slug(text))))
        .collect();

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, base) in &headings {
        *counts.entry(base.clone()).or_insert(0) += 1;
    }

    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(headings.len());
    for (block, base) in headings {
        let name = if counts.get(&base) == Some(&1) {
            base
        } else {
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            format!("{base}-{n}")
        };
        out.push(HeadingAnchor { block, slug: name });
    }
    out
}

fn heading_text(doc: &Document, id: NodeId) -> Option<&str> {
    let node = doc.arena.get(id)?;
    matches!(node.kind, BlockKind::Heading(_)).then(|| doc.collapsed_display(id).trim())
}

pub fn find_anchor(anchors: &[HeadingAnchor], raw: &str) -> Option<BlockId> {
    let want = decode_anchor(raw).to_lowercase();
    if want.is_empty() {
        return None;
    }
    if let Some(hit) = anchors.iter().find(|a| a.slug == want) {
        return Some(hit.block);
    }
    if has_index_suffix(&want) {
        return None;
    }
    let first = format!("{want}-1");
    anchors.iter().find(|a| a.slug == first).map(|a| a.block)
}

fn has_index_suffix(slug: &str) -> bool {
    match slug.rsplit_once('-') {
        Some((_, tail)) => !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}
