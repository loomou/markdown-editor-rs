use crate::block::{BlockId, BlockKind, NodeExtra};
use crate::document::Document;
use std::sync::Arc;

fn is_mermaid(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .is_some_and(|token| token.eq_ignore_ascii_case("mermaid"))
}

fn lang_index(doc: &mut Document, lang: &str) -> u32 {
    let langs = Arc::make_mut(&mut doc.langs);
    if let Some(index) = langs.iter().position(|known| known == lang) {
        return index as u32;
    }
    let index = langs.len() as u32;
    langs.push(lang.to_string());
    index
}

pub(super) fn set_fence_lang(doc: &mut Document, block: BlockId, lang: &str) -> bool {
    let Some(id) = doc.live_id(block) else {
        return false;
    };
    let NodeExtra::CodeFence {
        lang: old,
        marker,
        len,
    } = doc.extra(id)
    else {
        return false;
    };
    let info = lang.trim();
    let want = if info.is_empty() {
        None
    } else {
        Some(lang_index(doc, info))
    };
    if want == old {
        return false;
    }
    let old_kind = doc.kind(block).unwrap_or(BlockKind::CodeBlock);
    let old_extra = doc.extra(id);
    let new_kind = if is_mermaid(info) {
        BlockKind::Mermaid
    } else {
        BlockKind::CodeBlock
    };
    let before = doc.revision;
    doc.set_extra(
        id,
        NodeExtra::CodeFence {
            lang: want,
            marker,
            len,
        },
    );
    if let Some(node) = doc.arena.get_mut(id) {
        node.kind = new_kind;
    }
    let mut changes = vec![doc.reproject_current(id)];
    changes.push(doc.attrs_change(id, old_kind, old_extra));
    let _ = doc.commit(before, changes);
    true
}
