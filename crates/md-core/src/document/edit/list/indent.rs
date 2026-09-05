use super::join::last_child;
use crate::block::{BlockKind, NodeExtra};
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::path::Path;
use crate::document::edit::{Caret, normalize};

pub(crate) fn lift_item(doc: &mut Document, path: &Path) -> Caret {
    let caret = Caret {
        block: path.leaf.index,
        offset: path.offset,
    };
    let Some(item) = path.item else {
        return caret;
    };
    let Some(list) = path.list else {
        return caret;
    };
    lift_items(doc, list, &[item], caret)
}

pub(crate) fn lift_items(
    doc: &mut Document,
    list: NodeId,
    items: &[NodeId],
    caret: Caret,
) -> Caret {
    if items.is_empty() {
        return caret;
    }
    let before = doc.revision;
    let mut changes = Vec::new();
    let lift_all = doc.arena.children(list).count() == items.len();
    let list_parent = doc.arena.get(list).and_then(|n| n.parent);
    let nested_item = list_parent.filter(|&p| {
        doc.arena
            .get(p)
            .is_some_and(|n| n.kind == BlockKind::ListItem)
    });
    if let Some(parent_item) = nested_item {
        let Some(outer) = doc.arena.get(parent_item).and_then(|n| n.parent) else {
            return caret;
        };
        if lift_all {
            let list_prev = doc.arena.get(list).and_then(|n| n.prev_sibling);
            let parent_prev = doc.arena.get(parent_item).and_then(|n| n.prev_sibling);
            let prune_parent = doc.arena.children(parent_item).all(|child| child == list);
            doc.arena.snapshot(list);
            if prune_parent {
                doc.arena.snapshot(parent_item);
            }
            for &item in items {
                doc.arena.detach(item);
            }
            let mut after = Some(parent_item);
            for &item in items {
                doc.arena.insert_after(outer, after, item);
                after = Some(item);
            }
            normalize::tombstone(doc, list);
            if prune_parent {
                normalize::tombstone(doc, parent_item);
                changes.push(DocChange::TreeSpliced {
                    parent: outer,
                    before: parent_prev,
                    removed: vec![parent_item],
                    inserted: items.to_vec(),
                });
            } else {
                changes.push(DocChange::TreeSpliced {
                    parent: parent_item,
                    before: list_prev,
                    removed: vec![list],
                    inserted: Vec::new(),
                });
                changes.push(DocChange::TreeSpliced {
                    parent: outer,
                    before: Some(parent_item),
                    removed: Vec::new(),
                    inserted: items.to_vec(),
                });
                doc.bump_structure(parent_item);
            }
            doc.bump_structure(outer);
            normalize::sync_loose_up(doc, outer, &mut changes);
        } else {
            changes.push(DocChange::TreeSpliced {
                parent: list,
                before: doc.arena.get(items[0]).and_then(|n| n.prev_sibling),
                removed: items.to_vec(),
                inserted: Vec::new(),
            });
            doc.bump_structure(list);
            for &item in items {
                doc.arena.detach(item);
            }
            let mut after = Some(parent_item);
            for &item in items {
                doc.arena.insert_after(outer, after, item);
                after = Some(item);
            }
            changes.push(DocChange::TreeSpliced {
                parent: outer,
                before: Some(parent_item),
                removed: Vec::new(),
                inserted: items.to_vec(),
            });
            doc.bump_structure(outer);
            normalize::sync_loose_up(doc, list, &mut changes);
            normalize::sync_loose_up(doc, outer, &mut changes);
        }
    } else if let Some(host) = list_parent {
        let list_prev = doc.arena.get(list).and_then(|n| n.prev_sibling);
        let item_prev = doc.arena.get(items[0]).and_then(|n| n.prev_sibling);
        if lift_all {
            doc.arena.snapshot(list);
        }
        let mut kids = Vec::new();
        let mut kids_by_item: Vec<(NodeId, Option<NodeId>, Vec<NodeId>)> = Vec::new();
        for &item in items {
            doc.arena.snapshot(item);
            let item_kids: Vec<NodeId> = doc.arena.children(item).collect();

            let anchor = item_kids
                .first()
                .and_then(|first| doc.arena.get(*first).and_then(|n| n.prev_sibling));
            for k in &item_kids {
                doc.arena.detach(*k);
            }
            if !item_kids.is_empty() {
                kids_by_item.push((item, anchor, item_kids.clone()));
            }
            kids.extend(item_kids);
        }
        for &item in items {
            doc.arena.detach(item);
            doc.arena.tombstone(item);
        }
        if lift_all {
            let mut after = list_prev;
            normalize::tombstone(doc, list);
            for k in &kids {
                doc.arena.insert_after(host, after, *k);
                after = Some(*k);
            }

            book_lifted_kids(doc, kids_by_item, &mut changes);

            changes.push(DocChange::TreeSpliced {
                parent: list,
                before: None,
                removed: items.to_vec(),
                inserted: Vec::new(),
            });
            changes.push(DocChange::TreeSpliced {
                parent: host,
                before: list_prev,
                removed: vec![list],
                inserted: kids,
            });
            doc.bump_structure(host);
        } else {
            book_lifted_kids(doc, kids_by_item, &mut changes);
            changes.push(DocChange::TreeSpliced {
                parent: list,
                before: item_prev,
                removed: items.to_vec(),
                inserted: Vec::new(),
            });
            doc.bump_structure(list);
            let mut after = Some(list);
            for k in &kids {
                doc.arena.insert_after(host, after, *k);
                after = Some(*k);
            }
            if !kids.is_empty() {
                changes.push(DocChange::TreeSpliced {
                    parent: host,
                    before: Some(list),
                    removed: Vec::new(),
                    inserted: kids,
                });
            }
            doc.bump_structure(host);
            normalize::prune_empty_up(doc, list, &mut changes);
            normalize::sync_loose_up(doc, list, &mut changes);
        }
    }
    let _ = doc.commit(before, changes);
    caret
}

