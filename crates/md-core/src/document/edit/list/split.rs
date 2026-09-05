use crate::block::{BlockKind, NodeExtra};
use crate::document::Document;
use crate::document::NodeId;
use crate::document::change::DocChange;
use crate::document::edit::path::Path;
use crate::document::edit::{Caret, normalize};

pub(crate) fn split_item(doc: &mut Document, path: &Path) -> Caret {
    let item = path.item.expect("item");
    let list = path.list.expect("list");
    let item_prev = doc.arena.get(item).and_then(|n| n.prev_sibling);
    let before = doc.revision;
    let (text_changed, new_leaf) = doc.split_leaf_nodes(path.leaf, path.offset);
    let new_item = doc.alloc_container(BlockKind::ListItem);
    if doc.extra(item).task_checked().is_some() {
        doc.set_extra(new_item, NodeExtra::TaskItem { checked: false });
    }
    doc.arena.insert_after(list, Some(item), new_item);
    let mut cur = Some(new_leaf);
    let mut moved: Vec<NodeId> = Vec::new();
    while let Some(id) = cur {
        let next = doc.arena.get(id).and_then(|n| n.next_sibling);
        doc.arena.detach(id);
        doc.arena.append_child(new_item, id);
        if id != new_leaf {
            moved.push(id);
        }
        cur = next;
    }
    doc.bump_structure(item);
    let mut changes = vec![
        text_changed,
        DocChange::TreeSpliced {
            parent: list,
            before: item_prev,
            removed: vec![item],
            inserted: vec![item, new_item],
        },
    ];

    if !moved.is_empty() {
        changes.push(DocChange::TreeSpliced {
            parent: item,
            before: Some(path.leaf),
            removed: moved.clone(),
            inserted: Vec::new(),
        });
        changes.push(DocChange::TreeSpliced {
            parent: new_item,
            before: Some(new_leaf),
            removed: Vec::new(),
            inserted: moved,
        });
    }
    doc.bump_structure(list);
    normalize::sync_loose_up(doc, list, &mut changes);
    let _ = doc.commit(before, changes);
    Caret {
        block: new_leaf.index,
        offset: 0,
    }
}
