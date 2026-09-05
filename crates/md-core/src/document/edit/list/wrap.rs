use crate::block::{BlockKind, ListMarker, NodeExtra};
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::path::Path;
use crate::document::edit::{Caret, normalize};

pub(crate) fn wrap_paragraph(
    doc: &mut Document,
    caret: Caret,
    ordered: bool,
    task: Option<bool>,
    consume: usize,
) -> Caret {
    let Some(leaf) = doc.live_id(caret.block) else {
        return caret;
    };
    let Some(parent) = doc.arena.get(leaf).and_then(|n| n.parent) else {
        return caret;
    };
    let prev = doc.arena.get(leaf).and_then(|n| n.prev_sibling);
    let before = doc.revision;
    let mut changes = Vec::new();
    if consume > 0 {
        changes.push(doc.rewrite_text(leaf, 0..consume, "").0);
    }
    let list = doc.alloc_container(BlockKind::List);
    doc.set_extra(
        list,
        if ordered {
            NodeExtra::List {
                start: Some(1),
                marker: ListMarker::Period,
                loose: false,
                source_loose: false,
            }
        } else {
            NodeExtra::List {
                start: None,
                marker: ListMarker::Dash,
                loose: false,
                source_loose: false,
            }
        },
    );
    let item = doc.alloc_container(BlockKind::ListItem);
    if let Some(checked) = task {
        doc.set_extra(item, NodeExtra::TaskItem { checked });
    }
    doc.arena.detach(leaf);
    doc.arena.append_child(item, leaf);
    doc.arena.append_child(list, item);
    doc.arena.insert_after(parent, prev, list);
    changes.push(DocChange::TreeSpliced {
        parent,
        before: prev,
        removed: vec![leaf],
        inserted: vec![list],
    });
    doc.bump_structure(parent);
    normalize::sync_loose_up(doc, list, &mut changes);
    let _ = doc.commit(before, changes);
    Caret {
        block: leaf.index,
        offset: 0,
    }
}

pub(crate) fn join_nested_into_host(doc: &mut Document, caret: Caret) -> Caret {
    let Some(path) = Path::at(doc, caret) else {
        return caret;
    };
    let Some(nested) = path.list else {
        return caret;
    };
    let Some(host_item) = doc.arena.get(nested).and_then(|n| n.parent) else {
        return caret;
    };
    if doc.arena.get(host_item).map(|n| n.kind) != Some(BlockKind::ListItem) {
        return caret;
    }
    let Some(host_list) = doc.arena.get(host_item).and_then(|n| n.parent) else {
        return caret;
    };
    if doc.arena.get(host_list).map(|n| n.kind) != Some(BlockKind::List) {
        return caret;
    }
    if doc.extra(nested).ordered_start().is_some() != doc.extra(host_list).ordered_start().is_some()
    {
        return caret;
    }
    let nest_prev = doc.arena.get(nested).and_then(|n| n.prev_sibling);
    let moved: Vec<NodeId> = doc.arena.children(nested).collect();
    let before = doc.revision;
    doc.arena.snapshot(nested);
    let mut after = Some(host_item);
    for k in &moved {
        doc.arena.detach(*k);
        doc.arena.insert_after(host_list, after, *k);
        after = Some(*k);
    }
    let mut changes = vec![DocChange::TreeSpliced {
        parent: host_item,
        before: nest_prev,
        removed: vec![nested],
        inserted: Vec::new(),
    }];
    normalize::tombstone(doc, nested);
    doc.bump_structure(host_item);
    if !moved.is_empty() {
        changes.push(DocChange::TreeSpliced {
            parent: host_list,
            before: Some(host_item),
            removed: Vec::new(),
            inserted: moved,
        });
        doc.bump_structure(host_list);
    }
    normalize::sync_loose_up(doc, host_list, &mut changes);
    let _ = doc.commit(before, changes);
    caret
}
