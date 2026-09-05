use super::super::Document;
use super::super::arena::NodeId;
use super::super::change::DocChange;
use super::Caret;
use crate::block::{BlockKind, NodeExtra};

pub(crate) fn tombstone(doc: &mut Document, id: NodeId) {
    doc.arena.detach(id);
    doc.arena.tombstone(id);
}

pub(crate) fn prune_empty_up(doc: &mut Document, start: NodeId, changes: &mut Vec<DocChange>) {
    let mut cur = Some(start);
    while let Some(id) = cur {
        let Some(node) = doc.arena.get(id) else {
            return;
        };
        let parent = node.parent;
        let prev = node.prev_sibling;
        let empty = matches!(node.kind, BlockKind::List | BlockKind::ListItem)
            && node.first_child.is_none();
        if !empty {
            return;
        }
        tombstone(doc, id);
        if let Some(p) = parent {
            changes.push(DocChange::TreeSpliced {
                parent: p,
                before: prev,
                removed: vec![id],
                inserted: Vec::new(),
            });
            doc.bump_structure(p);
        }
        cur = parent;
    }
}

pub(crate) fn sync_loose_up(doc: &mut Document, start: NodeId, changes: &mut Vec<DocChange>) {
    sync_loose_up_inner(doc, start, changes, None);
}

pub(crate) fn sync_loose_after_join(
    doc: &mut Document,
    start: NodeId,
    joined_list: NodeId,
    changes: &mut Vec<DocChange>,
) {
    sync_loose_up_inner(doc, start, changes, Some(joined_list));
}

fn sync_loose_up_inner(
    doc: &mut Document,
    start: NodeId,
    changes: &mut Vec<DocChange>,
    clear_source_loose: Option<NodeId>,
) {
    let mut cur = Some(start);
    while let Some(id) = cur {
        let Some(node) = doc.arena.get(id) else {
            return;
        };
        let parent = node.parent;
        if node.kind == BlockKind::List {
            sync_loose(doc, id, changes, clear_source_loose == Some(id));
        }
        cur = parent;
    }
}

fn sync_loose(
    doc: &mut Document,
    list: NodeId,
    changes: &mut Vec<DocChange>,
    clear_source_loose: bool,
) {
    let extra = doc.extra(list);
    let structural_loose = doc
        .arena
        .children(list)
        .any(|item| doc.arena.children(item).nth(1).is_some());
    let source_loose = extra.list_source_loose() && !clear_source_loose;
    let want = source_loose || structural_loose;
    if extra.list_loose() == want && extra.list_source_loose() == source_loose {
        return;
    }
    let old_extra = extra;
    doc.set_extra(
        list,
        NodeExtra::List {
            start: extra.ordered_start(),
            marker: extra.list_marker(),
            loose: want,
            source_loose,
        },
    );
    changes.push(doc.attrs_change(list, BlockKind::List, old_extra));
}

impl Document {
    pub(crate) fn ensure_trailing_blank(&mut self) {
        let _ = ensure_trailing_blank_paragraph(self, None);
    }

    pub(crate) fn ensure_trailing_blank_at(&mut self, caret: Caret) -> Caret {
        ensure_trailing_blank_paragraph(self, Some(caret))
    }

    pub(crate) fn clamp_live_caret(&self, caret: Caret) -> Caret {
        if let Some(id) = self.live_id(caret.block) {
            let n = self.caret_text(id).len();
            return Caret {
                block: caret.block,
                offset: caret.offset.min(n),
            };
        }
        match self.first_text_leaf() {
            Some(block) => Caret { block, offset: 0 },
            None => caret,
        }
    }
}

pub(crate) fn is_blank_paragraph(doc: &Document, id: NodeId) -> bool {
    doc.arena
        .get(id)
        .is_some_and(|n| n.kind == BlockKind::Paragraph)
        && matches!(doc.extra(id), NodeExtra::None)
        && doc.display(id).is_empty()
        && doc.leaf_source(id).trim().is_empty()
}

pub(crate) fn ensure_trailing_blank_paragraph(doc: &mut Document, caret: Option<Caret>) -> Caret {
    let before_rev = doc.revision();
    let mut changes = Vec::new();
    let root = doc.root;
    loop {
        let last = doc.arena.get(root).and_then(|n| n.last_child);
        let prev = last.and_then(|id| doc.arena.get(id).and_then(|n| n.prev_sibling));
        if last.is_some_and(|id| is_blank_paragraph(doc, id))
            && prev.is_some_and(|id| is_blank_paragraph(doc, id))
        {
            let id = last.expect("last");

            let drop_sentinel = match caret {
                None => true,
                Some(c) => prev.is_some_and(|p| c.block == p.index),
            };
            if !drop_sentinel {
                break;
            }
            tombstone(doc, id);
            doc.bump_structure(root);
            changes.push(DocChange::TreeSpliced {
                parent: root,
                before: prev,
                removed: vec![id],
                inserted: Vec::new(),
            });
            continue;
        }
        break;
    }
    let last = doc.arena.get(root).and_then(|n| n.last_child);

    if let Some(id) = last.filter(|&id| is_blank_paragraph(doc, id)) {
        if !changes.is_empty() {
            let _ = doc.commit(before_rev, changes);
        }
        return caret.unwrap_or(Caret {
            block: id.index,
            offset: 0,
        });
    }
    let para = doc.alloc_leaf(BlockKind::Paragraph);
    doc.arena.append_child(root, para);
    doc.bump_structure(root);
    changes.push(DocChange::TreeSpliced {
        parent: root,
        before: last,
        removed: Vec::new(),
        inserted: vec![para],
    });
    let _ = doc.commit(before_rev, changes);
    caret.unwrap_or(Caret {
        block: para.index,
        offset: 0,
    })
}
