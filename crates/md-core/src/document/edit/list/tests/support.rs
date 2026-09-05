use crate::block::BlockKind;
use crate::document::Document;
use crate::document::arena::NodeId;
use crate::document::change::{ChangeSet, DocChange};
use crate::document::edit::Caret;

pub(super) fn caret(block: u32, offset: usize) -> Caret {
    Caret { block, offset }
}

pub(super) fn first_list(doc: &Document) -> NodeId {
    doc.preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .expect("list")
}

pub(super) fn items(doc: &Document, list: NodeId) -> Vec<NodeId> {
    doc.arena.children(list).collect()
}

pub(super) fn lead(doc: &Document, item: NodeId) -> NodeId {
    doc.arena.children(item).next().expect("lead")
}

pub(super) fn assert_changeset_parents_live(doc: &Document, changes: &ChangeSet) {
    let removed_in_batch: std::collections::HashSet<NodeId> = changes
        .changes
        .iter()
        .filter_map(|c| match c {
            DocChange::TreeSpliced { removed, .. } => Some(removed.iter().copied()),
            _ => None,
        })
        .flatten()
        .collect();
    for c in &changes.changes {
        match c {
            DocChange::TreeSpliced {
                parent,
                before,
                inserted,
                ..
            } => {
                assert!(
                    doc.arena.get(*parent).is_some() || removed_in_batch.contains(parent),
                    "TreeSpliced parent is tombstoned and its removal is not on the books"
                );
                if let Some(b) = before {
                    assert!(
                        doc.arena.get(*b).is_some(),
                        "TreeSpliced before is tombstoned"
                    );
                }
                for n in inserted {
                    assert!(
                        doc.arena.get(*n).is_some(),
                        "TreeSpliced inserted is tombstoned"
                    );
                }
            }
            DocChange::AttrsChanged { node, .. } => {
                assert!(
                    doc.arena.get(*node).is_some(),
                    "AttrsChanged node is tombstoned"
                );
            }
            _ => {}
        }
    }
}
