use super::span::delete_sel;
use super::{Caret, Sel, list, path};
use crate::block::BlockKind;
use crate::document::change::DocChange;
use crate::document::{Document, next_grapheme_boundary};

pub(super) fn delete_backward(doc: &mut Document, sel: Sel) -> Caret {
    if let Some(caret) = delete_sel(doc, sel) {
        return caret;
    }
    let at = sel.head;
    if at.offset == 0 {
        if let Some(caret) = super::table::try_delete_empty_table(doc, at) {
            return caret;
        }
        if doc.kind(at.block) == Some(BlockKind::TableCell) {
            return at;
        }
        if let Some(id) = doc.live_id(at.block)
            && let Some(caret) = doc.try_demote_heading(id)
        {
            return caret;
        }
        if let Some(id) = doc.live_id(at.block)
            && let Some(caret) = doc.try_demote_empty_block(id)
        {
            return caret;
        }
        if let Some(id) = doc.live_id(at.block)
            && let Some(caret) = doc.try_lift_wrapper(id)
        {
            return caret;
        }
        if delete_preceding_thematic_break(doc, at) {
            return at;
        }
        if let Some(path) = path::Path::at(doc, at)
            && path.item.is_some()
        {
            return list::delete_backward(doc, &path, at);
        }
        return match doc.merge_into_prev(at.block) {
            Some((_, block, offset)) => Caret { block, offset },
            None => at,
        };
    }
    let (_, offset) = doc.delete_back(at.block, at.offset);
    Caret {
        block: at.block,
        offset,
    }
}

fn delete_preceding_thematic_break(doc: &mut Document, at: Caret) -> bool {
    let Some(current) = doc.live_id(at.block) else {
        return false;
    };
    let Some(rule) = doc.preorder_prev(current) else {
        return false;
    };
    if doc.arena.get(rule).map(|node| node.kind) != Some(BlockKind::ThematicBreak) {
        return false;
    }
    let Some(parent) = doc.arena.get(rule).and_then(|node| node.parent) else {
        return false;
    };
    let before_sibling = doc.arena.get(rule).and_then(|node| node.prev_sibling);
    let before = doc.revision;
    doc.arena.snapshot(rule);
    doc.arena.detach(rule);
    doc.arena.tombstone(rule);
    doc.bump_structure(parent);
    let _ = doc.commit(
        before,
        vec![DocChange::TreeSpliced {
            parent,
            before: before_sibling,
            removed: vec![rule],
            inserted: Vec::new(),
        }],
    );
    true
}

pub(super) fn delete_forward(doc: &mut Document, sel: Sel) -> Caret {
    if let Some(caret) = delete_sel(doc, sel) {
        return caret;
    }
    let at = sel.head;
    let Some(id) = doc.live_id(at.block) else {
        return at;
    };
    let text = doc.caret_text(id);
    if at.offset >= text.len() {
        if delete_following_thematic_break(doc, id) {
            return at;
        }
        if let Some((_, block, offset)) = doc.merge_into_next(at.block) {
            return Caret { block, offset };
        }
        return at;
    }
    let end = next_grapheme_boundary(text, at.offset);
    let (_, offset) = doc.replace_text_with_caret(at.block, at.offset..end, "");
    Caret {
        block: at.block,
        offset,
    }
}

fn delete_following_thematic_break(doc: &mut Document, current: crate::document::NodeId) -> bool {
    let Some(rule) = doc.arena.get(current).and_then(|node| node.next_sibling) else {
        return false;
    };
    if doc.arena.get(rule).map(|node| node.kind) != Some(BlockKind::ThematicBreak) {
        return false;
    }
    let Some(parent) = doc.arena.get(rule).and_then(|node| node.parent) else {
        return false;
    };
    let before = doc.revision;
    doc.arena.snapshot(rule);
    doc.arena.detach(rule);
    doc.arena.tombstone(rule);
    doc.bump_structure(parent);
    let _ = doc.commit(
        before,
        vec![DocChange::TreeSpliced {
            parent,
            before: Some(current),
            removed: vec![rule],
            inserted: Vec::new(),
        }],
    );
    true
}
