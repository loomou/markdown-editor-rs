use crate::block::BlockKind;
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::path::Path;
use crate::document::edit::{Caret, normalize};

pub(crate) fn lists_touching_leaf(doc: &Document, leaf: crate::block::BlockId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut walk = doc.live_id(leaf);
    while let Some(id) = walk {
        if doc.arena.get(id).is_some_and(|n| n.kind == BlockKind::List) {
            out.push(id);
        }
        walk = doc.arena.get(id).and_then(|n| n.parent);
    }
    out
}

pub(crate) fn lists_touching_span(doc: &Document, span: &[crate::block::BlockId]) -> Vec<NodeId> {
    let mut out = Vec::new();
    for &leaf in span {
        for list in lists_touching_leaf(doc, leaf) {
            if !out.contains(&list) {
                out.push(list);
            }
        }
    }
    out
}

pub(crate) fn collapse_empty_after_span(
    doc: &mut Document,
    lists: &[NodeId],
    caret: Caret,
    drop_empty_items: bool,
) -> Caret {
    let mut lists: Vec<NodeId> = lists
        .iter()
        .copied()
        .filter(|&id| doc.arena.get(id).is_some())
        .collect();
    lists.sort_by_key(|&id| std::cmp::Reverse(list_depth(doc, id)));
    let before = doc.revision();
    let mut changes = Vec::new();
    let mut caret = caret;
    for list in lists {
        if doc.arena.get(list).is_none() {
            continue;
        }
        caret = collapse_one_list(doc, list, caret, drop_empty_items, &mut changes);
    }
    if !changes.is_empty() {
        let _ = doc.commit(before, changes);
    }
    caret
}

fn list_depth(doc: &Document, list: NodeId) -> usize {
    let mut depth = 0;
    let mut walk = doc.arena.get(list).and_then(|n| n.parent);
    while let Some(id) = walk {
        if doc.arena.get(id).is_some_and(|n| n.kind == BlockKind::List) {
            depth += 1;
        }
        walk = doc.arena.get(id).and_then(|n| n.parent);
    }
    depth
}

fn subtree_has_content(doc: &Document, id: NodeId) -> bool {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.arena.get(id) else {
            continue;
        };
        if node.kind.is_text_leaf() && !doc.display(id).is_empty() {
            return true;
        }
        if node.kind == BlockKind::Image && !doc.leaf_source(id).is_empty() {
            return true;
        }
        if matches!(
            node.kind,
            BlockKind::ThematicBreak | BlockKind::CodeBlock | BlockKind::Math | BlockKind::Mermaid
        ) {
            return true;
        }
        if node.kind == BlockKind::Table {
            return true;
        }
        if node.kind == BlockKind::BlockQuote {
            return true;
        }
        let mut child = node.last_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.arena.get(child_id).and_then(|n| n.prev_sibling);
        }
    }
    false
}

fn is_under(doc: &Document, id: NodeId, root: NodeId) -> bool {
    if id == root {
        return true;
    }
    let mut walk = doc.arena.get(id).and_then(|n| n.parent);
    while let Some(p) = walk {
        if p == root {
            return true;
        }
        walk = doc.arena.get(p).and_then(|n| n.parent);
    }
    false
}