fn book_lifted_kids(
    doc: &mut Document,
    kids_by_item: Vec<(NodeId, Option<NodeId>, Vec<NodeId>)>,
    changes: &mut Vec<DocChange>,
) {
    for (item, anchor, item_kids) in kids_by_item {
        changes.push(DocChange::TreeSpliced {
            parent: item,
            before: anchor,
            removed: item_kids,
            inserted: Vec::new(),
        });
        doc.bump_structure(item);
    }
}

pub(crate) fn sink_items(
    doc: &mut Document,
    list: NodeId,
    items: &[NodeId],
    caret: Caret,
) -> Caret {
    let Some(&first) = items.first() else {
        return caret;
    };
    let Some(prev) = doc
        .arena
        .get(first)
        .and_then(|n| n.prev_sibling)
        .filter(|&p| {
            doc.arena
                .get(p)
                .is_some_and(|n| n.kind == BlockKind::ListItem)
        })
    else {
        return caret;
    };
    let before = doc.revision;
    let mut changes = Vec::new();
    let existing = last_child(doc, prev)
        .filter(|&c| doc.arena.get(c).is_some_and(|n| n.kind == BlockKind::List));
    let created = existing.is_none();
    let inner = match existing {
        Some(inner) => inner,
        None => {
            let inner = doc.alloc_container(BlockKind::List);
            let extra = if doc.extra(list).ordered_start().is_some() {
                NodeExtra::List {
                    start: Some(1),
                    marker: doc.extra(list).list_marker(),
                    loose: false,
                    source_loose: false,
                }
            } else {
                NodeExtra::List {
                    start: None,
                    marker: doc.extra(list).list_marker(),
                    loose: false,
                    source_loose: false,
                }
            };
            doc.set_extra(inner, extra);
            inner
        }
    };
    let prev_attach = if created { last_child(doc, prev) } else { None };
    let inner_last = last_child(doc, inner);
    let item_prev = doc.arena.get(first).and_then(|n| n.prev_sibling);
    if created {
        doc.arena.append_child(prev, inner);
    }
    for &item in items {
        doc.arena.detach(item);
        doc.arena.append_child(inner, item);
    }
    changes.push(DocChange::TreeSpliced {
        parent: list,
        before: item_prev,
        removed: items.to_vec(),
        inserted: Vec::new(),
    });
    doc.bump_structure(list);
    if created {
        changes.push(DocChange::TreeSpliced {
            parent: prev,
            before: prev_attach,
            removed: Vec::new(),
            inserted: vec![inner],
        });
        doc.bump_structure(prev);

        if !items.is_empty() {
            changes.push(DocChange::TreeSpliced {
                parent: inner,
                before: None,
                removed: Vec::new(),
                inserted: items.to_vec(),
            });
            doc.bump_structure(inner);
        }
    } else {
        changes.push(DocChange::TreeSpliced {
            parent: inner,
            before: inner_last,
            removed: Vec::new(),
            inserted: items.to_vec(),
        });
        doc.bump_structure(inner);
    }
    normalize::sync_loose_up(doc, list, &mut changes);
    normalize::sync_loose_up(doc, inner, &mut changes);
    let _ = doc.commit(before, changes);
    caret
}
