use super::indent::lift_item;
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::path::Path;
use crate::document::edit::{Caret, normalize};

pub(super) fn is_text_leaf(doc: &Document, id: NodeId) -> bool {
    doc.arena.get(id).is_some_and(|n| n.kind.is_text_leaf())
}

pub(super) fn last_child(doc: &Document, id: NodeId) -> Option<NodeId> {
    doc.arena.get(id).and_then(|n| n.last_child)
}

pub(crate) fn delete_backward(doc: &mut Document, path: &Path, at: Caret) -> Caret {
    if !path.is_item_lead(doc) {
        return join_sibling_leaf(doc, path).unwrap_or(at);
    }
    if let Some(prev) = path.prev_item(doc) {
        return join_prev(doc, path, prev);
    }
    lift_item(doc, path)
}

fn join_sibling_leaf(doc: &mut Document, path: &Path) -> Option<Caret> {
    let leaf = path.leaf;
    let prev = doc.arena.get(leaf)?.prev_sibling?;
    if !is_text_leaf(doc, prev) {
        return None;
    }
    let parent = doc.arena.get(leaf)?.parent?;
    let list = path.list?;
    let before = doc.revision;
    let (text_changes, join_at) = doc.append_leaf_source(prev, leaf);
    let leaf_prev = doc.arena.get(leaf).and_then(|n| n.prev_sibling);
    normalize::tombstone(doc, leaf);
    doc.bump_structure(parent);
    let mut changes = text_changes;
    changes.push(DocChange::TreeSpliced {
        parent,
        before: leaf_prev,
        removed: vec![leaf],
        inserted: Vec::new(),
    });
    normalize::sync_loose_after_join(doc, parent, list, &mut changes);
    let _ = doc.commit(before, changes);
    Some(Caret {
        block: prev.index,
        offset: join_at,
    })
}

fn join_prev(doc: &mut Document, path: &Path, prev_item: NodeId) -> Caret {
    let item = path.item.expect("item");
    let list = path.list.expect("list");
    let leaf = path.leaf;
    let before = doc.revision;
    let mut changes = Vec::new();
    let prev_last = last_child(doc, prev_item);
    let merge_text = prev_last.is_some_and(|p| is_text_leaf(doc, p) && is_text_leaf(doc, leaf));
    let mut caret = Caret {
        block: leaf.index,
        offset: 0,
    };
    let kids: Vec<NodeId> = doc.arena.children(item).collect();
    if merge_text {
        let prev_leaf = prev_last.expect("prev leaf");
        let (text_changes, join_at) = doc.append_leaf_source(prev_leaf, leaf);
        changes.extend(text_changes);
        doc.arena.snapshot(leaf);
        doc.arena.tombstone(leaf);
        caret = Caret {
            block: prev_leaf.index,
            offset: join_at,
        };
    }
    let moved: Vec<NodeId> = kids
        .iter()
        .copied()
        .filter(|&k| doc.arena.get(k).is_some())
        .collect();
    for k in &moved {
        doc.arena.detach(*k);
    }
    doc.arena.snapshot(item);
    changes.push(DocChange::TreeSpliced {
        parent: item,
        before: None,
        removed: kids,
        inserted: Vec::new(),
    });
    for k in &moved {
        doc.arena.append_child(prev_item, *k);
    }
    let prev_prev = doc.arena.get(prev_item).and_then(|n| n.prev_sibling);
    normalize::tombstone(doc, item);
    changes.push(DocChange::TreeSpliced {
        parent: list,
        before: prev_prev,
        removed: vec![prev_item, item],
        inserted: vec![prev_item],
    });
    if !moved.is_empty() {
        changes.push(DocChange::TreeSpliced {
            parent: prev_item,
            before: prev_last,
            removed: Vec::new(),
            inserted: moved,
        });
    }
    doc.bump_structure(list);
    doc.bump_structure(prev_item);
    normalize::prune_empty_up(doc, list, &mut changes);
    if doc.arena.get(list).is_some() {
        normalize::sync_loose_after_join(doc, list, list, &mut changes);
    }
    let _ = doc.commit(before, changes);
    caret
}