fn collapse_one_list(
    doc: &mut Document,
    list: NodeId,
    caret: Caret,
    drop_empty_items: bool,
    changes: &mut Vec<DocChange>,
) -> Caret {
    if !subtree_has_content(doc, list) {
        let keep = doc
            .live_id(caret.block)
            .filter(|&id| is_under(doc, id, list))
            .or_else(|| {
                if doc.live_id(caret.block).is_none() {
                    first_text_leaf(doc, list)
                } else {
                    None
                }
            });
        return unwrap_or_drop_list(doc, list, keep, caret, changes);
    }
    if !drop_empty_items {
        return caret;
    }

    let items: Vec<NodeId> = doc.arena.children(list).collect();
    let survivor_item = Path::at(doc, caret)
        .filter(|p| p.list == Some(list))
        .and_then(|p| p.item);
    let mut caret = caret;
    for &item in &items {
        if Some(item) == survivor_item || subtree_has_content(doc, item) {
            continue;
        }
        drop_empty_item(doc, item, changes);
    }
    if let Some(item) = survivor_item.filter(|&id| doc.arena.get(id).is_some())
        && !subtree_has_content(doc, item)
    {
        let next = doc
            .arena
            .children(list)
            .find(|&id| id != item)
            .and_then(|id| first_text_leaf(doc, id));
        drop_empty_item(doc, item, changes);
        if let Some(leaf) = next.filter(|id| doc.arena.get(*id).is_some()) {
            caret = Caret {
                block: leaf.index,
                offset: 0,
            };
        }
    }
    caret
}

fn unwrap_or_drop_list(
    doc: &mut Document,
    list: NodeId,
    keep: Option<NodeId>,
    caret: Caret,
    changes: &mut Vec<DocChange>,
) -> Caret {
    let Some(host) = doc.arena.get(list).and_then(|n| n.parent) else {
        return caret;
    };
    let list_prev = doc.arena.get(list).and_then(|n| n.prev_sibling);
    snapshot_subtree(doc, list);
    if let Some(keep) = keep {
        doc.arena.detach(keep);
    }
    doc.arena.detach(list);
    tombstone_tree(doc, list, keep);
    let inserted = if let Some(keep) = keep {
        doc.arena.insert_after(host, list_prev, keep);
        vec![keep]
    } else {
        Vec::new()
    };
    doc.bump_structure(host);
    changes.push(DocChange::TreeSpliced {
        parent: host,
        before: list_prev,
        removed: vec![list],
        inserted,
    });
    if doc.arena.get(host).is_some() {
        normalize::sync_loose_up(doc, host, changes);
    }
    if let Some(keep) = keep {
        Caret {
            block: keep.index,
            offset: caret.offset.min(doc.display(keep).len()),
        }
    } else {
        caret
    }
}

fn first_text_leaf(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        let node = doc.arena.get(id)?;
        if node.kind.is_text_leaf() {
            return Some(id);
        }
        let mut child = node.last_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.arena.get(child_id).and_then(|n| n.prev_sibling);
        }
    }
    None
}

fn snapshot_subtree(doc: &mut Document, id: NodeId) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        doc.arena.snapshot(id);
        stack.extend(doc.arena.children(id));
    }
}

fn tombstone_tree(doc: &mut Document, id: NodeId, keep: Option<NodeId>) {
    let mut stack = vec![(id, false)];
    while let Some((id, visited)) = stack.pop() {
        if Some(id) == keep {
            continue;
        }
        if visited {
            doc.arena.tombstone(id);
            continue;
        }
        let children: Vec<NodeId> = doc.arena.children(id).collect();
        stack.push((id, true));
        stack.extend(children.into_iter().rev().map(|child| (child, false)));
    }
}

fn drop_empty_item(doc: &mut Document, item: NodeId, changes: &mut Vec<DocChange>) {
    let Some(parent) = doc.arena.get(item).and_then(|n| n.parent) else {
        return;
    };
    let prev = doc.arena.get(item).and_then(|n| n.prev_sibling);
    snapshot_subtree(doc, item);
    doc.arena.detach(item);
    tombstone_tree(doc, item, None);
    doc.bump_structure(parent);
    changes.push(DocChange::TreeSpliced {
        parent,
        before: prev,
        removed: vec![item],
        inserted: Vec::new(),
    });
    if doc.arena.get(parent).is_some() {
        normalize::prune_empty_up(doc, parent, changes);
        if doc.arena.get(parent).is_some() {
            normalize::sync_loose_up(doc, parent, changes);
        }
    }
}
